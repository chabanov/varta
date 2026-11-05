//! Deno KV's key codec (`denokv_proto` 0.14.0 `encode_key`/`decode_key`, the
//! FoundationDB tuple layer subset Deno uses), byte for byte.
//!
//! Varta keys hold strings and numbers only ([`Part::String`],
//! [`Part::Float`]); a decoded key also reports the parts Deno can write
//! that Varta does not read ([`Part::Int`], [`Part::Bytes`], booleans), so
//! the decoder can name them. Integers are only measured, never built: their
//! digits are not needed to reject them.
use alloc::{string::String, vec::Vec};

const BYTES: u8 = 0x01;
const STRING: u8 = 0x02;
const NEGINTSTART: u8 = 0x0b;
const INTZERO: u8 = 0x14;
const POSINTEND: u8 = 0x1d;
const DOUBLE: u8 = 0x21;
const FALSE: u8 = 0x26;
const TRUE: u8 = 0x27;
/// The byte after an embedded zero byte of a string or bytes part.
const ESCAPE: u8 = 0xff;
/// The terminator of a string or bytes part.
const TERMINATOR: u8 = 0x00;
/// The longest integer whose length is folded into its tag.
const MAX_INLINE_INT_BYTES: u8 = 8;
/// The canonical NaNs a key stores, by sign (`canonicalize_f64`).
const CANONICAL_NAN_POSITIVE: u64 = 0x7ff8_0000_0000_0000;
const CANONICAL_NAN_NEGATIVE: u64 = 0xfff8_0000_0000_0000;
/// The sign bit of a binary64.
const SIGN_BIT: u64 = 1 << 63;

/// One decoded key part.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    String(String),
    Float(f64),
    /// A BigInt part (not read by Varta).
    Int,
    /// A `Uint8Array` part (not read by Varta).
    Bytes,
    Boolean(bool),
}

/// Why stored key bytes are not a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("malformed KV key")]
pub struct Malformed;

/// One encoded part Varta writes.
pub enum Encode<'a> {
    String(&'a str),
    Float(f64),
}

/// Encode the parts of a key.
#[must_use]
pub fn encode<'a>(parts: impl IntoIterator<Item = Encode<'a>>) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        match part {
            Encode::String(text) => {
                out.push(STRING);
                for &byte in text.as_bytes() {
                    out.push(byte);
                    if byte == TERMINATOR {
                        out.push(ESCAPE);
                    }
                }
                out.push(TERMINATOR);
            }
            Encode::Float(number) => {
                out.push(DOUBLE);
                let bits = if number.is_nan() {
                    if number.is_sign_negative() {
                        CANONICAL_NAN_NEGATIVE
                    } else {
                        CANONICAL_NAN_POSITIVE
                    }
                } else {
                    number.to_bits()
                };
                let ordered = if bits & SIGN_BIT != 0 {
                    bits ^ u64::MAX
                } else {
                    bits ^ SIGN_BIT
                };
                out.extend_from_slice(&ordered.to_be_bytes());
            }
        }
    }
    out
}

fn take<'a>(input: &mut &'a [u8], count: usize) -> Result<&'a [u8], Malformed> {
    if input.len() < count {
        return Err(Malformed);
    }
    let (taken, rest) = input.split_at(count);
    *input = rest;
    Ok(taken)
}

/// A string or bytes part: its unescaped bytes up to the terminator.
fn slice(input: &mut &[u8]) -> Result<Vec<u8>, Malformed> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < input.len() {
        let byte = input[at];
        at += 1;
        if byte == TERMINATOR {
            if input.get(at) == Some(&ESCAPE) {
                out.push(TERMINATOR);
                at += 1;
                continue;
            }
            *input = &input[at..];
            return Ok(out);
        }
        out.push(byte);
    }
    Err(Malformed)
}

/// Decode stored key bytes, failing on the whole key as Deno does.
pub fn decode(mut input: &[u8]) -> Result<Vec<Part>, Malformed> {
    let mut parts = Vec::new();
    while let Some((&tag, rest)) = input.split_first() {
        input = rest;
        parts.push(match tag {
            STRING => Part::String(String::from_utf8(slice(&mut input)?).map_err(|_| Malformed)?),
            BYTES => {
                slice(&mut input)?;
                Part::Bytes
            }
            NEGINTSTART => {
                let [length] = take(&mut input, 1)? else {
                    return Err(Malformed);
                };
                take(&mut input, usize::from(length ^ ESCAPE))?;
                Part::Int
            }
            POSINTEND => {
                let [length] = take(&mut input, 1)? else {
                    return Err(Malformed);
                };
                take(&mut input, usize::from(*length))?;
                Part::Int
            }
            tag if (INTZERO - MAX_INLINE_INT_BYTES..=INTZERO + MAX_INLINE_INT_BYTES)
                .contains(&tag) =>
            {
                take(&mut input, usize::from(tag.abs_diff(INTZERO)))?;
                Part::Int
            }
            DOUBLE => {
                let mut word = [0; size_of::<u64>()];
                word.copy_from_slice(take(&mut input, size_of::<u64>())?);
                let ordered = u64::from_be_bytes(word);
                Part::Float(f64::from_bits(if ordered & SIGN_BIT == 0 {
                    ordered ^ u64::MAX
                } else {
                    ordered ^ SIGN_BIT
                }))
            }
            FALSE => Part::Boolean(false),
            TRUE => Part::Boolean(true),
            _ => return Err(Malformed),
        });
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn strings_escape_zero_bytes_and_floats_sort_by_value() {
        assert_eq!(
            encode([Encode::String("a\0b")]),
            [STRING, b'a', 0, ESCAPE, b'b', 0]
        );
        let order = [f64::NEG_INFINITY, -1.5, -0.0, 0.0, 2.0, f64::INFINITY];
        let encoded: Vec<_> = order.iter().map(|n| encode([Encode::Float(*n)])).collect();
        assert!(encoded.windows(2).all(|pair| pair[0] < pair[1]));
        for (number, bytes) in order.iter().zip(&encoded) {
            assert_eq!(decode(bytes).unwrap(), [Part::Float(*number)]);
        }
        assert_eq!(
            decode(&encode([Encode::String("x"), Encode::Float(f64::NAN)])).unwrap()[0],
            Part::String("x".into())
        );
    }

    #[test]
    fn foreign_parts_are_named_and_truncation_is_malformed() {
        // Deno's encodings of 1n, -1n, 2^64n, a Uint8Array and booleans.
        let foreign = [
            (&[INTZERO + 1, 1][..], Part::Int),
            (&[INTZERO - 1, 0xfe][..], Part::Int),
            (&[POSINTEND, 9, 1, 0, 0, 0, 0, 0, 0, 0, 0][..], Part::Int),
            (
                &[NEGINTSTART, 9 ^ ESCAPE, 0, 0, 0, 0, 0, 0, 0, 0, 0][..],
                Part::Int,
            ),
            (&[INTZERO][..], Part::Int),
            (&[BYTES, 1, 0][..], Part::Bytes),
            (&[TRUE][..], Part::Boolean(true)),
            (&[FALSE][..], Part::Boolean(false)),
        ];
        for (bytes, part) in foreign {
            assert_eq!(decode(bytes).unwrap(), [part], "{bytes:?}");
        }
        for bytes in [
            &[STRING, b'a'][..],
            &[DOUBLE, 0, 0],
            &[INTZERO + 2, 1],
            &[POSINTEND],
            &[0x7f],
            &[STRING, 0xff, 0],
        ] {
            assert_eq!(decode(bytes), Err(Malformed), "{bytes:?}");
        }
    }
}
