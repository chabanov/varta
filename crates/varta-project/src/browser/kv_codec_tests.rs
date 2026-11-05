//! The checked-in browser KV codec (`kv-codec.wasm`, the wasm32 build of
//! `varta-kv-codec`) against the same crate compiled natively, over the
//! values Deno wrote (the Deno 2.9.4 fixture rows and the Deno 2.9.7 probe).
//!
//! Each stored value and key is decoded into a set of checked schemas by
//! both builds, each over its own copy of one fake guest (memory, a bump
//! allocator with the guest heap's block headers, the exception channel and
//! the fuel tank). The two must agree exactly: status, result handle or
//! fatal text, every guest byte and allocation, the fuel charged, the
//! exception raised and the result length. Every decoded value is then
//! staged back by both, which must produce the same staged record (V8
//! bytes, key bytes and fuel) or the same failure.
use varta_common::{
    abi::{self, StaticRegion},
    json::{JsonSchema, JsonValue, SchemaBounds, build::Heap},
    kv::codec as C,
    limits::ProgramLimits,
    value::{self, ModuleBounds, ValueAbiError},
};
use varta_kv_codec::{
    Deadline, charge,
    guest::{self, Entry},
    protocol::{self, Stored},
    stage::{StageError, Stager},
    transport::Writer,
    wire::StageMode,
};
use wasmtime::{Caller, Engine, Extern, Instance, Linker, Module, Store};

const CODEC: &[u8] = include_bytes!("kv-codec.wasm");
const ROWS: &str = include_str!("../../../varta-runtime/src/kv_cases/deno-2.9.4/rows.json");
const PROBE: &str = include_str!("../../../varta-runtime/src/kv_cases/deno-2.9.7-probe/rows.txt");
/// Fake guest memory: the owned heap's base and room for decoded values.
const HEAP_ROOM: usize = 1 << 20;
const GUEST_BYTES: usize = abi::OWNED_STRING_BASE + HEAP_ROOM;
/// Where the schema text sits, inside the single-page static region.
const SCHEMA_AT: usize = 1024;
/// The checked schemas (schema 18 JSON) every value is decoded into.
const SCHEMAS: [&str; 8] = [
    r#"{"version":18,"root":0,"types":[{"kind":"string"}]}"#,
    r#"{"version":18,"root":0,"types":[{"kind":"float64"}]}"#,
    r#"{"version":18,"root":0,"types":[{"kind":"boolean"}]}"#,
    r#"{"version":18,"root":0,"types":[{"kind":"null"}]}"#,
    r#"{"version":18,"root":0,"types":[{"kind":"undefined"}]}"#,
    r#"{"version":18,"root":0,"types":[{"kind":"array","element":1},{"kind":"float64"}]}"#,
    r#"{"version":18,"root":0,"types":[{"kind":"array","element":1},{"kind":"string"}]}"#,
    r#"{"version":18,"root":0,"types":[{"kind":"array","element":1},{"kind":"primitiveUnion","members":[2,3]},{"kind":"string"},{"kind":"float64"}]}"#,
];

/// One fake guest: what either build may change.
#[derive(Debug, Clone, PartialEq)]
struct Fake {
    memory: Vec<u8>,
    next: usize,
    fuel: u64,
    thrown: Option<u64>,
    length: Option<i32>,
    retained: Vec<u64>,
}

