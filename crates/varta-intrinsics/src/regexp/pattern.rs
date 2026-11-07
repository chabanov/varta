//! The RegExp pattern compiler: ECMAScript 2025 pattern syntax (§22.2.1) to
//! matcher programs, with one implementation for both compile times.
//!
//! The ahead-of-time compiler (`varta-compiler` `regexp`) compiles regular
//! expression literals and constant `new RegExp` calls with it; v38 programs
//! link [`construct`] into the guest (`regexp_compile`), so a pattern built at
//! run time is parsed and compiled as ordinary guest code metered as fuel and
//! produces the same format-2 program (`varta_common::regexp`) the linked
//! matcher runs. The v37 linear-time subset (the parser's `full` switch off)
//! and its program format 1 stay in the compiler, which reuses this parser.
//!
//! Patterns are UTF-16 code units, as ECMAScript strings are; a pattern may
//! hold lone surrogates. Early errors carry V8's message text without the
//! `Invalid regular expression: /…/…: ` prefix, which the caller formats.
use alloc::{string::String, vec, vec::Vec};

pub mod case;
mod case_table;
mod classes;
mod emoji_table;
pub mod parse;
pub mod program;
mod property;
pub mod set;

pub use parse::Flags;

/// A pattern that cannot be compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegExpError {
    /// An early SyntaxError of the pattern: V8's message, which follows
    /// `Invalid regular expression: /pattern/flags: ` (flags in canonical
    /// order).
    Pattern(&'static str),
    /// Unknown, repeated or incompatible flags (V8: `Invalid flags supplied
    /// to RegExp constructor 'flags'`, with the flags as written).
    Flags,
    /// A valid construct outside the v37 linear-time subset, named
    /// precisely.
    Unsupported(&'static str),
    /// V37: a numbered backreference `\n` (outside the linear-time subset).
    Backreference(u32),
    /// A v38 program beyond `varta_common::regexp::MAX_PROGRAM_UNITS`.
    TooLarge,
}

/// A compiled pattern: its programs and groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled {
    /// The full program, with every capture.
    pub units: Vec<u16>,
    /// The program without group captures, empty checks or resets where
    /// possible: the same match bounds (slots 0 and 1) for operations that
    /// need nothing else.
    pub lean: Vec<u16>,
    pub groups: usize,
    /// Named groups with their one-based indices, in index order.
    pub names: Vec<(String, usize)>,
}

impl Compiled {
    /// The names of groups `1..=groups` joined by `,` (empty for an unnamed
    /// group), or empty without named groups: the form the RegExp preludes
    /// read.
    #[must_use]
    pub fn joined_names(&self) -> String {
        let mut joined = String::new();
        if self.names.is_empty() {
            return joined;
        }
        for group in 1..=self.groups {
            if group > 1 {
                joined.push(GROUP_SEPARATOR);
            }
            if let Some((name, _)) = self.names.iter().find(|(_, index)| *index == group) {
                joined.push_str(name);
            }
        }
        joined
    }
}

/// The separator of [`Compiled::joined_names`].
pub const GROUP_SEPARATOR: char = ',';

/// Parse and compile a v38 pattern (`full_regexp`): the whole ES2025
/// grammar to a backtracking program (format 2).
pub fn compile_full(pattern: &[u16], flags: Flags) -> Result<Compiled, RegExpError> {
    let parsed = parse::parse(pattern, flags, true)?;
    program::compile(&parsed.node, parsed.groups, &parsed.names, flags)
}

/// EscapeRegExpPattern as V8 implements it: the `source` of a RegExp built
/// from `pattern` — `/` outside a class and line terminators escaped, the
/// empty pattern as `(?:)`.
#[must_use]
pub fn escape_source(pattern: &[u16]) -> Vec<u16> {
    const EMPTY: &str = "(?:)";
    const BACKSLASH: u16 = b'\\' as u16;
    const SLASH: u16 = b'/' as u16;
    const OPEN: u16 = b'[' as u16;
    const CLOSE: u16 = b']' as u16;
    if pattern.is_empty() {
        return EMPTY.encode_utf16().collect();
    }
    let mut source = Vec::with_capacity(pattern.len());
    let (mut escaped, mut class) = (false, false);
    for &unit in pattern {
        let text = match unit {
            0x0A => Some(if escaped { "n" } else { "\\n" }),
            0x0D => Some(if escaped { "r" } else { "\\r" }),
            0x2028 => Some(if escaped { "u2028" } else { "\\u2028" }),
            0x2029 => Some(if escaped { "u2029" } else { "\\u2029" }),
            SLASH if !escaped && !class => Some("\\/"),
            _ => None,
        };
        match text {
            Some(text) => source.extend(text.encode_utf16()),
            None => source.push(unit),
        }
        if escaped {
            escaped = false;
        } else {
            match unit {
                BACKSLASH => escaped = true,
                OPEN => class = true,
                CLOSE => class = false,
                _ => {}
            }
        }
    }
    source
}

