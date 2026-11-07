//! Proleptic Gregorian day arithmetic (ECMA-262 §21.4.1.3–9) over day
//! numbers since 1970-01-01, exact for every `i64` the time value range
//! reaches.
const DAYS_PER_ERA: i64 = 146_097;
const YEARS_PER_ERA: i64 = 400;
/// Days from 0000-03-01 to 1970-01-01.
const EPOCH_SHIFT: i64 = 719_468;
const DAYS_PER_WEEK: i64 = 7;
/// 1970-01-01 was a Thursday (weekday 4, Sunday = 0).
const EPOCH_WEEKDAY: i64 = 4;

/// The day number of `year`-`month`-`day` (`month` 1–12; `day` may run past
/// the month's end).
#[must_use]
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(YEARS_PER_ERA);
    let year_of_era = y - era * YEARS_PER_ERA;
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * DAYS_PER_ERA + day_of_era - EPOCH_SHIFT
}

/// `(year, month 1–12, day 1–31)` of a day number.
#[must_use]
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + EPOCH_SHIFT;
    let era = z.div_euclid(DAYS_PER_ERA);
    let day_of_era = z - era * DAYS_PER_ERA;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524
        - day_of_era / (DAYS_PER_ERA - 1))
        / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era + era * YEARS_PER_ERA + i64::from(month <= 2);
    (year, month, day)
}

/// The year containing a day number.
#[must_use]
pub fn year_of_day(days: i64) -> i64 {
    civil_from_days(days).0
}

/// WeekDay (§21.4.1.10) of a day number: 0 = Sunday.
#[must_use]
pub fn weekday(days: i64) -> i64 {
    (days + EPOCH_WEEKDAY).rem_euclid(DAYS_PER_WEEK)
}
