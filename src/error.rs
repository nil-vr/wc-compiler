use std::{fmt, path::PathBuf};

use chrono::NaiveDate;
use miette::{Diagnostic, NamedSource, SourceOffset, SourceSpan};

use crate::{Event, EventFile};

#[derive(Debug, Diagnostic, thiserror::Error)]
pub struct EventParseError {
    pub error: toml::de::Error,
    #[source_code]
    pub src: NamedSource,
    #[label]
    pub location: Option<SourceSpan>,
}

impl fmt::Display for EventParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.message().fmt(f)
    }
}

impl From<&EventFile<'_>> for NamedSource {
    fn from(value: &EventFile) -> Self {
        NamedSource::new(value.path.to_string_lossy(), value.content.clone())
    }
}

impl EventParseError {
    pub fn new(error: toml::de::Error, source: &EventFile) -> Self {
        Self {
            src: source.into(),
            location: error.span().map(|span| span.into()),
            error,
        }
    }
}

#[derive(Debug, Diagnostic, thiserror::Error)]
pub struct StateParseError {
    pub error: serde_json::Error,
    #[source_code]
    pub src: NamedSource,
    #[label]
    pub location: SourceSpan,
}

impl fmt::Display for StateParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl StateParseError {
    pub fn new(error: serde_json::Error, name: &str, source: Vec<u8>) -> Self {
        Self {
            location: SourceSpan::new(
                SourceOffset::from_location(
                    String::from_utf8_lossy(&source),
                    error.line(),
                    error.column(),
                ),
                SourceOffset::from(1),
            ),
            src: NamedSource::new(name, source),
            error,
        }
    }
}

#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("Unknown time zone {name:?}")]
pub struct MissingTimeZone {
    name: String,
    #[source_code]
    src: NamedSource,
    #[label]
    location: SourceSpan,
}

impl MissingTimeZone {
    pub fn new(event: &Event) -> Self {
        Self {
            name: event.event.timezone.as_ref().as_ref().to_owned(),
            src: event.source.into(),
            location: event.event.timezone.span().into(),
        }
    }
}

#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("Image {path:?} is too large ({width}x{height})")]
#[help("Images cannot be larger than 2048x2048")]
pub struct ImageTooLarge {
    pub path: PathBuf,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("Ignoring poster {extra:?} and using {found:?} instead")]
#[help("Events should only have one poster")]
#[diagnostic(severity("warning"))]
pub struct MultiplePosters {
    pub found: PathBuf,
    pub extra: PathBuf,
}

#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("The event is confirmed for {date}, but the event is not happening on this day.")]
#[diagnostic(severity("warning"))]
pub struct ConfirmedOutOfRange {
    pub date: NaiveDate,
    #[source_code]
    pub src: NamedSource,
}

#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("The event is canceled for {date}, but the event is not happening on this day.")]
#[diagnostic(severity("warning"))]
pub struct CanceledOutOfRange {
    pub date: NaiveDate,
    #[source_code]
    pub src: NamedSource,
}
