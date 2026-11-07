//! Linked local time (v38 `local_time`, K9 D4): `date_zone_offset` and
//! `date_zone_name` over `varta_intl`'s bundled tzdata and CLDR data, so
//! every host computes the same offsets and names.
use crate::{
    arena::entry,
    guest::{Guest, checked},
    text::owned,
};
use alloc::string::String;
use varta_intl::{
    data::{self, Provider, Source},
    names,
    zone::Zone,
};
use varta_intrinsics::string;

/// Zone identifiers and locale tags are short host inputs
/// (`varta_common::host_inputs::MAX_INPUT_BYTES` bytes of ASCII, two bytes
/// per UTF-16 unit); a longer argument names nothing.
const MAX_NAME_BYTES: usize = 2 * 256;
/// The zone of an identifier the database does not know (ECMA-262
/// SystemTimeZoneIdentifier answers an available zone).
const FALLBACK_ZONE: &str = "UTC";
/// Local and UTC time values reach at most a day beyond the time value
/// range (MakeDate of clipped components); farther values clip to NaN with
/// any offset.
const MAX_TIME: f64 = 8.64e15 + 8.64e7;
/// The `date_zone_offset` mode that answers the daylight flag of the zone
/// name (`date_zone_name` depends on the instant only through it).
const DAYLIGHT_FLAG: f64 = 2.0;

/// The module's linked locale data segments.
struct Linked;

impl Source for Linked {
    fn read(&self, marker: u32, offset: u32, dest: &mut [u8]) -> bool {
        crate::guest::intl_read(marker, offset, dest)
    }
    fn len(&self, marker: u32) -> Option<u32> {
        crate::guest::intl_len(marker)
    }
}

/// The text of a short UTF-16 argument; empty when it is longer than
/// [`MAX_NAME_BYTES`] or not well-formed.
fn text(ptr: i32, len: i32) -> String {
    if usize::try_from(len).map_or(true, |len| len > MAX_NAME_BYTES) {
        return String::new();
    }
    let bytes = checked(string::read(&Guest, ptr, len, MAX_NAME_BYTES));
    char::decode_utf16(string::units(&bytes))
        .collect::<Result<String, _>>()
        .unwrap_or_default()
}

fn zone(ptr: i32, len: i32) -> Zone {
    Zone::find(&text(ptr, len))
        .or_else(|| Zone::find(FALLBACK_ZONE))
        .unwrap_or_else(|| core::arch::wasm32::unreachable())
}

/// The milliseconds of a finite time value within reach (time values are
/// integral: TimeClip and MakeDate of integral components), else `None`.
fn milliseconds(time: f64) -> Option<i64> {
    (time.is_finite() && time.abs() <= MAX_TIME).then_some(time as i64)
}

/// `date_zone_offset (zone ptr, len, time, mode) -> offset`: the offset of
/// LocalTime(t) (`mode` 0) or of UTC(t) (`mode` 1) in milliseconds, or
/// (`mode` 2) the daylight flag the zone name of the instant uses (1 or 0,
/// [`names::equivalent_time`]); 0 for a time value outside the range.
#[unsafe(no_mangle)]
pub extern "C" fn date_zone_offset(ptr: i32, len: i32, time: f64, local: f64) -> f64 {
    entry(|| {
        let Some(time) = milliseconds(time) else {
            return 0.0;
        };
        let zone = zone(ptr, len);
        if local == DAYLIGHT_FLAG {
            return f64::from(u8::from(zone.daylight(names::equivalent_time(time))));
        }
        let offset = if local == 0.0 {
            zone.offset_at(time)
        } else {
            zone.offset_of_local(time)
        };
        offset.ms as f64
    })
}

/// `date_zone_name (zone ptr, len, locale ptr, len, time) -> string`: the
/// localized long zone name `Date.prototype.toString` shows at `time`.
#[unsafe(no_mangle)]
pub extern "C" fn date_zone_name(
    zone_ptr: i32,
    zone_len: i32,
    locale_ptr: i32,
    locale_len: i32,
    time: f64,
) -> i32 {
    entry(|| {
        let zone = zone(zone_ptr, zone_len);
        let locale = data::default_locale(&Linked, &names::locale(&text(locale_ptr, locale_len)));
        let name = names::long_name(
            &Provider(Linked),
            &locale,
            &zone,
            milliseconds(time).unwrap_or(0),
        );
        owned(&varta_common::text::encode(&name))
    })
}
