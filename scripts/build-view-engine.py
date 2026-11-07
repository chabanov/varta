#!/usr/bin/env python3
"""Build the browser host's view style engine and refresh the bundle's copy.

The module is `crates/varta-style-wasm` (the safe `varta-style` engine plus
its export shim) compiled to one core wasm32 module with its own memory. The
browser bundle embeds the checked-in result
(`crates/varta-project/src/browser/view-engine.wasm`); the view model and the
DOM applier instantiate it, so the browser host decides stylesheets, values,
the cascade and computed values with the code the native renderer runs.

Run this script after changing either engine crate or the `varta-common`
files it compiles; the project test `view_engine_matches_its_sources` fails
while the recorded source fingerprint is stale. `--check` only compares the
fingerprint.

The build is offline and locked and uses the workspace's `intrinsics` profile
(size-optimized, abort on panic). It uses `wasm32-unknown-unknown` when that
target is installed and otherwise `wasm32-wasip2`, whose linker is asked for a
plain core module (no component) and whose libc supplies only the memory
routines; the module imports nothing.
"""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = ("crates/varta-style", "crates/varta-style-wasm", "crates/varta-common/src/view")
# The `varta-common` files of its `no_std` core besides `src/view` (listed
# in SOURCES). The project test `view_engine_matches_its_sources` lists the
# same files.
COMMON_CORE = (
    "crates/varta-common/Cargo.toml",
    "crates/varta-common/src/lib.rs",
    "crates/varta-common/src/abi.rs",
    "crates/varta-common/src/abi/static_region.rs",
    "crates/varta-common/src/bigint.rs",
    "crates/varta-common/src/console.rs",
    "crates/varta-common/src/exceptions.rs",
    "crates/varta-common/src/fuel.rs",
    "crates/varta-common/src/json/mod.rs",
    "crates/varta-common/src/json/build.rs",
    "crates/varta-common/src/json/descriptor.rs",
    "crates/varta-common/src/json/error.rs",
    "crates/varta-common/src/json/read.rs",
    "crates/varta-common/src/json/scan.rs",
    "crates/varta-common/src/json/schema.rs",
    "crates/varta-common/src/limits.rs",
    "crates/varta-common/src/memory.rs",
    "crates/varta-common/src/text.rs",
    "crates/varta-common/src/value.rs",
    "crates/varta-common/src/value/error.rs",
    "crates/varta-common/src/value/primitive.rs",
    "crates/varta-common/src/value/reference.rs",
    "crates/varta-common/src/value/vector.rs",
    "crates/varta-common/src/value/dictionary.rs",
    "crates/varta-common/src/function.rs",
    "crates/varta-common/src/function/view.rs",
    "crates/varta-common/src/view.rs",
)
OUTPUT = ROOT / "crates/varta-project/src/browser/view-engine.wasm"
METADATA = ROOT / "crates/varta-project/src/browser/view-engine.json"
PACKAGE = "varta-style-wasm"
PROFILE = "intrinsics"
# The engine's linear-memory (shadow) stack, first in memory so an overflow
# traps instead of corrupting data. Every recursion of the engine is bounded
# by a named limit (value tokens, condition and rule nesting).
STACK_BYTES = 1024 * 1024
FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3


def source_files():
    files = []
    for directory in SOURCES:
        base = ROOT / directory
        for path in base.rglob("*"):
            if path.is_file() and "target" not in path.relative_to(base).parts:
                files.append(path.relative_to(ROOT).as_posix())
    files.extend(COMMON_CORE)
    return sorted(files)


def fingerprint():
    """FNV-1a 64 over `path NUL content NUL` of every source file, sorted."""
    value = FNV_OFFSET
    for name in source_files():
        for chunk in (name.encode(), b"\0", (ROOT / name).read_bytes(), b"\0"):
            for byte in chunk:
                value = ((value ^ byte) * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


def installed_targets():
    result = subprocess.run(
        ["rustup", "target", "list", "--installed"],
        check=True, capture_output=True, text=True)
    return set(result.stdout.split())


def target_directory(environment):
    """Cargo's target directory as the build sees it (`CARGO_TARGET_DIR` or
    a configured `build.target-dir` included)."""
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--offline", "--locked"],
        check=True, capture_output=True, text=True, cwd=ROOT, env=environment)
    return pathlib.Path(json.loads(result.stdout)["target_directory"])


def build():
    targets = installed_targets()
    link = [f"-zstack-size={STACK_BYTES}", "--stack-first"]
    if "wasm32-unknown-unknown" in targets:
        target = "wasm32-unknown-unknown"
    elif "wasm32-wasip2" in targets:
        target = "wasm32-wasip2"
        link = ["--skip-wit-component", *link, "-lc"]
    else:
        sys.exit("build-view-engine: install wasm32-unknown-unknown or wasm32-wasip2")
    home = pathlib.Path.home()
    flags = [f"-Clink-arg={arg}" for arg in link] + [
        f"--remap-path-prefix={ROOT}=/varta",
        f"--remap-path-prefix={home}=/home",
    ]
    variable = "CARGO_TARGET_" + target.upper().replace("-", "_") + "_RUSTFLAGS"
    environment = {**os.environ, variable: " ".join(flags)}
    subprocess.run(
        ["cargo", "build", "-p", PACKAGE, "--target", target, "--profile", PROFILE,
         "--offline", "--locked"],
        check=True, cwd=ROOT, env=environment)
    built = target_directory(environment) / target / PROFILE / (PACKAGE.replace("-", "_") + ".wasm")
    shutil.copyfile(built, OUTPUT)
    rustc = subprocess.run(["rustc", "--version"], check=True, capture_output=True,
                           text=True).stdout.strip()
    METADATA.write_text(json.dumps({
        "sourceFingerprint": fingerprint(),
        "target": target,
        "rustc": rustc,
        "bytes": OUTPUT.stat().st_size,
    }, indent=2) + "\n")
    print(f"build-view-engine: wrote {OUTPUT.relative_to(ROOT)} "
          f"({OUTPUT.stat().st_size} bytes, {target})")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true",
                        help="only verify the recorded source fingerprint")
    args = parser.parse_args()
    if args.check:
        recorded = json.loads(METADATA.read_text())["sourceFingerprint"]
        if recorded != fingerprint():
            sys.exit("build-view-engine: view-engine.wasm is stale; run scripts/build-view-engine.py")
        print("build-view-engine: fingerprint current")
        return
    build()


if __name__ == "__main__":
    main()
