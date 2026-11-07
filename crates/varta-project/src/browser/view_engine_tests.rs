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
use std::sync::Arc;
use varta_common::view::{Attribute, CssProperty, Tag};
use varta_style::fonts::{
    FaceStyle, Family, FontSet, Generic, HostFamily, HostFonts, SizedMetrics, host_metrics,
};
use varta_style::session::{self, Session};
use varta_view::{Access, BatchWriter, Broker, View};
use wasmtime::{Caller, Engine, Instance, Linker, Module, Store, TypedFunc};

const ENGINE: &[u8] = include_bytes!("view-engine.wasm");
const CORPUS: &str = include_str!("view-engine-corpus.json");
const EXPECTED: &str = include_str!("view-engine-expected.json");
/// The recorded inputs every case is computed with.
const WIDTH: f64 = 1600.0;
const HEIGHT: f64 = 1000.0;
const ROOT_FONT: f64 = 16.0;

/// The packaged faces every host registers (the style engine's metric
/// fixtures), as `(family, bytes, generic aliases)`.
const FACES: &[(&str, &[u8], &[Generic])] = &[
    (
        "Varta Metrics",
        include_bytes!("../../../varta-style/tests/fixtures/metrics.ttf"),
        &[Generic::Monospace],
    ),
    (
        "Varta Metrics",
        include_bytes!("../../../varta-style/tests/fixtures/metrics-bold.ttf"),
        &[],
    ),
];

/// The browser's measurements in every host of this test: a deterministic
/// stand-in for the bridge (`view-engine.test.mjs` answers the same) for
/// the default font and the generic families the faces do not name.
fn measures(kind: u32, family: &str, weight: f64, italic: bool, size: f64) -> Option<[f64; 5]> {
    // `varta-style-wasm`'s import kinds: named, generic, default.
    let (ex, cap, ch, line) = match (kind, family) {
        (2, _) | (1, "serif") => (0.45, 0.66, 0.5, 1.15),
        (1, "sans-serif") => (0.52, 0.72, 0.56, 1.15),
        (1, "system-ui") => (0.53, 0.7, 0.62, 1.21),
        _ => return None,
    };
    let bold = if weight >= 600.0 { 1.05 } else { 1.0 };
    let slant = if italic { 1.02 } else { 1.0 };
    Some([
        ex * size * bold,
        cap * size,
        ch * size * bold * slant,
        size,
        (line * size).round(),
    ])
}

#[derive(Debug)]
struct TestHost;
impl HostFonts for TestHost {
    fn metrics(&self, family: HostFamily<'_>, style: FaceStyle, size: f64) -> Option<SizedMetrics> {
        let (kind, name) = match family {
            HostFamily::Named(name) => (0, name),
            HostFamily::Generic(generic) => (1, generic.name()),
            HostFamily::Default => (2, ""),
        };
        let [ex, cap, ch, ic, line] = measures(kind, name, style.weight, style.italic, size)?;
        host_metrics(ex, cap, ch, ic, line)
    }
}

/// The font set of the native session and renderer.
fn font_set() -> Arc<FontSet> {
    let mut set = FontSet::new(None, Some(Arc::new(TestHost)));
    for (family, bytes, generics) in FACES {
        let aliases: Vec<Family> = generics.iter().copied().map(Family::Generic).collect();
        set.add_file(family, &aliases, bytes).unwrap();
    }
    Arc::new(set)
}

