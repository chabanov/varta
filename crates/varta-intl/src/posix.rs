//! POSIX TZ strings (POSIX.1-2024 §8.3, with the RFC 8536 §3.3.1
//! extensions: hours up to 167 and negative transition times), the rule a
//! TZif footer states for instants after the last explicit transition.
use crate::calendar::{days_from_civil, weekday, year_of_day};
use crate::zone::Offset;

const MS_PER_SECOND: i64 = 1000;
const MS_PER_DAY: i64 = 86_400_000;
const SECONDS_PER_HOUR: i64 = 3600;
const SECONDS_PER_MINUTE: i64 = 60;
/// The default transition time of a rule without one: 02:00:00.
const DEFAULT_TIME: i64 = 2 * SECONDS_PER_HOUR;
/// The largest hour a TZ string offset may state (RFC 8536 allows 167 in
/// transition times; offsets stay within 24 by POSIX, 167 accepted here as
/// tzcode does).
const MAX_HOURS: i64 = 167;
const DAYS_PER_WEEK: i64 = 7;
/// Day-of-year bounds of the `Jn` (1–365) and `n` (0–365) forms.
const LAST_JULIAN_DAY: i64 = 365;
const MONTHS: i64 = 12;
const LAST_WEEK: i64 = 5;
const FEBRUARY: i64 = 2;
const MARCH: i64 = 3;

/// When a transition happens in a year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Day {
    /// `Jn`: day 1–365, February 29 never counted.
    Julian(i64),
    /// `n`: day 0–365, counting February 29 in leap years.
    Ordinal(i64),
    /// `Mm.w.d`: weekday `d` (0 = Sunday) of week `w` (5 = the last) of
    /// month `m`.
    Month { month: i64, week: i64, weekday: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Transition {
    day: Day,
    /// Seconds after local midnight (may be negative or exceed a day).
    time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Daylight {
    /// UTC offset in milliseconds while daylight saving time applies.
    offset: i64,
    start: Transition,
    end: Transition,
}

/// A parsed TZ string: a standard offset and optional daylight rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// UTC offset of standard time in milliseconds (east positive).
    standard: i64,
    daylight: Option<Daylight>,
}

impl Rule {
    /// Parse a TZ string; `None` when it is not a valid rule.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut scan = Scan {
            text: text.as_bytes(),
            at: 0,
        };
        scan.name()?;
        let standard = -scan.offset()?;
        if scan.done() {
            return Some(Self {
                standard,
                daylight: None,
            });
        }
        scan.name()?;
        let offset = if scan.peek().is_some_and(|byte| byte != b',') {
            -scan.offset()?
        } else {
            standard + SECONDS_PER_HOUR * MS_PER_SECOND
        };
        // POSIX leaves a rule without dates to the implementation; tzcode
        // uses the United States rules since 2007.
        let (start, end) = if scan.done() {
            (
                Transition {
                    day: Day::Month {
                        month: MARCH,
                        week: 2,
                        weekday: 0,
                    },
                    time: DEFAULT_TIME,
                },
                Transition {
                    day: Day::Month {
                        month: 11,
                        week: 1,
                        weekday: 0,
                    },
                    time: DEFAULT_TIME,
                },
            )
        } else {
            scan.expect(b',')?;
            let start = scan.transition()?;
            scan.expect(b',')?;
            let end = scan.transition()?;
            (start, end)
        };
        if !scan.done() {
            return None;
        }
        Some(Self {
            standard,
            daylight: Some(Daylight { offset, start, end }),
        })
    }

    /// The offset of the rule at the instant `time` (milliseconds).
    #[must_use]
    pub fn offset_at(&self, time: i64) -> Offset {
        let standard = Offset {
            ms: self.standard,
            dst: false,
        };
        let Some(daylight) = self.daylight else {
            return standard;
        };
        let year = year_of_day((time + self.standard).div_euclid(MS_PER_DAY));
        let (start, end) = self.bounds(&daylight, year);
        let inside = if start < end {
            start <= time && time < end
        } else {
            !(end <= time && time < start)
        };
        if inside {
            Offset {
                ms: daylight.offset,
                dst: true,
            }
        } else {
            standard
        }
    }

    /// The instants daylight saving time starts and ends in `year`.
    fn bounds(&self, daylight: &Daylight, year: i64) -> (i64, i64) {
        let start = local_instant(year, daylight.start) - self.standard;
        let end = local_instant(year, daylight.end) - daylight.offset;
        (start, end)
    }

    /// The first transition of the rule strictly after `time`.
    #[must_use]
    pub fn next_transition(&self, time: i64) -> Option<i64> {
        self.transitions_near(time).filter(|&at| at > time).min()
    }

    /// The transitions of the years around `time`.
    fn transitions_near(&self, time: i64) -> impl Iterator<Item = i64> {
        let year = year_of_day((time + self.standard).div_euclid(MS_PER_DAY));
        let daylight = self.daylight;
        (year - 1..=year + 1)
            .flat_map(move |year| {
                daylight.map_or([None, None], |daylight| {
                    let (start, end) = self.bounds(&daylight, year);
                    [Some(start), Some(end)]
                })
            })
            .flatten()
    }

    /// The standard offset and, when the rule has daylight saving time, the
    /// daylight offset, in milliseconds.
    #[must_use]
    pub fn offsets(&self) -> (i64, Option<i64>) {
        (self.standard, self.daylight.map(|daylight| daylight.offset))
    }
}

