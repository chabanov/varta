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
    // A request above the v38 production memory is capped to it without a
    // host; a host ceiling narrows it further.
    let production = varta_common::limits::V38_MAX_MEMORY_BYTES;
    let requested = 2 * production as u64;
    let mut source = artifact(LanguageProfile::CheckedF64Utf16V38);
    source.manifest.budgets = Some(BudgetRequest {
        memory_bytes: Some(requested),
        ..Default::default()
    });
    let plain = BrowserArtifact::prepare(&source, None).unwrap();
    assert!(plain.ceiling().is_none());
    assert_eq!(plain.budgets().budgets.memory_bytes, production);
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
        Some(requested)
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

/// The browser's fatal turn and resource texts are the native runtime's
/// typed errors, with the turn, the kind, the limit and the request
/// substituted.
#[test]
fn fatal_failure_texts_equal_the_native_runtime_errors() {
    use varta_common::limits::{Resource, ResourceExhausted};
    use varta_runtime::{BudgetExhausted, TurnKind};
    let fatal = &varta_common::browser::registry()["effects"]["fatal"];
    let text = |key: &str| fatal[key].as_str().unwrap().to_owned();
    let (turn, limit) = (7_u64, BudgetKind::Fuel.production());
    for kind in [
        TurnKind::Main,
        TurnKind::Timer,
        TurnKind::Network,
        TurnKind::HostEvent,
        TurnKind::View,
    ] {
        let native = BudgetExhausted {
            turn,
            kind,
            budget: BudgetKind::Fuel,
            limit,
        }
        .to_string();
        let name = fatal["kinds"]
            .as_object()
            .unwrap()
            .values()
            .find(|name| name.as_str() == Some(kind.name()))
            .unwrap_or_else(|| panic!("{} has no registry kind", kind.name()));
        let browser = text("fuelExhausted")
            .replace(&text("turnPlaceholder"), &turn.to_string())
            .replace(&text("kindPlaceholder"), name.as_str().unwrap())
            .replace(&text("limitPlaceholder"), &limit.to_string());
        assert_eq!(browser, native);
    }
    let requested = 64_u32;
    for resource in [
        Resource::Memory,
        Resource::ValueSize,
        Resource::RootStack,
        Resource::LiveFrames,
    ] {
        let native = ResourceExhausted {
            resource,
            requested,
        }
        .to_string();
        let browser = fatal["resources"][(resource as i32).to_string()]
            .as_str()
            .unwrap()
            .replace(&text("requestPlaceholder"), &requested.to_string());
        assert_eq!(browser, native);
    }
    assert_eq!(
        fatal["resourceExport"],
        varta_common::limits::RESOURCE_FAILURE
    );
    assert_eq!(
        fatal["requestExport"],
        varta_common::limits::RESOURCE_REQUEST
    );
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

/// Browser effect admission is the native profile's import admission: each
/// implemented effect is admitted exactly under the profiles whose native
/// host imports it, and refused under every other profile.
#[test]
fn effect_admission_follows_the_native_profile_imports() {
    use super::bundle::unimplemented_effects;
    use varta_common::{HostOperation as H, language::SUPPORTED_PROFILES};
    let pilot = [H::DateNow, H::MathRandom, H::EnvSnapshot, H::ExitFloat];
    for profile in SUPPORTED_PROFILES.iter().copied() {
        for operation in pilot {
            assert_eq!(
                unimplemented_effects(profile, [operation.name()]).is_empty(),
                varta_common::browser::admits(profile, operation),
                "{} under {:?}",
                operation.name(),
                profile
            );
        }
    }
    // The first native profile of each pilot effect, and the one before it.
    for (operation, first, before) in [
        (
            H::DateNow,
            LanguageProfile::CheckedF64Utf16V37,
            LanguageProfile::CheckedF64Utf16V36,
        ),
        (
            H::MathRandom,
            LanguageProfile::CheckedF64Utf16V17,
            LanguageProfile::CheckedF64Utf16V16,
        ),
        (
            H::EnvSnapshot,
            LanguageProfile::CheckedF64Utf16V38,
            LanguageProfile::CheckedF64Utf16V37,
        ),
        (
            H::ExitFloat,
            LanguageProfile::CheckedF64Utf8V1,
            LanguageProfile::CheckedI32Utf8V1,
        ),
    ] {
        assert!(unimplemented_effects(first, [operation.name()]).is_empty());
        assert_eq!(
            unimplemented_effects(before, [operation.name()]),
            [operation.name()]
        );
    }
}

fn profile_source(source: &str, profile: LanguageProfile) -> Artifact {
    let mut artifact = artifact(profile);
    artifact.wasm = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|error| panic!("{error}\n{source}"));
    artifact
}

