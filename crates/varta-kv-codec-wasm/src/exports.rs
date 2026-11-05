//! The exported calls (`varta_common::kv::codec`): each reads its input
//! from the arena, runs the shared value layer and leaves its output there.
use crate::{arena, host};
use alloc::{boxed::Box, format, string::String, vec, vec::Vec};
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering::Relaxed};
use varta_common::{
    abi::StaticRegion,
    json::{JsonSchema, SchemaBounds},
    value::ModuleBounds,
};
use varta_kv_codec::{
    charge,
    guest::{self, Entry},
    protocol::{self, KvError, Stored},
    stage::{StageError, Stager},
    transport::{Reader, Writer},
    wire::{self, StageMode, codec},
};

/// The current call's input and output in the arena.
static INPUT: AtomicUsize = AtomicUsize::new(0);
static INPUT_LENGTH: AtomicUsize = AtomicUsize::new(0);
static OUTPUT: AtomicUsize = AtomicUsize::new(0);
static OUTPUT_LENGTH: AtomicUsize = AtomicUsize::new(0);
/// The guest module's static region (`kv_configure`).
static EXTENSION_PAGES: AtomicU32 = AtomicU32::new(0);
static INTRINSIC_PAGES: AtomicU32 = AtomicU32::new(0);

/// The guest module's bounds: a v38 module and its static region.
fn bounds() -> Option<ModuleBounds> {
    let region = StaticRegion::with_extension_pages(EXTENSION_PAGES.load(Relaxed))?
        .with_intrinsic_pages(INTRINSIC_PAGES.load(Relaxed))?;
    Some(ModuleBounds::v38(region))
}

fn input() -> &'static [u8] {
    let (ptr, len) = (INPUT.load(Relaxed), INPUT_LENGTH.load(Relaxed));
    if len == 0 {
        return &[];
    }
    // SAFETY: `kv_input` reserved this arena range for the current call;
    // the arena is reset only by the next `kv_input`.
    unsafe { core::slice::from_raw_parts(ptr as *const u8, len) }
}

/// Leave `bytes` as the call's output and return `status`.
fn output(status: u32, bytes: Vec<u8>) -> u32 {
    let bytes = Box::leak(bytes.into_boxed_slice());
    OUTPUT.store(bytes.as_ptr() as usize, Relaxed);
    OUTPUT_LENGTH.store(bytes.len(), Relaxed);
    status
}

fn fatal(message: &str) -> u32 {
    output(codec::FATAL, message.as_bytes().to_vec())
}

fn malformed_input() -> u32 {
    fatal("kv: malformed codec input")
}

fn failed(error: &KvError) -> u32 {
    output(codec::FAILED, error.text().into_bytes())
}

fn failure(failure: host::Failure) -> u32 {
    match failure {
        host::Failure::Fatal(message) => fatal(&message),
        host::Failure::Deadline => output(codec::DEADLINE, Vec::new()),
    }
}

