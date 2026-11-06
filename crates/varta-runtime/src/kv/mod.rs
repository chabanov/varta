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
//!
//! Everything the imports do to values (staging, the V8 codec, Deno's key
//! codec, the descriptors, the decode into the module's schema and the fair
//! fuel) is the shared value layer `varta_kv_codec`, which the browser host
//! runs too; this module adds the SQLite storage of `store` and the broker.
//!
//! Under v38 every operation also pays fair host fuel (`varta_common::fuel`,
//! `varta_kv_codec::charge`): `KV_OPERATION` per open, read and commit,
//! `KV_BYTE` per key or value byte staged, committed or returned by a read,
//! `KV_ENTRY` per entry a read returns, and the checked JSON rates for the
//! schema and the values a decode builds.
mod clock;
pub(crate) mod store;

use crate::{RuntimeContext, permission::Decision};
use denokv_proto::{KvValue, MutationKind};
use std::collections::BTreeMap;
use store::{Database, Wait};
use varta_common::fuel;
use varta_common::{
    HostOperation,
    kv::{self, EntryField},
};
#[cfg(test)]
use varta_kv_codec::v8;
use varta_kv_codec::wire::StageMode;
use varta_kv_codec::{
    charge, guest,
    protocol::{self, KvError, Staged, Stored},
    stage,
};
use wasmtime::{Caller, Extern, Linker};

fn fatal(message: impl std::fmt::Display) -> wasmtime::Error {
    wasmtime::Error::msg(format!("kv: {message}"))
}

/// The stored bytes of a value.
fn value_bytes(value: &KvValue) -> usize {
    match value {
        KvValue::V8(bytes) | KvValue::Bytes(bytes) => bytes.len(),
        KvValue::U64(_) => size_of::<u64>(),
    }
}

/// Deno's form of a stored value.
fn kv_value(value: Stored) -> KvValue {
    match value {
        Stored::V8(bytes) => KvValue::V8(bytes),
        Stored::U64(number) => KvValue::U64(number),
        Stored::Bytes(bytes) => KvValue::Bytes(bytes),
    }
}

/// The shared form of a value Deno stored.
fn stored(value: &KvValue) -> Stored {
    match value {
        KvValue::V8(bytes) => Stored::V8(bytes.clone()),
        KvValue::U64(number) => Stored::U64(*number),
        KvValue::Bytes(bytes) => Stored::Bytes(bytes.clone()),
    }
}

