//! An integrity-bound, source-free browser bundle. The loader and Worker are
//! self-contained; neither follows unchecked module imports at execution time.
use super::BrowserArtifact;
use crate::{ProjectError, Result};
use base64::Engine;
use sha2::Digest;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use varta_common::{HostOperation as H, budget::BudgetKind, language::LanguageProfile};

const SCHEMA_VERSION: u32 = 1;
const TRUSTED_TYPES_POLICY: &str = "varta-worker";
const WORKER: &str = include_str!("worker.js");
const LOADER: &str = include_str!("loader.js");
const SCHEDULER: &str = include_str!("scheduler.js");
const REASONS: &str = include_str!("reasons.js");
const NETWORK_POLICY: &str = include_str!("network-policy.js");
const NETWORK: &str = include_str!("network.js");
const TRANSPORT: &str = include_str!("network-transport.js");
const TIMERS: &str = include_str!("timers.js");
const VIEW_CODEC: &str = include_str!("view-codec.mjs");
const VIEW_ENGINE_MODULE: &str = include_str!("view-engine.mjs");
const VIEW_MODEL: &str = include_str!("view-model.mjs");
const VIEW_SELECT: &str = include_str!("view-select.mjs");
const VIEW_DOM: &str = include_str!("view-dom.mjs");
const UI_TIMING: &str = include_str!("ui-timing.mjs");
const VIEW_QUEUE: &str = include_str!("view-queue.mjs");
const VIEW_WORKER: &str = include_str!("view-worker.mjs");
const VIEW_POLICY: &str = include_str!("view-policy.mjs");
const VIEW_FONTS: &str = include_str!("view-fonts.mjs");
const VIEW_NAMING: &str = include_str!("view-naming.mjs");
const PERMISSIONS: &str = include_str!("permission-broker.mjs");
const PERMISSION_STORE: &str = include_str!("permission-store.mjs");
const PERMISSION_PROMPT: &str = include_str!("permission-prompt.mjs");
const EVENT_SOURCES: &str = include_str!("event-sources.mjs");
const ENVIRONMENT: &str = include_str!("environment.mjs");
const ENVIRONMENT_ABI: &str = include_str!("environment-abi.mjs");
const KV_STORE: &str = include_str!("kv-store.mjs");
/// The KV value layer (`varta-kv-codec`) compiled to wasm32 by
/// `scripts/build-kv-codec.py`; served beside a program that imports KV.
const KV_CODEC: &[u8] = include_bytes!("kv-codec.wasm");
const KV_CODEC_FILE: &str = "varta-kv-codec.wasm";
/// The view style engine (`varta-style`) compiled to wasm32 by
/// `scripts/build-view-engine.py`; served beside a program with a view, and
/// instantiated by the Worker's and the page's view models.
const VIEW_ENGINE: &[u8] = include_bytes!("view-engine.wasm");
const VIEW_ENGINE_FILE: &str = "varta-view-engine.wasm";
const FUEL: &str = include_str!("../../../varta-common/browser/fuel.mjs");
const ORIGINAL: &str = "app.wasm";
const IMAGE: &str = "app.private.wasm";
const MANIFEST: &str = "app.manifest.json";
const WORKER_FILE: &str = "varta-worker.js";
const LOADER_FILE: &str = "varta-host.js";
// Standalone page geometry only. Embedded views keep their owner's container.
// This exact stylesheet is authorized by its CSP hash, never unsafe-inline.
const PAGE_STYLE: &str = "html,body{height:100%;margin:0}body{display:flex;flex-direction:column}#varta-view{flex:1;min-height:0}#varta-view:empty{display:none}#varta-output{flex:none;margin:0;max-height:25vh;overflow:auto;white-space:pre-wrap}#varta-output:empty{display:none}";

