//! UTF-16LE guest strings. Byte lengths are physical; source lengths count u16 units.
use alloc::{string::String, vec::Vec};

#[derive(Debug, thiserror::Error)]
#[error("invalid string encoding")]
pub struct TextError;

#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
pub fn units(bytes: &[u8]) -> Result<Vec<u16>, TextError> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(TextError);
    }
    Ok(pairs.iter().map(|p| u16::from_le_bytes(*p)).collect())
}
/// Resource names must round-trip exactly: no replacement, normalization or NUL.
pub fn resource(bytes: &[u8]) -> Result<String, TextError> {
    let text = strict(bytes)?;
    if text.contains('\0') {
        return Err(TextError);
    }
    Ok(text)
}
pub fn strict(bytes: &[u8]) -> Result<String, TextError> {
    String::from_utf16(&units(bytes)?).map_err(|_| TextError)
}
/// Convert JSON source without losing raw lone units inside string tokens.
/// Invalid escapes and units outside a string remain errors, never repaired JSON.
pub fn json_input(bytes: &[u8]) -> Result<Vec<u8>, TextError> {
    json_input_classified(bytes).map_err(|_| TextError)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JsonTextError {
    #[error("JSON input byte limit")]
    Limit,
    #[error("invalid UTF-16 byte length")]
    Encoding,
    #[error("invalid JSON source text")]
    Syntax,
}
/// Separate transport/resource violations from invalid JSON source text.
pub fn json_input_classified(bytes: &[u8]) -> Result<Vec<u8>, JsonTextError> {
    json_input_limited(bytes, crate::abi::MAX_INPUT_BYTES)
}
/// [`json_input_classified`] with the profile's JSON input limit
/// (`DataLimits::json_input_bytes`) on the UTF-16 input and UTF-8 output.
pub fn json_input_limited(bytes: &[u8], limit: usize) -> Result<Vec<u8>, JsonTextError> {
    if bytes.len() > limit {
        return Err(JsonTextError::Limit);
    }
    let units = units(bytes).map_err(|_| JsonTextError::Encoding)?;
    let mut output = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    for value in char::decode_utf16(units) {
        match value {
            Err(error) => {
                if !quoted || escaped {
                    return Err(JsonTextError::Syntax);
                }
                output.extend_from_slice(
                    alloc::format!("\\u{:04x}", error.unpaired_surrogate()).as_bytes(),
                );
            }
            Ok(c) => {
                if quoted {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        quoted = false;
                    }
                } else if c == '"' {
                    quoted = true;
                }
                output.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            }
        }
        if output.len() > limit {
            return Err(JsonTextError::Limit);
        }
    }
    Ok(output)
}
/// serde_json's byte-string visitor supplies WTF-8 for escaped lone surrogates.
/// Validate ordinary UTF-8 runs with std; only canonical surrogate triples extend it.
pub fn from_wtf8(bytes: &[u8]) -> Result<Vec<u16>, TextError> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == 0xed
            && (0xa0..=0xbf).contains(&bytes[index + 1])
            && (0x80..=0xbf).contains(&bytes[index + 2])
        {
            result.extend(
                core::str::from_utf8(&bytes[start..index])
                    .map_err(|_| TextError)?
                    .encode_utf16(),
            );
            result.push(
                ((bytes[index] as u16 & 15) << 12)
                    | ((bytes[index + 1] as u16 & 63) << 6)
                    | (bytes[index + 2] as u16 & 63),
            );
            index += 3;
            start = index;
        } else {
            index += 1;
        }
    }
    result.extend(
        core::str::from_utf8(&bytes[start..])
            .map_err(|_| TextError)?
            .encode_utf16(),
    );
    Ok(result)
}
/// Well-formed JSON: paired surrogates become their scalar; unpaired units are escaped.
#[cfg(feature = "std")]
pub fn write_json(writer: &mut impl std::io::Write, units: &[u16]) -> std::io::Result<()> {
    writer.write_all(b"\"")?;
    for value in char::decode_utf16(units.iter().copied()) {
        match value {
            Err(error) => write!(writer, "\\u{:04x}", error.unpaired_surrogate())?,
            Ok('"') => writer.write_all(b"\\\"")?,
            Ok('\\') => writer.write_all(b"\\\\")?,
            Ok('\u{8}') => writer.write_all(b"\\b")?,
            Ok('\u{c}') => writer.write_all(b"\\f")?,
            Ok('\n') => writer.write_all(b"\\n")?,
            Ok('\r') => writer.write_all(b"\\r")?,
            Ok('\t') => writer.write_all(b"\\t")?,
            Ok(c) if c < '\u{20}' => write!(writer, "\\u{:04x}", c as u32)?,
            Ok(c) => {
                writer.write_all(c.encode_utf8(&mut [0; 4]).as_bytes())?;
            }
        }
    }
    writer.write_all(b"\"")
}

/// ECMAScript WhiteSpace plus LineTerminator code units; U+0085 is excluded.
#[must_use]
pub fn ecmascript_whitespace(unit: u16) -> bool {
    matches!(unit,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_conversion_is_exact_and_resources_never_use_replacement() {
        let bytes = encode("aé😀\0");
        assert_eq!(units(&bytes).unwrap(), [97, 233, 0xd83d, 0xde00, 0]);
        assert_eq!(strict(&bytes).unwrap(), "aé😀\0");
        assert!(resource(&bytes).is_err());
        assert_eq!(resource(&encode("aé😀")).unwrap(), "aé😀");
        for data in [vec![0], vec![0, 0xd8], vec![0, 0xdc]] {
            assert!(resource(&data).is_err());
        }
        assert_eq!(
            from_wtf8(&[0xed, 0xa0, 0x80, 0x61, 0xed, 0xbf, 0xbf]).unwrap(),
            [0xd800, 97, 0xdfff]
        );
        for data in [
            vec![0xff],
            vec![0xc0, 0x80],
            vec![0xed, 0xa0],
            vec![0xed, 0xa0, 0x41],
            vec![0xf4, 0x90, 0x80, 0x80],
        ] {
            assert!(from_wtf8(&data).is_err());
        }
    }
    #[test]
    fn utf16_json_round_trips_every_code_unit_through_checked_schema() {
        use crate::json::{JsonSchema, JsonType, JsonValue, UTF16_SCHEMA_VERSION};
        let schema = JsonSchema {
            version: UTF16_SCHEMA_VERSION,
            root: 0,
            types: vec![JsonType::String],
        };
        // Include every u16, with separators so every surrogate is also tested unpaired.
        for unit in 0..=u16::MAX {
            let input = [unit, b'x' as u16];
            let mut encoded = Vec::new();
            write_json(&mut encoded, &input).unwrap();
            assert_eq!(
                schema.parse_json(&encoded).unwrap(),
                JsonValue::String16(input.to_vec())
            );
        }
        let mut encoded = Vec::new();
        write_json(&mut encoded, &[0xd83d, 0xde00]).unwrap();
        assert_eq!(encoded, "\"😀\"".as_bytes());
    }
    #[test]
    fn utf16_json_transport_preserves_tokens_and_rejects_invalid_escapes() {
        let raw = [34, 0xd800, 34]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(json_input(&raw).unwrap(), br#""\ud800""#);
        for input in [vec![0xd800], vec![34, 92, 0xd800, 34]] {
            assert!(
                json_input(
                    &input
                        .into_iter()
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>()
                )
                .is_err()
            );
        }
        let input = r#"{"x":"\\\"escaped","y":"\udfff"}"#;
        assert_eq!(json_input(&encode(input)).unwrap(), input.as_bytes());
    }
}
