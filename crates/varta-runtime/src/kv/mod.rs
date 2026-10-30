//! Deno KV compatible storage (v38 `kv`): the `kv_*` host operations.
//!
//! The runtime holds database ids, staged keys and values (already encoded)
//! and the entries of the last read, never a guest reference. Reads and
//! commits run on the `denokv_sqlite` worker of the database and the host
//! call waits for them within the turn (a commit only until its cutoff, see
//! `store`): the operations are synchronous host calls, and the prelude's
//! promises settle in the same turn. Every failure the guest can cause is recoverable (`kv::FAILED` and
//! `kv_text`); malformed descriptors and ids are fatal, because only a
//! tampered module produces them.
mod decode;
mod stage;
pub(crate) mod store;
pub(crate) mod v8;

use crate::{RuntimeContext, permission::Decision};
use denokv_proto::{KeyPart, KvValue, MutationKind};
use std::collections::BTreeMap;
use store::{Database, KvError, Wait};
use varta_common::{
    HostOperation,
    kv::{self, EntryField, MutationCode, Selector, StageMode},
};
use wasmtime::{Caller, Extern, Linker};

fn fatal(message: impl std::fmt::Display) -> wasmtime::Error {
    wasmtime::Error::msg(format!("kv: {message}"))
}

/// One staged guest value, encoded when it was staged.
struct Staged {
    /// The stored form of a value.
    value: KvValue,
    /// The key bytes, when the value is a key (an array of strings and
    /// numbers), or why it is none.
    key: Result<Vec<u8>, String>,
}

impl Staged {
    fn size(&self) -> usize {
        let value = match &self.value {
            KvValue::V8(bytes) | KvValue::Bytes(bytes) => bytes.len(),
            KvValue::U64(_) => size_of::<u64>(),
        };
        value + self.key.as_ref().map_or(0, Vec::len)
    }
}

/// One entry of the last read.
struct Entry {
    key: Vec<u8>,
    value: KvValue,
    versionstamp: denokv_proto::Versionstamp,
    range: usize,
    cursor: String,
}

/// The KV state of one instance.
#[derive(Default)]
pub(crate) struct Kv {
    databases: BTreeMap<u32, Database>,
    next_id: u32,
    staged: Vec<Staged>,
    staged_bytes: usize,
    entries: Vec<Entry>,
    /// The last outcome: a versionstamp, or `<ErrorName>\n<message>`.
    text: String,
    runtime: Option<tokio::runtime::Runtime>,
}

impl Kv {
    fn fail(&mut self, error: &KvError) -> f64 {
        self.text = format!(
            "{}{}{}",
            error.name,
            kv::DESCRIPTOR_SEPARATOR,
            error.message
        );
        self.staged.clear();
        self.staged_bytes = 0;
        kv::FAILED
    }

    /// Run `operation` on database `id` with the instance's waiting runtime
    /// (a current-thread Tokio runtime with a timer, created by the first
    /// read or commit).
    fn run<T>(
        &mut self,
        id: u32,
        operation: impl FnOnce(&mut Database, &tokio::runtime::Runtime) -> T,
    ) -> Result<T, wasmtime::Error> {
        if self.runtime.is_none() {
            self.runtime = Some(
                tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                    .map_err(fatal)?,
            );
        }
        let Self {
            databases, runtime, ..
        } = self;
        let runtime = runtime.as_ref().ok_or_else(|| fatal("runtime"))?;
        let database = databases
            .get_mut(&id)
            .ok_or_else(|| fatal("unknown database id"))?;
        Ok(operation(database, runtime))
    }

    /// Take the staged value `id` of the current descriptor.
    fn staged(&self, id: &str) -> Result<&Staged, wasmtime::Error> {
        let index: usize = id.parse().map_err(|_| fatal("stage id"))?;
        self.staged
            .get(index)
            .ok_or_else(|| fatal("unknown stage id"))
    }

