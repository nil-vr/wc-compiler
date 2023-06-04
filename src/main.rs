use std::{
    borrow::Cow,
    collections::{hash_map::Entry, BTreeMap, BTreeSet, HashMap},
    ffi::OsStr,
    fmt,
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{self, BufReader, BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use clap::Parser;
use error::StateParseError;
use iso639_enum::IsoCompat;
use miette::{
    miette, Context, Diagnostic, IntoDiagnostic, MietteHandler, NamedSource, Report, ReportHandler,
    Result, Severity,
};

use output::{Hashtag, Zone};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{de::Visitor, Deserialize, Serialize};
use sha2::{digest::Output, Digest, Sha256};
use state::State;
use tempfile::NamedTempFile;

use crate::error::{
    CanceledOutOfRange, ConfirmedOutOfRange, ImageTooLarge, MissingTimeZone, MultiplePosters,
};

mod error;
mod input;
mod output;
mod state;
mod time;

#[derive(Parser)]
struct Args {
    input: PathBuf,
    output: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let errors = Arc::new(AtomicUsize::new(0));
    miette::set_hook({
        let errors = errors.clone();
        Box::new(move |_| {
            Box::new(Handler {
                inner: MietteHandler::new(),
                errors: errors.clone(),
            })
        })
    })
    .unwrap();

    if !args.output.exists() {
        if let Err(err) = fs::create_dir_all(&args.output)
            .into_diagnostic()
            .wrap_err("Could not create output directory")
        {
            eprintln!("{err:?}");
            return ExitCode::FAILURE;
        }
    }

    let now = Utc::now();

    let mut state = match load_state(&args.output) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("{error:?}");
            return ExitCode::FAILURE;
        }
    };
    let mut posters = Posters::load(args.output.join("posters"), &state, now);

    let mut files = BTreeSet::<PathBuf>::new();
    match fs::read_dir(&args.input)
        .into_diagnostic()
        .wrap_err("Collecting input failed.")
    {
        Ok(dir) => {
            for file in dir {
                match file.into_diagnostic().wrap_err("Collecting input failed.") {
                    Ok(file) => {
                        files.insert(file.path());
                    }
                    Err(error) => {
                        eprintln!("{error:?}");
                    }
                }
            }
        }
        Err(error) => {
            eprintln!("{error:?}");
        }
    }

    let meta_file = if let Some(meta_file) = files
        .iter()
        .find(|f| f.file_name() == Some(OsStr::new("meta.toml")))
    {
        match fs::read_to_string(meta_file)
            .into_diagnostic()
            .wrap_err_with(|| format!("Reading {} failed.", meta_file.display()))
        {
            Ok(content) => Arc::new(content),
            Err(error) => {
                eprintln!("{error:?}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        eprintln!("{:?}", miette!("meta.toml not found."));
        return ExitCode::FAILURE;
    };

    let meta = match input::Meta::deserialize(toml::Deserializer::new(&meta_file))
        .map_err(|error| error::EventParseError {
            src: NamedSource::new("meta.toml", meta_file.clone()),
            location: error.span().map(|s| s.into()),
            error,
        })
        .wrap_err("Parsing meta.toml failed.")
    {
        Ok(meta) => meta,
        Err(error) => {
            eprintln!("{error:?}");
            return ExitCode::FAILURE;
        }
    };

    let output_meta = output::Meta {
        title: &meta.title,
        description: meta.description.as_deref(),
        link: meta.link.as_deref(),
        compiled_time: now.timestamp(),
        languages: meta
            .languages
            .iter()
            .map(|(&id, language)| {
                (
                    id,
                    output::MetaLanguage {
                        title: language.title.as_deref(),
                        description: language.description.as_deref(),
                        link: language.link.as_deref(),
                    },
                )
            })
            .collect(),
    };

    let mut event_files = Vec::new();
    for file in files.iter().filter(|f| {
        f.file_name() != Some(OsStr::new("meta.toml")) && f.extension() == Some(OsStr::new("toml"))
    }) {
        match fs::read_to_string(file)
            .into_diagnostic()
            .wrap_err_with(|| format!("Reading {} failed.", file.display()))
        {
            Ok(content) => {
                event_files.push(EventFile {
                    path: file,
                    content: Arc::new(content),
                });
            }
            Err(error) => {
                eprintln!("{error:?}");
            }
        };
    }

    let mut input_events = Vec::with_capacity(event_files.len());
    for file in event_files.iter() {
        match input::Event::deserialize(toml::Deserializer::new(&file.content))
            .map_err(|error| error::EventParseError::new(error, file))
            .wrap_err_with(|| format!("Parsing {} failed.", file.path.display()))
        {
            Ok(input) => {
                input_events.push(Event {
                    source: file,
                    event: input,
                });
            }
            Err(error) => {
                eprintln!("{error:?}");
            }
        }
    }

    let zones = time::collect_zones(now);

    let mut output_events = Vec::with_capacity(input_events.len());
    for event in input_events.iter() {
        match prepare_event(event, &files, &zones, now, &mut posters).wrap_err_with(|| {
            format!(
                "File {} could not be processed.",
                event.source.path.display(),
            )
        }) {
            Ok(event) => output_events.push(event),
            Err(error) => eprintln!("{error:?}"),
        }
    }

    if errors.load(Ordering::SeqCst) == 0 {
        posters.save(&mut state);
        if let Err(e) = safely_save(&args.output, "state.json", |mut t| {
            serde_json::to_writer_pretty(&mut t, &state).into_diagnostic()?;
            t.write_all(b"\n").into_diagnostic()
        }) {
            eprintln!("{e:?}");
            return ExitCode::FAILURE;
        }

        if let Err(e) = safely_save(&args.output, "data.json", |mut t| {
            serde_json::to_writer(
                &mut t,
                &output::Data {
                    meta: &output_meta,
                    events: &output_events,
                    zones: &zones,
                },
            )
            .into_diagnostic()?;
            t.write_all(b"\n").into_diagnostic()
        }) {
            eprintln!("{e:?}");
            return ExitCode::FAILURE;
        }
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn load_state(output_path: &Path) -> miette::Result<State> {
    let state_path = output_path.join("state.json");
    let state = match fs::read(&state_path) {
        Ok(state) => state,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            eprintln!("Initializing new state");
            return Ok(Default::default());
        }
        Err(e) => {
            return Err(e)
                .into_diagnostic()
                .wrap_err_with(|| format!("Could not read {}", state_path.display()))
        }
    };
    match serde_json::from_slice(&state) {
        Ok(state) => Ok(state),
        Err(e) => Err(StateParseError::new(e, &output_path.to_string_lossy(), state).into()),
    }
}

fn safely_save(
    output_path: &Path,
    name: &str,
    save: impl FnOnce(&mut BufWriter<&mut NamedTempFile>) -> miette::Result<()>,
) -> miette::Result<()> {
    let save_path = output_path.join(name);
    tempfile::Builder::new()
        .tempfile_in(output_path)
        .into_diagnostic()
        .and_then(|mut t| {
            {
                let mut t = BufWriter::new(&mut t);
                save(&mut t)?;
                t.flush().into_diagnostic()?;
            }
            t.persist(&save_path).into_diagnostic()?;
            Ok(())
        })
        .wrap_err_with(|| format!("Could not save {}", save_path.display()))
}

struct Handler {
    inner: MietteHandler,
    errors: Arc<AtomicUsize>,
}

impl ReportHandler for Handler {
    fn debug(
        &self,
        error: &(dyn Diagnostic),
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        let severity = error.severity().unwrap_or(miette::Severity::Error);
        if severity == Severity::Error {
            self.errors.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.debug(error, f)
    }
}

pub struct EventFile<'a> {
    path: &'a Path,
    content: Arc<String>,
}

pub struct Event<'a> {
    source: &'a EventFile<'a>,
    event: input::Event<'a>,
}

impl<'a> Event<'a> {
    pub fn get_time_for_day(
        &self,
        date: NaiveDate,
        timezone: Tz,
        force: bool,
    ) -> Result<Option<DateTime<Tz>>> {
        if let Some(start_date) = self.event.start_date {
            if date < start_date {
                return Ok(None);
            }
        }
        if let Some(end_date) = self.event.end_date {
            if end_date < date {
                return Ok(None);
            }
        }
        let day = match date.weekday() {
            chrono::Weekday::Mon => self.event.days.monday.as_ref(),
            chrono::Weekday::Tue => self.event.days.tuesday.as_ref(),
            chrono::Weekday::Wed => self.event.days.wednesday.as_ref(),
            chrono::Weekday::Thu => self.event.days.thursday.as_ref(),
            chrono::Weekday::Fri => self.event.days.friday.as_ref(),
            chrono::Weekday::Sat => self.event.days.saturday.as_ref(),
            chrono::Weekday::Sun => self.event.days.sunday.as_ref(),
        };
        if !force && day.is_none() {
            return Ok(None);
        }
        let time = day.and_then(|d| d.start).unwrap_or(self.event.start).0;
        Ok(date.and_time(time).and_local_timezone(timezone).earliest())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Pc,
    Quest,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Language(iso639_enum::Language);

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct LanguageVisitor;

        impl<'de> Visitor<'de> for LanguageVisitor {
            type Value = Language;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "an ISO 639-1 language code")
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                iso639_enum::Language::from_iso639_1(v)
                    .map(Language)
                    .map_err(E::custom)
            }
        }

        deserializer.deserialize_str(LanguageVisitor)
    }
}

impl Ord for Language {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .iso639_1()
            .cmp(&other.0.iso639_1())
            .then_with(|| (self.0 as usize).cmp(&(other.0 as usize)))
    }
}

