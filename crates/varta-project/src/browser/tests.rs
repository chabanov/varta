use super::*;
use varta_common::{
    abi,
    budget::{BudgetKind, BudgetRequest},
    language::LanguageProfile,
};

fn artifact(profile: LanguageProfile) -> Artifact {
    let wasm = varta_compiler::compile_typescript_with_profile(
        "export function main():void { console.log(123); }",
        profile,
    )
    .unwrap();
    let mut manifest = Manifest::new(&[]);
    manifest.abi_version = profile.host_abi();
    Artifact { wasm, manifest }
}

#[test]
fn prepared_browser_image_owns_original_and_policies_without_rewriting_them() {
    let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
    let original = source.wasm.clone();
    let empty = HostPolicy::from_manifest(source.manifest.clone()).unwrap();
    let prepared = BrowserArtifact::prepare(&source, Some(&empty)).unwrap();
    assert_eq!(prepared.original().wasm, original);
    assert_ne!(prepared.execution_image(), original);
    assert!(prepared.ceiling().unwrap().capabilities.is_empty());
    assert_eq!(
        prepared.inspection().language.profile,
        Some(LanguageProfile::CheckedF64Utf16V38)
    );
    assert!(
        prepared
            .inspection()
            .imports
            .iter()
            .any(|import| import.name == "log_f64_v2")
    );
    source.wasm.clear();
    source.manifest.abi_version = abi::VERSION;
    assert_eq!(prepared.original().wasm, original);
    assert_eq!(
        prepared.original().manifest.abi_version,
        LanguageProfile::CheckedF64Utf16V38.host_abi()
    );
}

#[test]
fn validated_host_budget_caps_request_and_no_host_keeps_production_cap() {
    let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
    source.manifest.budgets = Some(BudgetRequest {
        memory_bytes: Some(16 * 1024 * 1024),
        ..Default::default()
    });
    let plain = BrowserArtifact::prepare(&source, None).unwrap();
    assert!(plain.ceiling().is_none());
    assert_eq!(plain.budgets().budgets.memory_bytes, abi::MAX_MEMORY_BYTES);
    assert!(
        plain
            .budgets()
            .capped
            .iter()
            .any(|cap| cap.kind == BudgetKind::Memory)
    );
    let mut ceiling = Manifest::new(&[]);
    ceiling.abi_version = source.manifest.abi_version;
    ceiling.budgets = Some(BudgetRequest {
        memory_bytes: Some(12 * 1024 * 1024),
        ..Default::default()
    });
    let host = HostPolicy::from_manifest(ceiling).unwrap();
    let capped = BrowserArtifact::prepare(&source, Some(&host)).unwrap();
    assert_eq!(capped.budgets().budgets.memory_bytes, 12 * 1024 * 1024);
    assert_eq!(
        capped.original().manifest.budget_request().memory_bytes,
        Some(16 * 1024 * 1024)
    );
}

#[test]
fn host_ceiling_smaller_than_initial_memory_fails_preparation() {
    let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
    source.manifest.budgets = Some(BudgetRequest {
        memory_bytes: Some(1),
        ..Default::default()
    });
    assert!(matches!(
        BrowserArtifact::prepare(&source, None),
        Err(crate::ProjectError::Compile(_))
    ));
}

#[test]
fn old_profiles_keep_frozen_budgets_and_failures_never_fall_back_unmetered() {
    let source = artifact(LanguageProfile::CheckedI32Utf8V1);
    let prepared = BrowserArtifact::prepare(&source, None).unwrap();
    assert_eq!(prepared.budgets().budgets, Budgets::PRODUCTION);
    let mut bad = source;
    bad.manifest.abi_version += 1;
    assert!(BrowserArtifact::prepare(&bad, None).is_err());
    bad.manifest.abi_version -= 1;
    // Duplicate a reserved metadata section; validation must reject it.
    let name = b"varta.language";
    bad.wasm.extend([
        0,
        u8::try_from(name.len() + 1).unwrap(),
        u8::try_from(name.len()).unwrap(),
    ]);
    bad.wasm.extend(name);
    assert!(BrowserArtifact::prepare(&bad, None).is_err());
}