    fn key(&self, id: &str) -> Result<Result<Vec<u8>, KvError>, wasmtime::Error> {
        Ok(self.staged(id)?.key.clone().map_err(KvError::type_error))
    }
}

/// A database id the guest passes back.
fn id_of(value: f64) -> Option<u32> {
    (value.fract() == 0.0 && value >= 1.0 && value <= f64::from(u32::MAX)).then_some(value as u32)
}

/// The descriptor fields after the version line.
fn fields(text: &str) -> Result<std::str::Split<'_, char>, wasmtime::Error> {
    let mut fields = text.split(kv::DESCRIPTOR_SEPARATOR);
    if fields.next() != Some(kv::DESCRIPTOR_VERSION) {
        return Err(fatal("descriptor version"));
    }
    Ok(fields)
}

fn next<'a>(fields: &mut impl Iterator<Item = &'a str>) -> Result<&'a str, wasmtime::Error> {
    fields.next().ok_or_else(|| fatal("descriptor ends early"))
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, wasmtime::Error> {
    text.parse().map_err(|_| fatal("descriptor number"))
}

fn descriptor(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<String, wasmtime::Error> {
    crate::host::input(caller, ptr, len, kv::MAX_DESCRIPTOR_BYTES, true)
        .map_err(|error| fatal(error.message))
}

/// The broker's decision on the exact grant `capability` for the path of
/// open database `id`; a denial is logged.
fn granted(
    context: &mut RuntimeContext,
    id: u32,
    capability: &str,
) -> Result<Result<(), KvError>, wasmtime::Error> {
    let path = context
        .kv
        .databases
        .get(&id)
        .ok_or_else(|| fatal("unknown database id"))?
        .path
        .clone();
    if let Decision::Denied = context.broker.database(&context.policy, capability, &path) {
        let error = KvError::denied(capability, &path);
        context.emit(format!("varta log: {}", error.message))?;
        return Ok(Err(error));
    }
    Ok(Ok(()))
}

fn open(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<f64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let path = descriptor(caller, ptr, len)?;
    let context = caller.data_mut();
    let target = match context
        .broker
        .database(&context.policy, kv::READ_CAPABILITY, &path)
    {
        Decision::Granted(target) => target,
        Decision::Denied => {
            let error = KvError::denied(kv::READ_CAPABILITY, &path);
            context.emit(format!("varta log: {}", error.message))?;
            return Ok(context.kv.fail(&error));
        }
    };
    let writable = context
        .policy
        .database(kv::WRITE_CAPABILITY, &path)
        .is_some();
    if context.kv.databases.len() == kv::MAX_OPEN_DATABASES {
        return Ok(context.kv.fail(&KvError::type_error(format!(
            "at most {} KV databases are open at once",
            kv::MAX_OPEN_DATABASES
        ))));
    }
    match store::open(target, &path, writable) {
        Ok(database) => {
            context.kv.next_id += 1;
            let id = context.kv.next_id;
            context.kv.databases.insert(id, database);
            Ok(f64::from(id))
        }
        Err(error) => Ok(context.kv.fail(&error)),
    }
}

fn stage(
    caller: &mut Caller<'_, RuntimeContext>,
    mode: i32,
    kind: i32,
    packed: u64,
) -> Result<f64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let mode = StageMode::from_code(mode).ok_or_else(|| fatal("stage mode"))?;
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(fatal("missing memory"));
    };
    let context = caller.data();
    let staged = {
        let mut stager =
            stage::Stager::new(memory.data(&*caller), context.module, context.deadline);
        stager.value(kind as u32, packed, 0)
    };
    let context = caller.data_mut();
    if context.kv.staged.len() == kv::MAX_STAGED {
        return Err(fatal("too many staged values"));
    }
    let value = match staged {
        Ok(value) => value,
        Err(stage::StageError::Unsupported(message)) => {
            return Ok(context.kv.fail(&KvError::type_error(message)));
        }
        Err(stage::StageError::Fatal(error)) => return Err(error),
    };
    let staged = match staged_value(mode, &value) {
        Ok(staged) => staged,
        Err(error) => return Ok(context.kv.fail(&error)),
    };
    context.kv.staged_bytes += staged.size();
    if context.kv.staged_bytes > kv::MAX_STAGED_BYTES {
        return Ok(context.kv.fail(&KvError::type_error(
            "the staged keys and values of one KV operation are too large",
        )));
    }
    context.kv.staged.push(staged);
    Ok((context.kv.staged.len() - 1) as f64)
}

