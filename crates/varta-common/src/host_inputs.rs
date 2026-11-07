//! Recorded host inputs of v38 local time and `Intl` (K9 D4): the host's
//! IANA time zone identifier and its default BCP 47 locale tag.
//!
//! Hosts supply both once per instance (`intl_time_zone`, `intl_locale`),
//! need no capability (they configure, they do not reveal state of the
//! world beyond what the zone and language settings say), and record them in
//! the run report so a run can be replayed with the same inputs. The guest
//! resolves everything else (offsets, transitions, names) from its bundled
//! data, so hosts never compute local time. Embedders pass explicit inputs;
//! without them a run uses [`HostInputs::default`] (UTC, `en-US`), which
//! keeps library and test runs independent of the machine.
use std::fmt;

/// The longest zone identifier or locale tag a host may supply, in bytes.
/// IANA identifiers are at most 30 bytes long and BCP 47 tags with
/// extensions rarely exceed 100; anything longer is not a real setting.
pub const MAX_INPUT_BYTES: usize = 256;
/// The zone of a run without an explicit input.
pub const DEFAULT_TIME_ZONE: &str = "UTC";
/// The locale of a run without an explicit input.
pub const DEFAULT_LOCALE: &str = "en-US";

/// The zone and locale a host supplies to an instance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostInputs {
    /// An IANA time zone identifier (`Europe/Kyiv`); the guest uses UTC for
    /// an identifier its tzdata does not know.
    pub time_zone: String,
    /// A BCP 47 language tag (`uk-UA`); the guest uses `und` data for a tag
    /// that does not parse.
    pub locale: String,
}

impl Default for HostInputs {
    fn default() -> Self {
        Self {
            time_zone: DEFAULT_TIME_ZONE.to_owned(),
            locale: DEFAULT_LOCALE.to_owned(),
        }
    }
}

/// A host input that cannot be passed to a guest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostInputError {
    #[error("invalid {kind} host input: {reason}")]
    Invalid {
        kind: InputKind,
        reason: &'static str,
    },
}

/// Which input was invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    TimeZone,
    Locale,
}

impl fmt::Display for InputKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TimeZone => "time zone",
            Self::Locale => "locale",
        })
    }
}

impl HostInputs {
    /// Check the syntax hosts must respect: non-empty printable ASCII of at
    /// most [`MAX_INPUT_BYTES`]; zone identifiers use `A-Z a-z 0-9 / _ + -`,
    /// locale tags `A-Z a-z 0-9 -`. Whether the zone or locale exists is
    /// the guest's question.
    pub fn validate(&self) -> Result<(), HostInputError> {
        check(&self.time_zone, InputKind::TimeZone, |byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'+' | b'-')
        })?;
        check(&self.locale, InputKind::Locale, |byte| {
            byte.is_ascii_alphanumeric() || byte == b'-'
        })
    }

    /// The locale tag of a POSIX locale name (`uk_UA.UTF-8`,
    /// `sr_RS@latin`): language and territory joined by `-`, codeset and
    /// modifier dropped; `C` and `POSIX` are `en-US-u-va-posix`, which ICU
    /// and V8 resolve to `en-US`, so they give [`DEFAULT_LOCALE`].
    #[must_use]
    pub fn locale_from_posix(name: &str) -> Option<String> {
        let base = name.split(['.', '@']).next().unwrap_or_default();
        if base.is_empty() || base == "C" || base == "POSIX" {
            return Some(DEFAULT_LOCALE.to_owned());
        }
        let tag = base.replace('_', "-");
        let valid = tag.len() <= MAX_INPUT_BYTES
            && tag
                .split('-')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()));
        valid.then_some(tag)
    }
}

fn check(text: &str, kind: InputKind, allowed: impl Fn(u8) -> bool) -> Result<(), HostInputError> {
    let reason = if text.is_empty() {
        "empty"
    } else if text.len() > MAX_INPUT_BYTES {
        "too long"
    } else if !text.bytes().all(allowed) {
        "unexpected character"
    } else {
        return Ok(());
    };
    Err(HostInputError::Invalid { kind, reason })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_are_checked_for_syntax_only() {
        assert!(HostInputs::default().validate().is_ok());
        let inputs = |time_zone: &str, locale: &str| HostInputs {
            time_zone: time_zone.to_owned(),
            locale: locale.to_owned(),
        };
        assert!(
            inputs("America/Argentina/Buenos_Aires", "sr-Latn-RS")
                .validate()
                .is_ok()
        );
        assert!(
            inputs("Etc/GMT+5", "en-US-u-ca-buddhist")
                .validate()
                .is_ok()
        );
        assert!(inputs("Mars/Olympus_Mons", "xx").validate().is_ok());
        for (zone, locale) in [
            ("", "en"),
            ("UTC", ""),
            ("Europe/Kyiv\0", "en"),
            ("UTC", "en_US"),
            ("UTC", "ü"),
        ] {
            assert!(
                inputs(zone, locale).validate().is_err(),
                "{zone:?} {locale:?}"
            );
        }
        assert!(
            inputs(&"A".repeat(MAX_INPUT_BYTES + 1), "en")
                .validate()
                .is_err()
        );
    }

    #[test]
    fn posix_locale_names_become_tags() {
        assert_eq!(
            HostInputs::locale_from_posix("uk_UA.UTF-8").as_deref(),
            Some("uk-UA")
        );
        assert_eq!(
            HostInputs::locale_from_posix("de_DE@euro").as_deref(),
            Some("de-DE")
        );
        assert_eq!(HostInputs::locale_from_posix("C").as_deref(), Some("en-US"));
        assert_eq!(
            HostInputs::locale_from_posix("POSIX").as_deref(),
            Some("en-US")
        );
        assert_eq!(
            HostInputs::locale_from_posix("C.UTF-8").as_deref(),
            Some("en-US")
        );
        assert_eq!(
            HostInputs::locale_from_posix("en_US..").as_deref(),
            Some("en-US")
        );
        assert_eq!(HostInputs::locale_from_posix("ü_X"), None);
    }
}