#[test]
fn browser_preparation_does_not_mount_host_filesystem_roots() {
    use varta_common::files::{FsRoots, RootDirectory};
    let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
    source.manifest.fs_roots = Some(FsRoots::Names(vec!["project".into()]));
    let mut ceiling = Manifest::new(&[]);
    ceiling.abi_version = source.manifest.abi_version;
    ceiling.fs_roots = Some(FsRoots::Directories(vec![RootDirectory {
        name: "project".into(),
        directory: "/varta-browser-preparation-does-not-open-this-directory".into(),
    }]));
    let host = HostPolicy::from_manifest(ceiling).unwrap();
    let prepared = BrowserArtifact::prepare(&source, Some(&host)).unwrap();
    assert_eq!(prepared.original().manifest.root_names(), ["project"]);
    assert_eq!(prepared.ceiling().unwrap().root_directories().len(), 1);
}

#[test]
fn unknown_imports_and_reserved_private_markers_are_rejected() {
    let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
    let name = b"log_f64_v2";
    let at = source
        .wasm
        .windows(name.len())
        .position(|window| window == name)
        .unwrap();
    source.wasm[at + name.len() - 1] = b'9';
    assert!(matches!(
        BrowserArtifact::prepare(&source, None),
        Err(crate::ProjectError::Runtime(_))
    ));
    let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
    let name = varta_common::browser::METER_SECTION.as_bytes();
    source.wasm.extend([
        0,
        u8::try_from(name.len() + 1).unwrap(),
        u8::try_from(name.len()).unwrap(),
    ]);
    source.wasm.extend(name);
    assert!(matches!(
        BrowserArtifact::prepare(&source, None),
        Err(crate::ProjectError::Compile(_))
    ));
}

#[test]
fn bundle_pins_original_manifest_image_and_self_contained_worker() {
    let source = artifact(LanguageProfile::CheckedF64Utf16V38);
    let prepared = BrowserArtifact::prepare(&source, None).unwrap();
    let bundle = prepared.bundle().unwrap();
    assert_eq!(bundle.files()["app.wasm"], source.wasm);
    assert_eq!(
        bundle.files()["app.manifest.json"],
        source.manifest.to_bytes().unwrap()
    );
    let loader = std::str::from_utf8(&bundle.files()["varta-host.js"]).unwrap();
    for name in [
        "app.wasm",
        "app.private.wasm",
        "app.manifest.json",
        "varta-worker.js",
    ] {
        assert!(
            loader.contains(&varta_runtime::artifact_identity(&bundle.files()[name])),
            "{name} not bound"
        );
    }
    let worker = std::str::from_utf8(&bundle.files()["varta-worker.js"]).unwrap();
    assert!(
        !worker
            .lines()
            .any(|line| line.starts_with("import ") || line.starts_with("export "))
    );
    let page = std::str::from_utf8(&bundle.files()["index.html"]).unwrap();
    assert!(page.contains("Content-Security-Policy"));
    assert!(page.contains("require-trusted-types-for 'script'"));
    assert!(page.contains("integrity=\"sha256-"));
    // Standalone layout must work under the unchanged deny-by-default CSP.
    // Authorize only the emitted style bytes; whitespace drift must change it.
    use base64::Engine;
    use sha2::Digest;
    let style = page
        .split_once("<style>")
        .unwrap()
        .1
        .split_once("</style>")
        .unwrap()
        .0;
    let style_hash = base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(style));
    assert!(page.contains(&format!("style-src 'sha256-{style_hash}'")));
    assert!(!page.contains("'unsafe-inline'"));
}

#[test]
fn bundle_accepts_environment_imports_with_checked_host_boundary() {
    let profile = LanguageProfile::CheckedF64Utf16V38;
    let mut source = artifact(profile);
    source.wasm = varta_compiler::compile_typescript_with_profile(
        "export function main():void {console.log(varta.env.get(\"TEST\"));}",
        profile,
    )
    .unwrap();
    let prepared = BrowserArtifact::prepare(&source, None).unwrap();
    assert!(prepared.bundle().is_ok());
}