/// The stored form of `value` staged in `mode`.
fn staged_value(mode: StageMode, value: &v8::Value) -> Result<Staged, KvError> {
    let text = || match value {
        v8::Value::String(units) => Ok(units),
        _ => Err(KvError::type_error("invalid staged KV value")),
    };
    let value_form =
        match mode {
            StageMode::Value => {
                KvValue::V8(v8::encode(value, kv::MAX_VALUE_BYTES).map_err(|_| value_too_large())?)
            }
            StageMode::U64 => {
                let digits = String::from_utf16(text()?)
                    .map_err(|_| KvError::type_error("invalid KvU64"))?;
                KvValue::U64(digits.parse().map_err(|_| {
                    KvError::range_error("Value must fit in a 64-bit unsigned integer")
                })?)
            }
            StageMode::Bytes => KvValue::Bytes(byte_values(value)?),
        };
    Ok(Staged {
        value: value_form,
        key: key_bytes(value),
    })
}

/// The bytes of a staged `Uint8Array`: its storage, an array of byte
/// values.
fn byte_values(value: &v8::Value) -> Result<Vec<u8>, KvError> {
    let invalid = || KvError::type_error("invalid staged bytes");
    let v8::Value::Object(object) = value else {
        return Err(invalid());
    };
    let v8::Object::Dense(elements) = &**object else {
        return Err(invalid());
    };
    elements
        .iter()
        .map(|element| match element {
            Some(v8::Value::Number(byte))
                if byte.fract() == 0.0 && (0.0..=f64::from(u8::MAX)).contains(byte) =>
            {
                Ok(*byte as u8)
            }
            _ => Err(invalid()),
        })
        .collect()
}

fn value_too_large() -> KvError {
    KvError::type_error(format!(
        "Value too large (max {} bytes)",
        kv::MAX_VALUE_BYTES
    ))
}

/// The encoded key of an array of strings and numbers (Deno's key parts
/// `string` and `number`).
fn key_bytes(value: &v8::Value) -> Result<Vec<u8>, String> {
    let v8::Value::Object(object) = value else {
        return Err("a KV key must be an array".into());
    };
    let v8::Object::Dense(elements) = &**object else {
        return Err("a KV key must be an array".into());
    };
    let mut parts = Vec::with_capacity(elements.len());
    for element in elements {
        parts.push(match element {
            Some(v8::Value::String(units)) => KeyPart::String(
                String::from_utf16(units)
                    .map_err(|_| "a KV key string must be valid UTF-16".to_owned())?,
            ),
            Some(v8::Value::Number(number)) => KeyPart::Float(*number),
            _ => return Err("a KV key part must be a string or a number".into()),
        });
    }
    denokv_proto::encode_key(&denokv_proto::Key(parts)).map_err(|error| error.to_string())
}

/// One parsed read range: its bounds, limit, direction and the common
/// prefix its cursors are relative to.
struct Range {
    read: denokv_proto::ReadRange,
    prefix: Vec<u8>,
}

