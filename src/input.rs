use std::{borrow::Cow, collections::HashMap};

use chrono::{Duration, NaiveDate, NaiveTime};
use serde::{de::Error, Deserialize, Deserializer};
use smallvec::{smallvec, SmallVec};
use toml::Spanned;

use crate::{Language, Platform, User, World};

//TODO: unused?
mod one_or_many;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event<'a> {
    #[serde(borrow, flatten)]
    pub info: EventInfo<'a>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    #[serde(borrow)]
    pub timezone: Spanned<Cow<'a, str>>,
    pub start: Time<NaiveTime>,
    pub duration: Time<Duration>,
    #[serde(default = "default_platforms")]
    pub platforms: SmallVec<[Platform; 2]>,
    #[serde(borrow, default = "default_days")]
    pub days: EventDays<'a>,
    #[serde(borrow, default)]
    pub languages: HashMap<Language, EventLanguage<'a>>,
    #[serde(default = "DateSet::all")]
    pub confirmed: DateSet,
    #[serde(default = "DateSet::none")]
    pub canceled: DateSet,
}

fn default_platforms() -> SmallVec<[Platform; 2]> {
    smallvec![Platform::Pc]
}

fn default_days() -> EventDays<'static> {
    EventDays {
        monday: Some(EventDay::default()),
        tuesday: Some(EventDay::default()),
        wednesday: Some(EventDay::default()),
        thursday: Some(EventDay::default()),
        friday: Some(EventDay::default()),
        saturday: Some(EventDay::default()),
        sunday: Some(EventDay::default()),
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventInfo<'a> {
    #[serde(borrow)]
    pub name: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub description: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub web: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub poster: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub hashtag: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub twitter: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub group: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub discord: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub join: Vec<User<'a>>,
    #[serde(borrow)]
    pub world: Option<World<'a>>,
    pub weeks: Option<SmallVec<[u8; 5]>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDays<'a> {
    #[serde(borrow)]
    pub monday: Option<EventDay<'a>>,
    #[serde(borrow)]
    pub tuesday: Option<EventDay<'a>>,
    #[serde(borrow)]
    pub wednesday: Option<EventDay<'a>>,
    #[serde(borrow)]
    pub thursday: Option<EventDay<'a>>,
    #[serde(borrow)]
    pub friday: Option<EventDay<'a>>,
    #[serde(borrow)]
    pub saturday: Option<EventDay<'a>>,
    #[serde(borrow)]
    pub sunday: Option<EventDay<'a>>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDay<'a> {
    #[serde(borrow, flatten)]
    pub info: EventInfo<'a>,
    pub start: Option<Time<NaiveTime>>,
    pub duration: Option<Time<Duration>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventLanguage<'a> {
    #[serde(borrow, flatten)]
    pub info: EventInfo<'a>,
    #[serde(borrow, flatten)]
    pub days: EventDays<'a>,
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Time<T>(pub T);

impl<'de> Deserialize<'de> for Time<NaiveTime> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match NaiveTime::default()
            .overflowing_add_signed(Time::<Duration>::deserialize(deserializer)?.0)
        {
            (time, 0) => Ok(Time(time)),
            (_, _) => Err(D::Error::custom("Time must be less than 24:00")),
        }
    }
}

impl<'de> Deserialize<'de> for Time<Duration> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawTime<'a> {
            #[serde(borrow)]
            String(Cow<'a, str>),
            Minutes(u16),
            Time(toml::value::Datetime),
        }

        let raw = RawTime::deserialize(deserializer)?;
        let minutes = match raw {
            RawTime::String(v) => {
                if let Some((hours, minutes)) = v.split_once(':') {
                    let hours: u16 = hours.parse().map_err(D::Error::custom)?;
                    let minutes: u16 = minutes.parse().map_err(D::Error::custom)?;
                    hours * 60 + minutes
                } else {
                    v.parse().map_err(D::Error::custom)?
                }
            }
            RawTime::Minutes(minutes) => minutes,
            RawTime::Time(time) => {
                if time.date.is_some() {
                    return Err(D::Error::custom("Time should not have a date"));
                }
                if time.offset.is_some() {
                    return Err(D::Error::custom("Time should not have an offset"));
                }
                let Some(time) = time.time else {
                return Err(D::Error::custom("Time must contain a time"));
            };
                if time.second != 0 || time.nanosecond != 0 {
                    return Err(D::Error::custom("Time must contain whole minutes"));
                }
                time.hour as u16 * 60 + time.minute as u16
            }
        };
        Ok(Time(Duration::minutes(minutes as i64)))
    }
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum DateSet {
    All(bool),
    Dates(Vec<NaiveDate>),
}

impl DateSet {
    pub fn all() -> Self {
        DateSet::All(true)
    }

    pub fn none() -> Self {
        DateSet::All(false)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta<'a> {
    #[serde(borrow)]
    pub title: Cow<'a, str>,
    #[serde(borrow)]
    pub description: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub link: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub languages: HashMap<Language, MetaLanguage<'a>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaLanguage<'a> {
    #[serde(borrow)]
    pub title: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub description: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub link: Option<Cow<'a, str>>,
}