impl PartialOrd for Language {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for Language {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.iso639_1().unwrap())
    }
}

impl Hash for Language {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as usize).hash(state);
    }
}

fn prepare_event<'a, 'b>(
    event: &'a Event<'a>,
    files: &'b BTreeSet<PathBuf>,
    zones: &'b BTreeMap<String, Zone>,
    now: DateTime<Utc>,
    posters: &'b mut Posters,
) -> Result<output::Event<'a>> {
    if !zones.contains_key(event.event.timezone.as_ref().as_ref()) {
        return Err(MissingTimeZone::new(event).into());
    }
    let Ok(tz) = Tz::from_str(event.event.timezone.as_ref().as_ref()) else {
        return Err(MissingTimeZone::new(event).into());
    };

    let poster = event
        .event
        .info
        .poster
        .as_deref()
        .map(Path::new)
        .map(Cow::Borrowed)
        .or_else(|| guess_poster(event, files).map(Cow::Owned));
    let poster = poster.and_then(try_load_poster);

    let name = event
        .event
        .info
        .name
        .as_deref()
        .map(Cow::Borrowed)
        .unwrap_or_else(|| event.source.path.file_stem().unwrap().to_string_lossy());

    let mut languages = BTreeMap::new();
    for (&language_id, language) in &event.event.languages {
        languages.insert(
            language_id,
            output::EventLanguage {
                name: language.info.name.as_deref(),
                info: convert_event_info(&language.info, posters),
                days: convert_event_days(&language.days, posters),
            },
        );
    }

    let confirmed = match &event.event.confirmed {
        input::DateSet::All(b) => output::DateSet::All(*b),
        input::DateSet::Dates(confirmed) => {
            let mut future = Vec::with_capacity(confirmed.len());
            for date in confirmed {
                let Some(time) = event.get_time_for_day(*date, tz, true)? else {
                    eprintln!(
                        "{:?}",
                        Report::new(ConfirmedOutOfRange {
                            date: *date,
                            src: event.source.into(),
                        }),
                    );
                    continue;
                };
                if now < time {
                    future.push(*date);
                }
            }
            if future.is_empty() {
                output::DateSet::All(false)
            } else {
                output::DateSet::Dates(future)
            }
        }
    };

    let canceled = match &event.event.canceled {
        input::DateSet::All(b) => output::DateSet::All(*b),
        input::DateSet::Dates(canceled) => {
            let mut future = Vec::with_capacity(canceled.len());
            for date in canceled {
                let Some(time) = event.get_time_for_day(*date, tz, false)? else {
                    eprintln!(
                        "{:?}",
                        Report::new(CanceledOutOfRange {
                            date: *date,
                            src: event.source.into(),
                        }),
                    );
                    continue;
                };
                if now < time {
                    future.push(*date);
                }
            }
            if future.is_empty() {
                output::DateSet::All(false)
            } else {
                output::DateSet::Dates(future)
            }
        }
    };

    Ok(output::Event {
        name,
        start_date: event
            .event
            .start_date
            .map(|d| {
                d.and_time(NaiveTime::MIN)
                    .and_local_timezone(tz)
                    .earliest()
                    .ok_or_else(|| miette!("Midnight of start date does not exist"))
                    .map(|t| t.timestamp())
            })
            .transpose()?,
        end_date: event
            .event
            .end_date
            .map(|d| {
                d.checked_add_days(Days::new(1))
                    .and_then(|d| d.and_time(NaiveTime::MIN).and_local_timezone(tz).earliest())
                    .ok_or_else(|| miette!("Midnight of day after end date does not exist"))
                    .map(|t| t.timestamp())
            })
            .transpose()?,
        info: output::EventInfo {
            poster: poster.as_ref().and_then(|p| posters.try_get_output(p)),
            ..convert_event_info(&event.event.info, posters)
        },
        timezone: event.event.timezone.as_ref().as_ref(),
        start: (event.event.start.0 - NaiveTime::default()).num_minutes() as i32,
        duration: event.event.duration.0.num_minutes() as i32,
        platforms: &event.event.platforms,
        days: convert_event_days(&event.event.days, posters),
        languages,
        confirmed,
        canceled,
    })
}