fn read(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<f64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let text = descriptor(caller, ptr, len)?;
    let context = caller.data_mut();
    let mut fields = fields(&text)?;
    let id = id_of(number(next(&mut fields)?)?).ok_or_else(|| fatal("database id"))?;
    let count: usize = number(next(&mut fields)?)?;
    if count > kv::MAX_READ_RANGES {
        return Ok(context.kv.fail(&KvError::type_error(format!(
            "Too many ranges (max {})",
            kv::MAX_READ_RANGES
        ))));
    }
    let mut ranges = Vec::with_capacity(count);
    let mut total = 0usize;
    for _ in 0..count {
        let range = match parse_range(&context.kv, &mut fields)? {
            Ok(range) => range,
            Err(error) => return Ok(context.kv.fail(&error)),
        };
        total += range.read.limit.get() as usize;
        ranges.push(range);
    }
    if fields.next().is_some() {
        return Err(fatal("descriptor has trailing fields"));
    }
    if total > kv::MAX_READ_ENTRIES {
        return Ok(context.kv.fail(&KvError::type_error(format!(
            "Too many entries (max {})",
            kv::MAX_READ_ENTRIES
        ))));
    }
    context.kv.staged.clear();
    context.kv.staged_bytes = 0;
    if let Err(error) = granted(context, id, kv::READ_CAPABILITY)? {
        return Ok(context.kv.fail(&error));
    }
    let deadline = context.deadline;
    let prefixes: Vec<Vec<u8>> = ranges.iter().map(|range| range.prefix.clone()).collect();
    let reads = ranges.into_iter().map(|range| range.read).collect();
    let outcome = context.kv.run(id, |database, runtime| {
        database.read(runtime, deadline, reads)
    })?;
    match outcome {
        Ok(outputs) => {
            context.kv.entries.clear();
            for (range, output) in outputs.into_iter().enumerate() {
                for entry in output.entries {
                    let cursor = cursor(&prefixes[range], &entry.key);
                    context.kv.entries.push(Entry {
                        key: entry.key,
                        value: entry.value,
                        versionstamp: entry.versionstamp,
                        range,
                        cursor,
                    });
                }
            }
            Ok(context.kv.entries.len() as f64)
        }
        Err(Wait::Failed(error)) => Ok(context.kv.fail(&error)),
        Err(Wait::Deadline) => Err(context.deadline_error()),
    }
}

