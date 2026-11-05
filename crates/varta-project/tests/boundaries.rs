//! The production dependency graph is part of the engine/tooling boundary.
use std::{collections::BTreeMap, path::Path, process::Command};

#[test]
fn engine_dependencies_cannot_point_back_to_project_tooling_or_cli() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
            "--locked",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let rules: BTreeMap<&str, &[&str]> = BTreeMap::from([
        ("varta-common", [].as_slice()),
        ("varta-bigint", ["varta-common"].as_slice()),
        ("varta-artifact", ["varta-common"].as_slice()),
        (
            "varta-launcher",
            ["varta-common", "varta-artifact", "varta-runtime"].as_slice(),
        ),
        (
            "varta-compiler",
            ["varta-common", "varta-bigint"].as_slice(),
        ),
        (
            "varta-runtime",
            [
                "varta-common",
                "varta-bigint",
                "varta-intrinsics",
                "varta-kv-codec",
                "varta-view",
            ]
            .as_slice(),
        ),
        // The KV value layer is shared by the runtime and, compiled to
        // wasm32, the browser host; it reads common's `no_std` core.
        ("varta-kv-codec", ["varta-common"].as_slice()),
        (
            "varta-kv-codec-wasm",
            ["varta-kv-codec", "varta-common"].as_slice(),
        ),
        // The linked intrinsics read the value layout from common's `no_std`
        // core (checked below: default features off).
        (
            "varta-intrinsics",
            ["varta-common", "varta-bigint"].as_slice(),
        ),
        (
            "varta-intrinsics-wasm",
            ["varta-intrinsics", "varta-common", "varta-bigint"].as_slice(),
        ),
        // The headless view renderer (v38) is engine code without a VM.
        ("varta-view", ["varta-common"].as_slice()),
        // The native shell and preview are distribution hosts over the
        // public engine APIs; packaged artifacts are read as the launcher
        // reads them.
        (
            "varta-desktop",
            [
                "varta-common",
                "varta-artifact",
                "varta-runtime",
                "varta-view",
            ]
            .as_slice(),
        ),
        (
            "varta-project",
            [
                "varta-common",
                "varta-compiler",
                "varta-runtime",
                "varta-artifact",
            ]
            .as_slice(),
        ),
        ("varta", ["varta-project"].as_slice()),
        (
            "varta-conformance",
            ["varta-common", "varta-compiler", "varta-runtime"].as_slice(),
        ),
    ]);
    for package in packages {
        let name = package["name"].as_str().unwrap();
        let allowed = rules.get(name).unwrap_or_else(|| {
            panic!("classify the new workspace package {name} as engine or tooling")
        });
        for dependency in package["dependencies"].as_array().unwrap() {
            let target = dependency["name"].as_str().unwrap();
            let dev = dependency["kind"] == "dev";
            if dependency.get("path").is_some() {
                if !dev {
                    assert!(
                        allowed.contains(&target),
                        "forbidden production dependency: {name} -> {target}"
                    );
                }
                // The wasm32 intrinsic library never needs `std`: it reaches
                // common only through its `no_std` value-layout core.
                if matches!(
                    name,
                    "varta-bigint"
                        | "varta-intrinsics"
                        | "varta-intrinsics-wasm"
                        | "varta-kv-codec"
                        | "varta-kv-codec-wasm"
                ) && target == "varta-common"
                {
                    assert_eq!(
                        dependency["uses_default_features"], false,
                        "{name} must depend on varta-common without default features"
                    );
                    assert!(
                        dependency["features"]
                            .as_array()
                            .is_some_and(|features| features.is_empty()),
                        "{name} must not enable varta-common features"
                    );
                }
                if matches!(
                    name,
                    "varta-common"
                        | "varta-bigint"
                        | "varta-intrinsics"
                        | "varta-intrinsics-wasm"
                        | "varta-kv-codec"
                        | "varta-kv-codec-wasm"
                        | "varta-view"
                        | "varta-compiler"
                        | "varta-runtime"
                        | "varta-artifact"
                        | "varta-launcher"
                ) {
                    assert!(
                        matches!(
                            target,
                            "varta-common"
                                | "varta-bigint"
                                | "varta-intrinsics"
                                | "varta-kv-codec"
                                | "varta-view"
                                | "varta-compiler"
                                | "varta-runtime"
                                | "varta-artifact"
                                | "varta-launcher"
                        ),
                        "engine tests must not depend on tooling: {name} -> {target}"
                    );
                }
            }
            if !dev
                && matches!(
                    name,
                    "varta-runtime" | "varta-launcher" | "varta-artifact" | "varta-view"
                )
            {
                assert!(
                    !target.starts_with("swc_")
                        && !matches!(
                            target,
                            "clap"
                                | "clap_builder"
                                | "clap_derive"
                                | "varta-project"
                                | "varta-compiler"
                                | "varta"
                        ),
                    "development code must not enter runtime distribution: {name} -> {target}"
                );
            }
            // The native shell is distribution code: no source tooling, not
            // even for its tests, which run the `varta` binary as a separate
            // process to compile their probe. Transitive paths are checked in
            // `only_the_runtime_reaches_the_kv_storage_engine`.
            if name == "varta-desktop" {
                assert!(
                    !is_source_tooling(target),
                    "the native shell must not depend on source tooling: {name} -> {target} ({})",
                    if dev { "dev" } else { "production" }
                );
            }
            if !dev && name == "varta-project" {
                assert_ne!(
                    target, "wasmtime",
                    "backend-specific inspection belongs to the runtime API"
                );
            }
            // The Deno KV storage engine (v38 `kv`) and its SQLite build are
            // host code of the runtime only.
            assert!(
                !STORAGE_CRATES.contains(&target) || name == "varta-runtime" && !dev,
                "the KV storage engine is a runtime host dependency only: {name} -> {target}"
            );
            assert!(
                !target.starts_with("accesskit")
                    || name == "varta-desktop" && (!dev || target == "accesskit_consumer"),
                "native accessibility belongs to the desktop host: {name} -> {target}"
            );
            assert!(
                !CLIPBOARD_CRATES.contains(&target) || name == "varta-desktop" && !dev,
                "OS clipboard I/O belongs to the desktop host: {name} -> {target}"
            );
        }
    }
}

