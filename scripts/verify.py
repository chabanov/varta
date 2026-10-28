#!/usr/bin/env python3
"""Explicit verification scopes; never infer a full pass from focused tests."""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
PACKAGES = (
    "varta", "varta-artifact", "varta-bigint", "varta-common", "varta-compiler",
    "varta-conformance", "varta-launcher", "varta-project", "varta-runtime",
)
CORE = ["varta-common", "varta-compiler", "varta-runtime", "varta-artifact"]


def cargo(*args: str) -> list[str]:
    return ["cargo", *args]


def companion() -> list[str]:
    return cargo("build", "-p", "varta-launcher", "--bin", "varta-runtime", "--locked")


def commands(args: argparse.Namespace) -> list[list[str]]:
    if args.scope == "local":
        result = [cargo("check", "-p", args.package, "--all-targets", "--locked")]
        if args.package in ("varta", "varta-project", "varta-launcher"):
            result.append(companion())
        test = cargo("test", "-p", args.package, "--locked")
        if args.filter:
            test.append(args.filter)
        return result + [test]
    if args.scope == "feature":
        return [
            companion(),
            cargo("test", "-p", "varta-runtime", "--lib", "--locked", args.filter),
            cargo("test", "-p", "varta-runtime", "--test", "public_api", "--locked", args.filter),
            cargo("test", "-p", "varta", "--test", "smoke", "--locked", args.filter),
        ]
    check = cargo("check")
    for package in CORE:
        check += ["-p", package]
    return [
        cargo("fmt", "--all", "--", "--check"),
        check + ["--lib", "--locked"],
        companion(),
        cargo("clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"),
        cargo("build", "--workspace", "--all-targets", "--locked"),
        cargo("test", "--workspace", "--locked"),
        cargo("run", "-p", "varta-conformance", "--locked", "--", "--json"),
    ]


def fingerprint() -> dict[str, str]:
    """Include staged, unstaged and untracked content, not just the HEAD commit."""
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip()
    files = subprocess.check_output(
        ["git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT,
    ).split(b"\0")
    digest = hashlib.sha256()
    for name in sorted(set(files) - {b""}):
        path = ROOT / os.fsdecode(name)
        digest.update(len(name).to_bytes(8, "little"))
        digest.update(name)
        if path.is_symlink():
            content = os.fsencode(os.readlink(path))
            kind = b"symlink"
        elif path.is_file():
            content = path.read_bytes()
            kind = b"executable" if path.stat().st_mode & 0o111 else b"file"
        else:
            content = b""
            kind = b"deleted"
        digest.update(kind + b"\0")
        digest.update(hashlib.sha256(content).digest())
    return {"head": head, "sourceSha256": digest.hexdigest()}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true", help="print commands only; executes no checks")
    scopes = parser.add_subparsers(dest="scope", required=True)
    local = scopes.add_parser("local", help="check/test one crate, optionally a test-name filter")
    local.add_argument("package", choices=PACKAGES)
    local.add_argument("--filter")
    feature = scopes.add_parser("feature", help="filtered runtime, public API and CLI tests")
    feature.add_argument("filter", help="nonempty test-name substring present in all three suites")
    scopes.add_parser("full", help="complete pre-commit gate, including CLI tests once")
    args = parser.parse_args()
    if args.scope == "feature" and not args.filter.strip():
        parser.error("feature filter must be nonempty")
    if args.scope == "local" and args.filter is not None and not args.filter.strip():
        parser.error("local filter must be nonempty when supplied")
    plan = commands(args)
    if args.plan:
        print("PLAN ONLY — no verification performed", flush=True)
        for command in plan:
            print(shlex.join(command))
        return 0

    # Reports are evidence, not a cache: no passing command is skipped automatically.
    output = ROOT / "target" / "verification"
    output.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix=datetime.datetime.now().strftime("%Y%m%d-%H%M%S-"), dir=output))
    before = fingerprint()
    env = os.environ.copy()
    env.setdefault("RUST_TEST_THREADS", "4")
    env["CARGO_TERM_COLOR"] = "never"
    report = {"scope": args.scope, "before": before, "testThreads": env["RUST_TEST_THREADS"], "commands": []}
    exit_code = 0
    for index, command in enumerate(plan, start=1):
        print(f"\n[{args.scope} {index}/{len(plan)}] {shlex.join(command)}", flush=True)
        log = run / f"{index:02d}.log"
        started = time.monotonic()
        with log.open("w") as stream:
            process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            assert process.stdout is not None
            for line in process.stdout:
                print(line, end="", flush=True)
                stream.write(line)
            code = process.wait()
        tests = None
        if command[:2] == ["cargo", "test"]:
            tests = sum(int(n) for n in re.findall(r"test result: ok\. (\d+) passed", log.read_text()))
            if code == 0 and tests == 0:
                print("FAILED: test selection ran zero passing tests; choose a matching filter.", flush=True)
                code = 1
        report["commands"].append({"argv": command, "exitCode": code, "passedTests": tests, "seconds": round(time.monotonic()-started, 3), "log": log.name})
        if code:
            exit_code = code
            break
    report["after"] = fingerprint()
    report["sourceUnchanged"] = before == report["after"]
    if not report["sourceUnchanged"]:
        print("FAILED: source changed during verification; these checks do not prove the final state.", flush=True)
        exit_code = 1
    report["passed"] = exit_code == 0
    (run / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"\n{'PASS' if not exit_code else 'FAIL'} ({args.scope} scope only). Report: {run / 'report.json'}", flush=True)
    return exit_code


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print("\nVerification interrupted; no successful report.", file=sys.stderr)
        sys.exit(130)