/// Parse one range of a read descriptor (Deno's `RawSelector` rules).
fn parse_range<'a>(
    state: &Kv,
    fields: &mut impl Iterator<Item = &'a str>,
) -> Result<Result<Range, KvError>, wasmtime::Error> {
    let selector = Selector::from_code(number(next(fields)?)?).ok_or_else(|| fatal("selector"))?;
    let first = next(fields)?;
    let second = next(fields)?;
    let limit: u32 = number(next(fields)?)?;
    let reverse = match next(fields)? {
        "0" => false,
        "1" => true,
        _ => return Err(fatal("reverse flag")),
    };
    let cursor = next(fields)?;
    if selector.has_second() == second.is_empty() {
        return Err(fatal("selector keys"));
    }
    let first = match state.key(first)? {
        Ok(key) => key,
        Err(error) => return Ok(Err(error)),
    };
    let second = if selector.has_second() {
        match state.key(second)? {
            Ok(key) => Some(key),
            Err(error) => return Ok(Err(error)),
        }
    } else {
        None
    };
    let Some(limit) = std::num::NonZeroU32::new(limit) else {
        return Ok(Err(KvError::type_error("limit must be greater than 0")));
    };
    let with = |key: &[u8], byte: u8| key.iter().copied().chain(Some(byte)).collect::<Vec<u8>>();
    // (start, end, prefix the cursor is relative to, selector start, selector end)
    let (start, end, prefix, low, high) = match (selector, second) {
        (Selector::Key, None) => {
            let end = with(&first, 0);
            (
                first.clone(),
                end.clone(),
                first.clone(),
                Some(first),
                Some(end),
            )
        }
        (Selector::Prefix, None) => (with(&first, 0), with(&first, 0xFF), first, None, None),
        (Selector::PrefixStart, Some(start)) => {
            if !start.starts_with(&first) || start.len() == first.len() {
                return Ok(Err(KvError::type_error(
                    "Start key is not in the keyspace defined by prefix",
                )));
            }
            (start.clone(), with(&first, 0xFF), first, Some(start), None)
        }
        (Selector::PrefixEnd, Some(end)) => {
            if !end.starts_with(&first) || end.len() == first.len() {
                return Ok(Err(KvError::type_error(
                    "End key is not in the keyspace defined by prefix",
                )));
            }
            (with(&first, 0), end.clone(), first, None, Some(end))
        }
        (Selector::StartEnd, Some(end)) => {
            if first > end {
                return Ok(Err(KvError::type_error(
                    "Start key is greater than end key",
                )));
            }
            let common = common_prefix(&first, &end).to_vec();
            (first.clone(), end.clone(), common, Some(first), Some(end))
        }
        _ => return Err(fatal("selector keys")),
    };
    let (start, end) = if cursor.is_empty() {
        (start, end)
    } else {
        let Some(cursor) = base64url_decode(cursor) else {
            return Ok(Err(KvError::type_error("invalid cursor")));
        };
        let joined: Vec<u8> = prefix.iter().chain(&cursor).copied().collect();
        let (first_key, last_key) = if reverse {
            (start, joined)
        } else {
            (with(&joined, 0), end)
        };
        if low.as_ref().is_some_and(|low| first_key < *low)
            || high.as_ref().is_some_and(|high| last_key > *high)
        {
            return Ok(Err(KvError::type_error("cursor out of bounds")));
        }
        (first_key, last_key)
    };
    if start.len() > kv::MAX_READ_KEY_BYTES || end.len() > kv::MAX_READ_KEY_BYTES {
        return Ok(Err(KvError::type_error(format!(
            "Key too large for read (max {} bytes)",
            kv::MAX_READ_KEY_BYTES
        ))));
    }
    Ok(Ok(Range {
        read: denokv_proto::ReadRange {
            start,
            end,
            limit,
            reverse,
        },
        prefix,
    }))
}

fn common_prefix<'a>(a: &'a [u8], b: &[u8]) -> &'a [u8] {
    let length = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    &a[..length]
}

/// The cursor of an entry: base64url (padded, as Deno's `BASE64_URL_SAFE`)
/// of its key after the selector's common prefix.
fn cursor(prefix: &[u8], key: &[u8]) -> String {
    base64url_encode(key.strip_prefix(prefix).unwrap_or_default())
}

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const PAD: u8 = b'=';

fn base64url_encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0u32, |word, (index, byte)| {
            word | u32::from(*byte) << (16 - 8 * index)
        });
        for index in 0..4 {
            if index <= chunk.len() {
                text.push(BASE64URL[(word >> (18 - 6 * index) & 0x3F) as usize] as char);
            } else {
                text.push(PAD as char);
            }
        }
    }
    text
}

/// Strict padded base64url: canonical padding and zero trailing bits.
fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index == bytes.len() / 4 - 1;
        let pads = chunk.iter().rev().take_while(|byte| **byte == PAD).count();
        if pads > 2 || pads > 0 && !last {
            return None;
        }
        let mut word = 0u32;
        for (position, byte) in chunk[..4 - pads].iter().enumerate() {
            let digit = BASE64URL.iter().position(|d| d == byte)? as u32;
            word |= digit << (18 - 6 * position);
        }
        let produced = 3 - pads;
        if word & ((1 << (8 * (3 - produced))) - 1) != 0 {
            return None;
        }
        out.extend((0..produced).map(|position| (word >> (16 - 8 * position)) as u8));
    }
    Some(out)
}