struct PosterInfo<'a> {
    pub source: Cow<'a, Path>,
    pub width: u16,
    pub height: u16,
    pub hash: Output<Sha256>,
}

struct Posters {
    directory: PathBuf,
    posters: Vec<state::Poster>,
    by_sha256: HashMap<Output<Sha256>, u8>,
    now: DateTime<Utc>,
}

impl Posters {
    fn load(directory: PathBuf, state: &State, now: DateTime<Utc>) -> Self {
        let posters = state.posters.clone();
        let mut by_sha256 = HashMap::with_capacity(posters.len());
        for (i, poster) in posters.iter().enumerate() {
            by_sha256.insert(poster.sha256, i as u8);
        }

        if !directory.exists() {
            if let Err(err) = fs::create_dir(&directory) {
                eprintln!("{err:?}");
            }
        }

        Posters {
            directory,
            posters,
            by_sha256,
            now,
        }
    }

    fn save(self, state: &mut State) {
        state.posters = self.posters;
    }

    fn try_get_output(&mut self, poster: &PosterInfo<'_>) -> Option<output::PosterInfo> {
        let index = match self.by_sha256.entry(poster.hash) {
            Entry::Occupied(e) => {
                let index = *e.get();
                self.posters[index as usize].last_used = self.now;
                index
            }
            Entry::Vacant(e) => {
                let index = if self.posters.len() < 255 {
                    let index = self.posters.len() as u8;
                    self.posters.push(state::Poster {
                        last_used: self.now,
                        sha256: poster.hash,
                    });
                    e.insert(index);
                    index
                } else {
                    let index = self
                        .posters
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, p)| p.last_used)
                        .unwrap()
                        .0 as u8;
                    e.insert(index);
                    self.by_sha256.remove(&self.posters[index as usize].sha256);
                    self.posters[index as usize] = state::Poster {
                        last_used: self.now,
                        sha256: poster.hash,
                    };
                    index
                };
                if let Err(err) =
                    fs::copy(&poster.source, self.directory.join(format!("{index:02x}")))
                {
                    eprintln!("{err:?}");
                    return None;
                }
                index
            }
        };
        Some(output::PosterInfo {
            number: index,
            width: poster.width,
            height: poster.height,
        })
    }
}

