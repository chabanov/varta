//! Offsets against Node 24.20.0 (ICU 78.3, tzdata 2026c): `node_offsets.txt`
//! holds, per zone, the offsets Node's `Date` applies at a fixed sequence of
//! instants (`LocalTime`, §21.4.1.25) and of local times (`UTC`, §21.4.1.26:
//! the earliest instant of a repeated time, the offset before a skipped
//! one), run-length encoded. `target/icusize/zone-oracle.mjs` printed them
//! with `TZ` set to each zone; the sample sequences below repeat its loops.
extern crate std;
use super::*;
use crate::calendar::days_from_civil;
use std::vec::Vec;

const FIXTURE: &str = include_str!("node_offsets.txt");
const YEARS: [i64; 29] = [
    -500, 1700, 1850, 1883, 1900, 1916, 1918, 1930, 1945, 1950, 1970, 1980, 1995, 2000, 2007, 2011,
    2014, 2020, 2024, 2025, 2026, 2030, 2037, 2038, 2050, 2100, 2400, 9999, 100000,
];
const LOCAL_YEARS: [i64; 9] = [1883, 1918, 1970, 2011, 2024, 2026, 2037, 2050, 2100];
const MS_PER_DAY: i64 = 86_400_000;
const MS_PER_HOUR: i64 = 3_600_000;
const MS_PER_MINUTE: i64 = 60_000;

/// `Date.UTC(year, month, day, hours, minutes, seconds, ms)` with a
/// zero-based month.
fn utc(year: i64, month: i64, day: i64, hours: i64, minutes: i64, seconds: i64, ms: i64) -> i64 {
    days_from_civil(year, month + 1, day) * MS_PER_DAY
        + hours * MS_PER_HOUR
        + minutes * MS_PER_MINUTE
        + seconds * 1000
        + ms
}

fn instants() -> Vec<i64> {
    let mut out = Vec::new();
    for year in YEARS {
        for month in [0, 2, 3, 5, 9, 10, 11] {
            for day in [1, 8, 15, 25] {
                for hours in [0, 1, 2, 3, 12, 23] {
                    out.push(utc(year, month, day, hours, 30, 7, 250));
                }
            }
        }
    }
    out
}

fn locals() -> Vec<i64> {
    let mut out = Vec::new();
    for year in LOCAL_YEARS {
        for month in [0, 2, 3, 8, 9, 10, 11] {
            for day in (1..=31).step_by(2) {
                for hours in [0, 1, 2, 3, 4, 23] {
                    for minutes in [0, 30, 45] {
                        out.push(utc(year, month, day, hours, minutes, 0, 0));
                    }
                }
            }
        }
    }
    out
}

fn expand(runs: &str) -> Vec<i64> {
    let mut out = Vec::new();
    for run in runs.split(',') {
        let (value, count) = run.split_once('*').unwrap_or((run, "1"));
        let value: i64 = value.parse().unwrap();
        for _ in 0..count.parse::<usize>().unwrap() {
            out.push(value);
        }
    }
    out
}

#[test]
fn offsets_match_node_at_instants_and_local_times() {
    let instants = instants();
    let locals = locals();
    let mut zones = 0;
    for line in FIXTURE.lines() {
        let mut parts = line.split('|');
        let (name, at, local) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        let zone = Zone::find(name).unwrap_or_else(|| panic!("{name} missing"));
        let expected = expand(at);
        assert_eq!(expected.len(), instants.len(), "{name}");
        for (&time, &offset) in instants.iter().zip(&expected) {
            assert_eq!(zone.offset_at(time).ms, offset, "{name} at instant {time}");
        }
        let expected = expand(local);
        assert_eq!(expected.len(), locals.len(), "{name}");
        for (&time, &offset) in locals.iter().zip(&expected) {
            assert_eq!(
                zone.offset_of_local(time).ms,
                offset,
                "{name} at local {time}"
            );
        }
        zones += 1;
    }
    assert_eq!(zones, 30);
}

