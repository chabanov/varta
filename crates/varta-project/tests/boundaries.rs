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
        ("varta-runtime", ["varta-common", "varta-bigint"].as_slice()),
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
                if matches!(
                    name,
                    "varta-common"
                        | "varta-bigint"
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
                                | "varta-compiler"
                                | "varta-runtime"
                                | "varta-artifact"
                                | "varta-launcher"
                        ),
                        "engine tests must not depend on tooling: {name} -> {target}"
                    );
                }
            }
            if !dev && matches!(name, "varta-runtime" | "varta-launcher" | "varta-artifact") {
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
            if !dev && name == "varta-project" {
                assert_ne!(
                    target, "wasmtime",
                    "backend-specific inspection belongs to the runtime API"
                );
            }
        }
    }
}
