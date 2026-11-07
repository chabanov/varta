//! Named time zones (ECMA-262 §21.4.1.20–26): UTC offsets of the IANA Time
//! Zone Database, read from its TZif encoding (RFC 8536, versions 2–4) and
//! its POSIX TZ footer for instants after the last explicit transition.
//!
//! Every function works on time values (milliseconds since the epoch, as
//! `i64` within the ECMAScript range) and offsets in milliseconds. The
//! database is `jiff_tzdb`'s copy (`DATA_VERSION`), the same on every host.
use crate::posix::Rule;

/// The bundled IANA Time Zone Database release.
pub const DATA_VERSION: &str = match jiff_tzdb::VERSION {
    Some(version) => version,
    None => "unknown",
};
const MS_PER_SECOND: i64 = 1000;
/// Half the width of the window around a local time in which every offset
/// that could map an instant onto it lies: TZif offsets stay within ±25:59:59
/// (RFC 8536 §3.2), so a window of 26 hours each way holds every candidate.
const WINDOW_MS: i64 = 26 * 3600 * MS_PER_SECOND;
const MAGIC: &[u8; 4] = b"TZif";
const HEADER_BYTES: usize = 44;
const COUNTS_AT: usize = 20;
/// Bytes of one local time type record: `utoff` (4), `isdst` (1), `desigidx` (1).
const TYPE_BYTES: usize = 6;
/// The version 1 data block's transition time and leap record sizes.
const V1_TIME_BYTES: usize = 4;
const V1_LEAP_BYTES: usize = 8;
/// The version 2+ data block's transition time and leap record sizes.
const TIME_BYTES: usize = 8;
const LEAP_BYTES: usize = 12;

/// `Link` lines of the IANA `backward` file (since release 2024b) whose
/// names `jiff_tzdb` still stores as separate zones without local mean time
/// or wartime history.
const SYSTEM_V_LINKS: [(&str, &str); 4] = [
    ("EST5EDT", "America/New_York"),
    ("CST6CDT", "America/Chicago"),
    ("MST7MDT", "America/Denver"),
    ("PST8PDT", "America/Los_Angeles"),
];

/// The UTC offset of a zone at an instant, and whether it is daylight
/// saving time (the TZif `isdst` flag of the local time type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offset {
    pub ms: i64,
    pub dst: bool,
}

/// A zone of the bundled database, parsed from its TZif bytes.
#[derive(Debug, Clone)]
pub struct Zone {
    /// The database's capitalization of the identifier.
    name: &'static str,
    /// Transition instants (seconds, big-endian `i64`), ascending.
    times: &'static [u8],
    /// The local time type index of each transition.
    indices: &'static [u8],
    /// The local time type records.
    types: &'static [u8],
    /// The footer's rule for instants after the last transition.
    rule: Option<Rule>,
}

impl Zone {
    /// The zone named `identifier` (compared ignoring ASCII case, as
    /// ECMA-262's IsTimeZoneOffsetString/GetAvailableNamedTimeZoneIdentifier
    /// compare), or `None` when the database has no such zone or its data
    /// does not parse.
    #[must_use]
    pub fn find(identifier: &str) -> Option<Self> {
        let (name, data) = jiff_tzdb::get(identifier)?;
        // The System V names that IANA 2024b turned into links (`backward`)
        // still carry their own rule-only data in this copy; read the data of
        // the zone they link to, keeping the name.
        let data = SYSTEM_V_LINKS
            .iter()
            .find(|(link, _)| *link == name)
            .and_then(|(_, target)| jiff_tzdb::get(target))
            .map_or(data, |(_, target)| target);
        Self::parse(name, data)
    }

