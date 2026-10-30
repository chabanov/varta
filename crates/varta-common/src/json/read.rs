//! A `no_std` reader of checked JSON schema bytes for the linked v38
//! decoder (the host reads schemas with `serde_json` and the derived
//! `Deserialize`).
//!
//! It accepts and rejects exactly what that strict `serde` reader does: the
//! JSON grammar of `serde_json` (numbers in range, well-formed strings),
//! structs as objects (members in any order; unknown, duplicate and missing
//! members rejected; `optional` defaulting to `false`) or as arrays of their
//! fields in declaration order, and the internally tagged `kind` of a type,
//! whose unit variants ignore any other members (and admit no further array
//! elements) while struct variants admit exactly their own.
use super::*;
use crate::json::schema::SchemaBounds;

fn bad(message: &str) -> CheckedJsonError {
    CheckedJsonError::Schema(message.into())
}

/// One parsed JSON value of schema text.
enum Raw {
    Null,
    Boolean(bool),
    /// A non-negative integer literal within `u64`.
    Integer(u64),
    /// Any other number in binary64 range.
    Number,
    String(String),
    Array(Vec<Raw>),
    Object(Vec<(String, Raw)>),
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn skip(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\t' | b'\r')) {
            self.pos += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> bool {
        self.skip();
        if self.peek() == Some(byte) {
            self.pos += 1;
            return true;
        }
        false
    }
    fn expect(&mut self, byte: u8) -> Result<(), CheckedJsonError> {
        if self.eat(byte) {
            Ok(())
        } else {
            Err(bad("schema syntax"))
        }
    }
    /// One value; containers nest at most as deep as the schema grammar's
    /// own structure plus ignored members, bounded by the input length.
    fn value(&mut self) -> Result<Raw, CheckedJsonError> {
        self.skip();
        match self.peek().ok_or_else(|| bad("schema syntax"))? {
            b'{' => {
                self.pos += 1;
                let mut members = Vec::new();
                if self.eat(b'}') {
                    return Ok(Raw::Object(members));
                }
                loop {
                    self.skip();
                    let key = self.string()?;
                    self.expect(b':')?;
                    members.push((key, self.value()?));
                    if self.eat(b',') {
                        continue;
                    }
                    self.expect(b'}')?;
                    return Ok(Raw::Object(members));
                }
            }
            b'[' => {
                self.pos += 1;
                let mut elements = Vec::new();
                if self.eat(b']') {
                    return Ok(Raw::Array(elements));
                }
                loop {
                    elements.push(self.value()?);
                    if self.eat(b',') {
                        continue;
                    }
                    self.expect(b']')?;
                    return Ok(Raw::Array(elements));
                }
            }
            b'"' => self.string().map(Raw::String),
            b't' => self.word(b"true", Raw::Boolean(true)),
            b'f' => self.word(b"false", Raw::Boolean(false)),
            b'n' => self.word(b"null", Raw::Null),
            _ => self.number(),
        }
    }
    fn word(&mut self, word: &[u8], value: Raw) -> Result<Raw, CheckedJsonError> {
        if !self.bytes[self.pos..].starts_with(word) {
            return Err(bad("schema syntax"));
        }
        self.pos += word.len();
        Ok(value)
    }
    fn number(&mut self) -> Result<Raw, CheckedJsonError> {
        let start = self.pos;
        let negative = self.peek() == Some(b'-');
        if negative {
            self.pos += 1;
        }
        let digits = self.pos;
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            _ => return Err(bad("schema syntax")),
        }
        let integer_end = self.pos;
        let mut plain = true;
        if self.peek() == Some(b'.') {
            plain = false;
            self.pos += 1;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            plain = false;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.digits()?;
        }
        if self.peek().is_some_and(|b| b.is_ascii_digit()) {
            return Err(bad("schema syntax"));
        }
        let text = core::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| bad("schema syntax"))?;
        if plain && !negative {
            let digits = core::str::from_utf8(&self.bytes[digits..integer_end])
                .map_err(|_| bad("schema syntax"))?;
            if let Ok(value) = digits.parse::<u64>() {
                return Ok(Raw::Integer(value));
            }
        }
        // `serde_json` reads every other number as binary64 (or i64).
        if text.parse::<f64>().is_ok_and(f64::is_finite) {
            Ok(Raw::Number)
        } else {
            Err(bad("number out of range"))
        }
    }
    fn digits(&mut self) -> Result<(), CheckedJsonError> {
        if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
            return Err(bad("schema syntax"));
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        Ok(())
    }
    /// A well-formed string (no lone surrogate escape).
    fn string(&mut self) -> Result<String, CheckedJsonError> {
        if self.peek() != Some(b'"') {
            return Err(bad("schema syntax"));
        }
        self.pos += 1;
        let mut text = String::new();
        let mut run = self.pos;
        let flush = |text: &mut String, bytes: &[u8]| -> Result<(), CheckedJsonError> {
            text.push_str(core::str::from_utf8(bytes).map_err(|_| bad("schema UTF-8"))?);
            Ok(())
        };
        loop {
            match self.peek().ok_or_else(|| bad("schema syntax"))? {
                b'"' => {
                    flush(&mut text, &self.bytes[run..self.pos])?;
                    self.pos += 1;
                    return Ok(text);
                }
                b'\\' => {
                    flush(&mut text, &self.bytes[run..self.pos])?;
                    self.pos += 1;
                    let escape = self.peek().ok_or_else(|| bad("schema syntax"))?;
                    self.pos += 1;
                    let c = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode()?,
                        _ => return Err(bad("schema escape")),
                    };
                    text.push(c);
                    run = self.pos;
                }
                0..0x20 => return Err(bad("control character in schema string")),
                _ => self.pos += 1,
            }
        }
    }
    /// The character of a `\u` escape (after `\u`), a surrogate pair taking
    /// a second escape.
    fn unicode(&mut self) -> Result<char, CheckedJsonError> {
        let high = self.hex()?;
        if !(0xd800..0xe000).contains(&high) {
            return char::from_u32(u32::from(high)).ok_or_else(|| bad("schema escape"));
        }
        if high >= 0xdc00 || !self.bytes[self.pos..].starts_with(b"\\u") {
            return Err(bad("lone surrogate in schema string"));
        }
        self.pos += 2;
        let low = self.hex()?;
        char::decode_utf16([high, low])
            .next()
            .and_then(Result::ok)
            .ok_or_else(|| bad("lone surrogate in schema string"))
    }
    fn hex(&mut self) -> Result<u16, CheckedJsonError> {
        let mut code = 0u16;
        for _ in 0..4 {
            let digit = self.peek().ok_or_else(|| bad("schema escape"))?;
            let value = (digit as char)
                .to_digit(16)
                .ok_or_else(|| bad("schema escape"))?;
            code = code << 4 | value as u16;
            self.pos += 1;
        }
        Ok(code)
    }
}