#[test]
fn bundle_writes_only_new_directories_and_keeps_every_generated_byte() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "varta-browser-bundle-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let prepared =
        BrowserArtifact::prepare(&artifact(LanguageProfile::CheckedF64Utf16V38), None).unwrap();
    let bundle = prepared.bundle().unwrap();
    let paths = bundle.write_new(&directory).unwrap();
    assert_eq!(paths.len(), bundle.files().len());
    assert!(bundle.write_new(&directory).is_err());
    for (name, bytes) in bundle.files() {
        assert_eq!(&std::fs::read(directory.join(name)).unwrap(), bytes);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn scheduled_programs_build_with_the_bounded_guest_checkpoint() {
    let profile = LanguageProfile::CheckedF64Utf16V38;
    let mut source = artifact(profile);
    source.wasm=varta_compiler::compile_typescript_with_profile(
        "async function task():Promise<void>{await Promise.resolve(1);console.log(2);} export function main():void{task();}",profile).unwrap();
    let prepared = BrowserArtifact::prepare(&source, None).unwrap();
    let bundle = prepared.bundle().unwrap();
    let worker = std::str::from_utf8(&bundle.files()["varta-worker.js"]).unwrap();
    assert!(worker.contains("scheduler.drain(instance, checkException)"));
    assert!(
        prepared
            .inspection()
            .imports
            .iter()
            .any(|op| op.name == "scheduler_enqueue")
    );
}

#[test]
fn host_policy_never_becomes_embedded_browser_authority() {
    let source = artifact(LanguageProfile::CheckedF64Utf16V38);
    let host = HostPolicy::from_manifest(source.manifest.clone()).unwrap();
    let prepared = BrowserArtifact::prepare(&source, Some(&host)).unwrap();
    assert!(
        matches!(prepared.bundle(),Err(crate::ProjectError::Usage(message)) if message.contains("cannot be embedded"))
    );
}

#[test]
fn network_bundle_admits_canonical_v38_and_limits_csp_to_package_origins() {
    use varta_common::{CapabilityGrant, CapabilityValue, RequestScope};
    let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
    source.wasm = varta_compiler::compile_typescript_with_profile(
        "export async function main():Promise<void>{const r=await fetch(\"https://api.test/v1/a\");console.log(await r.text());}",
        LanguageProfile::CheckedF64Utf16V38,
    ).unwrap();
    source.manifest = Manifest::new(&[CapabilityGrant {
        id: "net.request".into(),
        value: CapabilityValue::Requests(RequestScope {
            urls: vec![
                "https://api.test/v1/**".into(),
                "https://api.test:443/v2/**".into(),
            ],
            methods: Some(vec!["GET".into()]),
            response_headers: None,
        }),
    }]);
    source.manifest.abi_version = LanguageProfile::CheckedF64Utf16V38.host_abi();
    let bundle = BrowserArtifact::prepare(&source, None)
        .unwrap()
        .bundle()
        .unwrap();
    let page = std::str::from_utf8(&bundle.files()["index.html"]).unwrap();
    assert!(
        page.contains("connect-src 'self' https://api.test;"),
        "{page}"
    );
    let worker = std::str::from_utf8(&bundle.files()["varta-worker.js"]).unwrap();
    assert!(worker.contains("function createNetwork("));
    assert!(worker.contains("networkBookkeeping"));
    // Fetch Headers cannot reconstruct the frozen arrival/repeated-field v37
    // head. Fail preparation explicitly rather than changing its wire meaning.
    source.wasm = varta_compiler::compile_typescript_with_profile(
        "export async function main():Promise<void>{const r=await fetch(\"https://api.test/a\");console.log(await r.text());}",
        LanguageProfile::CheckedF64Utf16V37,
    ).unwrap();
    source.manifest.abi_version = LanguageProfile::CheckedF64Utf16V37.host_abi();
    let error = BrowserArtifact::prepare(&source, None)
        .unwrap()
        .bundle()
        .err()
        .unwrap();
    assert!(error.to_string().contains("net_request_start"), "{error}");
}

#[test]
fn network_origin_punctuation_never_becomes_html_or_a_csp_directive() {
    use varta_common::{CapabilityGrant, CapabilityValue, RequestScope};
    for origin in [
        "http://a\"b.test/**",
        "http://a'b.test/**",
        "http://a;b.test/**",
        "http://a&b.test/**",
        "http://a_b.test/**",
        "http://[::1]/**",
    ] {
        let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
        source.manifest = Manifest::new(&[CapabilityGrant {
            id: "net.request".into(),
            value: CapabilityValue::Requests(RequestScope {
                urls: vec![origin.into()],
                methods: None,
                response_headers: None,
            }),
        }]);
        source.manifest.abi_version = LanguageProfile::CheckedF64Utf16V38.host_abi();
        // These are URL-parser-valid domains, but cannot be raw CSP tokens.
        source.manifest.validate_package().unwrap();
        let error = BrowserArtifact::prepare(&source, None)
            .unwrap()
            .bundle()
            .err()
            .unwrap();
        assert!(
            error
                .to_string()
                .contains("cannot be represented in browser CSP"),
            "{error}"
        );
    }
}

#[test]
fn packaged_resources_have_generated_names_and_pinned_bytes_without_changing_portable_input() {
    use base64::Engine;
    let png=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==").unwrap();
    let resource = BrowserResource::png("icons/red.png", png.clone()).unwrap();
    let source = artifact(LanguageProfile::CheckedF64Utf16V38);
    let bundle = BrowserArtifact::prepare(&source, None)
        .unwrap()
        .bundle_with_resources(&[resource])
        .unwrap();
    assert_eq!(bundle.files().len(), 7);
    assert_eq!(bundle.files()["varta-resource-0.png"], png);
    assert_eq!(bundle.files()["app.wasm"], source.wasm);
    assert_eq!(
        bundle.files()["app.manifest.json"],
        source.manifest.to_bytes().unwrap()
    );
    let loader = std::str::from_utf8(&bundle.files()["varta-host.js"]).unwrap();
    assert!(loader.contains("icons/red.png"));
    assert!(loader.contains(&varta_runtime::artifact_identity(&png)));
    assert!(!bundle.files().contains_key("icons/red.png"));
}

fn v38_source(source: &str) -> Artifact {
    let profile = LanguageProfile::CheckedF64Utf16V38;
    let mut artifact = artifact(profile);
    artifact.wasm = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|error| panic!("{error}\n{source}"));
    artifact
}