fn try_load_poster(image_path: Cow<'_, Path>) -> Option<PosterInfo<'_>> {
    let file = match File::open(&image_path)
        .into_diagnostic()
        .with_context(|| format!("Could not open {}", image_path.display()))
    {
        Ok(file) => file,
        Err(e) => {
            eprintln!("{:?}", e);
            return None;
        }
    };
    let mut reader = BufReader::new(file);
    match imagesize::reader_size(&mut reader)
        .map_err(|e| miette!(e))
        .wrap_err_with(|| format!("Image {} could not be processed.", image_path.display()))
    {
        Ok(size) => {
            if size.width > 2048 || size.height > 2048 {
                eprintln!(
                    "{:?}",
                    Report::new(ImageTooLarge {
                        path: image_path.to_path_buf(),
                        width: size.width,
                        height: size.height,
                    }),
                );
                None
            } else {
                let mut hasher = Sha256::new();
                match reader
                    .seek(SeekFrom::Start(0))
                    .and_then(|_| io::copy(&mut reader, &mut hasher))
                    .into_diagnostic()
                    .wrap_err_with(|| format!("Could not read {}", image_path.display()))
                {
                    Ok(_) => Some(PosterInfo {
                        source: image_path,
                        width: size.width as u16,
                        height: size.height as u16,
                        hash: hasher.finalize(),
                    }),
                    Err(e) => {
                        eprintln!("{:?}", e);
                        None
                    }
                }
            }
        }
        Err(error) => {
            eprintln!("{error:?}");
            None
        }
    }
}

fn convert_event_days<'a>(
    value: &'a input::EventDays<'a>,
    posters: &mut Posters,
) -> output::EventDays<'a> {
    output::EventDays {
        monday: value
            .monday
            .as_ref()
            .map(|day| convert_event_day(day, posters)),
        tuesday: value
            .tuesday
            .as_ref()
            .map(|day| convert_event_day(day, posters)),
        wednesday: value
            .wednesday
            .as_ref()
            .map(|day| convert_event_day(day, posters)),
        thursday: value
            .thursday
            .as_ref()
            .map(|day| convert_event_day(day, posters)),
        friday: value
            .friday
            .as_ref()
            .map(|day| convert_event_day(day, posters)),
        saturday: value
            .saturday
            .as_ref()
            .map(|day| convert_event_day(day, posters)),
        sunday: value
            .sunday
            .as_ref()
            .map(|day| convert_event_day(day, posters)),
    }
}