impl Fake {
    fn new(schema: &str) -> Self {
        let mut memory = vec![0; GUEST_BYTES];
        memory[SCHEMA_AT..SCHEMA_AT + schema.len()].copy_from_slice(schema.as_bytes());
        Self {
            memory,
            next: abi::OWNED_STRING_BASE,
            fuel: 0,
            thrown: None,
            length: None,
            retained: Vec::new(),
        }
    }
    /// A block of `len` payload bytes with the guest heap's header, or 0.
    fn allocate(&mut self, len: usize) -> usize {
        let size = (abi::STRING_HEADER_BYTES + len).next_multiple_of(value::HEAP_ALIGNMENT);
        let header = self.next;
        if header + size > self.memory.len() {
            return 0;
        }
        let word = |at: usize, value: u32, memory: &mut Vec<u8>| {
            memory[at..at + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
        };
        word(header, u32::try_from(size).unwrap(), &mut self.memory);
        word(
            header + value::HEAP_FLAGS_OFFSET,
            value::HEAP_ALLOCATED_FLAG,
            &mut self.memory,
        );
        self.next += size;
        header + abi::STRING_HEADER_BYTES
    }
    fn range(&self, at: usize, len: usize) -> Option<std::ops::Range<usize>> {
        let end = at.checked_add(len)?;
        (end <= self.memory.len()).then_some(at..end)
    }
}

struct Never;
impl Deadline for Never {
    fn passed(&self) -> bool {
        false
    }
}

/// The native build's guest over a [`Fake`], as the wasm32 shim's `host`
/// module is over the Worker's imports.
struct Native(Fake);

fn json(message: &str) -> String {
    format!("checked JSON: {message}")
}

impl Heap for Native {
    type Error = String;
    fn allocate(&mut self, len: usize) -> Result<usize, String> {
        let ptr = self.0.allocate(len);
        varta_kv_codec::build::check_block(&self.0.memory[..], ptr, len).map_err(json)?;
        Ok(ptr)
    }
    fn write(&mut self, at: usize, bytes: &[u8]) -> Result<(), String> {
        let range = self.0.range(at, bytes.len()).ok_or(json("memory range"))?;
        self.0.memory[range].copy_from_slice(bytes);
        Ok(())
    }
    fn zero(&mut self, at: usize, len: usize) -> Result<(), String> {
        let range = self.0.range(at, len).ok_or(json("memory range"))?;
        self.0.memory[range].fill(0);
        Ok(())
    }
    fn retain(&mut self, handle: u64) -> Result<(), String> {
        self.0.retained.push(handle);
        Ok(())
    }
}

impl guest::Guest for Native {
    type Memory = [u8];
    fn memory(&self) -> &[u8] {
        &self.0.memory
    }
    fn ready(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn deadline(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn charge(&mut self, fuel: u64) -> Result<(), String> {
        self.0.fuel += fuel;
        Ok(())
    }
    fn throw(&mut self, cell: u64) -> Result<(), String> {
        self.0.thrown = Some(cell);
        Ok(())
    }
    fn result_length(&mut self, high: i32) -> Result<(), String> {
        self.0.length = Some(high);
        Ok(())
    }
    fn dispose(&mut self, value: JsonValue) -> Result<(), String> {
        drop(value);
        Ok(())
    }
    fn json_fatal(&self, message: String) -> String {
        json(&message)
    }
    fn kv_fatal(&self, message: &'static str) -> String {
        format!("kv: {message}")
    }
    fn abi_fatal(&self, error: ValueAbiError) -> String {
        error.to_string()
    }
    fn deadline_error(&self) -> String {
        unreachable!("the fake deadline never passes")
    }
}

/// The wasm32 build, instantiated once over a [`Fake`] it reaches only
/// through the `varta_kv_host` imports, as the Worker provides them.
struct Wasm {
    store: Store<Fake>,
    instance: Instance,
}

/// The codec's own memory and the fake guest, borrowed together.
fn split<'a>(caller: &'a mut Caller<'_, Fake>) -> (&'a mut [u8], &'a mut Fake) {
    let Some(Extern::Memory(memory)) = caller.get_export(C::EXPORT_MEMORY) else {
        panic!("the codec exports no memory");
    };
    memory.data_and_store_mut(caller)
}

fn copy(from: &[u8], at: u32, to: &mut [u8], into: u32, len: u32) -> u32 {
    let (at, into, len) = (at as usize, into as usize, len as usize);
    match (from.get(at..at + len), to.get_mut(into..into + len)) {
        (Some(from), Some(to)) => {
            to.copy_from_slice(from);
            1
        }
        _ => 0,
    }
}

impl Wasm {
    fn new() -> Self {
        let engine = Engine::default();
        let module = Module::new(&engine, CODEC).unwrap();
        let mut linker = Linker::<Fake>::new(&engine);
        let host = C::IMPORT_MODULE;
        linker
            .func_wrap(host, C::IMPORT_MEMORY_SIZE, |caller: Caller<'_, Fake>| {
                u32::try_from(caller.data().memory.len()).unwrap()
            })
            .unwrap()
            .func_wrap(
                host,
                C::IMPORT_MEMORY_READ,
                |mut caller: Caller<'_, Fake>, at: u32, into: u32, len: u32| {
                    let (own, fake) = split(&mut caller);
                    copy(&fake.memory, at, own, into, len)
                },
            )
            .unwrap()
            .func_wrap(
                host,
                C::IMPORT_MEMORY_WRITE,
                |mut caller: Caller<'_, Fake>, at: u32, from: u32, len: u32| {
                    let (own, fake) = split(&mut caller);
                    copy(own, from, &mut fake.memory, at, len)
                },
            )
            .unwrap()
            .func_wrap(
                host,
                C::IMPORT_MEMORY_ZERO,
                |mut caller: Caller<'_, Fake>, at: u32, len: u32| {
                    let fake = caller.data_mut();
                    match fake.range(at as usize, len as usize) {
                        Some(range) => {
                            fake.memory[range].fill(0);
                            1u32
                        }
                        None => 0,
                    }
                },
            )
            .unwrap()
            .func_wrap(
                host,
                C::IMPORT_ALLOCATE,
                |mut caller: Caller<'_, Fake>, len: u32| {
                    u32::try_from(caller.data_mut().allocate(len as usize)).unwrap()
                },
            )
            .unwrap()
            .func_wrap(
                host,
                C::IMPORT_RETAIN,
                |mut caller: Caller<'_, Fake>, handle: u64| {
                    caller.data_mut().retained.push(handle);
                    handle
                },
            )
            .unwrap()
            .func_wrap(host, C::IMPORT_DEADLINE, || 0u32)
            .unwrap()
            .func_wrap(
                host,
                C::IMPORT_CHARGE,
                |mut caller: Caller<'_, Fake>, fuel: u64| caller.data_mut().fuel += fuel,
            )
            .unwrap()
            .func_wrap(host, C::IMPORT_CHANNEL_READY, || 1u32)
            .unwrap()
            .func_wrap(
                host,
                C::IMPORT_CHANNEL_THROW,
                |mut caller: Caller<'_, Fake>, cell: u64| {
                    caller.data_mut().thrown = Some(cell);
                    1u32
                },
            )
            .unwrap()
            .func_wrap(
                host,
                C::IMPORT_RESULT_LENGTH,
                |mut caller: Caller<'_, Fake>, high: i32| {
                    caller.data_mut().length = Some(high);
                    1u32
                },
            )
            .unwrap();
        let mut store = Store::new(&engine, Fake::new(""));
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let mut wasm = Self { store, instance };
        // A single-page guest: empty static and intrinsic sections.
        let mut sections = Writer::default();
        sections.bytes(&[]);
        sections.bytes(&[]);
        wasm.input(&sections.0);
        let configure = wasm.export::<(), u32>(C::EXPORT_CONFIGURE);
        assert_eq!(configure.call(&mut wasm.store, ()).unwrap(), C::OK);
        wasm
    }
    fn export<P: wasmtime::WasmParams, R: wasmtime::WasmResults>(
        &mut self,
        name: &str,
    ) -> wasmtime::TypedFunc<P, R> {
        self.instance
            .get_typed_func::<P, R>(&mut self.store, name)
            .unwrap()
    }
    /// Start a call with `input` in the codec's arena.
    fn input(&mut self, input: &[u8]) {
        let reserve = self.export::<u32, u32>(C::EXPORT_INPUT);
        let at = reserve
            .call(&mut self.store, u32::try_from(input.len()).unwrap())
            .unwrap();
        let memory = self
            .instance
            .get_memory(&mut self.store, C::EXPORT_MEMORY)
            .unwrap();
        memory.write(&mut self.store, at as usize, input).unwrap();
    }
    /// The output of the call just made.
    fn output(&mut self) -> Vec<u8> {
        let at = self
            .export::<(), u32>(C::EXPORT_OUTPUT)
            .call(&mut self.store, ())
            .unwrap();
        let len = self
            .export::<(), u32>(C::EXPORT_OUTPUT_LENGTH)
            .call(&mut self.store, ())
            .unwrap();
        let memory = self
            .instance
            .get_memory(&mut self.store, C::EXPORT_MEMORY)
            .unwrap();
        memory.data(&self.store)[at as usize..(at + len) as usize].to_vec()
    }
    fn decode(
        &mut self,
        guest: Fake,
        key: bool,
        input: &[u8],
        schema_len: usize,
    ) -> (u32, Vec<u8>, Fake) {
        *self.store.data_mut() = guest;
        self.input(input);
        let decode = self.export::<(u32, i32, i32), u32>(C::EXPORT_DECODE);
        let status = decode
            .call(
                &mut self.store,
                (
                    u32::from(key),
                    i32::try_from(SCHEMA_AT).unwrap(),
                    i32::try_from(schema_len).unwrap(),
                ),
            )
            .unwrap();
        let output = self.output();
        (status, output, self.store.data().clone())
    }
    fn stage(&mut self, guest: Fake, kind: u32, packed: u64) -> (u32, Vec<u8>) {
        *self.store.data_mut() = guest;
        self.input(&[]);
        let stage = self.export::<(i32, i32, i64), u32>(C::EXPORT_STAGE);
        let status = stage
            .call(
                &mut self.store,
                (StageMode::Value as i32, kind as i32, packed as i64),
            )
            .unwrap();
        (status, self.output())
    }
}

