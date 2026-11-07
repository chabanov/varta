//! Value18 record display trailers (v38 `node_console`).
//!
//! Every value18 record descriptor ends, after its entries and a wide
//! record's edge bitmap, with a display trailer: a kind word, a name length
//! word and the name's UTF-8 bytes. The logged notation (`util.inspect`)
//! reads it to print `ClassName { … }` for a class instance and to recognise
//! the engine-owned layouts that Node prints in a notation of their own
//! (Error objects, `Map`, `Set`, `Date`). A structural record has the
//! [`RecordDisplay::Object`] kind and no name. The trailer is additive:
//! readers that do not display a record read the descriptor prefix
//! ([`descriptor_bytes`]) exactly as before, and earlier layouts have no
//! trailer.
use super::*;
use crate::memory::Memory;

/// Bytes of a display trailer before its name: the kind and the name length.
pub const RECORD_DISPLAY_BYTES: usize = 8;

/// What a record descriptor's display trailer says about its records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RecordDisplay {
    /// A structural record (object literal, interface, decoded JSON); no name.
    Object = 0,
    /// An instance of the named class.
    Class = 1,
    /// An instance of the named Error class (an Error subclass or a native
    /// family root): its `#name`, `#message` and `#cause` slots are the
    /// Error object's own state.
    Error = 2,
    /// The engine's `AggregateError`: an Error whose `errors` field is the
    /// non-enumerable own property of ECMA-262 §20.5.7.
    AggregateError = 3,
    /// The engine's `Map` storage (`keys` and `values` vectors, holes for
    /// deleted entries).
    Map = 4,
    /// The engine's `Set` storage (a `keys` vector, holes for deleted
    /// entries).
    Set = 5,
    /// The engine's `Date` (its `time` field is the time value).
    Date = 6,
}
impl RecordDisplay {
    /// Every kind, in tag order.
    pub const ALL: [Self; 7] = [
        Self::Object,
        Self::Class,
        Self::Error,
        Self::AggregateError,
        Self::Map,
        Self::Set,
        Self::Date,
    ];
    #[must_use]
    pub fn from_tag(tag: u32) -> Option<Self> {
        Self::ALL.get(tag as usize).copied()
    }
    /// Whether records of this kind are Error objects.
    #[must_use]
    pub fn is_error(self) -> bool {
        matches!(self, Self::Error | Self::AggregateError)
    }
}

/// Whether record descriptors of layout `version` end with a display
/// trailer: value18 only.
#[must_use]
pub fn display_layout(version: u32) -> bool {
    version == HASHED_DICTIONARY_VALUE_VERSION
}

/// The display trailer bytes of a descriptor: kind, name length and name.
#[must_use]
pub fn display_trailer(display: RecordDisplay, name: &str) -> alloc::vec::Vec<u8> {
    let mut bytes = alloc::vec::Vec::with_capacity(RECORD_DISPLAY_BYTES + name.len());
    bytes.extend((display as u32).to_le_bytes());
    bytes.extend((name.len() as u32).to_le_bytes());
    bytes.extend(name.as_bytes());
    bytes
}

/// The display kind and the static address and length of the name of the
/// value18 descriptor at `descriptor` with `count` fields: the trailer and
/// its name lie in the module's static region, the kind is known, a named
/// kind has a nonempty UTF-8 name of at most [`MAX_RECORD_KEY_BYTES`] and a
/// structural record none. The caller has validated the descriptor itself.
pub fn record_display<M: Memory + ?Sized>(
    data: &M,
    descriptor: usize,
    count: usize,
    module: &ModuleBounds,
) -> Result<(RecordDisplay, usize, usize), ValueAbiError> {
    let invalid = || ValueAbiError("noncanonical record display trailer".into());
    let at = descriptor
        .checked_add(descriptor_bytes(count))
        .ok_or_else(invalid)?;
    if !module.static_region.contains(at, RECORD_DISPLAY_BYTES) {
        return Err(invalid());
    }
    let word = |at: usize| data.u32_at(at).ok_or_else(invalid);
    let display = RecordDisplay::from_tag(word(at)?).ok_or_else(invalid)?;
    let len = word(at + 4)? as usize;
    let name = at + RECORD_DISPLAY_BYTES;
    if (display == RecordDisplay::Object) != (len == 0)
        || len > MAX_RECORD_KEY_BYTES
        || !module.static_region.contains(name, len)
    {
        return Err(invalid());
    }
    let bytes = data.borrow(name, len).ok_or_else(invalid)?;
    core::str::from_utf8(&bytes).map_err(|_| invalid())?;
    Ok((display, name, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module() -> ModuleBounds {
        ModuleBounds::v38(crate::abi::StaticRegion::SINGLE_PAGE)
    }

    #[test]
    fn trailers_round_trip_and_reject_noncanonical_names() {
        let descriptor = 64;
        let mut data = alloc::vec![0u8; 512];
        let at = descriptor + descriptor_bytes(1);
        let write = |data: &mut alloc::vec::Vec<u8>, bytes: &[u8]| {
            data[at..at + bytes.len()].copy_from_slice(bytes);
        };
        write(&mut data, &display_trailer(RecordDisplay::Class, "Point"));
        let (display, name, len) =
            record_display(data.as_slice(), descriptor, 1, &module()).unwrap();
        assert_eq!(display, RecordDisplay::Class);
        assert_eq!(&data[name..name + len], b"Point");
        write(&mut data, &display_trailer(RecordDisplay::Object, ""));
        assert_eq!(
            record_display(data.as_slice(), descriptor, 1, &module())
                .unwrap()
                .0,
            RecordDisplay::Object
        );
        // A named kind needs a name, a structural record has none, and an
        // unknown kind or invalid UTF-8 is rejected.
        write(&mut data, &display_trailer(RecordDisplay::Class, ""));
        assert!(record_display(data.as_slice(), descriptor, 1, &module()).is_err());
        write(&mut data, &[0, 0, 0, 0, 1, 0, 0, 0, b'x']);
        assert!(record_display(data.as_slice(), descriptor, 1, &module()).is_err());
        write(&mut data, &[7, 0, 0, 0, 1, 0, 0, 0, b'x']);
        assert!(record_display(data.as_slice(), descriptor, 1, &module()).is_err());
        write(&mut data, &[1, 0, 0, 0, 1, 0, 0, 0, 0xff]);
        assert!(record_display(data.as_slice(), descriptor, 1, &module()).is_err());
    }
}
