#!/usr/bin/env python3
"""Explicit verification scopes; never infer a full pass from focused tests."""
from __future__ import annotations

import argparse
import dataclasses
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
PACKAGES = (
    "varta", "varta-artifact", "varta-bigint", "varta-common", "varta-compiler",
    "varta-conformance", "varta-desktop", "varta-intrinsics", "varta-launcher", "varta-project",
    "varta-runtime", "varta-view",
)
CORE = ["varta-common", "varta-compiler", "varta-runtime", "varta-artifact"]
# The browser host's engine modules (registry, fuel, Worker, loader) run in
# Node's built-in test runner; no npm dependency is installed.
BROWSER_NODE_TESTS = ("crates/varta-common/browser/fuel.test.mjs", "crates/varta-project/src/browser/*.test.mjs")
BROWSER_NODE_PACKAGES = ("varta-common", "varta-project")
NODE_PASSED = re.compile(r"^(?:#|\u2139) pass (\d+)$", re.MULTILINE)
# The browser oracle gate. Chrome runs over the DevTools protocol with the
# executable chosen exactly as scripts/browser-probe.mjs chooses it; the
# Playwright drivers additionally need Firefox, WebKit and the Chrome channel.
CHROME_ENV = "VARTA_CHROME_BIN"
DEFAULT_CHROME = {"darwin": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"}
FALLBACK_CHROME = "chromium"
PLAYWRIGHT_ENV = "VARTA_PLAYWRIGHT_MODULE"
# Firefox reads its application data from these; each run isolates them in
# its own directory (on macOS a shared user directory can stop the launch).
FIREFOX_DATA_ENV = ("MOZ_APP_DATA", "MOZ_LOCAL_APP_DATA")
VIEW_ORACLE_ENV = "VARTA_VIEW_ORACLE"
REGISTRY_MODULE = "crates/varta-common/browser/registry.mjs"
# Drivers over the browser_bundle_probe fixture directory.
BUNDLE_DRIVERS = (
    "view-bundle", "resources", "fonts", "permissions", "permission-storage", "consent",
    "host-events", "navigation",
)
# Drivers that build temporary projects with the varta executable.
EXECUTABLE_DRIVERS = ("dev", "effects", "kv", "environment", "streaming")
STYLE_WORK_MODES = ((), ("--tree-depth",), ("--cascade-work",), ("--expanded-values",), ("--naming-work",))
# The direct-DOM variables driver runs once per engine selection.
VARIABLES_DRIVER = "scripts/test-browser-variables.mjs"
PLAYWRIGHT_ENGINES = ("chrome", "firefox", "webkit")
# Engine selections with a recorded defect: never run by the gate and never
# counted as passed; the run's verdict lists them.
KNOWN_GAPS = {
    (VARIABLES_DRIVER, "firefox"): "Firefox throws 'too much recursion' in view-css.mjs rewriteRootSelector",
}
# A run whose commands all passed but which skipped some is not a pass.
INCOMPLETE_EXIT = 3
PLAN_RUN = Path("<run>")
SKIPPED, KNOWN_GAP = "skipped", "known gap"


@dataclasses.dataclass(frozen=True)
class NotRun:
    """A planned command that is not executed: skipped for a missing engine or a known gap."""

    argv: list[str]
    reason: str
    kind: str = SKIPPED


def cargo(*args: str) -> list[str]:
    return ["cargo", *args]


def program(command: list[str]) -> list[str]:
    """The argv after an optional leading `env NAME=value ...` prefix."""
    if command[:1] != ["env"]:
        return command
    rest = command[1:]
    while rest and "=" in rest[0]:
        rest = rest[1:]
    return rest


def browser_node_tests() -> list[str]:
    """The Node suites as explicit sorted paths (no shell glob expansion)."""
    paths = []
    for pattern in BROWSER_NODE_TESTS:
        matched = sorted(ROOT.glob(pattern))
        # `node --test` without paths would discover tests in the whole tree.
        if not matched:
            raise SystemExit(f"no browser Node tests match {pattern}")
        paths += matched
    return ["node", "--test", *(str(path.relative_to(ROOT)) for path in paths)]


def companion() -> list[str]:
    return cargo("build", "-p", "varta-launcher", "--bin", "varta-runtime", "--locked")


def chrome_executable() -> str:
    return os.environ.get(CHROME_ENV, DEFAULT_CHROME.get(sys.platform, FALLBACK_CHROME))


def playwright_version(module: Path) -> str:
    """The version of the package that owns the module, as the drivers report it."""
    for directory in module.resolve().parents:
        manifest = directory / "package.json"
        if manifest.is_file():
            version = json.loads(manifest.read_text()).get("version")
            if not isinstance(version, str):
                raise SystemExit(f"no version in {manifest}")
            return version
    raise SystemExit(f"no package.json above {module}")


def browser(run: Path, playwright: Path | None) -> list[list[str] | NotRun]:
    """Registry drift, oracle-enabled Node suites, then Chrome and Playwright drivers.

    Fixture and oracle products go to the run directory, which is new for each
    run; the bundle builders refuse an existing directory.
    """
    registry, oracle = run / "registry.mjs", run / "view-oracle.json"
    bundles, network = run / "bundles", run / "network-bundles"
    varta = str(Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug" / "varta")
    with_oracle = ["env", f"{VIEW_ORACLE_ENV}={oracle}"]
    result: list[list[str] | NotRun] = [
        cargo("run", "-p", "varta-common", "--example", "generate_browser_registry", "--locked", "--",
              str(registry)),
        ["git", "diff", "--no-index", "--exit-code", REGISTRY_MODULE, str(registry)],
        cargo("run", "-p", "varta-view", "--example", "browser_contract_probe", "--locked", "--", str(oracle)),
        with_oracle + browser_node_tests(),
        cargo("build", "-p", "varta", "-p", "varta-launcher", "--locked"),
        cargo("run", "-p", "varta-project", "--example", "browser_bundle_probe", "--locked", "--", str(bundles)),
    ]
    chrome = [
        with_oracle + ["node", "scripts/test-browser-view.mjs"],
        # Generates its own network fixtures while its loopback servers run.
        ["node", "scripts/test-browser-bundle.mjs", str(network), "--generate-network"],
        *(["node", f"scripts/test-browser-{name}.mjs", str(bundles)] for name in BUNDLE_DRIVERS),
        *(["node", f"scripts/test-browser-{name}.mjs", varta] for name in EXECUTABLE_DRIVERS),
        *(["node", "scripts/test-browser-style-work-bundle.mjs", varta, *mode] for mode in STYLE_WORK_MODES),
    ]
    module = str(playwright) if playwright else f"<{PLAYWRIGHT_ENV}>"
    engines: list[tuple[list[str], str | None]] = [(command, None) for command in (
        ["node", "scripts/test-browser-matrix.mjs", str(bundles), module, "--effects", varta, "--kv", varta],
        ["node", "scripts/test-browser-integrity-matrix.mjs", str(bundles), module],
        ["node", "scripts/test-browser-dev-matrix.mjs", varta, module],
    )]
    engines += [(["node", VARIABLES_DRIVER, str(oracle), module, engine], KNOWN_GAPS.get((VARIABLES_DRIVER, engine)))
                for engine in PLAYWRIGHT_ENGINES]
    engines.append((["node", "scripts/test-browser-variable-bundle.mjs", varta, module], None))
    executable = chrome_executable()
    missing_chrome = None if shutil.which(executable) else (
        f"Chrome executable not found: {executable} (set {CHROME_ENV})")
    # The Playwright drivers launch the installed Chrome channel as well.
    missing_engines = missing_chrome or (None if playwright else (
        f"no Playwright module (--playwright or {PLAYWRIGHT_ENV}); Firefox and WebKit not run"))
    result += [NotRun(command, missing_chrome) if missing_chrome else command for command in chrome]
    isolated = ["env", *(f"{name}={ROOT / run / 'firefox' / name.lower()}" for name in FIREFOX_DATA_ENV)]
    for command, gap in engines:
        if gap:
            result.append(NotRun(isolated + command, gap, KNOWN_GAP))
        elif missing_engines:
            result.append(NotRun(isolated + command, missing_engines))
        else:
            result.append(isolated + command)
    return result


def commands(args: argparse.Namespace, run: Path) -> list[list[str] | NotRun]:
    if args.scope == "browser":
        return browser(run, args.playwright)
    if args.scope == "local":
        result = [cargo("check", "-p", args.package, "--all-targets", "--locked")]
        if args.package in ("varta", "varta-project", "varta-launcher"):
            result.append(companion())
        test = cargo("test", "-p", args.package, "--locked")
        if args.filter:
            test.append(args.filter)
        result.append(test)
        # A cargo test-name filter does not select Node tests; run them only
        # for an unfiltered check of a crate that owns browser modules.
        if args.package in BROWSER_NODE_PACKAGES and not args.filter:
            result.append(browser_node_tests())
        return result
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
        # The value-layout core the wasm32 intrinsic library builds against.
        cargo("check", "-p", "varta-common", "-p", "varta-intrinsics", "--no-default-features",
              "--lib", "--locked"),
        companion(),
        cargo("clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"),
        cargo("build", "--workspace", "--all-targets", "--locked"),
        cargo("test", "--workspace", "--locked"),
        browser_node_tests(),
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
    browser_scope = scopes.add_parser(
        "browser", help="browser oracle gate: registry drift, oracle Node suites, Chrome and Playwright drivers")
    browser_scope.add_argument(
        "--playwright", type=Path, default=os.environ.get(PLAYWRIGHT_ENV) or None, metavar="MODULE",
        help=f"Playwright module (its index.mjs) for the Firefox/WebKit/Chrome drivers; default ${PLAYWRIGHT_ENV}")
    args = parser.parse_args()
    if args.scope == "feature" and not args.filter.strip():
        parser.error("feature filter must be nonempty")
    if args.scope == "local" and args.filter is not None and not args.filter.strip():
        parser.error("local filter must be nonempty when supplied")
    if args.scope == "browser" and args.playwright is not None:
        if not args.playwright.is_file():
            parser.error(f"Playwright module not found: {args.playwright}")
        # The drivers run from the workspace root, not the caller's directory.
        args.playwright = args.playwright.resolve()
    output = Path("target") / "verification"
    if args.plan:
        print("PLAN ONLY — no verification performed", flush=True)
        for command in commands(args, output / PLAN_RUN):
            if isinstance(command, NotRun):
                print(f"NOT RUN, {command.kind} ({command.reason}): {shlex.join(command.argv)}")
            else:
                print(shlex.join(command))
        return 0

    # Reports are evidence, not a cache: no passing command is skipped automatically.
    (ROOT / output).mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix=datetime.datetime.now().strftime("%Y%m%d-%H%M%S-"), dir=ROOT / output))
    plan = commands(args, run.relative_to(ROOT))
    before = fingerprint()
    env = os.environ.copy()
    env.setdefault("RUST_TEST_THREADS", "4")
    env["CARGO_TERM_COLOR"] = "never"
    report = {"scope": args.scope, "before": before, "testThreads": env["RUST_TEST_THREADS"], "commands": []}
    if args.scope == "browser":
        report["browser"] = {
            "chrome": shutil.which(chrome_executable()),
            "playwrightModule": str(args.playwright) if args.playwright else None,
            "playwrightVersion": playwright_version(args.playwright) if args.playwright else None,
        }
    exit_code = 0
    for index, command in enumerate(plan, start=1):
        if isinstance(command, NotRun):
            print(f"\n[{args.scope} {index}/{len(plan)}] NOT RUN, {command.kind} ({command.reason}): "
                  f"{shlex.join(command.argv)}", flush=True)
            report["commands"].append({"argv": command.argv, "notRun": command.kind, "reason": command.reason})
            continue
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
        elif program(command)[:2] == ["node", "--test"]:
            tests = sum(int(n) for n in NODE_PASSED.findall(log.read_text()))
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
    skipped = sum(entry.get("notRun") == SKIPPED for entry in report["commands"])
    gaps = sum(entry.get("notRun") == KNOWN_GAP for entry in report["commands"])
    report["skipped"], report["knownGaps"] = skipped, gaps
    verdict = "FAIL" if exit_code else "PASS"
    if not exit_code and skipped:
        # Skipped engines are never reported as passing.
        exit_code = INCOMPLETE_EXIT
        verdict = f"INCOMPLETE: {skipped} skipped command(s) not verified"
    if gaps:
        verdict += f"; {gaps} known-gap command(s) not run and not verified"
    report["passed"] = exit_code == 0
    (run / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"\n{verdict} ({args.scope} scope only). Report: {run / 'report.json'}", flush=True)
    return exit_code


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print("\nVerification interrupted; no successful report.", file=sys.stderr)
        sys.exit(130)