    /// The identifier with the database's capitalization.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    fn parse(name: &'static str, data: &'static [u8]) -> Option<Self> {
        let (version, counts) = header(data)?;
        if version < b'2' {
            return None;
        }
        // Skip the version 1 data block; the 64-bit block follows.
        let v1 = block_bytes(&counts, V1_TIME_BYTES, V1_LEAP_BYTES)?;
        let second = data.get(HEADER_BYTES + v1..)?;
        let (_, counts) = header(second)?;
        let [isut, isstd, leap, time, kind, chars] = counts;
        if kind == 0 {
            return None;
        }
        let body = second.get(HEADER_BYTES..)?;
        let times_end = time.checked_mul(TIME_BYTES)?;
        let indices_end = times_end.checked_add(time)?;
        let types_end = indices_end.checked_add(kind.checked_mul(TYPE_BYTES)?)?;
        let end = block_bytes(&counts, TIME_BYTES, LEAP_BYTES)?;
        debug_assert!(end >= types_end + chars + leap * LEAP_BYTES + isstd + isut);
        let block = body.get(..end)?;
        let indices = block.get(times_end..indices_end)?;
        if indices.iter().any(|&index| usize::from(index) >= kind) {
            return None;
        }
        let footer = body.get(end..)?;
        let rule = footer_rule(footer)?;
        Some(Self {
            name,
            times: block.get(..times_end)?,
            indices,
            types: block.get(indices_end..types_end)?,
            rule,
        })
    }

    fn transitions(&self) -> usize {
        self.indices.len()
    }

    fn transition(&self, index: usize) -> i64 {
        let bytes = &self.times[index * TIME_BYTES..(index + 1) * TIME_BYTES];
        // The slice has exactly TIME_BYTES bytes.
        i64::from_be_bytes(bytes.try_into().unwrap_or([0; TIME_BYTES])) * MS_PER_SECOND
    }

    fn local_type(&self, index: usize) -> Offset {
        let record = &self.types[index * TYPE_BYTES..(index + 1) * TYPE_BYTES];
        let utoff = i32::from_be_bytes([record[0], record[1], record[2], record[3]]);
        Offset {
            ms: i64::from(utoff) * MS_PER_SECOND,
            dst: record[4] != 0,
        }
    }

    /// GetNamedTimeZoneOffsetNanoseconds (§21.4.1.21), in milliseconds: the
    /// offset in effect at the instant `time`.
    #[must_use]
    pub fn offset_at(&self, time: i64) -> Offset {
        let count = self.transitions();
        // The first transition after `time`.
        let after = self.partition(time);
        if after == 0 {
            // Before the first transition (or with none): type 0, or the
            // footer when there are no transitions at all (RFC 8536 §3.2).
            if count == 0
                && let Some(rule) = &self.rule
            {
                return rule.offset_at(time);
            }
            return self.local_type(0);
        }
        if after == count
            && time > self.transition(count - 1)
            && let Some(rule) = &self.rule
        {
            return rule.offset_at(time);
        }
        self.local_type(usize::from(self.indices[after - 1]))
    }

    /// The number of transitions at or before `time`.
    fn partition(&self, time: i64) -> usize {
        let (mut low, mut high) = (0, self.transitions());
        while low < high {
            let middle = low + (high - low) / 2;
            if self.transition(middle) <= time {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    /// The first transition strictly after `time`, explicit or from the
    /// footer rule.
    fn next_transition(&self, time: i64) -> Option<i64> {
        let after = self.partition(time);
        if after < self.transitions() {
            return Some(self.transition(after));
        }
        let rule = self.rule.as_ref()?;
        let floor = match self.transitions() {
            0 => time,
            count => time.max(self.transition(count - 1)),
        };
        rule.next_transition(floor)
    }

    /// Whether `time` falls in daylight saving time: the TZif `isdst` flag.
    /// The bundled data is the IANA *rearguard* form ICU also compiles, so
    /// negative daylight saving time (`Europe/Dublin`, `Africa/Windhoek`
    /// 1994–2017, `Africa/Casablanca`) appears as positive daylight saving
    /// time in summer, as in ICU.
    #[must_use]
    pub fn daylight(&self, time: i64) -> bool {
        self.offset_at(time).dst
    }

    /// The zone's current standard offset and, when it observes daylight
    /// saving time now, its daylight offset (ICU's `getRawOffset` and
    /// `getRawOffset() + getDSTSavings()`): the footer rule's, or without a
    /// rule the last period's offset.
    #[must_use]
    pub fn current_offsets(&self) -> (i64, Option<i64>) {
        match &self.rule {
            Some(rule) => rule.offsets(),
            None => {
                let last = self.indices.last().map_or(0, |&index| usize::from(index));
                (self.local_type(last).ms, None)
            }
        }
    }

    /// The offsets in effect from `start` to `end` (instants), as segments
    /// `(first instant, offset)` in order; the first segment begins before
    /// `start`.
    fn segments(&self, start: i64, end: i64, out: &mut [(i64, Offset); SEGMENTS]) -> usize {
        out[0] = (i64::MIN, self.offset_at(start));
        let mut count = 1;
        let mut at = start;
        while count < SEGMENTS {
            match self.next_transition(at) {
                Some(next) if next <= end => {
                    out[count] = (next, self.offset_at(next));
                    count += 1;
                    at = next;
                }
                _ => break,
            }
        }
        count
    }

    /// The offset UTC(t) (§21.4.1.26) subtracts from the local time value
    /// `local`: the offset of the earliest instant whose local time is
    /// `local`, or for a local time skipped by a transition, the offset in
    /// effect before that transition.
    #[must_use]
    pub fn offset_of_local(&self, local: i64) -> Offset {
        let mut segments = [(0, Offset { ms: 0, dst: false }); SEGMENTS];
        let count = self.segments(
            local.saturating_sub(WINDOW_MS),
            local.saturating_add(WINDOW_MS),
            &mut segments,
        );
        let segments = &segments[..count];
        let mut earliest: Option<(i64, Offset)> = None;
        for (index, &(start, offset)) in segments.iter().enumerate() {
            let instant = local - offset.ms;
            let end = segments.get(index + 1).map_or(i64::MAX, |next| next.0);
            if start <= instant && instant < end && earliest.is_none_or(|(at, _)| instant < at) {
                earliest = Some((instant, offset));
            }
        }
        if let Some((_, offset)) = earliest {
            return offset;
        }
        // A skipped local time: the offset of the last segment whose local
        // start is not after it.
        segments
            .iter()
            .rev()
            .find(|(start, offset)| *start == i64::MIN || start + offset.ms <= local)
            .map_or(segments[0].1, |&(_, offset)| offset)
    }
}

/// Segments of offsets examined around one local time: transitions within
/// 52 hours are at most a handful (two DST changes and a rule change); the
/// walk stops early beyond this bound, which keeps the work constant.
const SEGMENTS: usize = 8;

/// The version byte and the six counts (`isutcnt`, `isstdcnt`, `leapcnt`,
/// `timecnt`, `typecnt`, `charcnt`) of a TZif header.
fn header(data: &[u8]) -> Option<(u8, [usize; 6])> {
    if data.get(..MAGIC.len())? != MAGIC || data.len() < HEADER_BYTES {
        return None;
    }
    let mut counts = [0; 6];
    for (index, count) in counts.iter_mut().enumerate() {
        let at = COUNTS_AT + index * 4;
        let bytes = [data[at], data[at + 1], data[at + 2], data[at + 3]];
        *count = usize::try_from(u32::from_be_bytes(bytes)).ok()?;
    }
    Some((data[MAGIC.len()], counts))
}

/// The byte length of a data block with these counts.
fn block_bytes(counts: &[usize; 6], time_bytes: usize, leap_bytes: usize) -> Option<usize> {
    let [isut, isstd, leap, time, kind, chars] = *counts;
    time.checked_mul(time_bytes)?
        .checked_add(time)?
        .checked_add(kind.checked_mul(TYPE_BYTES)?)?
        .checked_add(chars)?
        .checked_add(leap.checked_mul(leap_bytes)?)?
        .checked_add(isstd)?
        .checked_add(isut)
}

/// The footer `\n<TZ string>\n`; an empty TZ string means no rule.
fn footer_rule(footer: &[u8]) -> Option<Option<Rule>> {
    let text = footer.strip_prefix(b"\n")?;
    let end = text.iter().position(|&byte| byte == b'\n')?;
    let text = core::str::from_utf8(&text[..end]).ok()?;
    if text.is_empty() {
        return Some(None);
    }
    Rule::parse(text).map(Some)
}

#[cfg(test)]
mod tests;