/// Milliseconds of the local (wall clock) instant of `transition` in `year`,
/// as a time value of local time.
fn local_instant(year: i64, transition: Transition) -> i64 {
    let day = match transition.day {
        Day::Julian(day) => {
            // February 29 is never counted: days after February shift by one
            // in leap years.
            let leap = days_from_civil(year, MARCH, 1) - days_from_civil(year, FEBRUARY, 1) == 29;
            let first = days_from_civil(year, 1, 1);
            first + day - 1 + i64::from(leap && day >= 60)
        }
        Day::Ordinal(day) => days_from_civil(year, 1, 1) + day,
        Day::Month {
            month,
            week,
            weekday: target,
        } => {
            let first = days_from_civil(year, month, 1);
            let next = if month == MONTHS {
                days_from_civil(year + 1, 1, 1)
            } else {
                days_from_civil(year, month + 1, 1)
            };
            let mut day = first
                + (target - weekday(first)).rem_euclid(DAYS_PER_WEEK)
                + (week - 1) * DAYS_PER_WEEK;
            while day >= next {
                day -= DAYS_PER_WEEK;
            }
            day
        }
    };
    day * MS_PER_DAY + transition.time * MS_PER_SECOND
}

struct Scan<'a> {
    text: &'a [u8],
    at: usize,
}

impl Scan<'_> {
    fn done(&self) -> bool {
        self.at >= self.text.len()
    }
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }
    fn expect(&mut self, byte: u8) -> Option<()> {
        (self.peek()? == byte).then(|| self.at += 1)
    }
    /// A zone abbreviation: three or more letters, or `<...>` quoting
    /// letters, digits and signs.
    fn name(&mut self) -> Option<()> {
        let start = self.at;
        if self.peek()? == b'<' {
            self.at += 1;
            while self.peek()?.is_ascii_alphanumeric() || matches!(self.peek()?, b'+' | b'-') {
                self.at += 1;
            }
            self.expect(b'>')?;
            return (self.at - start >= 5).then_some(());
        }
        while self.peek().is_some_and(|byte| byte.is_ascii_alphabetic()) {
            self.at += 1;
        }
        (self.at - start >= 3).then_some(())
    }
    fn number(&mut self) -> Option<i64> {
        let start = self.at;
        let mut value: i64 = 0;
        while let Some(byte) = self.peek().filter(u8::is_ascii_digit) {
            value = value.checked_mul(10)?.checked_add(i64::from(byte - b'0'))?;
            self.at += 1;
        }
        (self.at > start).then_some(value)
    }
    /// `[+|-]hh[:mm[:ss]]` in milliseconds (west of Greenwich positive, as
    /// written).
    fn offset(&mut self) -> Option<i64> {
        Some(self.signed_time()? * MS_PER_SECOND)
    }
    fn signed_time(&mut self) -> Option<i64> {
        let sign = match self.peek() {
            Some(b'-') => {
                self.at += 1;
                -1
            }
            Some(b'+') => {
                self.at += 1;
                1
            }
            _ => 1,
        };
        let hours = self.number()?;
        if hours > MAX_HOURS {
            return None;
        }
        let mut seconds = hours * SECONDS_PER_HOUR;
        for scale in [SECONDS_PER_MINUTE, 1] {
            if self.peek() != Some(b':') {
                break;
            }
            self.at += 1;
            let part = self.number()?;
            if part >= SECONDS_PER_MINUTE {
                return None;
            }
            seconds += part * scale;
        }
        Some(sign * seconds)
    }
    fn transition(&mut self) -> Option<Transition> {
        let day = match self.peek()? {
            b'J' => {
                self.at += 1;
                let day = self.number()?;
                (1..=LAST_JULIAN_DAY)
                    .contains(&day)
                    .then_some(Day::Julian(day))?
            }
            b'M' => {
                self.at += 1;
                let month = self.number()?;
                self.expect(b'.')?;
                let week = self.number()?;
                self.expect(b'.')?;
                let weekday = self.number()?;
                let valid = (1..=MONTHS).contains(&month)
                    && (1..=LAST_WEEK).contains(&week)
                    && weekday < DAYS_PER_WEEK;
                valid.then_some(Day::Month {
                    month,
                    week,
                    weekday,
                })?
            }
            _ => {
                let day = self.number()?;
                (day <= LAST_JULIAN_DAY).then_some(Day::Ordinal(day))?
            }
        };
        let time = if self.peek() == Some(b'/') {
            self.at += 1;
            self.signed_time()?
        } else {
            DEFAULT_TIME
        };
        Some(Transition { day, time })
    }
}
