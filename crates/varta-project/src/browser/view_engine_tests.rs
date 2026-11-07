//! The checked-in browser view engine (`view-engine.wasm`, the wasm32 build
//! of `varta-style`) against the same crate compiled natively and against
//! the native renderer (`varta-view`), over the cross-host corpus
//! (`view-engine-corpus.json`: the Tailwind application's candidates and
//! targeted S1–S3 cases).
//!
//! Each case runs as one transaction in a style session: elements with
//! their classes, the stylesheet, inline values, the cascade budgets. The
//! wasm32 and native sessions must return the same status for every call
//! and the same computed bytes; the native renderer must accept exactly the
//! cases the sessions accept and compute the same values for them. The
//! verdicts are recorded in `view-engine-expected.json`, which the browser
//! host's own test (`view-engine.test.mjs`) checks the JavaScript bridge
//! against, so both hosts are tied to one record.
use std::collections::BTreeMap;
use varta_common::view::{CssProperty, Tag};
use varta_style::session::{self, Session};
use varta_view::{Access, BatchWriter, Broker, View};
use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

const ENGINE: &[u8] = include_bytes!("view-engine.wasm");
const CORPUS: &str = include_str!("view-engine-corpus.json");
const EXPECTED: &str = include_str!("view-engine-expected.json");
/// The recorded inputs every case is computed with.
const WIDTH: f64 = 1600.0;
const HEIGHT: f64 = 1000.0;
const ROOT_FONT: f64 = 16.0;

/// One session's calls, natively or through the wasm32 module.
trait Driver {
    fn begin(&mut self);
    fn commit(&mut self) -> u32;
    fn abort(&mut self);
    fn create(&mut self, id: u32) -> u32;
    fn insert(&mut self, parent: u32, child: u32) -> u32;
    fn class(&mut self, id: u32, text: &str) -> u32;
    fn style(&mut self, id: u32, property: u16, text: &str) -> u32;
    fn sheet(&mut self, id: u32, text: &str) -> u32;
    fn charge(&mut self) -> u32;
    fn environment(&mut self, flags: u32);
    fn computed(&mut self, id: u32) -> Vec<u8>;
}

impl Driver for Session {
    fn begin(&mut self) {
        Session::begin(self);
    }
    fn commit(&mut self) -> u32 {
        Session::commit(self)
    }
    fn abort(&mut self) {
        Session::abort(self);
    }
    fn create(&mut self, id: u32) -> u32 {
        Session::create(self, id, Some(Tag::Div))
    }
    fn insert(&mut self, parent: u32, child: u32) -> u32 {
        Session::insert(self, parent, child, 0)
    }
    fn class(&mut self, id: u32, text: &str) -> u32 {
        self.set_class(id, text)
    }
    fn style(&mut self, id: u32, property: u16, text: &str) -> u32 {
        self.set_style(id, property, text)
    }
    fn sheet(&mut self, id: u32, text: &str) -> u32 {
        self.stylesheet(id, text)
    }
    fn charge(&mut self) -> u32 {
        Session::charge(self)
    }
    fn environment(&mut self, flags: u32) {
        self.set_environment(session::environment(
            (WIDTH, HEIGHT),
            ROOT_FONT,
            ROOT_FONT,
            flags,
        ));
    }
    fn computed(&mut self, id: u32) -> Vec<u8> {
        session::encode_computed(&Session::computed(self, id))
    }
}

