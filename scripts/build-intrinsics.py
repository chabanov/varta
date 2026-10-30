#!/usr/bin/env python3
"""Build the v38 guest intrinsic library and refresh the compiler's copy.

The library is `crates/varta-intrinsics-wasm` (the safe `varta-intrinsics`
code plus its export shim) compiled to one core wasm32 module. The compiler
embeds the checked-in result (`crates/varta-compiler/src/link/intrinsics.wasm`)
and links the functions a program uses into its guest module, so ordinary
builds and tests need no wasm32 target.

Run this script after changing either intrinsics crate; the compiler test
`intrinsic_library_matches_its_sources` fails while the recorded source
fingerprint is stale. `--check` only compares the fingerprint.

The build is offline and locked. It uses `wasm32-unknown-unknown` when that
target is installed and otherwise `wasm32-wasip2`, whose linker is asked for a
plain core module (no component) and whose libc supplies only `memcmp`.
"""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = ("crates/varta-bigint", "crates/varta-intrinsics", "crates/varta-intrinsics-wasm")
# The `varta-common` files of its `no_std` value-layout core, which the
# library compiles (the compiler test `intrinsic_library_matches_its_sources`
# lists the same files).
COMMON_CORE = (
    "crates/varta-common/Cargo.toml",
    "crates/varta-common/src/lib.rs",
    "crates/varta-common/src/abi.rs",
    "crates/varta-common/src/abi/static_region.rs",
    "crates/varta-common/src/bigint.rs",
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
OUTPUT = ROOT / "crates/varta-compiler/src/link/intrinsics.wasm"
METADATA = ROOT / "crates/varta-compiler/src/link/intrinsics.json"
PACKAGE = "varta-intrinsics-wasm"
PROFILE = "intrinsics"
# The library's own memory layout is a placeholder: the compiler's linker
# relocates the reached data segments into a module's intrinsic region of
# memory 0 through the relocations kept here (`--emit-relocs`, one segment
# per input section), and places the shadow stack there itself
# (`abi::INTRINSIC_STACK_BYTES`). The placeholder stack is one page.
STACK_BYTES = 64 * 1024
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


def build():
    targets = installed_targets()
    link = [f"-zstack-size={STACK_BYTES}", "--emit-relocs", "--no-merge-data-segments"]
    if "wasm32-unknown-unknown" in targets:
        target = "wasm32-unknown-unknown"
    elif "wasm32-wasip2" in targets:
        target = "wasm32-wasip2"
        link = ["--skip-wit-component", *link, "-lc"]
    else:
        sys.exit("build-intrinsics: install wasm32-unknown-unknown or wasm32-wasip2")
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
    built = ROOT / "target" / target / PROFILE / (PACKAGE.replace("-", "_") + ".wasm")
    shutil.copyfile(built, OUTPUT)
    rustc = subprocess.run(["rustc", "--version"], check=True, capture_output=True,
                           text=True).stdout.strip()
    METADATA.write_text(json.dumps({
        "sourceFingerprint": fingerprint(),
        "target": target,
        "rustc": rustc,
        "bytes": OUTPUT.stat().st_size,
    }, indent=2) + "\n")
    print(f"build-intrinsics: wrote {OUTPUT.relative_to(ROOT)} "
          f"({OUTPUT.stat().st_size} bytes, {target})")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true",
                        help="only verify the recorded source fingerprint")
    args = parser.parse_args()
    if args.check:
        recorded = json.loads(METADATA.read_text())["sourceFingerprint"]
        if recorded != fingerprint():
            sys.exit("build-intrinsics: intrinsics.wasm is stale; run scripts/build-intrinsics.py")
        print("build-intrinsics: fingerprint current")
        return
    build()


if __name__ == "__main__":
    main()