/// Older profiles keep the pilot effects their native host imports: v37's
/// clock, entropy and exit and v20's entropy and exit produce bundles. The
/// Chrome effect acceptance compares their runs with native.
#[test]
fn older_profile_bundles_admit_exactly_their_native_pilot_effects() {
    use varta_common::HostOperation as H;
    for (profile, source, effects) in [
        (
            LanguageProfile::CheckedF64Utf16V37,
            "export function main():void{console.log(Date.now()>0);console.log(Math.random()<1);varta.process.exit(3);}",
            &[H::DateNow, H::MathRandom, H::ExitFloat][..],
        ),
        (
            LanguageProfile::CheckedF64Utf16V20,
            "export function main():void{console.log(Math.random()<1);varta.process.exit(3);}",
            &[H::MathRandom, H::ExitFloat][..],
        ),
    ] {
        let prepared = BrowserArtifact::prepare(&profile_source(source, profile), None).unwrap();
        for operation in effects {
            assert!(
                prepared
                    .inspection()
                    .imports
                    .iter()
                    .any(|import| import.name == operation.name()),
                "{} not imported under {:?}",
                operation.name(),
                profile
            );
        }
        prepared.bundle().unwrap();
    }
}

/// The default i32 profile exits through its own `process_exit` import,
/// which the browser host does not implement: no bundle exists.
#[test]
fn the_i32_profile_exit_is_refused_before_a_bundle_exists() {
    use varta_common::HostOperation as H;
    let profile = LanguageProfile::CheckedI32Utf8V1;
    let prepared = BrowserArtifact::prepare(
        &profile_source(
            "export function main():void{varta.process.exit(3);}",
            profile,
        ),
        None,
    )
    .unwrap();
    let error = match prepared.bundle() {
        Ok(_) => panic!("the i32 exit produced a bundle"),
        Err(error) => error.to_string(),
    };
    assert!(
        error.ends_with(&format!(
            "browser host effects not implemented: {}",
            H::Exit.name()
        )),
        "{error}"
    );
}

/// The prefix of a guest diagnostic line in native output.
const LOG_PREFIX: &str = "varta log: ";

/// The browser registry's invalid-input text is the native runtime's: an
/// authorized `process.exit` with a non-integer code logs it natively and
/// continues, exactly as the Worker's handler does.
#[test]
fn invalid_input_failure_text_equals_the_native_runtime_diagnostic() {
    use varta_common::{CapabilityGrant, CapabilityValue, HostOperation as H};
    let failures = &varta_common::browser::registry()["effects"]["failures"];
    let invalid = failures["invalid"].as_str().unwrap();
    let source = v38_source(
        "export function main():void{varta.process.exit(1.5);console.log('after exit');}",
    );
    let grant = CapabilityGrant {
        id: H::ExitFloat.capability().unwrap().into(),
        value: CapabilityValue::Boolean(true),
    };
    let output = varta_runtime::VartaRuntime::new()
        .run_wasm_bytes(&source.wasm, &[grant])
        .unwrap();
    assert_eq!(
        output.lines,
        [
            format!("{LOG_PREFIX}{invalid}"),
            format!("{LOG_PREFIX}after exit")
        ]
    );
}

/// A v38 program that opens a KV database, writes and reads a credential
/// and logs what rejects.
const KV_SOURCE: &str = "export async function main(): Promise<void> {\
  try {\
    const kv = await varta.kv.open('/data/app.kv');\
    await kv.set(['credentials', 'apiKey'], 'secret');\
    const entry = await kv.get<string>(['credentials', 'apiKey']);\
    console.log(entry.value ?? 'none');\
    kv.close();\
  } catch (error) {\
    if (error instanceof Error) { console.log(error.name + ': ' + error.message); }\
  }\
}";

