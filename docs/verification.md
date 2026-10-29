# Scoped development verification

Develop coherent batches of related behavior. Run focused checks while implementing
and fixing them, cover their callers when a feature is ready, then run the full
gate once on the completed code batch before committing. A full pass does not need
to be repeated after a documentation-only edit. New code/build inputs invalidate
any previous check whose scope includes those inputs.

Python 3 and the pinned Rust toolchain are required. The helper resolves the
workspace from its own location; these examples start at the workspace root:

```sh
python3 scripts/verify.py local varta-bigint
python3 scripts/verify.py local varta-runtime --filter array_inference
python3 scripts/verify.py feature array_inference
python3 scripts/verify.py full
python3 scripts/verify.py --plan full
```

`--plan` prints commands without running or verifying anything. Local checks run
Cargo check for all targets of one crate, then that crate's tests, optionally
filtered. Project, CLI (package `varta`) and launcher checks build the companion after
the Cargo check and before their tests. Feature checks
build the companion, then select matching runtime unit tests, public engine API
tests and CLI smoke tests. Each selected suite must execute passing tests; a typo
that matches zero tests fails. Where names differ, run explicit Cargo commands
with the appropriate filters instead of widening a filter until it happens to pass.

## Change-to-check map

| Change | During implementation | At the feature/batch boundary |
| --- | --- | --- |
| Isolated numeric algorithm | Local numeric crate and edge/error/resource cases | Relevant runtime conversion/operator tests once integrated; full before code commit |
| Parsing, lowering, inference or emission | Compiler check/tests plus focused runtime execution tests | Public API/module diagnostics and CLI/source-free artifact cases; full |
| Runtime semantics | Focused runtime tests with positive, error and resource cases | Affected public API and artifact paths; full |
| ABI, value layout, GC, roots or exception channels | Common/ABI validation and affected runtime suites | Historical-profile rejection, canonical payloads, collections/roots, host-free helper checks, public API/packaged execution; full |
| Permissions or host boundaries | Allowed/denied calls, exact grants and host ceilings | Malformed inputs, fatal limits, status and finalizer behavior through public API/CLI; full |
| Artifact, launcher or packaging | Artifact/launcher/project tests and focused CLI paths | Full, plus separate `cargo test -p varta --test smoke --locked` after the companion build |
| Dependency/build changes | Relevant isolated builds and engine dependency-boundary test | Full; verify lockfile and runtime distribution boundaries |
| Conformance inventory/runner | Local `varta-conformance` tests and reviewed case changes | Baseline JSON, strict JSON with expected exit status and explicit gap review; full for code/inventory changes |
| Documentation only | Diff, links, documented commands and consistency | No repeated engine suite if engine/build inputs are unchanged |
| Verification script or CI orchestration only | Syntax, command plans, real focused runs, unmatched filters and failure propagation | Check CI command parity and logs/fingerprints; unchanged engine suites need no full repeat |

The boundary column describes required coverage, not a requirement to rerun every
suite after every edit. The feature helper is a convenience for matching test names;
ABI/security changes need the wider suites in this table. Production limits and
compatibility requirements stay intact. Never remove meaningful negative/resource
tests just to shorten a run.

## Full gate

The helper and CI run these commands in order, stopping on failure:

```sh
cargo fmt --all -- --check
cargo check -p varta-common -p varta-compiler -p varta-runtime -p varta-artifact --lib --locked
cargo build -p varta-launcher --bin varta-runtime --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
cargo run -p varta-conformance --locked -- --json
```

Workspace tests already include `varta`'s `smoke` integration suite. A separate
repeat is reserved for launcher/packaging changes or a named flakiness investigation.
CI retains complete Linux/macOS coverage. A local full pass proves only that host.
Known unsupported conformance cases remain gaps, even when the baseline matches.

## Evidence and failure handling

The helper streams command output and records per-command logs, exit status,
passed-test counts, durations, scope and before/after source fingerprints in
`target/verification/<run>/report.json`. Source fingerprints include HEAD plus
tracked/untracked nonignored file content, executable bits and symlink targets.
A source change during a run fails it; interrupted runs do not receive a passing
report. The helper defaults to four test threads, respects RUST_TEST_THREADS and
never raises guest limits. It has no cache and never automatically skips a check.

Keep the previous passing report as evidence for unchanged inputs; do not present
it as a pass for later code. Relevant inputs also include toolchain, lockfile,
build options/environment and the runtime companion; a source fingerprint alone
cannot prove those stayed unchanged. After a failure, rerun the affected selection
while fixing it, then run the final full gate once after the last code fix.
Report exactly what ran and what remains unverified. Full core completion still
requires the roadmap's acceptance criteria, independent of this faster workflow.