/// Current R1 effect implementations. Unsupported effects fail before producing
/// a runnable bundle; extending this surface requires matching effect tests.
/// An implementation is admitted only under the profiles whose native host
/// imports that operation ([`admitted_effect`]).
const EFFECTS: &[H] = &[
    H::LogInt,
    H::LogFloatV2,
    H::LogUndefined,
    H::LogString16,
    H::EnvGet16,
    H::EnvHas16,
    H::PromiseReact,
    H::PromiseSettle,
    H::SchedulerEnqueue,
    H::PromiseUnhandled,
    H::PromiseHandled,
    H::TimerStart,
    H::TimerStartV2,
    H::TimerClear,
    H::TimerTake,
    H::TimerUncaught,
    H::NetRequestStartV2,
    H::NetRequestWatch,
    H::NetRequestTake,
    H::NetRequestText,
    H::NetBodyRead,
    H::NetBodyReadBytes,
    H::NetRequestAbort,
    H::ViewCommit,
    H::PermissionQuery,
    H::PermissionRequest,
    H::EventWatch,
    H::EventTake,
    H::EventText,
    H::EventSourceOpen,
    H::EventSourceClose,
    // Pilot effects: the clock behind `time.now`, ungated entropy,
    // the `env.get` snapshot and the `process.exit` exit.
    H::DateNow,
    H::MathRandom,
    H::EnvSnapshot,
    H::ExitFloat,
    // V38 local time: the recorded zone and locale (`worker.js`).
    H::IntlTimeZone,
    H::IntlLocale,
    // V38 KV storage: OPFS container and the shared value layer
    // (`kv-store.mjs`, `kv-codec.wasm`).
    H::KvOpen,
    H::KvRead,
    H::KvCommit,
    H::KvStage,
    H::KvEntry,
    H::KvKey,
    H::KvValue,
    H::KvText,
    H::KvClose,
];

/// Whether the browser host implements `name` for a `profile` module: an
/// implemented effect that the profile's native host can import, exactly as
/// the generated registry's `admittedUnder` lists it for the Worker gate. An
/// implementation never widens a profile: the pilot effects keep the native
/// first profile of each operation (`number_random` from v17,
/// `process_exit_f64` in the f64 profiles, `date_now` from v37,
/// `env_snapshot` in v38).
fn admitted_effect(profile: LanguageProfile, name: &str) -> bool {
    EFFECTS.iter().any(|operation| {
        operation.name() == name && varta_common::browser::admits(profile, *operation)
    })
}

/// The imports of a `profile` module that the browser host refuses, in
/// import order.
pub(super) fn unimplemented_effects<'a>(
    profile: LanguageProfile,
    imports: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    imports
        .into_iter()
        .filter(|name| !admitted_effect(profile, name))
        .collect()
}