/// The prefix of a pattern's early-error message.
pub const SYNTAX_PREFIX: &str = "Invalid regular expression: /";
/// The flags error message before the flags as written.
pub const FLAGS_PREFIX: &str = "Invalid flags supplied to RegExp constructor '";

/// The run-time construction of a v38 RegExp (`new RegExp(pattern, flags)`
/// with values only known at run time), as the linked `regexp_compile`
/// returns it to the RegExp prelude.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constructed {
    pub flags: Flags,
    pub compiled: Compiled,
    /// The `source` (EscapeRegExpPattern of the pattern).
    pub source: Vec<u16>,
}

/// RegExpInitialize up to the program (§22.2.3.3): parse `flags` and
/// `pattern`, then compile. On an early error, the SyntaxError message V8
/// throws, as UTF-16 (the pattern may hold lone surrogates).
pub fn construct(pattern: &[u16], flags: &[u16]) -> Result<Constructed, Vec<u16>> {
    let parsed_flags = Flags::parse(flags, true).map_err(|_| flags_message(flags))?;
    let compiled = compile_full(pattern, parsed_flags).map_err(|error| match error {
        RegExpError::Pattern(message) => pattern_message(pattern, parsed_flags, message),
        RegExpError::Flags => flags_message(flags),
        // Under `full` only the program format's size limit is unsupported:
        // V8's text for a pattern beyond its own limits.
        RegExpError::Unsupported(_) | RegExpError::Backreference(_) | RegExpError::TooLarge => {
            pattern_message(pattern, parsed_flags, TOO_LARGE)
        }
    })?;
    Ok(Constructed {
        flags: parsed_flags,
        compiled,
        source: escape_source(pattern),
    })
}

/// V8's message for a pattern beyond the engine's limits.
pub const TOO_LARGE: &str = "Regular expression too large";

/// The `construction` record (`varta_common::regexp::construction`) of
/// `new RegExp(pattern, flags)`: a constructed RegExp, or the SyntaxError
/// message.
#[must_use]
pub fn encode(pattern: &[u16], flags: &[u16]) -> Vec<u16> {
    use varta_common::regexp::construction as record;
    fn word(record: &mut [u16], index: usize, value: u32) {
        record[2 * index] = value as u16;
        record[2 * index + 1] = (value >> u16::BITS) as u16;
    }
    let constructed = match construct(pattern, flags) {
        Ok(constructed) => constructed,
        Err(message) => {
            let mut text = vec![0; record::MESSAGE_UNIT];
            word(&mut text, record::STATUS, record::SYNTAX_ERROR as u32);
            text.extend(message);
            return text;
        }
    };
    let Constructed {
        flags,
        compiled,
        source,
    } = constructed;
    let names: Vec<u16> = compiled.joined_names().encode_utf16().collect();
    let flags_text: Vec<u16> = flags.text().encode_utf16().collect();
    let mut text = vec![0; record::TEXT_UNIT];
    word(&mut text, record::STATUS, record::OK as u32);
    word(&mut text, record::BITS, flags.bits());
    // Lengths are below the 1 MiB string limit the pattern was read with,
    // or program sizes below `MAX_PROGRAM_UNITS`.
    for (index, length) in [
        (record::COUNT, compiled.groups + 1),
        (record::PROGRAM_UNITS, compiled.units.len()),
        (record::LEAN_UNITS, compiled.lean.len()),
        (record::NAMES_UNITS, names.len()),
        (record::SOURCE_UNITS, source.len()),
        (record::FLAGS_UNITS, flags_text.len()),
    ] {
        word(&mut text, index, length as u32);
    }
    for part in [
        &compiled.units,
        &compiled.lean,
        &names,
        &source,
        &flags_text,
    ] {
        text.extend_from_slice(part);
    }
    text
}

/// [`encode`] over UTF-16LE strings, as the linked `regexp_compile` reads
/// and returns them.
#[must_use]
pub fn encode_bytes(pattern: &[u8], flags: &[u8]) -> Vec<u8> {
    fn units(bytes: &[u8]) -> Vec<u16> {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect()
    }
    encode(&units(pattern), &units(flags))
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

/// `Invalid flags supplied to RegExp constructor '<flags>'`.
#[must_use]
pub fn flags_message(flags: &[u16]) -> Vec<u16> {
    let mut message: Vec<u16> = FLAGS_PREFIX.encode_utf16().collect();
    message.extend_from_slice(flags);
    message.push(u16::from(b'\''));
    message
}

/// `Invalid regular expression: /<pattern>/<flags>: <message>`.
#[must_use]
pub fn pattern_message(pattern: &[u16], flags: Flags, message: &str) -> Vec<u16> {
    let mut text: Vec<u16> = SYNTAX_PREFIX.encode_utf16().collect();
    text.extend_from_slice(pattern);
    text.push(u16::from(b'/'));
    text.extend(flags.text().encode_utf16());
    text.extend(": ".encode_utf16());
    text.extend(message.encode_utf16());
    text
}

#[cfg(test)]
mod tests;
