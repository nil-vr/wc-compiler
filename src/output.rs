use std::{borrow::Cow, collections::BTreeMap};

use chrono::NaiveDate;
use serde::Serialize;

use crate::{Language, Platform, User, World};

#[derive(Serialize)]
pub struct Data<'a> {
    pub meta: &'a Meta<'a>,
    pub events: &'a [Event<'a>],
    pub zones: &'a BTreeMap<String, Zone>,
}

#[derive(Serialize)]
pub struct Event<'a> {
    pub name: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<i64>,
    #[serde(flatten)]
    pub info: EventInfo<'a>,
    #[serde(rename = "tz")]
    pub timezone: &'a str,
    pub start: i32,
    pub duration: i32,
    pub platforms: &'a [Platform],
    #[serde(flatten)]
    pub days: EventDays<'a>,
    #[serde(rename = "lang", skip_serializing_if = "BTreeMap::is_empty")]
    pub languages: BTreeMap<Language, EventLanguage<'a>>,
    #[serde(skip_serializing_if = "DateSet::is_none")]
    pub canceled: DateSet,
    #[serde(skip_serializing_if = "DateSet::is_all")]
    pub confirmed: DateSet,
}

#[derive(Serialize)]
pub struct EventDays<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monday: Option<EventDay<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tuesday: Option<EventDay<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wednesday: Option<EventDay<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thursday: Option<EventDay<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friday: Option<EventDay<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saturday: Option<EventDay<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunday: Option<EventDay<'a>>,
}

#[derive(Serialize)]
pub struct EventDay<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i32>,
    #[serde(flatten)]
    pub info: EventInfo<'a>,
}

#[derive(Serialize)]
pub struct EventLanguage<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
    #[serde(flatten)]
    pub info: EventInfo<'a>,
    #[serde(flatten)]
    pub days: EventDays<'a>,
}

#[derive(Clone, Copy, Serialize)]
pub struct PosterInfo {
    #[serde(rename = "n")]
    pub number: u8,
    #[serde(rename = "w")]
    pub width: u16,
    #[serde(rename = "h")]
    pub height: u16,
}

#[derive(Serialize)]
pub struct EventInfo<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<PosterInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashtag: Option<Hashtag<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<&'a str>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub join: &'a [User<'a>],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world: Option<&'a World<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weeks: Option<&'a [u8]>,
    #[serde(rename = "desc", skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
}

#[derive(Serialize)]
pub struct Zone {
    #[serde(rename = "r")]
    pub offsets: Vec<Rule>,
}

#[derive(Serialize)]
pub struct Rule {
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(rename = "o", skip_serializing_if = "Option::is_none")]
    pub offset: Option<i16>,
}

#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum DateSet {
    All(bool),
    Dates(Vec<NaiveDate>),
}

impl DateSet {
    pub fn is_none(&self) -> bool {
        matches!(self, DateSet::All(false))
    }

    pub fn is_all(&self) -> bool {
        matches!(self, DateSet::All(true))
    }
}

#[derive(Serialize)]
pub struct Meta<'a> {
    pub title: &'a str,
    #[serde(rename = "desc", skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<&'a str>,
    #[serde(rename = "ts")]
    pub compiled_time: i64,
    #[serde(rename = "lang", skip_serializing_if = "BTreeMap::is_empty")]
    pub languages: BTreeMap<Language, MetaLanguage<'a>>,
}

#[derive(Serialize)]
pub struct MetaLanguage<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<&'a str>,
    #[serde(rename = "desc", skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Hashtag<'a> {
    Safe(&'a str),
    Escaped { display: &'a str, escaped: String },
}
