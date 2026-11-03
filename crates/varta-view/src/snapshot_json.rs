//! Bounded diagnostic JSON export. The view remains usable when export fails.
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFormat {
    Compact,
    Pretty,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("view snapshot JSON exceeds its {limit}-byte limit")]
    Limit { limit: usize },
    #[error("cannot allocate the view snapshot JSON buffer")]
    Allocation,
    #[error("cannot encode view snapshot JSON: {0}")]
    Encoding(serde_json::Error),
}

/// Encode an existing view snapshot without publishing a partial JSON document.
/// Both formats include escaping/keys/indentation in the same byte allowance.
pub fn encode_snapshot(
    snapshot: &serde_json::Value,
    format: SnapshotFormat,
) -> Result<Vec<u8>, SnapshotError> {
    encode(
        snapshot,
        format,
        varta_common::view::DRAFT_MAX_SNAPSHOT_JSON_BYTES,
    )
}

#[derive(Clone, Copy)]
enum Failure {
    Limit,
    Allocation,
}

struct Buffer {
    bytes: Vec<u8>,
    limit: usize,
    failure: Option<Failure>,
}

impl Buffer {
    fn fail(&mut self, failure: Failure) -> io::Error {
        self.failure = Some(failure);
        io::Error::from(match failure {
            Failure::Limit => io::ErrorKind::FileTooLarge,
            Failure::Allocation => io::ErrorKind::OutOfMemory,
        })
    }
}

impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(failure) = self.failure {
            return Err(self.fail(failure));
        }
        let Some(length) = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|length| *length <= self.limit)
        else {
            return Err(self.fail(Failure::Limit));
        };
        if length > self.bytes.capacity() {
            // Geometric growth avoids reallocating on every escaped character;
            // cap the requested capacity before reserving or copying anything.
            let capacity = length
                .max(self.bytes.capacity().saturating_mul(2))
                .max(1024)
                .min(self.limit);
            if self
                .bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .is_err()
            {
                return Err(self.fail(Failure::Allocation));
            }
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode(
    snapshot: &serde_json::Value,
    format: SnapshotFormat,
    limit: usize,
) -> Result<Vec<u8>, SnapshotError> {
    let mut buffer = Buffer {
        bytes: Vec::new(),
        limit,
        failure: None,
    };
    let result = match format {
        SnapshotFormat::Compact => serde_json::to_writer(&mut buffer, snapshot),
        SnapshotFormat::Pretty => serde_json::to_writer_pretty(&mut buffer, snapshot),
    };
    if let Err(error) = result {
        return Err(match buffer.failure {
            Some(Failure::Limit) => SnapshotError::Limit { limit },
            Some(Failure::Allocation) => SnapshotError::Allocation,
            None => SnapshotError::Encoding(error),
        });
    }
    Ok(buffer.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn encoded_boundary_counts_utf8_escaping_keys_and_both_formats() {
        for value in [
            json!(null),
            json!(42),
            json!(["é😀", "\u{1}\n\"\\"]),
            json!({"key\n":{"nested":[false,"界"]}}),
        ] {
            for format in [SnapshotFormat::Compact, SnapshotFormat::Pretty] {
                let expected = match format {
                    SnapshotFormat::Compact => serde_json::to_vec(&value).unwrap(),
                    SnapshotFormat::Pretty => serde_json::to_vec_pretty(&value).unwrap(),
                };
                assert_eq!(encode(&value, format, expected.len()).unwrap(), expected);
                assert!(matches!(
                    encode(&value, format, expected.len() - 1),
                    Err(SnapshotError::Limit { .. })
                ));
                assert_eq!(
                    encode(&value, format, expected.len() + 1).unwrap(),
                    expected
                );
            }
        }
    }
    #[test]
    fn refused_write_never_allocates_or_copies_an_oversized_fragment() {
        let mut buffer = Buffer {
            bytes: Vec::new(),
            limit: 7,
            failure: None,
        };
        assert!(buffer.write_all(&[b'x'; 8192]).is_err());
        assert!(buffer.bytes.is_empty());
        assert_eq!(buffer.bytes.capacity(), 0);
        assert!(buffer.write_all(b"x").is_err(), "failure is sticky");
    }
    #[test]
    fn incremental_writes_keep_requested_capacity_inside_a_non_power_of_two_limit() {
        let mut buffer = Buffer {
            bytes: Vec::new(),
            limit: 1027,
            failure: None,
        };
        for _ in 0..1027 {
            buffer.write_all(b"x").unwrap();
            assert!(buffer.bytes.capacity() <= 1027);
        }
        assert!(buffer.write_all(b"x").is_err());
        assert_eq!(buffer.bytes.len(), 1027);
    }
    #[test]
    fn public_encoder_enforces_the_shared_limit_at_the_exact_byte() {
        let limit = varta_common::view::DRAFT_MAX_SNAPSHOT_JSON_BYTES;
        let mut value = json!("a".repeat(limit - 2));
        assert_eq!(
            encode_snapshot(&value, SnapshotFormat::Compact)
                .unwrap()
                .len(),
            limit
        );
        if let serde_json::Value::String(text) = &mut value {
            text.push('a');
        }
        assert!(
            matches!(encode_snapshot(&value,SnapshotFormat::Compact),Err(SnapshotError::Limit{limit:l}) if l==limit)
        );
    }
}