/// The turn deadline as the shared value layer polls it.
struct Deadline(std::time::Instant);
impl varta_kv_codec::Deadline for Deadline {
    fn passed(&self) -> bool {
        std::time::Instant::now() >= self.0
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
    /// The clock `expireIn` is stamped on (`clock`).
    clock: clock::Clock,
}

impl Drop for Kv {
    /// The databases close first, as the fields would; the waiting runtime
    /// then shuts down outside any async context, which a native server's
    /// request task (K7 H2) may be.
    fn drop(&mut self) {
        drop(std::mem::take(&mut self.databases));
        if let Some(runtime) = self.runtime.take() {
            crate::blocking(move || drop(runtime));
        }
    }
}

impl Kv {
    fn fail(&mut self, error: &KvError) -> f64 {
        self.text = error.text();
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
}

/// A descriptor only a tampered module produces.
fn malformed(error: protocol::Fatal) -> wasmtime::Error {
    fatal(error.0)
}

fn descriptor(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<String, wasmtime::Error> {
    crate::host::input(caller, ptr, len, kv::MAX_DESCRIPTOR_BYTES, true)
        .map_err(|error| error.into_trap(fatal))
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
        return Ok(context.kv.fail(&KvError::too_many_databases()));
    }
    // V38 fair fuel: the open, after the grant and before the file.
    crate::fair_fuel::charge(caller, fuel::KV_OPERATION)?;
    let context = caller.data_mut();
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
    // The shared code is keyed by the wire codes, compiled from one source.
    let mode = StageMode::from_code(mode).ok_or_else(|| fatal("stage mode"))?;
    let Some(Extern::Memory(memory)) = caller.get_export(varta_common::abi::MEMORY_EXPORT) else {
        return Err(fatal("missing memory"));
    };
    let context = caller.data();
    let deadline = Deadline(context.deadline);
    let staged = {
        stage::Stager::new(memory.data(&*caller), context.module, &deadline).value(
            kind as u32,
            packed,
            0,
        )
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
        Err(stage::StageError::Fatal(message)) => return Err(wasmtime::Error::msg(message)),
        Err(stage::StageError::Abi(error)) => return Err(wasmtime::Error::new(error)),
    };
    let staged = match protocol::staged_value(mode, &value) {
        Ok(staged) => staged,
        Err(error) => return Ok(context.kv.fail(&error)),
    };
    // V38 fair fuel: the encoded key and value bytes, before they are kept.
    crate::fair_fuel::charge(caller, charge::staged(staged.size()))?;
    let context = caller.data_mut();
    context.kv.staged_bytes += staged.size();
    if context.kv.staged_bytes > kv::MAX_STAGED_BYTES {
        return Ok(context.kv.fail(&KvError::type_error(kv::STAGED_TOO_LARGE)));
    }
    context.kv.staged.push(staged);
    Ok((context.kv.staged.len() - 1) as f64)
}

fn read(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<f64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let text = descriptor(caller, ptr, len)?;
    let context = caller.data_mut();
    let request = match protocol::parse_read(&text, &context.kv.staged).map_err(malformed)? {
        Ok(request) => request,
        Err(error) => return Ok(context.kv.fail(&error)),
    };
    context.kv.staged.clear();
    context.kv.staged_bytes = 0;
    let id = request.id;
    if let Err(error) = granted(context, id, kv::READ_CAPABILITY)? {
        return Ok(context.kv.fail(&error));
    }
    let deadline = context.deadline;
    let prefixes: Vec<Vec<u8>> = request
        .ranges
        .iter()
        .map(|range| range.prefix.clone())
        .collect();
    let reads = request
        .ranges
        .into_iter()
        .map(|range| denokv_proto::ReadRange {
            start: range.start,
            end: range.end,
            limit: range.limit,
            reverse: range.reverse,
        })
        .collect();
    // V38 fair fuel: the read, after the grant and before the database.
    crate::fair_fuel::charge(caller, fuel::KV_OPERATION)?;
    let outcome = caller.data_mut().kv.run(id, |database, runtime| {
        database.read(runtime, deadline, reads)
    })?;
    match outcome {
        Ok(outputs) => {
            // The entries returned, before the guest sees their count.
            let returned = charge::returned(
                outputs
                    .iter()
                    .flat_map(|output| &output.entries)
                    .map(|entry| entry.key.len() + value_bytes(&entry.value)),
            );
            crate::fair_fuel::charge(caller, returned)?;
            let context = caller.data_mut();
            context.kv.entries.clear();
            for (range, output) in outputs.into_iter().enumerate() {
                for entry in output.entries {
                    let cursor = protocol::cursor(&prefixes[range], &entry.key);
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
        Err(Wait::Failed(error)) => Ok(caller.data_mut().kv.fail(&error)),
        Err(Wait::Deadline) => Err(caller.data().deadline_error()),
    }
}

fn commit(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<f64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let text = descriptor(caller, ptr, len)?;
    let context = caller.data_mut();
    // One reading of the KV clock for every `expireIn` of the commit.
    let now = context.kv.clock.now();
    let parsed = protocol::parse_commit(&text, &context.kv.staged, |milliseconds| {
        clock::expire_at(now, milliseconds)
    })
    .map_err(malformed)?;
    let plan = match parsed {
        Ok(plan) => plan,
        Err(error) => return Ok(context.kv.fail(&error)),
    };
    context.kv.staged.clear();
    context.kv.staged_bytes = 0;
    let id = plan.id;
    if let Err(error) = granted(context, id, kv::WRITE_CAPABILITY)? {
        return Ok(context.kv.fail(&error));
    }
    let deadline = context.deadline;
    // V38 fair fuel: the commit and the key and value bytes it writes, after
    // the grant and before anything is submitted.
    crate::fair_fuel::charge(caller, charge::committed(plan.written()))?;
    let checks = plan
        .checks
        .into_iter()
        .map(|check| denokv_proto::Check {
            key: check.key,
            versionstamp: check.versionstamp,
        })
        .collect();
    let mutations = plan
        .mutations
        .into_iter()
        .map(|mutation| denokv_proto::Mutation {
            key: mutation.key,
            kind: match mutation.kind {
                protocol::Kind::Set(value) => MutationKind::Set(kv_value(value)),
                protocol::Kind::Delete => MutationKind::Delete,
                protocol::Kind::Sum(number) => MutationKind::Sum {
                    value: KvValue::U64(number),
                    min_v8: Vec::new(),
                    max_v8: Vec::new(),
                    clamp: false,
                },
                protocol::Kind::Min(number) => MutationKind::Min(KvValue::U64(number)),
                protocol::Kind::Max(number) => MutationKind::Max(KvValue::U64(number)),
            },
            expire_at: mutation.expire_at,
        })
        .collect();
    let context = caller.data_mut();
    let outcome = context.kv.run(id, |database, runtime| {
        database.commit(runtime, deadline, checks, mutations)
    })?;
    match outcome {
        Ok(Some(versionstamp)) => {
            context.kv.text = protocol::versionstamp_text(&versionstamp);
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
        EntryField::Versionstamp => protocol::versionstamp_text(&entry.versionstamp),
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

/// The key or value of an entry decoded into the module's checked schema
/// (`varta_kv_codec::guest::decode`).
fn decoded(
    caller: &mut Caller<'_, RuntimeContext>,
    index: f64,
    schema_ptr: i32,
    schema_len: i32,
    key: bool,
) -> Result<i64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let Some(Extern::Memory(memory)) = caller.get_export(varta_common::abi::MEMORY_EXPORT) else {
        return Err(fatal("missing memory"));
    };
    let context = caller.data();
    // The entry is copied out of the instance state the decode writes to;
    // a missing one fails once the schema is checked.
    let entry = entry_at(&context.kv, index).ok().map(|entry| {
        if key {
            (entry.key.clone(), None)
        } else {
            (Vec::new(), Some(stored(&entry.value)))
        }
    });
    let deadline = Deadline(context.deadline);
    let mut host = crate::json_parse::guest::Host::new(caller, memory);
    let module = host.module();
    let entry = entry.as_ref().map(|(key, value)| match value {
        Some(value) => guest::Entry::Value(value),
        None => guest::Entry::Key(key),
    });
    guest::decode(
        &mut host,
        module,
        &deadline,
        (schema_ptr, schema_len),
        entry,
        |bytes, limits| {
            varta_common::json::JsonSchema::from_bytes_within(bytes, limits)
                .map_err(|error| error.to_string())
        },
    )
    .map(|value| value as i64)
}

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvOpen.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| open(&mut caller, ptr, len),
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvRead.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| read(&mut caller, ptr, len),
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvCommit.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| commit(&mut caller, ptr, len),
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvStage.name(),
        |mut caller: Caller<'_, RuntimeContext>, mode: i32, kind: i32, lo: i32, hi: i32| {
            let packed = ((hi as u32 as u64) << i32::BITS) | (lo as u32 as u64);
            stage(&mut caller, mode, kind, packed)
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvEntry.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         index: f64,
         field: f64|
         -> Result<i32, wasmtime::Error> {
            let text = entry(caller.data(), index, field)?;
            crate::host::result16(&mut caller, text.as_bytes())
                .map_err(|error| error.into_trap(fatal))
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvKey.name(),
        |mut caller: Caller<'_, RuntimeContext>, index: f64, ptr: i32, len: i32| {
            decoded(&mut caller, index, ptr, len, true)
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvValue.name(),
        |mut caller: Caller<'_, RuntimeContext>, index: f64, ptr: i32, len: i32| {
            decoded(&mut caller, index, ptr, len, false)
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvText.name(),
        |mut caller: Caller<'_, RuntimeContext>| -> Result<i32, wasmtime::Error> {
            caller.data().check_deadline()?;
            let text = caller.data().kv.text.clone();
            crate::host::result16(&mut caller, text.as_bytes())
                .map_err(|error| error.into_trap(fatal))
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::KvClose.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| -> Result<(), wasmtime::Error> {
            caller.data().check_deadline()?;
            if let Some(id) = protocol::id_of(id) {
                caller.data_mut().kv.databases.remove(&id);
            }
            Ok(())
        },
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "kv_extent_tests.rs"]
mod extent_tests;
#[cfg(test)]
#[path = "kv_fuel_tests.rs"]
mod fuel_tests;
#[cfg(test)]
#[path = "kv_tests.rs"]
mod tests;