fn integer(raw: Raw) -> Result<u32, CheckedJsonError> {
    match raw {
        Raw::Integer(value) => u32::try_from(value).map_err(|_| bad("schema integer")),
        _ => Err(bad("schema integer")),
    }
}
fn string(raw: Raw) -> Result<String, CheckedJsonError> {
    match raw {
        Raw::String(text) => Ok(text),
        _ => Err(bad("schema string")),
    }
}
fn integers(raw: Raw) -> Result<Vec<u32>, CheckedJsonError> {
    match raw {
        Raw::Array(elements) => elements.into_iter().map(integer).collect(),
        _ => Err(bad("schema sequence")),
    }
}

/// The members of a struct (`names` in declaration order), from an object
/// or an array of exactly its fields (`required` of them at least).
fn members<const N: usize>(
    raw: Raw,
    names: [&str; N],
    required: usize,
) -> Result<[Option<Raw>; N], CheckedJsonError> {
    let mut values: [Option<Raw>; N] = core::array::from_fn(|_| None);
    match raw {
        Raw::Object(members) => {
            for (key, value) in members {
                let index = names
                    .iter()
                    .position(|name| *name == key)
                    .ok_or_else(|| bad("unknown schema member"))?;
                if values[index].replace(value).is_some() {
                    return Err(bad("duplicate schema member"));
                }
            }
            if values[..required].iter().any(Option::is_none) {
                return Err(bad("missing schema member"));
            }
        }
        Raw::Array(elements) => {
            if !(required..=N).contains(&elements.len()) {
                return Err(bad("schema sequence length"));
            }
            for (slot, value) in values.iter_mut().zip(elements) {
                *slot = Some(value);
            }
        }
        _ => return Err(bad("schema struct")),
    }
    Ok(values)
}
fn take(value: &mut Option<Raw>) -> Result<Raw, CheckedJsonError> {
    value.take().ok_or_else(|| bad("missing schema member"))
}

fn field(raw: Raw) -> Result<JsonField, CheckedJsonError> {
    let [mut name, mut ty, optional] = members(raw, ["name", "ty", "optional"], 2)?;
    Ok(JsonField {
        name: string(take(&mut name)?)?,
        ty: integer(take(&mut ty)?)?,
        optional: match optional {
            None => false,
            Some(Raw::Boolean(value)) => value,
            Some(_) => return Err(bad("schema boolean")),
        },
    })
}