/// Compiler, SWC, project tooling, CLI and argument-parser crates.
fn is_source_tooling(name: &str) -> bool {
    name.starts_with("swc_")
        || matches!(
            name,
            "clap"
                | "clap_builder"
                | "clap_derive"
                | "varta-compiler"
                | "varta-project"
                | "varta"
                | "varta-conformance"
        )
}

/// The Deno KV storage crates (`denokv_sqlite` and what Varta names of it).
const STORAGE_CRATES: [&str; 5] = [
    "denokv_sqlite",
    "denokv_proto",
    "rusqlite",
    "libsqlite3-sys",
    "deno_error",
];

const CLIPBOARD_CRATES: [&str; 3] = ["arboard", "wl-clipboard-rs", "clipboard-win"];

/// No engine contract, compiler, artifact or intrinsic crate reaches SQLite,
/// even transitively: only the runtime (and what links it) does.
#[test]
fn only_the_runtime_reaches_the_kv_storage_engine() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    // Resolve only the host platform: an offline resolve of every platform needs
    // crates that a host build never downloads, so the result would depend on
    // the cache instead of the graph.
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--offline",
            "--locked",
            "--filter-platform",
            &host_triple(),
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let names: BTreeMap<&str, &str> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|package| {
            (
                package["id"].as_str().unwrap(),
                package["name"].as_str().unwrap(),
            )
        })
        .collect();
    let edges: BTreeMap<&str, Vec<&str>> = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            let normal = node["deps"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|dep| {
                    dep["dep_kinds"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|kind| kind["kind"].is_null() || kind["kind"] == "build")
                })
                .map(|dep| dep["pkg"].as_str().unwrap())
                .collect();
            (node["id"].as_str().unwrap(), normal)
        })
        .collect();
    let id_of = |root: &str| {
        names
            .iter()
            .find(|(_, name)| **name == root)
            .map(|(id, _)| *id)
            .unwrap()
    };
    // Whether package `start` reaches `target` through normal and build edges.
    let reaches_from = |start: &str, target: &str| {
        let mut pending = vec![start];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            if names[id] == target {
                return true;
            }
            pending.extend(edges.get(id).into_iter().flatten().copied());
        }
        false
    };
    let reaches = |root: &str, target: &str| reaches_from(id_of(root), target);
    for engine in [
        "varta-common",
        "varta-bigint",
        "varta-compiler",
        "varta-artifact",
        "varta-intrinsics",
        "varta-intrinsics-wasm",
        "varta-kv-codec",
        "varta-kv-codec-wasm",
        "varta-view",
    ] {
        assert!(
            !reaches(engine, "libsqlite3-sys"),
            "{engine} reaches SQLite"
        );
    }
    assert!(reaches("varta-runtime", "libsqlite3-sys"));
    // Accessibility adapters must not increase the runtime/distribution graph,
    // even indirectly through another crate or a build dependency.
    for package in metadata["workspace_members"].as_array().unwrap() {
        let name = names[package.as_str().unwrap()];
        if name != "varta-desktop" {
            assert!(!reaches(name, "accesskit"), "{name} reaches AccessKit");
            assert!(!reaches(name, "arboard"), "{name} reaches OS clipboard I/O");
        }
    }
    // Nor does any production path of the native shell, or any path through
    // one of its dev-dependencies, reach source tooling.
    let desktop = id_of("varta-desktop");
    let desktop_dev: Vec<&str> = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == desktop)
        .unwrap()["deps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|dep| {
            dep["dep_kinds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|kind| kind["kind"] == "dev")
        })
        .map(|dep| dep["pkg"].as_str().unwrap())
        .collect();
    assert!(
        !desktop_dev.is_empty(),
        "varta-desktop has dev-dependencies to check"
    );
    for name in names
        .values()
        .copied()
        .filter(|name| is_source_tooling(name))
    {
        assert!(
            !reaches("varta-desktop", name),
            "varta-desktop reaches {name}"
        );
        for dev in &desktop_dev {
            assert!(
                !reaches_from(dev, name),
                "varta-desktop's dev-dependency {} reaches {name}",
                names[dev]
            );
        }
    }
    assert!(reaches("varta-desktop", "varta-artifact"));
    assert!(reaches("varta-desktop", "accesskit"));
    assert!(reaches("varta-desktop", "arboard"));
}

/// The target triple of the toolchain running the tests, from `rustc -vV`.
fn host_triple() -> String {
    const HOST_PREFIX: &str = "host: ";
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| String::from("rustc"));
    let output = Command::new(rustc).arg("-vV").output().unwrap();
    assert!(output.status.success(), "rustc -vV failed");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix(HOST_PREFIX))
        .expect("rustc -vV names the host triple")
        .to_owned()
}