/// The wasm32 module in wasmtime.
struct Wasm {
    store: Store<()>,
    instance: Instance,
}
impl Wasm {
    fn new(engine: &Engine, module: &Module) -> Self {
        let mut store = Store::new(engine, ());
        let instance = Instance::new(&mut store, module, &[]).unwrap();
        Self { store, instance }
    }
    fn func<P: wasmtime::WasmParams, R: wasmtime::WasmResults>(
        &mut self,
        name: &str,
    ) -> TypedFunc<P, R> {
        self.instance
            .get_typed_func::<P, R>(&mut self.store, name)
            .unwrap()
    }
    fn call0(&mut self, name: &str) -> u32 {
        self.func::<(), u32>(name)
            .call(&mut self.store, ())
            .unwrap()
    }
    fn input(&mut self, text: &str) {
        let at = self
            .func::<u32, u32>("view_engine_input")
            .call(&mut self.store, u32::try_from(text.len()).unwrap())
            .unwrap();
        let memory = self.instance.get_memory(&mut self.store, "memory").unwrap();
        memory
            .write(&mut self.store, at as usize, text.as_bytes())
            .unwrap();
    }
}
impl Driver for Wasm {
    fn begin(&mut self) {
        self.func::<(), ()>("view_engine_begin")
            .call(&mut self.store, ())
            .unwrap();
    }
    fn commit(&mut self) -> u32 {
        self.call0("view_engine_commit")
    }
    fn abort(&mut self) {
        self.func::<(), ()>("view_engine_abort")
            .call(&mut self.store, ())
            .unwrap();
    }
    fn create(&mut self, id: u32) -> u32 {
        self.func::<(u32, u32), u32>("view_engine_create")
            .call(&mut self.store, (id, u32::from(Tag::Div.code())))
            .unwrap()
    }
    fn insert(&mut self, parent: u32, child: u32) -> u32 {
        self.func::<(u32, u32, u32), u32>("view_engine_insert")
            .call(&mut self.store, (parent, child, 0))
            .unwrap()
    }
    fn class(&mut self, id: u32, text: &str) -> u32 {
        self.input(text);
        self.func::<u32, u32>("view_engine_class")
            .call(&mut self.store, id)
            .unwrap()
    }
    fn style(&mut self, id: u32, property: u16, text: &str) -> u32 {
        self.input(text);
        self.func::<(u32, u32), u32>("view_engine_style")
            .call(&mut self.store, (id, u32::from(property)))
            .unwrap()
    }
    fn sheet(&mut self, id: u32, text: &str) -> u32 {
        self.input(text);
        self.func::<u32, u32>("view_engine_sheet")
            .call(&mut self.store, id)
            .unwrap()
    }
    fn charge(&mut self) -> u32 {
        self.call0("view_engine_charge")
    }
    fn environment(&mut self, flags: u32) {
        self.func::<(f64, f64, f64, f64, u32), ()>("view_engine_environment")
            .call(
                &mut self.store,
                (WIDTH, HEIGHT, ROOT_FONT, ROOT_FONT, flags),
            )
            .unwrap();
    }
    fn computed(&mut self, id: u32) -> Vec<u8> {
        let length = self
            .func::<u32, u32>("view_engine_computed")
            .call(&mut self.store, id)
            .unwrap();
        let at = self.call0("view_engine_output");
        let memory = self.instance.get_memory(&mut self.store, "memory").unwrap();
        let mut bytes = vec![0; length as usize];
        memory.read(&self.store, at as usize, &mut bytes).unwrap();
        bytes
    }
}

/// One corpus case.
struct Case {
    name: String,
    sheet: String,
    classes: Vec<String>,
    style: Vec<(CssProperty, String)>,
}
fn corpus() -> Vec<Case> {
    let value: serde_json::Value = serde_json::from_str(CORPUS).unwrap();
    value["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| Case {
            name: case["name"].as_str().unwrap().into(),
            sheet: case["sheet"].as_str().unwrap().into(),
            classes: case["classes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|class| class.as_str().unwrap().into())
                .collect(),
            style: case["style"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    let name = pair[0].as_str().unwrap();
                    let property = CssProperty::ALL
                        .iter()
                        .copied()
                        .find(|property| property.name() == name)
                        .unwrap();
                    (property, pair[1].as_str().unwrap().into())
                })
                .collect(),
        })
        .collect()
}
fn element(index: usize) -> u32 {
    u32::try_from(index).unwrap() + 2
}

