//! Console output streams (v38 `console_methods`).
//!
//! A host captures every console line of an execution in one ordered list
//! (`RunOutput::lines` of the native runtime, the browser worker's `logs`).
//! Standard output lines start with [`LOG_PREFIX`]; the standard-error
//! operation (`HostOperation::LogStderr16`, `log_stderr16`) writes lines that
//! start with the prefix of their [`StderrLevel`], so a host routes a line
//! by its prefix alone: no standard-output line can start with a
//! standard-error prefix.

/// The prefix of a standard-output console line.
pub const LOG_PREFIX: &str = "varta log: ";

/// The level word of `log_stderr16`: which console method wrote the line.
/// Both levels are standard error (Node's `console.error` and
/// `console.warn` write the same stream); the browser host forwards each to
/// the console method of the same name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum StderrLevel {
    /// `console.error` and `console.trace`.
    Error = 0,
    /// `console.warn`, `console.assert` and the console's process warnings.
    Warn = 1,
}

impl StderrLevel {
    pub const ALL: [Self; 2] = [Self::Error, Self::Warn];

    /// The level of a guest level word; any other word is a noncanonical
    /// call.
    #[must_use]
    pub fn from_word(word: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|level| *level as i32 == word)
    }

    /// The prefix of a line of this level.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Error => "varta error: ",
            Self::Warn => "varta warn: ",
        }
    }

    /// The browser console method a line of this level is forwarded to.
    #[must_use]
    pub const fn method(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
        }
    }
}

/// What the linked `console_render` intrinsic
/// (`HostOperation::ConsoleRender`) renders from its request cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RenderMode {
    /// `console.dir`: `util.inspect(value, options)`.
    Inspect = 0,
    /// `console.table(data, properties)`.
    Table = 1,
}

impl RenderMode {
    pub const ALL: [Self; 2] = [Self::Inspect, Self::Table];

    /// The mode of a guest mode word.
    #[must_use]
    pub fn from_word(word: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| *mode as i32 == word)
    }
}

/// Whether a captured console line belongs to standard error.
#[must_use]
pub fn is_standard_error(line: &str) -> bool {
    StderrLevel::ALL
        .iter()
        .any(|level| line.starts_with(level.prefix()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_round_trip_and_prefixes_are_disjoint() {
        for level in StderrLevel::ALL {
            assert_eq!(StderrLevel::from_word(level as i32), Some(level));
            assert!(!level.prefix().starts_with(LOG_PREFIX));
            assert!(is_standard_error(&format!("{}x", level.prefix())));
        }
        assert_eq!(StderrLevel::from_word(2), None);
        assert_eq!(StderrLevel::from_word(-1), None);
        assert!(!is_standard_error("varta log: varta error: x"));
    }
}
