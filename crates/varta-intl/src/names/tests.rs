//! Zone names against Node 24.20.0 (ICU 78.3, CLDR 48): `node_names.txt`
//! holds `zone|locale|name...`, the parenthesized name of
//! `new Date(t).toString()` with `TZ` set to the zone and `LANG` to the
//! locale (POSIX form), at the instants of [`INSTANTS`].
extern crate std;
use super::*;
use std::vec::Vec;

const FIXTURE: &str = include_str!("node_names.txt");

fn utc(year: i64, month: i64, day: i64) -> i64 {
    days_from_civil(year, month + 1, day) * MS_PER_DAY
}

fn instants() -> [i64; 9] {
    [
        utc(1800, 0, 1),
        utc(1943, 6, 1),
        utc(1999, 0, 15),
        utc(2000, 6, 15),
        utc(2024, 0, 15),
        utc(2024, 6, 15),
        utc(2024, 2, 20),
        utc(2100, 6, 1),
        -6_000_000_000_000_000,
    ]
}

/// Rows whose names differ from Node's, by cause (owner decision pending):
/// the bundled CLDR 49 renamed or newly translated them (Node 24.20 ships
/// CLDR 48: `Pacific/Honolulu`, `Pacific/Apia`, Greenland outside English,
/// the Ukrainian North American names, Spanish (419), Japanese mountain
/// time, localized GMT formats in French, Arabic and Portuguese); ICU4X's
/// compiled data keeps no daylight names for metazones that no longer
/// observe daylight saving time (Moscow and Türkiye summer time in 2000);
/// ICU4X models Morocco's metazone membership differently from ICU4C.
const DIFFERENT: &[(&str, &str)] = &[
    ("US/Eastern", "uk-UA"),
    ("America/New_York", "uk-UA"),
    ("America/Los_Angeles", "uk-UA"),
    ("America/Phoenix", "uk-UA"),
    ("America/Phoenix", "ja-JP"),
    ("Pacific/Apia", "en-US"),
    ("Pacific/Apia", "fr-FR"),
    ("Pacific/Apia", "ja-JP"),
    ("Pacific/Apia", "ar-EG"),
    ("Europe/Istanbul", "es-419"),
    ("Europe/Istanbul", "ar-EG"),
    ("Etc/GMT+5", "pt-BR"),
    ("Etc/GMT+5", "fr-FR"),
    ("Etc/GMT+5", "ar-EG"),
    ("Asia/Kolkata", "es-419"),
    ("Asia/Calcutta", "es-419"),
    ("Antarctica/Troll", "pt-BR"),
    ("Antarctica/Troll", "ar-EG"),
    ("Africa/Windhoek", "pt-BR"),
    ("Africa/Windhoek", "ar-EG"),
];
/// Zones that differ in every listed locale but English (`Greenland`) or in
/// all of them.
const DIFFERENT_ZONES: &[(&str, bool)] = &[
    ("Pacific/Honolulu", true),
    ("Africa/Casablanca", true),
    ("America/Godthab", false),
];
/// The one instant (2000-07-15, summer time) whose name differs everywhere
/// for these zones.
const DIFFERENT_SUMMERS: &[&str] = &["Europe/Moscow", "Europe/Istanbul"];
/// Rows that differ from Node, pinned so a data change is noticed.
const MISMATCHES: usize = 404;

fn expected_difference(name: &str, tag: &str, time: i64) -> bool {
    DIFFERENT.contains(&(name, tag))
        || DIFFERENT_ZONES
            .iter()
            .any(|&(zone, english)| zone == name && (english || tag != "en-US"))
        || (DIFFERENT_SUMMERS.contains(&name) && time == utc(2000, 6, 15))
}

#[test]
fn names_match_node() {
    let instants = instants();
    let mut unexpected = Vec::new();
    let mut mismatches = 0;
    let mut rows = 0;
    for line in FIXTURE.lines() {
        let mut parts = line.split('|');
        let name = parts.next().unwrap();
        let tag = parts.next().unwrap();
        let zone = Zone::find(name).unwrap();
        let locale = locale(tag);
        for (&time, expected) in instants.iter().zip(parts) {
            let got = long_name(&locale, &zone, time);
            if got != expected {
                mismatches += 1;
                if !expected_difference(name, tag, time) {
                    unexpected.push(std::format!("{name} {tag} {time}: {got:?} != {expected:?}"));
                }
            }
            rows += 1;
        }
    }
    assert_eq!(rows, 261 * 9);
    assert!(
        unexpected.is_empty(),
        "{}:\n{}",
        unexpected.len(),
        unexpected.join("\n")
    );
    assert_eq!(mismatches, MISMATCHES);
}

#[test]
fn equivalent_years_follow_v8() {
    // 1943 starts on a Friday and is not a leap year: V8 maps it to 2027.
    assert_eq!(equivalent_year(1943), 2027);
    assert_eq!(equivalent_year(1800), 2031);
    assert_eq!(equivalent_time(0), 0);
    assert_eq!(equivalent_time(MAX_EPOCH_MS), MAX_EPOCH_MS);
    let july_1943 = utc(1943, 6, 1) + 3_600_000;
    assert_eq!(equivalent_time(july_1943), utc(2027, 6, 1) + 3_600_000);
}
