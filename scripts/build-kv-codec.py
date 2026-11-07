#!/usr/bin/env python3
"""Build the browser host's KV codec module and refresh the bundle's copy.

The module is `crates/varta-kv-codec-wasm` (the safe `varta-kv-codec` value
layer plus its export shim) compiled to one core wasm32 module with its own
memory. The browser bundle embeds the checked-in result
(`crates/varta-project/src/browser/kv-codec.wasm`) and the Worker instantiates
it beside the guest, so ordinary builds and tests need no wasm32 target.

Run this script after changing either codec crate or the `varta-common`
files it compiles; the project test `kv_codec_matches_its_sources` fails
while the recorded source fingerprint is stale. `--check` only compares the
fingerprint.

The build is offline and locked and uses the workspace's `intrinsics` profile
(size-optimized, abort on panic). It uses `wasm32-unknown-unknown` when that
target is installed and otherwise `wasm32-wasip2`, whose linker is asked for a
plain core module (no component) and whose libc supplies only the memory
routines; the module imports nothing but `varta_kv_host`.
"""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = ("crates/varta-kv-codec", "crates/varta-kv-codec-wasm")
# The `varta-common` files of its `no_std` core that the codec compiles, and
# the KV wire facts it includes (`varta_kv_codec::wire`). The project test
# `kv_codec_matches_its_sources` lists the same files.
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
    "crates/varta-common/src/kv.rs",
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
)
OUTPUT = ROOT / "crates/varta-project/src/browser/kv-codec.wasm"
METADATA = ROOT / "crates/varta-project/src/browser/kv-codec.json"
PACKAGE = "varta-kv-codec-wasm"
PROFILE = "intrinsics"
# The codec's linear-memory (shadow) stack, which holds only its address-taken
# locals; it comes first in memory, so an overflow traps instead of
# corrupting data. It does not bound recursion: a wasm call frame lives on the
# engine's native stack, which no module can size. The stage walk, the V8
# codec and the classifier recurse once per nesting level, and the shared
# limit `varta_kv_codec::MAX_VALUE_DEPTH` (derived and measured in
# crates/varta-kv-codec/src/lib.rs) keeps them within every engine's stack.
STACK_BYTES = 16 * 1024 * 1024
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
        sys.exit("build-kv-codec: install wasm32-unknown-unknown or wasm32-wasip2")
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
    print(f"build-kv-codec: wrote {OUTPUT.relative_to(ROOT)} "
          f"({OUTPUT.stat().st_size} bytes, {target})")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true",
                        help="only verify the recorded source fingerprint")
    args = parser.parse_args()
    if args.check:
        recorded = json.loads(METADATA.read_text())["sourceFingerprint"]
        if recorded != fingerprint():
            sys.exit("build-kv-codec: kv-codec.wasm is stale; run scripts/build-kv-codec.py")
        print("build-kv-codec: fingerprint current")
        return
    build()


if __name__ == "__main__":
    main()
