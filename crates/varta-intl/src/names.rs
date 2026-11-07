//! The time zone name of `Date.prototype.toString` and `toTimeString`
//! (ECMA-262 §21.4.4.41.3 TimeZoneString leaves it implementation-defined).
//!
//! Varta names the zone as Node 24 does (V8 over ICU): the CLDR long
//! specific name (ICU `TimeZone::getDisplayName(daylight, LONG, locale)`)
//! in the default locale, chosen by
//!
//! - the zone's own name for that variant (`Europe/London` daylight is
//!   "British Summer Time"), else the name of the metazone the zone belongs
//!   to *now* (ICU looks the metazone up at the current date; Varta at the
//!   fixed [`REFERENCE_SECONDS`], 2026-07-01, so the name never depends on
//!   the clock),
//!   else the localized GMT format of the zone's current standard offset, or
//!   its daylight offset when the variant is daylight and the zone observes
//!   daylight saving time now;
//! - the daylight flag of the instant, read from an *equivalent* instant for
//!   time values outside 1970-01-01 to 2038-01-19 as V8's
//!   `DateCache::LocalTimezone` does (`EquivalentTime`: the same month and
//!   day in a year of 2008–2035 that starts on the same weekday and has the
//!   same length), with ICU's (rearguard) daylight flags.
//!
//! The offset in the `GMT±hhmm` part is never affected: it is the real offset
//! of the instant.
use crate::calendar::{civil_from_days, days_from_civil, weekday};
use crate::zone::Zone;
use alloc::string::String;
use icu_datetime::NoCalendarFormatter;
use icu_datetime::fieldsets::zone::{LocalizedOffsetLong, SpecificLong};
use icu_locale_core::Locale;
use icu_provider::prelude::*;
use icu_time::provider::TimezonePeriodsV1;
use icu_time::zone::{IanaParser, UtcOffset, ZoneNameTimestamp};
use writeable::Writeable;

const MS_PER_DAY: i64 = 86_400_000;
const MS_PER_SECOND: i64 = 1000;
/// V8's `DateCache::kMaxEpochTimeInMs`: the last instant (2³¹ − 1 seconds)
/// it asks the platform about directly.
const MAX_EPOCH_MS: i64 = (i32::MAX as i64) * MS_PER_SECOND;
/// The instant whose metazones and offsets stand for "now": 2026-07-01
/// 00:00 UTC, within the bundled data releases (tzdata 2026e, CLDR 49).
pub const REFERENCE_SECONDS: i64 = 1_782_864_000;

/// The locale of a BCP 47 tag, or `und` when the tag does not parse.
#[must_use]
pub fn locale(tag: &str) -> Locale {
    tag.parse().unwrap_or(Locale::UNKNOWN)
}

/// V8's `DateCache::EquivalentYear`: a year of 2008–2035 that starts on the
/// same weekday and has the same length as `year`.
fn equivalent_year(year: i64) -> i64 {
    let leap = days_from_civil(year + 1, 1, 1) - days_from_civil(year, 1, 1) == 366;
    let week_day = weekday(days_from_civil(year, 1, 1));
    let recent = (if leap { 1956 } else { 1967 }) + (week_day * 12) % 28;
    2008 + (recent + 3 * 28 - 2008) % 28
}

/// V8's `DateCache::EquivalentTime` for instants outside `[0,
/// kMaxEpochTimeInMs]`; others are returned unchanged.
#[must_use]
pub fn equivalent_time(time: i64) -> i64 {
    if (0..=MAX_EPOCH_MS).contains(&time) {
        return time;
    }
    let days = time.div_euclid(MS_PER_DAY);
    let within = time - days * MS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    days_from_civil(equivalent_year(year), month, day) * MS_PER_DAY + within
}

/// The locale data markers [`long_name`] loads (the linker links their
/// segments with `date_zone_name`).
pub const MARKERS: &[DataMarkerInfo] = &[
    icu_time::provider::iana::TimezoneIdentifiersIanaCoreV1::INFO,
    TimezonePeriodsV1::INFO,
    icu_datetime::provider::time_zones::TimezoneNamesEssentialsV1::INFO,
    icu_datetime::provider::time_zones::TimezoneNamesLocationsOverrideV1::INFO,
    icu_datetime::provider::time_zones::TimezoneNamesLocationsRootV1::INFO,
    icu_datetime::provider::time_zones::TimezoneNamesSpecificLongV1::INFO,
    icu_datetime::provider::time_zones::TimezoneNamesStandardLongV1::INFO,
    icu_decimal::provider::DecimalSymbolsV1::INFO,
    icu_decimal::provider::DecimalDigitsV1::INFO,
];

/// The long time zone name `Date.prototype.toString` shows for `zone` at
/// the instant `time` in `locale`, from the locale data of `provider`.
#[must_use]
pub fn long_name<P: BufferProvider + ?Sized>(
    provider: &P,
    locale: &Locale,
    zone: &Zone,
    time: i64,
) -> String {
    let daylight = zone.daylight(equivalent_time(time));
    let (standard, current_daylight) = zone.current_offsets();
    // ICU's fallback: the localized GMT format of the current standard
    // offset, or of the daylight offset for a daylight name of a zone that
    // observes daylight saving time now.
    let fallback = match current_daylight {
        Some(offset) if daylight => offset,
        _ => standard,
    };
    let deserializing = provider.as_deserializing();
    let Ok(parser) = IanaParser::try_new_unstable(&deserializing) else {
        return gmt(provider, locale, fallback);
    };
    let id = parser.as_borrowed().parse(zone.name());
    let now = ZoneNameTimestamp::from_epoch_seconds(REFERENCE_SECONDS);
    let periods: Option<DataResponse<TimezonePeriodsV1>> =
        deserializing.load(Default::default()).ok();
    let offsets = periods.and_then(|periods| periods.payload.get().get(id, now).map(|(o, _)| o));
    let Some(offsets) = offsets else {
        return gmt(provider, locale, fallback);
    };
    // The metazone's offset of the variant, which makes the formatter pick
    // that variant's name.
    let offset = if daylight {
        offsets.daylight
    } else {
        Some(offsets.standard)
    };
    let Some(offset) = offset else {
        return gmt(provider, locale, fallback);
    };
    let info = id.with_offset(Some(offset)).with_zone_name_timestamp(now);
    match NoCalendarFormatter::try_new_with_buffer_provider(provider, locale.into(), SpecificLong) {
        Ok(formatter) => formatter.format(&info).write_to_string().into_owned(),
        Err(_) => gmt(provider, locale, fallback),
    }
}

/// The localized GMT format (`GMT-05:00`) of an offset in milliseconds.
fn gmt<P: BufferProvider + ?Sized>(provider: &P, locale: &Locale, offset: i64) -> String {
    let seconds = i32::try_from(offset / MS_PER_SECOND).unwrap_or(0);
    let offset = UtcOffset::try_from_seconds(seconds).unwrap_or(UtcOffset::zero());
    match NoCalendarFormatter::try_new_with_buffer_provider(
        provider,
        locale.into(),
        LocalizedOffsetLong,
    ) {
        Ok(formatter) => formatter.format(&offset).write_to_string().into_owned(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests;