fn commit(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<f64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let text = descriptor(caller, ptr, len)?;
    let context = caller.data_mut();
    let mut fields = fields(&text)?;
    let id = id_of(number(next(&mut fields)?)?).ok_or_else(|| fatal("database id"))?;
    let plan = match parse_commit(&context.kv, &mut fields)? {
        Ok(plan) => plan,
        Err(error) => return Ok(context.kv.fail(&error)),
    };
    if fields.next().is_some() {
        return Err(fatal("descriptor has trailing fields"));
    }
    context.kv.staged.clear();
    context.kv.staged_bytes = 0;
    if let Err(error) = granted(context, id, kv::WRITE_CAPABILITY)? {
        return Ok(context.kv.fail(&error));
    }
    let deadline = context.deadline;
    let (checks, mutations) = plan;
    let outcome = context.kv.run(id, |database, runtime| {
        database.commit(runtime, deadline, checks, mutations)
    })?;
    match outcome {
        Ok(Some(versionstamp)) => {
            context.kv.text = store::versionstamp_text(&versionstamp);
            Ok(kv::COMMITTED)
        }
        Ok(None) => {
            context.kv.text.clear();
            Ok(kv::CHECK_FAILED)
        }
        Err(Wait::Failed(error)) => Ok(context.kv.fail(&error)),
        Err(Wait::Deadline) => Err(context.deadline_error()),
    }
}

type Plan = (Vec<denokv_proto::Check>, Vec<denokv_proto::Mutation>);

/// Parse the checks and mutations of a commit descriptor and check Deno's
/// limits (`op_kv_atomic_write`).
fn parse_commit<'a>(
    state: &Kv,
    fields: &mut impl Iterator<Item = &'a str>,
) -> Result<Result<Plan, KvError>, wasmtime::Error> {
    let checks: usize = number(next(fields)?)?;
    if checks > kv::MAX_CHECKS {
        return Ok(Err(KvError::type_error(format!(
            "Too many checks (max {})",
            kv::MAX_CHECKS
        ))));
    }
    let mut parsed_checks = Vec::with_capacity(checks);
    for _ in 0..checks {
        let key = state.key(next(fields)?)?;
        let versionstamp = next(fields)?;
        let key = match key {
            Ok(key) => key,
            Err(error) => return Ok(Err(error)),
        };
        let versionstamp = if versionstamp.is_empty() {
            None
        } else {
            match store::parse_versionstamp(versionstamp) {
                Some(versionstamp) => Some(versionstamp),
                None => return Ok(Err(KvError::type_error("invalid versionstamp"))),
            }
        };
        parsed_checks.push(denokv_proto::Check { key, versionstamp });
    }
    let mutations: usize = number(next(fields)?)?;
    if mutations > kv::MAX_MUTATIONS {
        return Ok(Err(KvError::type_error(format!(
            "Too many mutations (max {})",
            kv::MAX_MUTATIONS
        ))));
    }
    let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
    let mut parsed = Vec::with_capacity(mutations);
    for _ in 0..mutations {
        let code = MutationCode::from_code(number(next(fields)?)?)
            .ok_or_else(|| fatal("mutation kind"))?;
        let key = state.key(next(fields)?)?;
        let value = next(fields)?;
        let expire_in = next(fields)?;
        if code.has_value() == value.is_empty()
            || code != MutationCode::Set && !expire_in.is_empty()
        {
            return Err(fatal("mutation fields"));
        }
        let key = match key {
            Ok(key) => key,
            Err(error) => return Ok(Err(error)),
        };
        let value = if code.has_value() {
            Some(match &state.staged(value)?.value {
                KvValue::V8(bytes) => KvValue::V8(bytes.clone()),
                KvValue::Bytes(bytes) => KvValue::Bytes(bytes.clone()),
                KvValue::U64(number) => KvValue::U64(*number),
            })
        } else {
            None
        };
        let kind = match (code, value) {
            (MutationCode::Set, Some(value)) => MutationKind::Set(value),
            (MutationCode::Delete, None) => MutationKind::Delete,
            (MutationCode::Sum, Some(value @ KvValue::U64(_))) => MutationKind::Sum {
                value,
                min_v8: Vec::new(),
                max_v8: Vec::new(),
                clamp: false,
            },
            (MutationCode::Min, Some(value @ KvValue::U64(_))) => MutationKind::Min(value),
            (MutationCode::Max, Some(value @ KvValue::U64(_))) => MutationKind::Max(value),
            _ => return Err(fatal("mutation value")),
        };
        let expire_at = if expire_in.is_empty() {
            None
        } else {
            let milliseconds: u64 = number(expire_in)?;
            match i64::try_from(milliseconds)
                .ok()
                .and_then(chrono::TimeDelta::try_milliseconds)
                .and_then(|delta| now.checked_add_signed(delta))
            {
                Some(at) => Some(at),
                None => {
                    return Ok(Err(KvError::type_error(format!(
                        "expireIn is too large: {milliseconds}"
                    ))));
                }
            }
        };
        parsed.push(denokv_proto::Mutation {
            key,
            kind,
            expire_at,
        });
    }
    let (mut payload, mut keys) = (0usize, 0usize);
    for key in parsed_checks
        .iter()
        .map(|check| &check.key)
        .chain(parsed.iter().map(|mutation| &mutation.key))
    {
        if key.is_empty() {
            return Ok(Err(KvError::type_error("key cannot be empty")));
        }
        if key.len() > kv::MAX_WRITE_KEY_BYTES {
            return Ok(Err(KvError::type_error(format!(
                "Key too large for write (max {} bytes)",
                kv::MAX_WRITE_KEY_BYTES
            ))));
        }
        payload += key.len();
    }
    for mutation in &parsed {
        let Some(value) = mutation.kind.value() else {
            continue;
        };
        let size = match value {
            KvValue::V8(bytes) | KvValue::Bytes(bytes) => bytes.len(),
            KvValue::U64(_) => size_of::<u64>(),
        };
        if size > kv::MAX_VALUE_BYTES {
            return Ok(Err(value_too_large()));
        }
        payload += size + mutation.key.len();
        keys += mutation.key.len();
    }
    if payload > kv::MAX_TOTAL_MUTATION_BYTES {
        return Ok(Err(KvError::type_error(format!(
            "Total mutation size too large (max {} bytes)",
            kv::MAX_TOTAL_MUTATION_BYTES
        ))));
    }
    if keys > kv::MAX_TOTAL_KEY_BYTES {
        return Ok(Err(KvError::type_error(format!(
            "Total key size too large (max {} bytes)",
            kv::MAX_TOTAL_KEY_BYTES
        ))));
    }
    Ok(Ok((parsed_checks, parsed)))
}