/// The KV imports are admitted under v38: a program using every one gets a
/// bundle whose Worker carries the KV host (`kv-store.mjs`) and the
/// checked-in codec module beside it, pinned in the descriptor; the loader
/// carries no KV host. A program without KV imports gets no codec.
#[test]
fn bundle_admits_the_v38_kv_imports() {
    use varta_common::HostOperation as H;
    let prepared = BrowserArtifact::prepare(&v38_source(KV_SOURCE), None).unwrap();
    for operation in H::ALL.iter().filter(|operation| operation.kv()) {
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
    assert!(worker.contains("const kvStore=(()=>{"));
    assert!(worker.contains("kvStore.createKvHost"));
    assert!(!worker.contains("import {registry}"));
    let loader = std::str::from_utf8(&bundle.files()["varta-host.js"]).unwrap();
    assert!(!loader.contains("createKvHost"));
    let codec = include_bytes!("kv-codec.wasm");
    assert_eq!(bundle.files()["varta-kv-codec.wasm"], codec);
    assert!(loader.contains(&varta_runtime::artifact_identity(codec)));
    let plain = BrowserArtifact::prepare(
        &v38_source("export function main():void{console.log(1);}"),
        None,
    )
    .unwrap()
    .bundle()
    .unwrap();
    assert!(!plain.files().contains_key("varta-kv-codec.wasm"));
}

/// The checked-in codec module is the build of its sources: the FNV-1a
/// fingerprint `scripts/build-kv-codec.py` records over the same files.
#[test]
fn kv_codec_matches_its_sources() {
    const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
    const FNV_PRIME: u64 = 0x0100_0000_01B3;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for directory in ["crates/varta-kv-codec", "crates/varta-kv-codec-wasm"] {
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
            "src/kv.rs",
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
    let metadata: serde_json::Value = serde_json::from_str(include_str!("kv-codec.json")).unwrap();
    assert_eq!(
        metadata["sourceFingerprint"],
        format!("{value:016x}"),
        "kv-codec.wasm is stale; run scripts/build-kv-codec.py"
    );
    assert_eq!(
        metadata["bytes"],
        include_bytes!("kv-codec.wasm").len(),
        "kv-codec.json does not describe kv-codec.wasm"
    );
}

/// The registry's failure texts are the native ones (the shared value
/// layer's `KvError`), with the path filled in.
#[test]
fn kv_texts_equal_the_native_texts() {
    use varta_kv_codec::protocol::KvError;
    let kv = &varta_common::browser::registry()["kv"];
    let path = "/data/$&.kv";
    let fill = |name: &str| {
        kv["texts"][name]
            .as_str()
            .unwrap()
            .replace(kv["pathPlaceholder"].as_str().unwrap(), path)
    };
    for (name, native) in [
        ("notFound", KvError::not_found(path)),
        ("locked", KvError::locked(path)),
        ("readOnly", KvError::read_only(path)),
        ("tooLate", KvError::too_late(path)),
        ("invalidName", KvError::invalid_name(path)),
        ("noFile", KvError::no_file(path)),
        ("tooManyDatabases", KvError::too_many_databases()),
    ] {
        assert_eq!(fill(name), native.message, "{name}");
    }
    assert_eq!(
        fill("unavailable").replace(kv["failurePlaceholder"].as_str().unwrap(), "x"),
        KvError::unavailable(path, "x").message
    );
    let denial = kv["denialText"]
        .as_str()
        .unwrap()
        .replace(kv["capabilityPlaceholder"].as_str().unwrap(), "kv.write")
        .replace(kv["pathPlaceholder"].as_str().unwrap(), path);
    assert_eq!(denial, KvError::denied("kv.write", path).message);
    assert_eq!(
        kv["texts"]["nonU64"]
            .as_str()
            .unwrap()
            .replace(kv["operationPlaceholder"].as_str().unwrap(), "sum"),
        "Failed to perform 'sum' mutation on a non-U64 value in the database"
    );
    // The shared value depth the codec enforces, for the browser tests.
    assert_eq!(
        kv["maxValueDepth"],
        serde_json::json!(varta_kv_codec::MAX_VALUE_DEPTH)
    );
    assert_eq!(
        kv["valueTooDeep"].as_str().unwrap(),
        varta_kv_codec::too_deep()
    );
    // The extent bounds the codec enforces.
    assert_eq!(
        kv["valueExtent"]["maxValues"],
        serde_json::json!(varta_kv_codec::MAX_VALUE_NODES)
    );
    assert_eq!(
        kv["valueExtent"]["maxUnits"],
        serde_json::json!(varta_kv_codec::MAX_VALUE_UNITS)
    );
    assert_eq!(
        kv["valueExtent"]["tooExpanded"].as_str().unwrap(),
        varta_kv_codec::too_expanded()
    );
}

/// KV admission is the native profile's import admission: every KV import
/// is admitted under v38 and refused under the profile before it.
#[test]
fn kv_admission_follows_the_native_profile_imports() {
    use super::bundle::unimplemented_effects;
    use varta_common::{HostOperation as H, language::SUPPORTED_PROFILES};
    let operations = H::ALL
        .iter()
        .copied()
        .filter(|operation| operation.kv())
        .collect::<Vec<_>>();
    assert_eq!(operations.len(), 9);
    for profile in SUPPORTED_PROFILES.iter().copied() {
        for operation in &operations {
            assert_eq!(
                unimplemented_effects(profile, [operation.name()]).is_empty(),
                varta_common::browser::admits(profile, *operation),
                "{} under {:?}",
                operation.name(),
                profile
            );
        }
    }
    let names = operations.iter().map(|operation| operation.name());
    assert!(unimplemented_effects(LanguageProfile::CheckedF64Utf16V38, names.clone()).is_empty());
    assert_eq!(
        unimplemented_effects(LanguageProfile::CheckedF64Utf16V37, names.clone()),
        names.collect::<Vec<_>>()
    );
}

/// The browser registry's KV denial is the native runtime's: an open that no
/// grant covers logs it and rejects with `NotAllowedError` and the same text,
/// which is what the browser host's every open produces.
#[test]
fn kv_denial_text_equals_the_native_runtime_denial() {
    let kv = &varta_common::browser::registry()["kv"];
    let path = "/data/app.kv";
    let denial = kv["denialText"]
        .as_str()
        .unwrap()
        .replace(
            kv["capabilityPlaceholder"].as_str().unwrap(),
            varta_common::kv::READ_CAPABILITY,
        )
        .replace(kv["pathPlaceholder"].as_str().unwrap(), path);
    let output = varta_runtime::VartaRuntime::new()
        .run_wasm_bytes(&v38_source(KV_SOURCE).wasm, &[])
        .unwrap();
    assert_eq!(
        output.lines,
        [
            format!("{LOG_PREFIX}{denial}"),
            format!(
                "{LOG_PREFIX}{}: {denial}",
                kv["notAllowed"].as_str().unwrap()
            )
        ]
    );
}

/// A manifest granting `kv.read` and `kv.write` builds: the browser host
/// stores KV databases in OPFS. Every descriptor carries the rules the
/// loader validates a KV permission ceiling with.
#[test]
fn bundle_admits_kv_grants() {
    use varta_common::{CapabilityGrant, CapabilityValue, kv};
    let grant = |id: &str| CapabilityGrant {
        id: id.into(),
        value: CapabilityValue::PathGlobs(vec!["/data/*.kv".into()]),
    };
    for source in [KV_SOURCE, "export function main():void{console.log(1);}"] {
        let mut artifact = v38_source(source);
        artifact.manifest =
            Manifest::new(&[grant(kv::READ_CAPABILITY), grant(kv::WRITE_CAPABILITY)]);
        artifact.manifest.abi_version = LanguageProfile::CheckedF64Utf16V38.host_abi();
        artifact.manifest.fs_roots = Some(varta_common::files::FsRoots::Names(vec!["data".into()]));
        artifact.manifest.validate_package().unwrap();
        let prepared = BrowserArtifact::prepare(&artifact, None).unwrap();
        for bundle in [
            prepared.bundle().unwrap(),
            prepared.development_bundle(&[]).unwrap(),
        ] {
            let loader = std::str::from_utf8(&bundle.files()["varta-host.js"]).unwrap();
            assert!(loader.contains(&format!(
                r#""maxPatterns":{}"#,
                varta_common::MAX_GRANT_PATTERNS
            )));
            assert_eq!(
                bundle.files().contains_key("varta-kv-codec.wasm"),
                source == KV_SOURCE
            );
        }
    }
}