/// Start a call: reset the arena and reserve `len` input bytes.
#[unsafe(no_mangle)]
pub extern "C" fn kv_input(len: u32) -> u32 {
    arena::reset();
    OUTPUT_LENGTH.store(0, Relaxed);
    let buffer = Box::leak(vec![0u8; len as usize].into_boxed_slice());
    INPUT.store(buffer.as_mut_ptr() as usize, Relaxed);
    INPUT_LENGTH.store(buffer.len(), Relaxed);
    buffer.as_mut_ptr() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn kv_output() -> u32 {
    OUTPUT.load(Relaxed) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn kv_output_len() -> u32 {
    OUTPUT_LENGTH.load(Relaxed) as u32
}

/// The module's `varta.static` and `varta.intrinsics` payloads.
#[unsafe(no_mangle)]
pub extern "C" fn kv_configure() -> u32 {
    let mut reader = Reader(input());
    let (Some(extension), Some(intrinsics)) = (reader.bytes(), reader.bytes()) else {
        return malformed_input();
    };
    let region = if extension.is_empty() {
        Ok(StaticRegion::SINGLE_PAGE)
    } else {
        StaticRegion::read_section(extension)
    };
    let region = region.and_then(|region| {
        if intrinsics.is_empty() {
            Ok(region)
        } else {
            region.read_intrinsic_section(intrinsics)
        }
    });
    match region {
        Ok(region) => {
            EXTENSION_PAGES.store(region.extension_pages(), Relaxed);
            INTRINSIC_PAGES.store(region.intrinsic_pages(), Relaxed);
            output(codec::OK, Vec::new())
        }
        Err(error) => fatal(&format!("kv: {error}")),
    }
}

/// Stage one guest value (`kv_stage`).
#[unsafe(no_mangle)]
pub extern "C" fn kv_stage(mode: i32, kind: i32, packed: i64) -> u32 {
    let Some(mode) = StageMode::from_code(mode) else {
        return fatal("kv: stage mode");
    };
    let Some(module) = bounds() else {
        return malformed_input();
    };
    let memory = host::GuestMemory;
    let mut stager = Stager::new(&memory, module, &host::Turn);
    let value = match stager.value(kind as u32, packed as u64, 0) {
        Ok(value) => value,
        Err(StageError::Unsupported(message)) => return failed(&KvError::type_error(message)),
        Err(StageError::Fatal(message)) => return fatal(&message),
        Err(StageError::Abi(error)) => return fatal(&format!("{error}")),
    };
    let staged = match protocol::staged_value(mode, &value, stager.extent()) {
        Ok(staged) => staged,
        Err(error) => return failed(&error),
    };
    let mut writer = Writer::default();
    writer.u64(charge::staged(staged.size()));
    writer.u32(staged.size() as u32);
    writer.staged(&staged);
    output(codec::OK, writer.0)
}

/// The staged table and the descriptor text of a read or commit input.
fn table_and_descriptor() -> Option<(Vec<protocol::Staged>, String)> {
    let mut reader = Reader(input());
    let table = reader.table()?;
    let text = core::str::from_utf8(reader.rest()).ok()?;
    Some((table, String::from(text)))
}

/// Parse a read descriptor (`kv_read`).
#[unsafe(no_mangle)]
pub extern "C" fn kv_parse_read() -> u32 {
    let Some((table, text)) = table_and_descriptor() else {
        return malformed_input();
    };
    let read = match protocol::parse_read(&text, &table) {
        Ok(Ok(read)) => read,
        Ok(Err(error)) => return failed(&error),
        Err(protocol::Fatal(message)) => return fatal(&format!("kv: {message}")),
    };
    let mut writer = Writer::default();
    writer.u32(read.id);
    writer.u32(read.ranges.len() as u32);
    for range in &read.ranges {
        writer.bytes(&range.start);
        writer.bytes(&range.end);
        writer.u32(range.limit.get());
        writer.u8(u8::from(range.reverse));
        writer.bytes(&range.prefix);
    }
    output(codec::OK, writer.0)
}

/// The fuel, cursors and versionstamp texts of a read's entries.
#[unsafe(no_mangle)]
pub extern "C" fn kv_read_entries() -> u32 {
    let mut reader = Reader(input());
    let Some(prefixes) = (|| {
        let count = reader.u32()? as usize;
        let mut prefixes = Vec::with_capacity(count.min(wire::MAX_READ_RANGES));
        for _ in 0..count {
            prefixes.push(reader.bytes()?);
        }
        Some(prefixes)
    })() else {
        return malformed_input();
    };
    let mut entries = Vec::new();
    let parsed = (|| {
        let count = reader.u32()? as usize;
        for _ in 0..count {
            let range = reader.u32()? as usize;
            let key = reader.bytes()?;
            let value = reader.u32()? as usize;
            let mut versionstamp = protocol::Versionstamp::default();
            for byte in &mut versionstamp {
                *byte = reader.u8()?;
            }
            entries.push((prefixes.get(range)?, key, value, versionstamp));
        }
        Some(())
    })();
    if parsed.is_none() {
        return malformed_input();
    }
    let mut writer = Writer::default();
    writer.u64(charge::returned(
        entries.iter().map(|(_, key, value, _)| key.len() + value),
    ));
    for (prefix, key, _, versionstamp) in &entries {
        writer.bytes(protocol::cursor(prefix, key).as_bytes());
        writer.bytes(protocol::versionstamp_text(versionstamp).as_bytes());
    }
    output(codec::OK, writer.0)
}

/// Parse a commit descriptor (`kv_commit`) with the KV clock at `now_ms`.
#[unsafe(no_mangle)]
pub extern "C" fn kv_parse_commit(now_ms: f64) -> u32 {
    let Some((table, text)) = table_and_descriptor() else {
        return malformed_input();
    };
    let now = now_ms as i64;
    let commit = match protocol::parse_commit(&text, &table, |milliseconds| {
        protocol::expire_at_ms(now, milliseconds)
    }) {
        Ok(Ok(commit)) => commit,
        Ok(Err(error)) => return failed(&error),
        Err(protocol::Fatal(message)) => return fatal(&format!("kv: {message}")),
    };
    let mut writer = Writer::default();
    writer.u32(commit.id);
    writer.u64(charge::committed(commit.written()));
    writer.u32(commit.checks.len() as u32);
    for check in &commit.checks {
        writer.bytes(&check.key);
        match check.versionstamp {
            Some(versionstamp) => {
                writer.u8(codec::PRESENT);
                versionstamp.iter().for_each(|byte| writer.u8(*byte));
            }
            None => writer.u8(codec::ABSENT),
        }
    }
    writer.u32(commit.mutations.len() as u32);
    for mutation in &commit.mutations {
        let code = match &mutation.kind {
            protocol::Kind::Set(_) => wire::MutationCode::Set,
            protocol::Kind::Delete => wire::MutationCode::Delete,
            protocol::Kind::Sum(_) => wire::MutationCode::Sum,
            protocol::Kind::Min(_) => wire::MutationCode::Min,
            protocol::Kind::Max(_) => wire::MutationCode::Max,
        };
        writer.u8(code as u8);
        writer.bytes(&mutation.key);
        match &mutation.kind {
            protocol::Kind::Set(value) => {
                writer.u8(value.encoding() as u8);
                writer.bytes(&value.bytes());
            }
            protocol::Kind::Delete => {}
            protocol::Kind::Sum(number)
            | protocol::Kind::Min(number)
            | protocol::Kind::Max(number) => {
                writer.u64(*number);
            }
        }
        match mutation.expire_at {
            Some(at) => {
                writer.u8(codec::PRESENT);
                writer.f64(at as f64);
            }
            None => writer.u8(codec::ABSENT),
        }
    }
    output(codec::OK, writer.0)
}

/// Decode an entry's key (`key` = 1) or value into the checked schema at
/// `schema_ptr` (`kv_key`, `kv_value`).
#[unsafe(no_mangle)]
pub extern "C" fn kv_decode(key: u32, schema_ptr: i32, schema_len: i32) -> u32 {
    let Some(module) = bounds() else {
        return malformed_input();
    };
    let mut reader = Reader(input());
    let entry_bytes;
    let stored;
    let entry = match reader.u8() {
        Some(codec::ENTRY_NONE) => None,
        Some(codec::ENTRY_KEY) if key != 0 => {
            entry_bytes = reader.rest();
            Some(Entry::Key(entry_bytes))
        }
        Some(codec::ENTRY_VALUE) if key == 0 => {
            let Some(value) = reader
                .u8()
                .and_then(|encoding| Stored::of(i64::from(encoding), reader.rest().to_vec()))
            else {
                return malformed_input();
            };
            stored = value;
            Some(Entry::Value(&stored))
        }
        _ => return malformed_input(),
    };
    let mut guest = host::Guest::new();
    let decoded = guest::decode(
        &mut guest,
        module,
        &host::Turn,
        (schema_ptr, schema_len),
        entry,
        |bytes, limits| {
            JsonSchema::read(bytes, SchemaBounds::of(limits), limits.schema_bytes)
                .map_err(|error| format!("{error}"))
        },
    );
    match decoded {
        Ok(handle) => {
            let mut writer = Writer::default();
            writer.u64(handle);
            output(codec::OK, writer.0)
        }
        Err(error) => failure(error),
    }
}