fn convert_event_day<'a>(
    value: &'a input::EventDay<'a>,
    posters: &mut Posters,
) -> output::EventDay<'a> {
    output::EventDay {
        name: value.info.name.as_deref(),
        duration: value.duration.map(|d| d.0.num_minutes() as i32),
        info: convert_event_info(&value.info, posters),
    }
}

fn convert_event_info<'a>(
    value: &'a input::EventInfo<'a>,
    posters: &mut Posters,
) -> output::EventInfo<'a> {
    output::EventInfo {
        poster: value
            .poster
            .as_deref()
            .and_then(|p| try_load_poster(Cow::Borrowed(Path::new(p))))
            .and_then(|p| posters.try_get_output(&p)),
        description: value.description.as_deref(),
        web: value.web.as_deref(),
        discord: value.discord.as_deref(),
        group: value.group.as_deref(),
        hashtag: value.hashtag.as_deref().map(Hashtag::from),
        twitter: value.twitter.as_deref(),
        join: &value.join,
        world: value.world.as_ref(),
        weeks: value.weeks.as_deref(),
    }
}

fn guess_poster(event: &Event, files: &BTreeSet<PathBuf>) -> Option<PathBuf> {
    let mut image_extensions = ["webp", "jpeg", "jpg", "png"].into_iter();
    let mut image_path = PathBuf::from(event.source.path);
    let found = loop {
        let Some(extension) = image_extensions.next() else {
            return None;
        };
        image_path.set_extension(extension);
        if files.contains(&image_path) {
            break image_path.clone();
        }
    };
    loop {
        let Some(extension) = image_extensions.next() else {
            return Some(found);
        };
        image_path.set_extension(extension);
        if files.contains(&image_path) {
            eprintln!(
                "{:?}",
                Report::new(MultiplePosters {
                    found: found.clone(),
                    extra: image_path.clone(),
                })
            )
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct User<'a> {
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    #[serde(borrow)]
    pub id: Cow<'a, str>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct World<'a> {
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    #[serde(borrow)]
    pub id: Cow<'a, str>,
}

impl<'a> From<&'a str> for Hashtag<'a> {
    fn from(value: &'a str) -> Self {
        const QUERY: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');
        const PATH: &AsciiSet = &QUERY.add(b'?').add(b'`').add(b'{').add(b'}');
        const USER_INFO: &AsciiSet = &PATH
            .add(b'/')
            .add(b':')
            .add(b';')
            .add(b'=')
            .add(b'@')
            .add(b'[')
            .add(b'\\')
            .add(b']')
            .add(b'^')
            .add(b'|');
        const COMPONENT: &AsciiSet = &USER_INFO.add(b'$').add(b'&').add(b'+').add(b',');
        let escaped = Cow::from(utf8_percent_encode(value, COMPONENT));
        if value == &escaped {
            Hashtag::Safe(value)
        } else {
            Hashtag::Escaped {
                display: value,
                escaped: escaped.into_owned(),
            }
        }
    }
}