fn module() -> ModuleBounds {
    ModuleBounds::v38(StaticRegion::SINGLE_PAGE)
}

fn read_schema(bytes: &[u8], limits: &ProgramLimits) -> Result<JsonSchema, String> {
    JsonSchema::read(bytes, SchemaBounds::of(limits), limits.schema_bytes)
        .map_err(|error| format!("{error}"))
}

/// The native decode as the codec's `kv_decode` reports it.
fn native_decode(guest: Fake, entry: Entry<'_>, schema_len: usize) -> (u32, Vec<u8>, Fake) {
    let mut native = Native(guest);
    let decoded = guest::decode(
        &mut native,
        module(),
        &Never,
        (
            i32::try_from(SCHEMA_AT).unwrap(),
            i32::try_from(schema_len).unwrap(),
        ),
        Some(entry),
        read_schema,
    );
    match decoded {
        Ok(handle) => (C::OK, handle.to_le_bytes().to_vec(), native.0),
        Err(message) => (C::FATAL, message.into_bytes(), native.0),
    }
}

/// The native stage as the codec's `kv_stage` reports it.
fn native_stage(guest: &Fake, kind: u32, packed: u64) -> (u32, Vec<u8>) {
    let staged = Stager::new(&guest.memory[..], module(), &Never).value(kind, packed, 0);
    let value = match staged {
        Ok(value) => value,
        Err(StageError::Unsupported(message)) => {
            return (
                C::FAILED,
                protocol::KvError::type_error(message).text().into_bytes(),
            );
        }
        Err(StageError::Fatal(message)) => return (C::FATAL, message.into_bytes()),
        Err(StageError::Abi(error)) => return (C::FATAL, error.to_string().into_bytes()),
    };
    match protocol::staged_value(StageMode::Value, &value) {
        Ok(staged) => {
            let mut writer = Writer::default();
            writer.u64(charge::staged(staged.size()));
            writer.u32(u32::try_from(staged.size()).unwrap());
            writer.staged(&staged);
            (C::OK, writer.0)
        }
        Err(error) => (C::FAILED, error.text().into_bytes()),
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// Every Deno-written `(key, encoding, value)`: the fixture's rows and the
/// probe's values (which have no stored key).
fn stored() -> Vec<(Option<Vec<u8>>, i64, Vec<u8>)> {
    let rows: serde_json::Value = serde_json::from_str(ROWS).unwrap();
    let mut entries: Vec<_> = rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                Some(hex(row["k"].as_str().unwrap())),
                row["v_encoding"].as_i64().unwrap(),
                hex(row["v"].as_str().unwrap()),
            )
        })
        .collect();
    for line in PROBE.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split_whitespace();
        let (_, encoding, value) = (fields.next(), fields.next(), fields.next());
        entries.push((
            None,
            encoding.unwrap().parse().unwrap(),
            hex(value.unwrap()),
        ));
    }
    entries
}