/// The fields of each struct variant, in declaration order.
fn variant_fields(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "bigInt" | "null" | "undefined" | "i32" | "float64" | "boolean" | "string" => &[],
        "primitiveUnion" | "referenceUnion" => &["members"],
        "record" => &["descriptor", "fields"],
        "array" => &["element"],
        "dictionary" => &["descriptor", "value"],
        _ => return None,
    })
}

fn ty(raw: Raw) -> Result<JsonType, CheckedJsonError> {
    // The tag and the variant's members, by name.
    let (kind, mut fields): (String, Vec<(String, Raw)>) = match raw {
        Raw::Object(members) => {
            let mut kind = None;
            let mut rest = Vec::new();
            for (key, value) in members {
                if key == "kind" {
                    if kind.replace(value).is_some() {
                        return Err(bad("duplicate schema member"));
                    }
                } else {
                    rest.push((key, value));
                }
            }
            let kind = string(kind.ok_or_else(|| bad("missing schema member"))?)?;
            let names = variant_fields(&kind).ok_or_else(|| bad("unknown schema kind"))?;
            if names.is_empty() {
                // Unit variants ignore the other members.
                rest.clear();
            }
            let mut seen: Vec<&str> = Vec::new();
            for (key, _) in &rest {
                let name = names
                    .iter()
                    .find(|name| **name == key)
                    .ok_or_else(|| bad("unknown schema member"))?;
                if seen.contains(name) {
                    return Err(bad("duplicate schema member"));
                }
                seen.push(name);
            }
            if seen.len() != names.len() {
                return Err(bad("missing schema member"));
            }
            (kind, rest)
        }
        Raw::Array(elements) => {
            let mut elements = elements.into_iter();
            let kind = string(elements.next().ok_or_else(|| bad("schema sequence length"))?)?;
            let names = variant_fields(&kind).ok_or_else(|| bad("unknown schema kind"))?;
            let values: Vec<Raw> = elements.collect();
            if values.len() != names.len() {
                return Err(bad("schema sequence length"));
            }
            let fields = names
                .iter()
                .map(|name| String::from(*name))
                .zip(values)
                .collect();
            (kind, fields)
        }
        _ => return Err(bad("schema type")),
    };
    let mut member = |name: &str| -> Result<Raw, CheckedJsonError> {
        let index = fields
            .iter()
            .position(|(key, _)| key == name)
            .ok_or_else(|| bad("missing schema member"))?;
        Ok(fields.swap_remove(index).1)
    };
    Ok(match kind.as_str() {
        "bigInt" => JsonType::BigInt,
        "null" => JsonType::Null,
        "undefined" => JsonType::Undefined,
        "i32" => JsonType::I32,
        "float64" => JsonType::Float64,
        "boolean" => JsonType::Boolean,
        "string" => JsonType::String,
        "primitiveUnion" => JsonType::PrimitiveUnion {
            members: integers(member("members")?)?,
        },
        "referenceUnion" => JsonType::ReferenceUnion {
            members: integers(member("members")?)?,
        },
        "record" => JsonType::Record {
            descriptor: integer(member("descriptor")?)?,
            fields: match member("fields")? {
                Raw::Array(elements) => elements
                    .into_iter()
                    .map(field)
                    .collect::<Result<_, _>>()?,
                _ => return Err(bad("schema sequence")),
            },
        },
        "array" => JsonType::Array {
            element: integer(member("element")?)?,
        },
        "dictionary" => JsonType::Dictionary {
            descriptor: integer(member("descriptor")?)?,
            value: integer(member("value")?)?,
        },
        _ => return Err(bad("unknown schema kind")),
    })
}

impl JsonSchema {
    /// Read and validate schema bytes of at most `schema_bytes` within
    /// `bounds` (`no_std`; the linked v38 decoder's reader).
    pub fn read(
        bytes: &[u8],
        bounds: SchemaBounds,
        schema_bytes: usize,
    ) -> Result<Self, CheckedJsonError> {
        if bytes.len() > schema_bytes {
            return Err(bad("byte limit"));
        }
        let mut reader = Reader { bytes, pos: 0 };
        let raw = reader.value()?;
        reader.skip();
        if reader.pos != bytes.len() {
            return Err(bad("trailing schema bytes"));
        }
        let [mut version, mut root, mut types] = members(raw, ["version", "root", "types"], 3)?;
        let schema = Self {
            version: integer(take(&mut version)?)?,
            root: integer(take(&mut root)?)?,
            types: match take(&mut types)? {
                Raw::Array(elements) => elements.into_iter().map(ty).collect::<Result<_, _>>()?,
                _ => return Err(bad("schema sequence")),
            },
        };
        schema.validate_bounded(bounds)?;
        Ok(schema)
    }
}