#[test]
fn identifiers_match_ignoring_case_and_keep_the_database_spelling() {
    assert_eq!(
        Zone::find("america/new_york").map(|z| z.name()),
        Some("America/New_York")
    );
    assert_eq!(Zone::find("UTC").map(|z| z.name()), Some("UTC"));
    assert!(Zone::find("America/NewYork").is_none());
    assert!(Zone::find("").is_none());
    assert!(Zone::find("+01:00").is_none());
}

#[test]
fn daylight_flags_follow_the_tzif_types() {
    let zone = Zone::find("America/New_York").unwrap();
    let summer = utc(2024, 6, 1, 12, 0, 0, 0);
    let winter = utc(2024, 0, 1, 12, 0, 0, 0);
    assert!(zone.offset_at(summer).dst);
    assert!(!zone.offset_at(winter).dst);
    // Far beyond the explicit transitions, the footer rule decides.
    let future = utc(3000, 6, 1, 12, 0, 0, 0);
    assert_eq!(
        zone.offset_at(future),
        Offset {
            ms: -4 * MS_PER_HOUR,
            dst: true
        }
    );
}

#[test]
fn posix_rules_parse_the_rfc_8536_forms() {
    use crate::posix::Rule;
    for text in [
        "EST5EDT,M3.2.0,M11.1.0",
        "<+0330>-3:30",
        "IST-1GMT0,M10.5.0,M3.5.0/1",
        "<-03>3<-02>,M3.5.0/-2,M10.5.0/-1",
        "AEST-10AEDT,M10.1.0,M4.1.0/3",
        "EST5EDT,0/0,J365/25",
        "XXX3EDT4,J60/2,300",
        "EST5EDT",
    ] {
        assert!(Rule::parse(text).is_some(), "{text}");
    }
    for text in [
        "",
        "E5",
        "EST",
        "EST5EDT,M13.1.0,M1.1.0",
        "EST5EDT,M3.2.0",
        "EST168",
    ] {
        assert!(Rule::parse(text).is_none(), "{text}");
    }
    // All-year daylight saving time (tzcode's convention).
    let rule = Rule::parse("EST5EDT,0/0,J365/25").unwrap();
    assert!(rule.offset_at(utc(2024, 0, 1, 0, 0, 0, 0)).dst);
    assert!(rule.offset_at(utc(2024, 11, 31, 23, 0, 0, 0)).dst);
}

#[test]
fn every_bundled_zone_parses() {
    let mut count = 0;
    for name in jiff_tzdb::available() {
        assert!(Zone::find(name).is_some(), "{name}");
        count += 1;
    }
    assert!(count > 500, "{count}");
}

/// Every zone Node 24.20.0 lists (`Intl.supportedValuesOf('timeZone')`) and
/// common legacy names, at noon UTC on January 15 and July 15 of every
/// second year from 1800 to 2026 (`node_zones.txt`, run-length encoded like
/// `node_offsets.txt`). Later years are left out because the bundled tzdata
/// release is newer than Node's 2026c (Manitoba and Inuvik differ from
/// 2027); `US/Pacific-New` and `Canada/East-Saskatchewan`, which IANA
/// removed (2020b, 2017c) and ICU still aliases, are not available here.
#[test]
fn every_zone_node_lists_has_its_offsets() {
    let mut instants = Vec::new();
    for year in (1800..=2026).step_by(2) {
        for month in [0, 6] {
            instants.push(utc(year, month, 15, 12, 0, 0, 0));
        }
    }
    let mut zones = 0;
    for line in include_str!("node_zones.txt").lines() {
        let (name, runs) = line.split_once('|').unwrap();
        let zone = Zone::find(name).unwrap_or_else(|| panic!("{name} missing"));
        let expected = expand(runs);
        assert_eq!(expected.len(), instants.len(), "{name}");
        for (&time, &offset) in instants.iter().zip(&expected) {
            assert_eq!(zone.offset_at(time).ms, offset, "{name} at {time}");
        }
        zones += 1;
    }
    assert_eq!(zones, 436);
}