/// What both builds did with one entry and schema, once they agreed.
#[derive(Debug, PartialEq)]
enum Outcome {
    Fatal,
    Thrown,
    Staged,
    StageFailed,
}

/// Decode `entry` (`input` on the wire) into `schema` with both builds,
/// then stage a decoded value back with both; every step must agree.
fn agree(
    wasm: &mut Wasm,
    schema: &str,
    (is_key, input, entry): (bool, &[u8], Entry<'_>),
) -> Outcome {
    let parsed = read_schema(schema.as_bytes(), &module().limits).unwrap();
    let context = format!("{} {schema} key={is_key}", hex_text(input));
    let native = native_decode(Fake::new(schema), entry, schema.len());
    let browser = wasm.decode(Fake::new(schema), is_key, input, schema.len());
    assert_eq!(browser.0, native.0, "status: {context}");
    assert_eq!(browser.1, native.1, "output: {context}");
    assert!(browser.2 == native.2, "guest state: {context}");
    if native.0 != C::OK {
        return Outcome::Fatal;
    }
    let guest = native.2;
    if guest.thrown.is_some() {
        return Outcome::Thrown;
    }
    let handle = u64::from_le_bytes(native.1.try_into().unwrap());
    let kind = parsed.types[parsed.root as usize].kind() as u32;
    let native_staged = native_stage(&guest, kind, handle);
    let browser_staged = wasm.stage(guest, kind, handle);
    assert_eq!(browser_staged, native_staged, "stage: {context}");
    if native_staged.0 == C::OK {
        Outcome::Staged
    } else {
        Outcome::StageFailed
    }
}

#[test]
fn the_wasm32_codec_agrees_with_the_native_value_layer_on_deno_values() {
    let mut wasm = Wasm::new();
    let mut outcomes = Vec::new();
    for (key, encoding, bytes) in stored() {
        let stored = Stored::of(encoding, bytes.clone()).unwrap();
        let mut value_input = vec![C::ENTRY_VALUE, u8::try_from(encoding).unwrap()];
        value_input.extend(&bytes);
        let key_input = key.as_ref().map(|key| {
            let mut input = vec![C::ENTRY_KEY];
            input.extend(key);
            input
        });
        for schema in SCHEMAS {
            outcomes.push(agree(
                &mut wasm,
                schema,
                (false, &value_input, Entry::Value(&stored)),
            ));
            if let (Some(input), Some(key)) = (&key_input, &key) {
                outcomes.push(agree(&mut wasm, schema, (true, input, Entry::Key(key))));
            }
        }
    }
    // Both outcomes are exercised, and every decoded value stages back.
    let count = |outcome| outcomes.iter().filter(|seen| **seen == outcome).count();
    assert!(
        count(Outcome::Staged) > 0 && count(Outcome::Thrown) > 0,
        "{outcomes:?}"
    );
    assert_eq!(count(Outcome::StageFailed) + count(Outcome::Fatal), 0);
}

/// `type Nested = Nested[] | null`.
const NESTED: &str = r#"{"version":18,"root":0,"types":[{"kind":"referenceUnion","members":[1,2]},{"kind":"array","element":0},{"kind":"null"}]}"#;

/// The V8 bytes of `depth` nested one-element arrays around `null`.
fn nested_bytes(depth: usize) -> Vec<u8> {
    let mut bytes = vec![
        0xFF,
        u8::try_from(varta_kv_codec::v8::WRITE_VERSION).unwrap(),
    ];
    bytes.extend(std::iter::repeat_n([b'A', 1], depth).flatten());
    bytes.push(b'0');
    bytes.extend(std::iter::repeat_n([b'$', 0, 1], depth).flatten());
    bytes
}

/// Both builds keep the shared depth limit alike: a stored value just below
/// and at `MAX_VALUE_DEPTH` decodes and stages back, one level deeper is the
/// same recoverable `TypeError` in the guest.
#[test]
fn the_wasm32_codec_keeps_the_shared_depth_limit_as_natively() {
    let mut wasm = Wasm::new();
    let max = varta_kv_codec::MAX_VALUE_DEPTH;
    for (depth, expected) in [
        (max - 1, Outcome::Staged),
        (max, Outcome::Staged),
        (max + 1, Outcome::Thrown),
    ] {
        let stored = Stored::V8(nested_bytes(depth));
        let mut input = vec![C::ENTRY_VALUE, u8::try_from(stored.encoding()).unwrap()];
        input.extend(stored.bytes());
        assert_eq!(
            agree(&mut wasm, NESTED, (false, &input, Entry::Value(&stored))),
            expected,
            "{depth}"
        );
    }
}

/// Levels around the stub in [`the_wasm32_codec_keeps_the_depth_limit_of_shared_parts_as_natively`].
const SHARED_WRAPS: usize = 10;

/// The V8 bytes of a two-element array of `Nested`: a chain `x` of `inner`
/// arrays around `null` and `y`, `SHARED_WRAPS` arrays around `null` (the
/// stub), `x` first when `x_first`.
fn pair_bytes(inner: usize, x_first: bool) -> Vec<u8> {
    let body = |depth: usize| {
        let mut bytes: Vec<u8> = std::iter::repeat_n([b'A', 1], depth).flatten().collect();
        bytes.push(b'0');
        bytes.extend(std::iter::repeat_n([b'$', 0, 1], depth).flatten());
        bytes
    };
    let (x, y) = (body(inner), body(SHARED_WRAPS));
    let mut bytes = vec![
        0xFF,
        u8::try_from(varta_kv_codec::v8::WRITE_VERSION).unwrap(),
        b'A',
        2,
    ];
    let (first, second) = if x_first { (x, y) } else { (y, x) };
    bytes.extend(first);
    bytes.extend(second);
    bytes.extend([b'$', 0, 2]);
    bytes
}

/// The array a `Nested` handle (a reference union cell) holds.
fn nested_array(guest: &Fake, handle: u64) -> value::Vector {
    let version = value::HASHED_DICTIONARY_VALUE_VERSION;
    let cell = value::ReferenceBox::read_in(&guest.memory[..], handle, version, module()).unwrap();
    value::Vector::read_in(&guest.memory[..], cell.payload, version, module()).unwrap()
}

/// Where element `index` of `array` sits in guest memory.
fn element_at(array: &value::Vector, index: usize) -> std::ops::Range<usize> {
    let at = array.storage + value::ARRAY_META_BYTES + index * value::ARRAY_SLOT_BYTES;
    at..at + value::ARRAY_SLOT_BYTES
}

/// Both builds stage a guest value with a shared part alike, keeping the
/// shared depth limit at the part's deepest reach in either order: the
/// stub at the bottom of `y` is replaced by the very cell of `x`, so `x` is
/// reached at level 1 and at `1 + SHARED_WRAPS`. Within the limit both stage
/// it, and what they stage decodes and stages back alike (the read-back
/// guarantee); one level deeper both refuse it by name before encoding.
#[test]
fn the_wasm32_codec_keeps_the_depth_limit_of_shared_parts_as_natively() {
    let mut wasm = Wasm::new();
    let max = varta_kv_codec::MAX_VALUE_DEPTH;
    let fits = max - 1 - SHARED_WRAPS;
    let refused = protocol::KvError::type_error(varta_kv_codec::too_deep())
        .text()
        .into_bytes();
    let kind = value::ValueKind::ReferenceUnion as u32;
    for x_first in [true, false] {
        for inner in [fits, fits + 1] {
            let stored = Stored::V8(pair_bytes(inner, x_first));
            let (status, handle, mut guest) =
                native_decode(Fake::new(NESTED), Entry::Value(&stored), NESTED.len());
            assert_eq!(status, C::OK, "{inner} {x_first}");
            assert_eq!(guest.thrown, None, "{inner} {x_first}");
            let handle = u64::from_le_bytes(handle.try_into().unwrap());
            let pair = nested_array(&guest, handle);
            let (x_slot, y_slot) = if x_first {
                (element_at(&pair, 0), element_at(&pair, 1))
            } else {
                (element_at(&pair, 1), element_at(&pair, 0))
            };
            let mut stub = u64::from_le_bytes(guest.memory[y_slot].try_into().unwrap());
            for _ in 1..SHARED_WRAPS {
                let level = nested_array(&guest, stub);
                stub = u64::from_le_bytes(guest.memory[element_at(&level, 0)].try_into().unwrap());
            }
            let bottom = element_at(&nested_array(&guest, stub), 0);
            let x_cell = guest.memory[x_slot].to_vec();
            guest.memory[bottom].copy_from_slice(&x_cell);
            let native = native_stage(&guest, kind, handle);
            assert_eq!(
                wasm.stage(guest.clone(), kind, handle),
                native,
                "{inner} {x_first}"
            );
            if inner > fits {
                assert_eq!(native, (C::FAILED, refused.clone()), "{inner} {x_first}");
                continue;
            }
            assert_eq!(native.0, C::OK, "{inner} {x_first}");
            // What staged reads back: the shared encoding decodes and stages
            // back alike in both builds.
            let staged = Stager::new(&guest.memory[..], module(), &Never)
                .value(kind, handle, 0)
                .unwrap();
            let stored = Stored::V8(
                varta_kv_codec::v8::encode(&staged, varta_kv_codec::wire::MAX_VALUE_BYTES).unwrap(),
            );
            let mut input = vec![C::ENTRY_VALUE, u8::try_from(stored.encoding()).unwrap()];
            input.extend(stored.bytes());
            assert_eq!(
                agree(&mut wasm, NESTED, (false, &input, Entry::Value(&stored))),
                Outcome::Staged,
                "{inner} {x_first}"
            );
        }
    }
}

fn hex_text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