/// A case's verdict: the status of each call in order (stopping at the
/// first refusal) and, when accepted, the deepest element's computed values.
#[derive(Debug, Clone, PartialEq)]
struct Verdict {
    statuses: Vec<u32>,
    computed: Vec<u8>,
}

fn run(driver: &mut impl Driver, case: &Case) -> Verdict {
    driver.begin();
    let mut statuses = Vec::new();
    let mut parent = varta_common::view::ROOT_NODE;
    let mut refused = false;
    let call = |status: u32, statuses: &mut Vec<u32>| {
        statuses.push(status);
        status != session::status::OK
    };
    for (index, class) in case.classes.iter().enumerate() {
        let id = element(index);
        refused = call(driver.create(id), &mut statuses)
            || call(driver.insert(parent, id), &mut statuses)
            || call(driver.class(id, class), &mut statuses);
        if refused {
            break;
        }
        parent = id;
    }
    if !refused {
        refused = call(driver.sheet(1, &case.sheet), &mut statuses);
    }
    let target = element(case.classes.len() - 1);
    for (property, value) in &case.style {
        if refused {
            break;
        }
        refused = call(driver.style(target, property.code(), value), &mut statuses);
    }
    if !refused {
        refused = call(driver.charge(), &mut statuses);
    }
    if refused {
        driver.abort();
        return Verdict {
            statuses,
            computed: Vec::new(),
        };
    }
    statuses.push(driver.commit());
    driver.environment(0);
    let computed = driver.computed(target);
    Verdict { statuses, computed }
}

/// The computed values of an encoded record.
fn decode(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut at = 0;
    while at < bytes.len() {
        let code = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let length = u32::from_le_bytes(bytes[at + 2..at + 6].try_into().unwrap()) as usize;
        let text = std::str::from_utf8(&bytes[at + 6..at + 6 + length]).unwrap();
        out.insert(
            CssProperty::from_code(code).unwrap().name().to_owned(),
            text.to_owned(),
        );
        at += 6 + length;
    }
    out
}

struct Deny;
impl Broker for Deny {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> Access {
        Access::Denied
    }
    fn clipboard_write(&mut self) -> Access {
        Access::Denied
    }
}

/// The native renderer's verdict on the same case: accepted, and the
/// deepest element's snapshot style.
fn native(case: &Case) -> (bool, serde_json::Value) {
    let mut view = View::default();
    view.set_environment(varta_view::Environment {
        width: WIDTH,
        height: HEIGHT,
        ..varta_view::Environment::default()
    })
    .unwrap();
    let mut batch = BatchWriter::new();
    let mut parent = varta_common::view::ROOT_NODE;
    for (index, class) in case.classes.iter().enumerate() {
        let id = element(index);
        batch
            .create_element(id, Tag::Div)
            .insert(parent, id, 0)
            .set_class(id, class);
        parent = id;
    }
    batch.stylesheet(1, &case.sheet);
    for (property, value) in &case.style {
        batch.set_style(parent, *property, value);
    }
    let accepted = view.commit(&batch.finish(), false, &mut Deny).is_ok();
    let mut node = view.snapshot()["tree"].clone();
    for _ in &case.classes {
        node = node["children"][0].clone();
    }
    (accepted, node["style"].clone())
}