/// The pilot's clock, entropy, environment snapshot and exit are admitted:
/// the bundle is produced and its Worker carries their handlers.
#[test]
fn bundle_admits_the_pilot_clock_entropy_environment_and_exit_effects() {
    use varta_common::HostOperation as H;
    let source = v38_source(
        "export function main():void{console.log(Date.now()>0);console.log(Math.random()<1);\
         console.log(Object.keys(globalThis.process.env).length);varta.process.exit(3);}",
    );
    let prepared = BrowserArtifact::prepare(&source, None).unwrap();
    for operation in [H::DateNow, H::MathRandom, H::EnvSnapshot, H::ExitFloat] {
        assert!(
            prepared
                .inspection()
                .imports
                .iter()
                .any(|import| import.name == operation.name()),
            "{} not imported",
            operation.name()
        );
    }
    let bundle = prepared.bundle().unwrap();
    let worker = std::str::from_utf8(&bundle.files()["varta-worker.js"]).unwrap();
    assert!(worker.contains("createEnvironmentSnapshot"));
    assert!(worker.contains("crypto.getRandomValues(entropy)"));
    assert!(worker.contains("class GuestExit"));
    let loader = std::str::from_utf8(&bundle.files()["varta-host.js"]).unwrap();
    assert!(loader.contains("maintenance_begin"));
}

/// Every other unimplemented effect is still refused before any bundle
/// exists, and the refusal names only the missing effect.
#[test]
fn other_unimplemented_effects_are_refused_before_a_bundle_exists() {
    let source = v38_source(
        "export function main():void{console.log(Date.now());console.log(varta.fs.read(\"/project/a.txt\"));}",
    );
    let prepared = BrowserArtifact::prepare(&source, None).unwrap();
    let error = match prepared.bundle() {
        Ok(_) => panic!("an unimplemented effect produced a bundle"),
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains("browser host effects not implemented: fs_read16"),
        "{error}"
    );
    assert!(!error.contains("date_now"), "{error}");
}

/// The browser's maintenance diagnostics are the native runtime's typed
/// budget errors, with the turn and the limit substituted.
#[test]
fn maintenance_exhaustion_texts_equal_the_native_runtime_errors() {
    use varta_runtime::{BudgetExhausted, TurnKind};
    let collector = &varta_common::browser::registry()["effects"]["collector"];
    for (template, wire, budget) in [
        ("fuelExhausted", "fuelBudget", BudgetKind::MaintenanceFuel),
        (
            "deadlineExhausted",
            "deadlineBudget",
            BudgetKind::MaintenanceDeadline,
        ),
    ] {
        let (turn, limit) = (7_u64, budget.production());
        let native = BudgetExhausted {
            turn,
            kind: TurnKind::Maintenance,
            budget,
            limit,
        }
        .to_string();
        let browser = collector[template]
            .as_str()
            .unwrap()
            .replace(
                collector["turnPlaceholder"].as_str().unwrap(),
                &turn.to_string(),
            )
            .replace(
                collector["limitPlaceholder"].as_str().unwrap(),
                &limit.to_string(),
            );
        assert_eq!(browser, native);
        assert_eq!(collector[wire], budget.wire_name());
    }
}