/// Engine-owned ESM sources become isolated lexical modules in the verified
/// loader/Worker. No generated file follows a transitive or unverified import.
fn embedded_sources(dom: bool) -> String {
    let modules = [
        (
            "environment",
            "",
            ENVIRONMENT,
            "snapshotEnvironment,createEnvironmentPolicy,createEnvironmentAccess,createEnvironmentSnapshot",
        ),
        (
            "environmentAbi",
            "",
            ENVIRONMENT_ABI,
            "createEnvironmentAbi",
        ),
        (
            "permissionBroker",
            "",
            PERMISSIONS,
            "snapshotDecisionFile,parsePermissionDescriptor,createPermissionBroker",
        ),
        (
            "eventSources",
            "",
            EVENT_SOURCES,
            "createEventSources,snapshotHostEvent",
        ),
        (
            "viewCodec",
            "",
            VIEW_CODEC,
            "ViewProtocolError,decodeViewBatch,encodeViewBatch,decodeViewEvent",
        ),
        (
            "viewEngine",
            "",
            VIEW_ENGINE_MODULE,
            "createViewEngine,readMediaInputs,ENGINE_STATUS,ENGINE_INPUTS,MEDIA_INPUT_QUERIES",
        ),
        (
            "viewSelect",
            "",
            VIEW_SELECT,
            "selectOwner,optionAttribute,normalizeSelect,syncSelect,syncSelectFor,setSelectValue,syncSelectChoice,optionValue,selectionTextBytes,selectOperationWork,selectInventoryWork",
        ),
        ("viewNaming", "", VIEW_NAMING, "potentialNamingWork"),
        (
            "viewModel",
            "const {decodeViewBatch,ViewProtocolError}=viewCodec;const {createViewEngine,ENGINE_STATUS}=viewEngine;const {potentialNamingWork}=viewNaming;const {selectOwner,optionAttribute,normalizeSelect,syncSelect,syncSelectFor,setSelectValue,syncSelectChoice,optionValue,selectionTextBytes,selectOperationWork,selectInventoryWork}=viewSelect;",
            VIEW_MODEL,
            "createViewModel",
        ),
        ("viewPolicy", "", VIEW_POLICY, "createViewPolicy"),
        (
            "viewQueue",
            "const {ViewProtocolError}=viewCodec;",
            VIEW_QUEUE,
            "createViewQueue",
        ),
        (
            "viewWorker",
            "const {ViewProtocolError,decodeViewEvent}=viewCodec;const {createViewModel}=viewModel;",
            VIEW_WORKER,
            "createWorkerView",
        ),
    ];
    let module = |name, bindings, source: &str, exports| {
        let body = source
            .lines()
            .filter(|line| !line.starts_with("import "))
            .map(|line| line.strip_prefix("export ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
        format!("const {name}=(()=>{{\n{bindings}\n{body}\nreturn {{{exports}}};\n}})();\n")
    };
    let mut source = modules
        .into_iter()
        .map(|(name, bindings, source, exports)| module(name, bindings, source, exports))
        .collect::<String>();
    if dom {
        source.push_str(&module("uiTiming", "", UI_TIMING, "createUiTiming"));
        source.push_str(&module(
            "permissionPrompt",
            "",
            PERMISSION_PROMPT,
            "createPromptDriver,createPermissionPrompt",
        ));
        source.push_str(&module(
            "permissionStore",
            "const {snapshotDecisionFile}=permissionBroker;",
            PERMISSION_STORE,
            "createDecisionStore,updateDecisionFile",
        ));
        source.push_str(&module(
            "viewFonts",
            "",
            VIEW_FONTS,
            "createFontNamespace,packagedFamilies,faceDescriptors,FontValueError",
        ));
        source.push_str(&module("viewDom", "const {decodeViewBatch,ViewProtocolError}=viewCodec;const {createViewModel}=viewModel;const {readMediaInputs,MEDIA_INPUT_QUERIES}=viewEngine;const {createFontNamespace}=viewFonts;", VIEW_DOM, "createViewDom,encodeViewEvent"));
    } else {
        // The Worker's KV imports; the parent page never handles KV.
        source.push_str(&module(
            "kvStore",
            "",
            KV_STORE,
            "createKvHost,createKvPolicy,createCodec,openKvStorage,syncHandlesSupported",
        ));
    }
    source
}

/// Fixed file names and immutable bytes, derived only from a prepared artifact.
pub struct BrowserBundle {
    files: BTreeMap<String, Vec<u8>>,
}

impl BrowserArtifact {
    /// Build a self-contained Worker and an ES-module loader with pinned SHA-256
    /// identities. Import admission is still the complete native preparation;
    /// this check additionally identifies not-yet-implemented browser effects.
    pub fn bundle(&self) -> Result<BrowserBundle> {
        self.bundle_with_resources(&[])
    }

    /// Package only checked, immutable resources. Logical names are mapped to
    /// generated file names and hashes; source paths never reach the browser.
    pub fn bundle_with_resources(
        &self,
        resources: &[super::BrowserResource],
    ) -> Result<BrowserBundle> {
        self.bundle_inner(resources, false)
    }

    // Runtime-owned local host memory only. Distribution paths still reject
    // every host ceiling; the dev server never exposes a write_new bundle.
    pub(super) fn development_bundle(
        &self,
        resources: &[super::BrowserResource],
    ) -> Result<BrowserBundle> {
        self.bundle_inner(resources, true)
    }

    fn bundle_inner(
        &self,
        resources: &[super::BrowserResource],
        runtime_host: bool,
    ) -> Result<BrowserBundle> {
        super::resources::validate(resources)?;
        if !resources.is_empty() && !self.inspection.language.profile.unwrap_or_default().tsx() {
            return Err(ProjectError::Usage("browser resources require v38".into()));
        }
        if self.ceiling.is_some() && !runtime_host {
            return Err(ProjectError::Usage(
                "host policies are runtime inputs and cannot be embedded in browser builds".into(),
            ));
        }
        if !self
            .inspection
            .exports
            .iter()
            .any(|name| name == varta_common::abi::START_EXPORT)
        {
            return Err(ProjectError::Usage(
                "browser run bundle requires _start".into(),
            ));
        }
        if self.inspection.http_service.is_some() {
            return Err(ProjectError::Usage(
                "browser host does not execute HTTP service contracts; use the native target"
                    .into(),
            ));
        }
        let gaps = unimplemented_effects(
            self.inspection.language.profile.unwrap_or_default(),
            self.inspection
                .imports
                .iter()
                .map(|import| import.name.as_str()),
        );
        if !gaps.is_empty() {
            return Err(ProjectError::Usage(format!(
                "browser host effects not implemented: {}",
                gaps.join(", ")
            )));
        }
        if self.inspection.imports.iter().any(|import| {
            [H::PermissionQuery, H::PermissionRequest]
                .iter()
                .any(|op| op.name() == import.name)
        }) {
            let scopes = self
                .original
                .manifest
                .capabilities
                .iter()
                .filter(|grant| {
                    matches!(
                        varta_common::capability_kind(&grant.id),
                        Some(
                            varta_common::GrantKind::Patterns
                                | varta_common::GrantKind::Urls
                                | varta_common::GrantKind::Listeners
                        )
                    )
                })
                .map(|grant| grant.id.as_str())
                .collect::<Vec<_>>();
            if !scopes.is_empty() {
                return Err(ProjectError::Usage(format!(
                    "browser permission resource scopes not implemented: {}",
                    scopes.join(", ")
                )));
            }
        }
        let manifest = self.original.manifest.to_bytes()?;
        let registry_data = varta_common::browser::registry();
        let registry = serde_json::to_string(&registry_data)
            .map_err(|e| ProjectError::Artifact(e.to_string()))?;
        // The input is an engine-owned static module, never package source.
        // Its only import is the generated registry we embed above.
        let fuel = FUEL
            .lines()
            .filter(|line| !line.starts_with("import "))
            .map(|line| line.strip_prefix("export ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
        let worker_view = embedded_sources(false);
        let worker = format!("const bundleSchemaVersion = {SCHEMA_VERSION};\nconst registry = {registry};\n{worker_view}\n{fuel}\n{REASONS}\n{SCHEDULER}\n{TIMERS}\n{NETWORK_POLICY}\n{NETWORK}\n{TRANSPORT}\n{WORKER}").into_bytes();
        let file = |name: &str, bytes: &[u8]| {
            serde_json::json!({
                "name":name,"bytes":bytes.len(),"identity":varta_runtime::artifact_identity(bytes)
            })
        };
        let profile = self.inspection.language.profile.unwrap_or_default();
        let budgets: BTreeMap<_, _> = BudgetKind::ALL
            .into_iter()
            .map(|kind| (kind.wire_name(), self.budgets.budgets.value(kind)))
            .collect();
        let rules: BTreeMap<_, _> = BudgetKind::ALL
            .into_iter()
            .map(|kind| {
                (
                    kind.wire_name(),
                    serde_json::json!({"maximum":kind.maximum(),"production":kind.production()}),
                )
            })
            .collect();
        let request = self.original.manifest.budget_request();
        let wanted: BTreeMap<_, _> = BudgetKind::ALL
            .into_iter()
            .map(|kind| {
                (
                    kind.wire_name(),
                    request.value(kind).unwrap_or(kind.production()),
                )
            })
            .collect();
        let mut resource_files = BTreeMap::new();
        let resource_descriptors = resources.iter().enumerate().map(|(index, resource)| {
            let name = format!("varta-resource-{index}.{}",resource.extension);
            // A font face carries its FontFace descriptors and the generic
            // families the applier maps to its family, as the native shell does.
            let descriptor = match &resource.packaged {
                Some(face) => serde_json::json!({"key":resource.name,"font":true,"weight":face.weight,"style":face.style.name(),"generics":face.generics,"mediaType":resource.media_type,"file":file(&name,&resource.bytes)}),
                None => serde_json::json!({"key":resource.name,"font":resource.font,"mediaType":resource.media_type,"file":file(&name,&resource.bytes)}),
            };
            resource_files.insert(name,resource.bytes.clone());
            descriptor
        }).collect::<Vec<_>>();
        // A program that imports KV gets the codec module beside it; every
        // descriptor carries the KV ceiling rules `resolvePermissions` checks.
        let uses_kv = self
            .inspection
            .imports
            .iter()
            .any(|import| import.name == H::KvOpen.name());
        let kv = &registry_data["kv"];
        let kv_ceiling = serde_json::json!({
            "capabilities":kv["capabilities"],"separator":kv["paths"]["separator"],
            "maxPatterns":kv["paths"]["maxPatterns"],"maxPatternBytes":kv["paths"]["maxPatternBytes"],
        });
        let descriptor = serde_json::json!({
            "schemaVersion":SCHEMA_VERSION,"profile":profile,"trustedTypesPolicy":TRUSTED_TYPES_POLICY,
            "hostModule":varta_common::abi::HOST_MODULE,
            "original":file(ORIGINAL,&self.original.wasm),"image":file(IMAGE,&self.image),
            "manifest":file(MANIFEST,&manifest),"worker":file(WORKER_FILE,&worker),
            "booleanCapabilities":registry_data["booleanCapabilities"],"network":registry_data["network"],
            "view":registry_data["view"],"hasView":self.inspection.imports.iter().any(|import|import.name==H::ViewCommit.name()),
            "permissions":registry_data["permissions"],"environment":registry_data["environment"],"effects":registry_data["effects"],
            "hasPermissions":!self.original.manifest.capabilities.is_empty(),
            "budgets":budgets,"budgetRules":rules,"requestedBudgets":wanted,"turnBudgets":profile.turn_budgets(),"imports":self.inspection.imports,
            "memoryExport":varta_common::abi::MEMORY_EXPORT,
            "pendingExport":varta_common::exceptions::PENDING,
            "startExport":varta_common::abi::START_EXPORT,
            "maxStringBytes":profile.limits().data.string_bytes,
            "decodeChunkBytes":varta_common::abi::WASM_PAGE_BYTES,
            "resources":resource_descriptors,
            "fairFuel":profile.fair_fuel(),
            "kv":kv_ceiling,
        });
        let mut descriptor = descriptor;
        if uses_kv {
            descriptor["kvCodec"] = file(KV_CODEC_FILE, KV_CODEC);
            resource_files.insert(KV_CODEC_FILE.to_owned(), KV_CODEC.to_vec());
        }
        if descriptor["hasView"] == true {
            descriptor["viewEngine"] = file(VIEW_ENGINE_FILE, VIEW_ENGINE);
            resource_files.insert(VIEW_ENGINE_FILE.to_owned(), VIEW_ENGINE.to_vec());
        }
        let descriptor = serde_json::to_string(&descriptor)
            .map_err(|e| ProjectError::Artifact(e.to_string()))?;
        let parent_view = embedded_sources(true);
        // The loader validates an embedder's recorded network log with the
        // transport module's reader before any file loads.
        let loader = format!("const descriptor = {descriptor};\nconst registry = {{network:descriptor.network,view:descriptor.view,permissions:descriptor.permissions,environment:descriptor.environment}};\n{parent_view}\n{NETWORK_POLICY}\n{TRANSPORT}\n{LOADER}").into_bytes();
        let origins = varta_common::browser::request_origins(&self.original.manifest)?;
        // URL domains admit punctuation that CSP host-source tokens do not.
        // Reject it before HTML generation: neither attribute escaping nor a
        // partially parsed CSP may widen the generated delivery boundary.
        if origins.iter().any(|origin| {
            !origin
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-:/".contains(&byte))
        }) {
            return Err(ProjectError::Usage(
                "request origin cannot be represented in browser CSP".into(),
            ));
        }
        let origins = origins.join(" ");
        let integrity = format!(
            "sha256-{}",
            base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(&loader))
        );
        let style_integrity = format!(
            "sha256-{}",
            base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(PAGE_STYLE))
        );
        let html=format!(r#"<!doctype html>
<html lang="en"><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src '{integrity}' 'wasm-unsafe-eval'; style-src '{style_integrity}'; worker-src blob:; img-src blob:; connect-src 'self' {origins}; base-uri 'none'; object-src 'none'; trusted-types {TRUSTED_TYPES_POLICY}; require-trusted-types-for 'script'">
<style>{PAGE_STYLE}</style>
<title>Varta application</title>
<main id="varta-view"></main>
<pre id="varta-output" aria-live="polite">Starting…</pre>
<script type="module" data-varta-bootstrap src="./{LOADER_FILE}" integrity="{integrity}" crossorigin="anonymous"></script>
</html>
"#).into_bytes();
        resource_files.extend(
            [
                (ORIGINAL, self.original.wasm.clone()),
                (IMAGE, self.image.clone()),
                (MANIFEST, manifest),
                (WORKER_FILE, worker),
                (LOADER_FILE, loader),
                ("index.html", html),
            ]
            .map(|(name, bytes)| (name.to_owned(), bytes)),
        );
        Ok(BrowserBundle {
            files: resource_files,
        })
    }
}

impl BrowserBundle {
    pub(super) fn into_files(self) -> BTreeMap<String, Vec<u8>> {
        self.files
    }
    #[must_use]
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
    /// Write only to a new directory. Existing bundles are never partly
    /// overwritten, and all admission/generation happens before the first write.
    pub fn write_new(&self, directory: &Path) -> Result<Vec<PathBuf>> {
        std::fs::create_dir(directory)?;
        let mut paths = Vec::new();
        for (name, bytes) in &self.files {
            use std::io::Write;
            let path = directory.join(name);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(bytes)?;
            paths.push(path);
        }
        Ok(paths)
    }
}