/// Entry bookkeeping observes the deadline but is not charged to the
/// host-call budget, like the network bookkeeping.
fn entry(context: &RuntimeContext, index: f64, field: f64) -> Result<String, wasmtime::Error> {
    context.check_deadline()?;
    let entry = entry_at(&context.kv, index)?;
    let field = (field.fract() == 0.0 && (0.0..=f64::from(u8::MAX)).contains(&field))
        .then(|| EntryField::from_code(field as u8))
        .flatten()
        .ok_or_else(|| fatal("entry field"))?;
    Ok(match field {
        EntryField::Versionstamp => store::versionstamp_text(&entry.versionstamp),
        EntryField::Encoding => match entry.value {
            KvValue::V8(_) => kv::ENCODING_V8,
            KvValue::U64(_) => kv::ENCODING_U64,
            KvValue::Bytes(_) => kv::ENCODING_BYTES,
        }
        .to_string(),
        EntryField::U64 => match entry.value {
            KvValue::U64(number) => number.to_string(),
            _ => return Err(fatal("entry is not a KvU64")),
        },
        EntryField::Bytes => match &entry.value {
            KvValue::Bytes(bytes) => bytes.iter().map(|byte| char::from(*byte)).collect(),
            _ => return Err(fatal("entry is not bytes")),
        },
        EntryField::Cursor => entry.cursor.clone(),
        EntryField::Range => entry.range.to_string(),
    })
}

