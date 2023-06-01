use std::collections::BTreeMap;

use chrono::{DateTime, Days, Utc};
use parse_zoneinfo::{
    line::{Line, LineParser},
    table::TableBuilder,
    transitions::TableTransitions,
};

use crate::output::{Rule, Zone};

struct TzFile {
    name: &'static str,
    content: &'static str,
}

macro_rules! include_tz {
    ($name:literal) => {
        TzFile {
            name: $name,
            content: include_str!(concat!("../tz/", $name)),
        }
    };
}

const FILES: &[TzFile] = &[
    include_tz!("africa"),
    include_tz!("antarctica"),
    include_tz!("asia"),
    include_tz!("australasia"),
    include_tz!("etcetera"),
    include_tz!("europe"),
    include_tz!("northamerica"),
    include_tz!("southamerica"),
];

pub fn collect_zones(now: DateTime<Utc>) -> BTreeMap<String, Zone> {
    let parser = LineParser::new();
    let mut table = TableBuilder::new();

    let now_ts = now.timestamp();
    let limit = now + Days::new(365 * 5);
    let limit_ts = limit.timestamp();

    for file in FILES {
        for (line_index, line) in file.content.lines().enumerate() {
            let line = if let Some(index) = line.find('#') {
                &line[..index]
            } else {
                line
            };
            let line = match parser.parse_str(line) {
                Ok(line) => line,
                Err(error) => {
                    panic!(
                        "Syntax error at {}:{}: {:?}",
                        file.name,
                        line_index + 1,
                        error,
                    );
                }
            };
            let result = match line {
                Line::Space => Ok(()),
                Line::Zone(zone) => table.add_zone_line(zone),
                Line::Continuation(continuation) => table.add_continuation_line(continuation),
                Line::Rule(rule) => table.add_rule_line(rule),
                Line::Link(link) => table.add_link_line(link),
            };
            if let Err(error) = result {
                panic!("Error at {}:{}: {}", file.name, line_index + 1, error);
            }
        }
    }

    let table = table.build();
    let mut zones = BTreeMap::new();

    for zone_name in table.zonesets.keys() {
        let Some(timespans) = table.timespans(zone_name) else {
            continue;
        };
        let mut spans: Vec<_> = [(i64::MIN, &timespans.first)]
            .into_iter()
            .chain(timespans.rest.iter().map(|(start, span)| (*start, span)))
            .collect();
        let current = spans
            .binary_search_by_key(&now_ts, |(start, _)| *start)
            .unwrap_or_else(|i| i.saturating_sub(1));
        spans.drain(0..current);
        spans.retain(|(start, _)| *start < limit_ts);

        zones.insert(
            zone_name.clone(),
            Zone {
                offsets: spans
                    .into_iter()
                    .map(|(start, span)| Rule {
                        start: Some(start).filter(|&s| s > now_ts),
                        offset: Some(span.total_offset() / 60)
                            .filter(|&o| o != 0)
                            .and_then(|o| i16::try_from(o).ok()),
                    })
                    .collect::<Vec<_>>(),
            },
        );
    }

    zones
}