#[test]
fn browser_and_native_decide_the_corpus_identically() {
    let engine = Engine::default();
    let module = Module::new(&engine, ENGINE).unwrap();
    let mut record = serde_json::Map::new();
    let (mut accepted, mut refused) = (0, 0);
    for case in corpus() {
        // A fresh session per case, as a fresh view starts.
        let browser = run(&mut Wasm::new(&engine, &module), &case);
        let shared = run(
            &mut Session::new(session::environment(
                (WIDTH, HEIGHT),
                ROOT_FONT,
                ROOT_FONT,
                0,
            )),
            &case,
        );
        assert_eq!(
            browser, shared,
            "{}: wasm32 and native sessions differ",
            case.name
        );
        let ok = browser.statuses.iter().all(|status| *status == 0);
        let (renderer, style) = native(&case);
        assert_eq!(
            renderer, ok,
            "{}: the renderer and the session disagree",
            case.name
        );
        let computed = decode(&browser.computed);
        if ok {
            accepted += 1;
            // Keyword-free values are what the renderer shows; it resolves
            // `inherit`/`initial` against its own user-agent origin.
            for (property, value) in &computed {
                if value != "inherit" && value != "initial" {
                    assert_eq!(
                        style.get(property).and_then(|v| v.as_str()),
                        Some(value.as_str()),
                        "{}: {property}",
                        case.name
                    );
                }
            }
        } else {
            refused += 1;
        }
        record.insert(
            case.name.clone(),
            serde_json::json!({"statuses": browser.statuses, "computed": computed}),
        );
    }
    assert!(
        accepted > 200 && refused > 20,
        "{accepted} accepted, {refused} refused"
    );
    let record = serde_json::Value::Object(record);
    let text = serde_json::to_string_pretty(&record).unwrap() + "\n";
    if std::env::var_os("VARTA_UPDATE_VIEW_ENGINE_EXPECTED").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/browser/view-engine-expected.json");
        std::fs::write(path, text).unwrap();
        return;
    }
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(EXPECTED).unwrap(),
        record,
        "view-engine-expected.json is stale; rerun with VARTA_UPDATE_VIEW_ENGINE_EXPECTED=1"
    );
}

#[test]
fn view_engine_matches_its_sources() {
    const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
    const FNV_PRIME: u64 = 0x0100_0000_01B3;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for directory in [
        "crates/varta-style",
        "crates/varta-style-wasm",
        "crates/varta-common/src/view",
    ] {
        let mut pending = vec![root.join(directory)];
        while let Some(path) = pending.pop() {
            for entry in std::fs::read_dir(&path).unwrap() {
                let path = entry.unwrap().path();
                let relative = path.strip_prefix(root.join(directory)).unwrap();
                if relative
                    .components()
                    .any(|part| part.as_os_str() == "target")
                {
                    continue;
                }
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.push(
                        path.strip_prefix(&root)
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .replace('\\', "/"),
                    );
                }
            }
        }
    }
    files.extend(
        [
            "Cargo.toml",
            "src/lib.rs",
            "src/abi.rs",
            "src/abi/static_region.rs",
            "src/bigint.rs",
            "src/exceptions.rs",
            "src/fuel.rs",
            "src/json/mod.rs",
            "src/json/build.rs",
            "src/json/descriptor.rs",
            "src/json/error.rs",
            "src/json/read.rs",
            "src/json/scan.rs",
            "src/json/schema.rs",
            "src/limits.rs",
            "src/memory.rs",
            "src/text.rs",
            "src/value.rs",
            "src/value/error.rs",
            "src/value/primitive.rs",
            "src/value/reference.rs",
            "src/value/symbol.rs",
            "src/value/vector.rs",
            "src/value/dictionary.rs",
            "src/function.rs",
            "src/function/view.rs",
            "src/view.rs",
        ]
        .map(|file| format!("crates/varta-common/{file}")),
    );
    files.sort();
    let mut value = FNV_OFFSET;
    for name in &files {
        let content = std::fs::read(root.join(name)).unwrap();
        for byte in name.bytes().chain([0]).chain(content).chain([0]) {
            value = (value ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
        }
    }
    let metadata: serde_json::Value =
        serde_json::from_str(include_str!("view-engine.json")).unwrap();
    assert_eq!(
        metadata["sourceFingerprint"],
        format!("{value:016x}"),
        "view-engine.wasm is stale; run scripts/build-view-engine.py"
    );
    assert_eq!(
        metadata["bytes"],
        ENGINE.len(),
        "view-engine.json does not describe view-engine.wasm"
    );
}