fn entry_at(state: &Kv, index: f64) -> Result<&Entry, wasmtime::Error> {
    (index.fract() == 0.0 && index >= 0.0)
        .then(|| state.entries.get(index as usize))
        .flatten()
        .ok_or_else(|| fatal("entry index"))
}

/// The key or value of an entry decoded into the module's checked schema.
fn decoded(
    caller: &mut Caller<'_, RuntimeContext>,
    index: f64,
    schema_ptr: i32,
    schema_len: i32,
    key: bool,
) -> Result<i64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(fatal("missing memory"));
    };
    let schema = crate::json_parse::checked_schema(caller, memory, schema_ptr, schema_len)?;
    let context = caller.data();
    let entry = entry_at(&context.kv, index)?;
    let limits = context.module.limits.data;
    let value = if key {
        decode::key_value(&entry.key)
    } else {
        match &entry.value {
            KvValue::V8(bytes) => v8::decode(
                bytes,
                v8::Limits {
                    depth: limits.json_depth,
                    nodes: limits.json_nodes,
                    elements: limits.array_elements,
                    string_units: limits.string_bytes / 2,
                },
            )
            .map_err(|error| {
                decode::DecodeError::Mismatch(format!("invalid stored value: {error}"))
            }),
            KvValue::U64(_) => Err(decode::DecodeError::Mismatch(
                "the stored value is a KvU64".into(),
            )),
            KvValue::Bytes(_) => Err(decode::DecodeError::Mismatch(
                "the stored value is a Uint8Array".into(),
            )),
        }
    };
    let classified = value
        .and_then(|value| decode::Classifier::new(&schema, limits, context.deadline).root(&value));
    match classified {
        Ok(value) => crate::json_parse::materialize(caller, memory, &schema, value, true)
            .map(|value| value as i64),
        Err(decode::DecodeError::Mismatch(message)) => {
            let message = format!(
                "KV {} does not match the checked type: {message}",
                if key { "key" } else { "value" }
            );
            crate::json_parse::type_error(caller, memory, &schema, &message)
                .map(|value| value as i64)
        }
        Err(decode::DecodeError::Deadline) => Err(caller.data().deadline_error()),
    }
}

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::KvOpen.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| open(&mut caller, ptr, len),
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::KvRead.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| read(&mut caller, ptr, len),
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::KvCommit.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| commit(&mut caller, ptr, len),
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::KvStage.name(),
        |mut caller: Caller<'_, RuntimeContext>, mode: i32, kind: i32, lo: i32, hi: i32| {
            let packed = ((hi as u32 as u64) << i32::BITS) | (lo as u32 as u64);
            stage(&mut caller, mode, kind, packed)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::KvEntry.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         index: f64,
         field: f64|
         -> Result<i32, wasmtime::Error> {
            let text = entry(caller.data(), index, field)?;
            crate::host::result16(&mut caller, text.as_bytes())
                .map_err(|error| fatal(error.message))
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::KvKey.name(),
        |mut caller: Caller<'_, RuntimeContext>, index: f64, ptr: i32, len: i32| {
            decoded(&mut caller, index, ptr, len, true)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::KvValue.name(),
        |mut caller: Caller<'_, RuntimeContext>, index: f64, ptr: i32, len: i32| {
            decoded(&mut caller, index, ptr, len, false)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::KvText.name(),
        |mut caller: Caller<'_, RuntimeContext>| -> Result<i32, wasmtime::Error> {
            caller.data().check_deadline()?;
            let text = caller.data().kv.text.clone();
            crate::host::result16(&mut caller, text.as_bytes())
                .map_err(|error| fatal(error.message))
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::KvClose.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| -> Result<(), wasmtime::Error> {
            caller.data().check_deadline()?;
            if let Some(id) = id_of(id) {
                caller.data_mut().kv.databases.remove(&id);
            }
            Ok(())
        },
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "kv_tests.rs"]
mod tests;
