//! Container admission only. Browser FontFace performs full table sanitization.
use super::resources::{MAX_DECODED_BYTES, MAX_RESOURCE_BYTES};
use crate::{ProjectError, Result};
use std::collections::BTreeSet;
const MAX_TABLES: usize = 256;
fn invalid() -> ProjectError {
    ProjectError::Usage("invalid or oversized browser font container".into())
}
fn word(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn short(bytes: &[u8], at: usize) -> usize {
    u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}
fn span(offset: usize, length: usize, total: usize) -> Result<usize> {
    offset
        .checked_add(length)
        .filter(|end| *end <= total)
        .ok_or_else(invalid)
}
fn flavor(bytes: &[u8]) -> bool {
    bytes == b"\0\x01\0\0" || bytes == b"OTTO"
}

pub(super) fn inspect(bytes: &[u8]) -> Result<(&'static str, &'static str, usize)> {
    if bytes.len() < 12 || bytes.len() > MAX_RESOURCE_BYTES {
        return Err(invalid());
    }
    if flavor(&bytes[..4]) {
        let count = short(bytes, 4);
        if !(1..=MAX_TABLES).contains(&count) || 12 + count * 16 > bytes.len() {
            return Err(invalid());
        }
        let mut tags = BTreeSet::new();
        let mut spans = Vec::new();
        let mut decoded = 12 + count * 16;
        for at in (12..12 + count * 16).step_by(16) {
            if !tags.insert(&bytes[at..at + 4]) {
                return Err(invalid());
            }
            let offset = word(bytes, at + 8);
            let length = word(bytes, at + 12);
            if offset < 12 + count * 16 || !offset.is_multiple_of(4) {
                return Err(invalid());
            }
            let end = span(offset, length, bytes.len())?;
            if length > 0 {
                spans.push((offset, end));
            }
            decoded = decoded.checked_add((length + 3) & !3).ok_or_else(invalid)?;
        }
        spans.sort_unstable();
        if spans.windows(2).any(|pair| pair[0].1 > pair[1].0) || decoded > MAX_DECODED_BYTES {
            return Err(invalid());
        }
        return Ok(if &bytes[..4] == b"OTTO" {
            ("font/otf", "otf", decoded)
        } else {
            ("font/ttf", "ttf", decoded)
        });
    }
    let woff2 = &bytes[..4] == b"wOF2";
    let header = if woff2 { 48 } else { 44 };
    if (&bytes[..4] != b"wOFF" && !woff2)
        || bytes.len() < header
        || !flavor(&bytes[4..8])
        || word(bytes, 8) != bytes.len()
        || short(bytes, 14) != 0
    {
        return Err(invalid());
    }
    let count = short(bytes, 12);
    let decoded = word(bytes, 16);
    if !(1..=MAX_TABLES).contains(&count)
        || decoded < 12 + count * 16
        || decoded > MAX_DECODED_BYTES
    {
        return Err(invalid());
    }
    if woff2 {
        if word(bytes, 20) == 0 || word(bytes, 20) > bytes.len() - header {
            return Err(invalid());
        }
        // The variable table/transform directory is checked by FontFace.
        Ok(("font/woff2", "woff2", decoded))
    } else {
        if header + count * 20 > bytes.len() {
            return Err(invalid());
        }
        let mut tags = BTreeSet::new();
        let mut spans = Vec::new();
        let mut actual = 12 + count * 16;
        for at in (header..header + count * 20).step_by(20) {
            if !tags.insert(&bytes[at..at + 4]) {
                return Err(invalid());
            }
            let offset = word(bytes, at + 4);
            let compressed = word(bytes, at + 8);
            let original = word(bytes, at + 12);
            if offset < header + count * 20
                || !offset.is_multiple_of(4)
                || compressed > original
                || original > MAX_DECODED_BYTES
            {
                return Err(invalid());
            }
            let end = span(offset, compressed, bytes.len())?;
            if compressed > 0 {
                spans.push((offset, end));
            }
            actual = actual
                .checked_add((original + 3) & !3)
                .ok_or_else(invalid)?;
        }
        spans.sort_unstable();
        if actual != decoded || spans.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(invalid());
        }
        Ok(("font/woff", "woff", decoded))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TTF: &[u8] = include_bytes!("../../tests/fixtures/browser-fonts/advance-400.ttf");
    #[test]
    fn font_containers_are_bounded_and_do_not_trust_table_offsets() {
        for bytes in [
            TTF,
            include_bytes!("../../tests/fixtures/browser-fonts/advance-400.otf").as_slice(),
            include_bytes!("../../tests/fixtures/browser-fonts/advance-400.woff").as_slice(),
            include_bytes!("../../tests/fixtures/browser-fonts/advance-400.woff2").as_slice(),
        ] {
            assert!(inspect(bytes).is_ok());
            assert!(inspect(&bytes[..bytes.len() - 1]).is_err());
        }
        let mut bad = TTF.to_vec();
        bad[20..24].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(inspect(&bad).is_err());
        let mut overlap = TTF.to_vec();
        let offset = overlap[20..24].to_vec();
        overlap[36..40].copy_from_slice(&offset);
        assert!(inspect(&overlap).is_err());
        let mut duplicate = TTF.to_vec();
        let tag = duplicate[12..16].to_vec();
        duplicate[28..32].copy_from_slice(&tag);
        assert!(inspect(&duplicate).is_err());
        let mut collection = TTF.to_vec();
        collection[..4].copy_from_slice(b"ttcf");
        assert!(inspect(&collection).is_err());
        let mut bomb =
            include_bytes!("../../tests/fixtures/browser-fonts/advance-400.woff2").to_vec();
        bomb[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(inspect(&bomb).is_err());
    }
}
