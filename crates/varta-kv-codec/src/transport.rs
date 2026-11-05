//! The byte layout of the browser host's calls into the codec module
//! (`varta_common::kv::codec`, `wire::codec` here): little-endian fields and
//! length-prefixed byte strings, and the staged-value record the Worker keeps
//! between a stage and the read or commit that consumes it.
use crate::{
    protocol::{Staged, Stored},
    wire::codec,
};
use alloc::{string::String, vec::Vec};

/// Appends fields to an output buffer.
#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    pub fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub fn f64(&mut self, value: f64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    /// A length-prefixed byte string.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.u32(bytes.len() as u32);
        self.0.extend_from_slice(bytes);
    }
    /// The staged record of `staged`.
    pub fn staged(&mut self, staged: &Staged) {
        match &staged.value {
            Ok(value) => {
                self.u8(value.encoding() as u8);
                self.bytes(&value.bytes());
            }
            Err(text) => {
                self.u8(codec::VALUE_FAILURE);
                self.bytes(text.as_bytes());
            }
        }
        match &staged.key {
            Ok(key) => {
                self.u8(codec::KEY_BYTES);
                self.bytes(key);
            }
            Err(text) => {
                self.u8(codec::KEY_FAILURE);
                self.bytes(text.as_bytes());
            }
        }
    }
}

/// Reads fields from an input buffer; `None` past its end.
pub struct Reader<'a>(pub &'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.0.len() < count {
            return None;
        }
        let (taken, rest) = self.0.split_at(count);
        self.0 = rest;
        Some(taken)
    }
    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }
    pub fn u8(&mut self) -> Option<u8> {
        Some(self.array::<1>()?[0])
    }
    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.array()?))
    }
    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.array()?))
    }
    pub fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.array()?))
    }
    /// A length-prefixed byte string.
    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let length = self.u32()? as usize;
        self.take(length)
    }
    /// The bytes after the read fields.
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        self.0
    }
    /// One staged record.
    pub fn staged(&mut self) -> Option<Staged> {
        let value = match self.u8()? {
            codec::VALUE_FAILURE => Err(String::from_utf8(self.bytes()?.to_vec()).ok()?),
            encoding => Ok(Stored::of(i64::from(encoding), self.bytes()?.to_vec())?),
        };
        let key = match self.u8()? {
            codec::KEY_BYTES => Ok(self.bytes()?.to_vec()),
            codec::KEY_FAILURE => Err(String::from_utf8(self.bytes()?.to_vec()).ok()?),
            _ => return None,
        };
        Some(Staged { value, key })
    }
    /// A staged table: the record count, then the records.
    pub fn table(&mut self) -> Option<Vec<Staged>> {
        let count = self.u32()? as usize;
        let mut table = Vec::with_capacity(count.min(crate::wire::MAX_STAGED));
        for _ in 0..count {
            table.push(self.staged()?);
        }
        Some(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn staged_records_round_trip() {
        let records = [
            Staged {
                value: Ok(Stored::V8(vec![0xff, 0x10, b'_'])),
                key: Ok(vec![2, b'a', 0]),
            },
            Staged {
                value: Ok(Stored::U64(u64::MAX)),
                key: Err("a KV key must be an array".into()),
            },
            Staged {
                value: Ok(Stored::Bytes(vec![])),
                key: Ok(vec![]),
            },
            Staged {
                value: Err("Value too large (max 65536 bytes)".into()),
                key: Ok(vec![2, b'k', 0]),
            },
        ];
        let mut writer = Writer::default();
        writer.u32(records.len() as u32);
        for record in &records {
            writer.staged(record);
        }
        writer.f64(1.5);
        let mut reader = Reader(&writer.0);
        assert_eq!(reader.table().unwrap(), records);
        assert_eq!(reader.f64(), Some(1.5));
        assert!(reader.rest().is_empty());
        assert_eq!(Reader(&[1, 0, 0]).u32(), None);
        // An unknown encoding or key kind is no record.
        assert_eq!(Reader(&[9, 0, 0, 0, 0, 0, 0, 0, 0, 0]).staged(), None);
    }
}