/// One session's calls, natively or through the wasm32 module.
trait Driver {
    fn begin(&mut self);
    fn commit(&mut self) -> u32;
    fn abort(&mut self);
    fn create(&mut self, id: u32, tag: Tag) -> u32;
    fn insert(&mut self, parent: u32, child: u32) -> u32;
    fn attribute(&mut self, id: u32, name: &str, value: &str) -> u32;
    fn value(&mut self, id: u32, value: &str) -> u32;
    fn flags(&mut self, id: u32, disabled: bool, checked: bool) -> u32;
    fn class(&mut self, id: u32, text: &str) -> u32;
    fn style(&mut self, id: u32, property: u16, text: &str) -> u32;
    fn sheet(&mut self, id: u32, text: &str) -> u32;
    fn charge(&mut self) -> u32;
    fn environment(&mut self, flags: u32);
    fn computed(&mut self, id: u32) -> Vec<u8>;
    /// The computed values of pseudo-element `kind` (its index in
    /// `PseudoElement::ALL`) of element `id`.
    fn computed_pseudo(&mut self, id: u32, kind: u32) -> Vec<u8>;
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
    fn create(&mut self, id: u32, tag: Tag) -> u32 {
        Session::create(self, id, Some(tag))
    }
    fn insert(&mut self, parent: u32, child: u32) -> u32 {
        Session::insert(self, parent, child, 0)
    }
    fn attribute(&mut self, id: u32, name: &str, value: &str) -> u32 {
        self.set_attribute(id, name, value)
    }
    fn value(&mut self, id: u32, value: &str) -> u32 {
        self.set_value(id, value)
    }
    fn flags(&mut self, id: u32, disabled: bool, checked: bool) -> u32 {
        self.set_flags(id, disabled, checked)
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
    fn computed_pseudo(&mut self, id: u32, kind: u32) -> Vec<u8> {
        let kind = varta_style::PseudoElement::ALL[kind as usize];
        session::encode_computed(&Session::computed_pseudo(self, id, kind))
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
        let mut linker = Linker::<()>::new(engine);
        linker
            .func_wrap(
                "varta_view",
                "font_metrics",
                |mut caller: Caller<'_, ()>,
                 kind: u32,
                 family: u32,
                 length: u32,
                 weight: f64,
                 italic: u32,
                 size: f64,
                 out: u32|
                 -> u32 {
                    let memory = caller
                        .get_export("memory")
                        .and_then(wasmtime::Extern::into_memory)
                        .unwrap();
                    let mut name = vec![0; length as usize];
                    memory.read(&caller, family as usize, &mut name).unwrap();
                    let name = String::from_utf8(name).unwrap();
                    let Some(values) = measures(kind, &name, weight, italic == 1, size) else {
                        return 0;
                    };
                    let bytes: Vec<u8> = values
                        .iter()
                        .flat_map(|value| value.to_le_bytes())
                        .collect();
                    memory.write(&mut caller, out as usize, &bytes).unwrap();
                    1
                },
            )
            .unwrap();
        let instance = linker.instantiate(&mut store, module).unwrap();
        let mut wasm = Self { store, instance };
        for (family, bytes, generics) in FACES {
            let bits = generics
                .iter()
                .map(|generic| 1u32 << Generic::ALL.iter().position(|g| g == generic).unwrap())
                .sum::<u32>();
            let name = family.as_bytes();
            let at = wasm
                .func::<u32, u32>("view_engine_input")
                .call(
                    &mut wasm.store,
                    u32::try_from(name.len() + bytes.len()).unwrap(),
                )
                .unwrap();
            let memory = wasm.instance.get_memory(&mut wasm.store, "memory").unwrap();
            memory.write(&mut wasm.store, at as usize, name).unwrap();
            memory
                .write(&mut wasm.store, at as usize + name.len(), bytes)
                .unwrap();
            let status = wasm
                .func::<(u32, u32), u32>("view_engine_face")
                .call(&mut wasm.store, (u32::try_from(name.len()).unwrap(), bits))
                .unwrap();
            assert_eq!(status, session::status::OK);
        }
        wasm
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
    fn create(&mut self, id: u32, tag: Tag) -> u32 {
        self.func::<(u32, u32), u32>("view_engine_create")
            .call(&mut self.store, (id, u32::from(tag.code())))
            .unwrap()
    }
    fn attribute(&mut self, id: u32, name: &str, value: &str) -> u32 {
        self.input(&format!("{name}{value}"));
        self.func::<(u32, u32), u32>("view_engine_attribute")
            .call(&mut self.store, (id, u32::try_from(name.len()).unwrap()))
            .unwrap()
    }
    fn value(&mut self, id: u32, value: &str) -> u32 {
        self.input(value);
        self.func::<u32, u32>("view_engine_value")
            .call(&mut self.store, id)
            .unwrap()
    }
    fn flags(&mut self, id: u32, disabled: bool, checked: bool) -> u32 {
        self.func::<(u32, u32, u32), u32>("view_engine_flags")
            .call(
                &mut self.store,
                (id, u32::from(disabled), u32::from(checked)),
            )
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
        self.output(length)
    }
    fn computed_pseudo(&mut self, id: u32, kind: u32) -> Vec<u8> {
        let length = self
            .func::<(u32, u32), u32>("view_engine_computed_pseudo")
            .call(&mut self.store, (id, kind))
            .unwrap();
        self.output(length)
    }
}
impl Wasm {
    /// The output buffer's first `length` bytes.
    fn output(&mut self, length: u32) -> Vec<u8> {
        let at = self.call0("view_engine_output");
        let memory = self.instance.get_memory(&mut self.store, "memory").unwrap();
        let mut bytes = vec![0; length as usize];
        memory.read(&self.store, at as usize, &mut bytes).unwrap();
        bytes
    }
}

/// One corpus case: a chain of elements (each a `div` unless `tags`
/// names another tag) with their classes, attributes (`[index, name,
/// value]`), field values (`[index, value]`) and checked inputs
/// (`[index]`), the stylesheet and the deepest element's inline values;
/// with `pseudo`, that pseudo-element of the deepest element is computed
/// too.
struct Case {
    pseudo: Option<varta_style::PseudoElement>,
    name: String,
    sheet: String,
    classes: Vec<String>,
    tags: Vec<Tag>,
    attributes: Vec<(usize, Attribute, String)>,
    values: Vec<(usize, String)>,
    checked: Vec<usize>,
    style: Vec<(CssProperty, String)>,
}
fn corpus() -> Vec<Case> {
    let value: serde_json::Value = serde_json::from_str(CORPUS).unwrap();
    value["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| Case {
            pseudo: case["pseudo"].as_str().map(|name| {
                *varta_style::PseudoElement::ALL
                    .iter()
                    .find(|kind| kind.name() == name)
                    .unwrap()
            }),
            name: case["name"].as_str().unwrap().into(),
            sheet: case["sheet"].as_str().unwrap().into(),
            classes: case["classes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|class| class.as_str().unwrap().into())
                .collect(),
            tags: case["classes"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    case["tags"]
                        .get(index)
                        .and_then(serde_json::Value::as_str)
                        .map_or(Tag::Div, |name| {
                            *Tag::ALL.iter().find(|tag| tag.name() == name).unwrap()
                        })
                })
                .collect(),
            attributes: list(&case["attributes"])
                .iter()
                .map(|row| {
                    let name = row[1].as_str().unwrap();
                    let attribute = *Attribute::ALL
                        .iter()
                        .find(|attribute| attribute.name() == name)
                        .unwrap();
                    (
                        usize::try_from(row[0].as_u64().unwrap()).unwrap(),
                        attribute,
                        row[2].as_str().unwrap().into(),
                    )
                })
                .collect(),
            values: list(&case["values"])
                .iter()
                .map(|row| {
                    (
                        usize::try_from(row[0].as_u64().unwrap()).unwrap(),
                        row[1].as_str().unwrap().into(),
                    )
                })
                .collect(),
            checked: list(&case["checked"])
                .iter()
                .map(|index| usize::try_from(index.as_u64().unwrap()).unwrap())
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
/// An optional array of a case.
fn list(value: &serde_json::Value) -> Vec<serde_json::Value> {
    value.as_array().cloned().unwrap_or_default()
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
    pseudo: Vec<u8>,
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
        refused = call(driver.create(id, case.tags[index]), &mut statuses)
            || call(driver.insert(parent, id), &mut statuses)
            || call(driver.class(id, class), &mut statuses);
        if refused {
            break;
        }
        parent = id;
    }
    // The model mirrors attributes, field values and checked states as it
    // admits them (the native view keeps its own).
    for (index, attribute, value) in &case.attributes {
        if !refused {
            refused = call(
                driver.attribute(element(*index), attribute.name(), value),
                &mut statuses,
            );
        }
    }
    for (index, value) in &case.values {
        if !refused {
            refused = call(driver.value(element(*index), value), &mut statuses);
        }
    }
    for index in &case.checked {
        if !refused {
            refused = call(driver.flags(element(*index), false, true), &mut statuses);
        }
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
            pseudo: Vec::new(),
        };
    }
    statuses.push(driver.commit());
    driver.environment(0);
    let computed = driver.computed(target);
    let pseudo = case.pseudo.map_or_else(Vec::new, |kind| {
        let index = varta_style::PseudoElement::ALL
            .iter()
            .position(|other| *other == kind)
            .unwrap();
        driver.computed_pseudo(target, u32::try_from(index).unwrap())
    });
    Verdict {
        statuses,
        computed,
        pseudo,
    }
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
    view.set_fonts(font_set());
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
            .create_element(id, case.tags[index])
            .insert(parent, id, 0)
            .set_class(id, class);
        parent = id;
    }
    for (index, attribute, value) in &case.attributes {
        batch.set_attribute(element(*index), *attribute, value);
    }
    for (index, value) in &case.values {
        batch.set_value(element(*index), value, 0);
    }
    for index in &case.checked {
        batch.set_checked(element(*index), 1, 0);
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
        let mut native_session = Session::new(session::environment(
            (WIDTH, HEIGHT),
            ROOT_FONT,
            ROOT_FONT,
            0,
        ));
        native_session.set_fonts(font_set());
        let shared = run(&mut native_session, &case);
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
        let mut entry = serde_json::json!({"statuses": browser.statuses, "computed": computed});
        if case.pseudo.is_some() && ok {
            entry["pseudo"] = serde_json::json!(decode(&browser.pseudo));
        }
        record.insert(case.name.clone(), entry);
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
            "src/console.rs",
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
            "src/value/snapshot.rs",
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
