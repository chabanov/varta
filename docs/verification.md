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
| Browser fuel transform | `cargo test -p varta-compiler --lib browser_meter` compares private images with locked native Wasmtime: results, remaining fuel, memory and selected fair host charges | Compiler suite/Clippy; after host integration, all shared charge families and Node/Chromium/Firefox/WebKit observations, permission denials, Worker termination and measured overhead; full before code commit |
| Runtime semantics | Focused runtime tests with positive, error and resource cases | Affected public API and artifact paths; full |
| ABI, value layout, GC, roots or exception channels | Common/ABI validation and affected runtime suites | Historical-profile rejection, canonical payloads, collections/roots, host-free helper checks, public API/packaged execution; full |
| Permissions or host boundaries | Allowed/denied calls, exact grants and host ceilings | Malformed inputs, fatal limits, status and finalizer behavior through public API/CLI; full |
| Artifact, launcher or packaging | Artifact/launcher/project tests and focused CLI paths | Full, plus separate `cargo test -p varta --test smoke --locked` after the companion build |
| Linked intrinsics (`varta-intrinsics`, `varta-intrinsics-wasm`) | `cargo test -p varta-intrinsics`, then `python3 scripts/build-intrinsics.py` (offline, locked; commit `intrinsics.wasm` and `intrinsics.json` together) and the compiler `link::` tests | Runtime `linked_intrinsic` corpus tests and the host suites of every family whose code moved; the v38 CLI smoke test; full |
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
cargo check -p varta-common -p varta-intrinsics --no-default-features --lib --locked
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

## Browser registry and fuel probe (development)

For the experimental native password/search fields, run the focused view tests
and the Wasm application boundary, then compare the native oracle with real Chrome:

```sh
cargo test -p varta-view --test fields --locked
cargo test -p varta-runtime --lib native_password_and_search_frames_deliver_original_guest_values --locked
cargo run -p varta-view --example native_fields_probe --locked -- target/native-fields-oracle.json
node scripts/test-native-fields.mjs target/native-fields-oracle.json
```

The oracle carries its original font bytes and compares fixed control boxes and
UTF-8 caret offsets for password graphemes, emoji and search text. It suppresses
Chrome's search-clear affordance to compare the admitted plain-field surface.
This is a Chrome sample, not a full browser or platform acceptance gate. The
`field-types` desktop fixture permits native keyboard/pointer observations;
cross-platform IME, accessibility and production packaging remain open.

For native checkbox/radio painting and the shared radio model:

```sh
cargo test -p varta-view --test checkables --locked
cargo test -p varta-runtime --lib native_checkable_frames_deliver_checked_events_and_isolate_radio_forms --locked
cargo run -p varta-view --example browser_contract_probe --locked -- target/browser-view-oracle.json
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node --test crates/varta-project/src/browser/*.test.mjs
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node scripts/test-browser-view.mjs
```

For single-select state, popup geometry and TSX initialization:

```sh
cargo test -p varta-view --test selects --locked
cargo test -p varta-runtime --lib native_select_popup_choice_and_equal_guest_echo_preserve_option_identity --locked
cargo test -p varta-runtime --lib tsx_select_values_follow_options_without_duplicate_reads_or_lost_dependencies --locked
```

The same browser contract oracle also compares select values, concrete selected
options and event bytes across native, browser model, DOM and deferred Worker
state. Its select-work fixtures compare accepted/rejected batches and the exact
failure position for ASCII/emoji values, mutated fallback text and deep owner
searches; rejection must leave DOM/model state unchanged. The select unit suites
also cover detached inventory, stale/equal no-ops and a fresh allowance after
rejection. The Worker session regression requires identical admission while an
edit is deferred, and duplicate-write fixtures keep original prepared stylesheet
indices during DOM replay. The Wasm boundary check is:

```sh
cargo test -p varta-runtime --lib select_work_exhaustion_is_fatal_and_keeps_the_previous_committed_view --locked
```

It requires the typed fatal view failure before a guest catch/following log and
retains the previous successful view commit. For portable and source-free native
execution, build the user binaries once and run:

```sh
cargo build -p varta -p varta-launcher --locked
node scripts/test-select-work-runtime.mjs
```

The probe deletes its source/config before execution and requires the exact work
failure, retained earlier output and no catch/following effect from both the
standalone runtime and the single binary. It moves the single binary into a fresh
directory and deletes the portable bundle before executing it. The select unit/Wasm suites additionally
cover logical character input without committed text and retain text-field insertion
rules. The desktop fixture permits native keyboard/pointer observations.
After building the `examples/select`
fixture with `--target browser`, run
`node scripts/test-browser-select-bundle.mjs <generated-web-directory>` to check
trusted character input and duplicate-value echoes through the packaged Worker.

The native oracle covers equal-name radios in separate forms/trees, empty names,
moving/renaming checked members and rollback. Real Chrome compares DOM checked
state with the shared model and native click/keydown/input/change event bytes,
including cancellation. The `checkables` desktop fixture uses no font and permits
pointer, Space, Tab and radio-arrow observations. Its explicit rectangular paint
subset does not establish platform-theme or whole-L0 visual parity.

For the local browser-dev host increment:

Input observation changes additionally need the project library tests (including
`modules::watch`) and CLI module integration tests. Cover equal-size edits,
conflicting repeated reads/restoration, missing `.ts`/`.tsx` candidates, failure
recovery, pinned no-follow replacements, resource/text/config inputs and journal
overflow. These checks establish rebuild input detection; they do not prove live
reload or page-session shutdown.

For the optional native flex-item projection:

```sh
cargo run -p varta-view --example browser_contract_probe --locked -- target/browser-view-flex-oracle.json
VARTA_VIEW_ORACLE=target/browser-view-flex-oracle.json node scripts/test-browser-view.mjs
```
This checks a native-projected result against real Chrome for one direct flex
child; it does not establish general computed-style or layout equivalence.

Live rebuild changes additionally need failed-source/config/import recovery,
no repeated compilation of unchanged errors, captured authority and cancellation
tests. Client tests cover serial bounded status reads, literal diagnostic text,
abort-before-navigation, pagehide races and unavailable/malformed responses. Chrome
must observe real page-realm replacement, missing-file creation without editing
the importer, bounded revision retention, identical-output recovery without reload
and termination of the preceding live view Worker.

Revision delivery changes need `browser::dev` library tests, the `browser_dev`
CLI smoke filter and the real browser-dev probe below. Check all file identities,
unambiguous framing, preserved CSP/SRI, missing root aliases, bounded retention,
reversion/deduplication, owned responses after eviction and rejection of expired
revision paths without substituting current files. Discover the asset directory
from the generated page; root asset URLs intentionally return missing.

```bash
cargo test -p varta-project browser::dev --lib --locked
cargo test -p varta --test smoke browser_dev --locked
cargo test -p varta-project --test boundaries --locked
cargo build -p varta --locked
node --test crates/varta-project/src/browser/dev-client.test.mjs
node scripts/test-browser-dev.mjs
```

These exercise the real CLI/server and Chrome: memory-only delivery, CSP/SRI,
runtime ceilings and scoped decisions, console/view/application modes, bounded
HTTP inputs, source/path/foreign-origin rejection, native-only flag rejection,
shutdown and no native root binding or production writes. Bootstrap drift and
origin-name rewriting have separate regressions. The completed dev/reload package
passes the consolidated **2946-test** full gate at
`target/verification/20261102-093345-4pi5qr3n/report.json`: all eight commands,
0 failures, 16 ignored and unchanged source hash
`ababac54e03c6070e9a05140d1d02fdfb6b1d312d607ec600067ef3b1d691a7b`.
Only checkpoint documentation changed afterward. The separate 85 Node tests and
scoped three-engine 63-check matrix are outside Cargo; the latter's report and
local drivers are archived at `target/browser-matrix-20261102/`. Its 13 executions
per engine compare exact native logs/usage; five integrity boundaries per engine
reject before guest/Worker start; three live dev sequences per engine cover
rebuild/recovery, trusted view input/replacement and server-loss termination.
This is neither complete browser coverage nor the cross-target UARP pilot.
The 13 shared execution cases now have a portable driver:

```sh
node scripts/test-browser-matrix.mjs <generated-fixture-directory> <playwright-module-path>
node scripts/test-browser-integrity-matrix.mjs <generated-fixture-directory> <playwright-module-path>
node scripts/test-browser-dev-matrix.mjs <varta-executable> <playwright-module-path>
```

Use Playwright 1.64.1 and its matching Firefox/WebKit downloads; the Chrome
channel uses an installed Chrome with its sandbox enabled. Generate fixtures
with `node scripts/test-browser-bundle.mjs <new-directory> --generate-network`.
On macOS, isolated `MOZ_APP_DATA` and `MOZ_LOCAL_APP_DATA` directories may be
needed for Firefox. The driver compares 39 executions with native golden logs
and usage. The integrity driver checks five tampered assets on each engine;
the dev driver checks rebuild recovery, live Worker replacement and shutdown on
each engine using isolated temporary projects and browser contexts.
Source/config/resource edits now rebuild automatically after a
successful initial startup; policy/decision-file edits still require restart.

The official registry has a Rust regeneration drift test; it does not yet include
all memory/view tables. Regenerate it explicitly after a registry change:

```bash
cargo run -p varta-common --example generate_browser_registry --locked -- crates/varta-common/browser/registry.mjs
cargo test -p varta-common --lib browser::
node --test crates/varta-common/browser/fuel.test.mjs
```

Check trusted preparation and private resource ceilings with:

```bash
cargo test -p varta-project --lib browser::
cargo test -p varta-project --test boundaries
cargo test -p varta-compiler --lib browser_meter
```

These checks include immutable original/policy ownership, malformed imports and
reserved private markers, legacy budgets, requested/host budget resolution,
initial allocations above the ceiling, and private growth matching native Store
limits. Preparation never binds filesystem roots; it is not browser effect
permission enforcement or an integrity-checked application loader.

For a real JS-engine fuel comparison, generate the fixed trusted v38 program and
its native golden observations, then run the shared observer under Node:

```bash
cargo run -p varta-compiler --example browser_meter_probe --locked -- target/browser-meter-probe
node crates/varta-common/browser/probe.mjs target/browser-meter-probe
```

The probe derives additional budgets from native host-call/byte-work thresholds;
compare outcomes, remaining fuel, call positions and Unicode output. Its files in
`target/browser-meter-probe/` are ignored verification products, not distributable
artifacts. For the browser check, serve the repository root on loopback and open
`/crates/varta-common/browser/probe.html`; require `data-result="pass"` and the
passing observation count, not just a browser exit code. Both JS runners share
`probe-run.mjs` and `FuelTank`. They accept only these trusted fixtures and do not
supply artifact admission or permission validation. The current local checkpoint
has matching observations in Node and headless Chrome; Firefox/WebKit, the full
corpus, other effects, Worker deadlines and official host packaging remain open.

The initial Worker console bundle has a separate real-browser acceptance command.
Use a new fixture directory (the builder refuses existing directories):

```bash
cargo run -p varta-project --example browser_bundle_probe --locked -- target/browser-bundle-probe
node scripts/test-browser-bundle.mjs target/browser-bundle-probe
cargo test -p varta-project --lib --test boundaries
cargo clippy -p varta-project --all-targets --locked -- -D warnings
```

The Node script uses Chrome's debugging protocol and a loopback fixture server,
with real wall-clock timers. It defaults to installed Chrome on macOS and
`chromium` elsewhere; set `VARTA_CHROME_BIN` for a different installed executable.
It launches a separate temporary profile, cleans up the browser/server/profile,
and restores every substituted file in `finally`. No virtual-time acceleration
is used for running-Worker deadline/cancellation evidence. The 34 fixed cases
include native golden log/status observations, chunked UTF-16, fuel/call/output
limits, active deadline/abort, invalid runtime ceilings, four integrity and two
length failures. Native golden files live beside bundles as test products and
are not distributed bundle files. Other engines/effects remain target gaps.

The project now uses existing locked SHA-256/base64 dependencies directly to
produce SRI of the exact generated loader bytes. The initial bundle rejects
unsupported effects and supplied host policies before writing; runtime host
ceilings remain separate inputs, preserving the existing policy boundary.

For distribution CLI changes, build the independent companion before focused
smoke tests (the native target alias is verified through that companion):

```bash
cargo build -p varta-launcher --bin varta-runtime --locked
cargo test -p varta --test smoke browser_build --locked
cargo clippy -p varta --all-targets --locked -- -D warnings
```

The CLI cases exercise original Wasm/manifest equivalence to the portable build,
original/private source-free validation, no overwrite, gap/type failures before
output, unmounted native roots, policy/selector rejection and native execution.
They do not rerun the browser engine suite: reuse the recorded browser checkpoint
only while loader/Worker/bundle-generation inputs remain unchanged.

For Promise/async Worker changes, the bundle probe now also covers FIFO/nested
jobs, await/finally, late handling, pending promises, bounded rejection retention,
primitive/Error/subclass/BigInt diagnostics, long fields, and shared fuel/call
allowances. Its async observations compare exact native remaining fuel,
host-call counts, microtasks and output bytes in addition to logs/status.

```bash
node --test crates/varta-project/src/browser/scheduler.test.mjs crates/varta-project/src/browser/reasons.test.mjs crates/varta-project/src/browser/timers.test.mjs
cargo test -p varta-common --lib browser:: --locked
cargo test -p varta-runtime --lib promise_tests --locked
cargo test -p varta-runtime --lib exceptions::tests --locked
```

The Node tests include malformed heap/descriptor ranges, slot indices before
BigInt shifts, duplicate/full queues, invalid outcomes, serial/job limits and
rejection bit-word boundaries. Regenerate the official registry before building
fixtures after a layout/limit change. The native long-message regression verifies
v38 Error/subclass rejection fields above the old string bound without increasing
production budgets or copying the full reason into retained host state.

The extended real-Chrome bundle probe also compares native timer ordering,
cancellation, intervals, delay normalization, unref, callback exceptions, deadline
rejection, optional/absent permissions and v38 renewed budgets. It exercises
empty/false/unrelated/exact runtime permission ceilings and invalid ceiling input.
Timer observations include total fuel/turns and final-turn remaining fuel.

For browser network changes, generate fixtures while the real loopback servers are
running (the destination is a new directory):

```bash
node --test crates/varta-project/src/browser/network.test.mjs crates/varta-project/src/browser/network-transport.test.mjs
node scripts/test-browser-bundle.mjs target/browser-network-probe --generate-network
```

The latter generates native golden observations first, then runs the same bundles
in Chrome against a different loopback origin with CORS/preflight. GET, POST, byte
bodies, custom headers, denied requests and pre-start/active abort compare native
logs and exact total/final-turn fuel, host calls, microtasks, output and turns.
Runtime ceiling tests cover URL/method denials and malformed scopes. A browser-only
redirect check and denied-target counter prove no followed unauthorized hop;
setting a browser cookie before the requests proves credentials remain omitted.
Transport unit cases cover bounded chunks/body size, exposed headers/encoding,
BOM/UTF-8 carry, text-to-byte transitions and explicit browser-header failures.
These checks do not establish Firefox/WebKit or arbitrary HTTP wire parity.

CSP-origin packaging checks additionally reject URL-valid punctuation/underscores/
IPv6 origins before HTML output; the runtime URL parser alone is insufficient for
safe CSP generation. These tests exercise full preparation and bundle generation.

For the browser view model/DOM foundation:

```bash
cargo run -p varta-view --example browser_contract_probe --locked -- target/browser-view-contract.json
VARTA_VIEW_ORACLE=target/browser-view-contract.json node --test crates/varta-project/src/browser/*.test.mjs
node scripts/test-browser-view.mjs target/browser-view.png
cargo test -p varta-view --locked
cargo test -p varta-common --lib browser:: --locked
cargo clippy -p varta-common -p varta-view -p varta-project --all-targets --locked -- -D warnings
```

The separate native process uses public `View`/`BatchWriter`/`EventRecord` and the
common CSS validator. Oracle-enabled Node checks compare acceptance, rollback,
structural state, CSS values and byte-exact records. Stylesheet cascade/layout and
accessibility are not claimed by the structural projection. Without
`VARTA_VIEW_ORACLE`, only the standalone browser-model boundary cases run.
The Chrome probe is a private DOM test under CSP, not a Worker/application session;
it checks atomic rejection, UTF-16, live node identity and field sequence handling,
CSSOM, non-gesture events and no ambient resource URLs. Its optional PNG output is
for local visual inspection. This private probe is complemented by the generated bundle/session acceptance below.

For the generated view loader/Worker path (use a fresh fixture directory):

```bash
cargo run -p varta-project --example browser_bundle_probe --locked -- target/browser-view-sessions
node scripts/test-browser-view-bundle.mjs target/browser-view-sessions
node scripts/test-browser-resources.mjs target/browser-view-sessions
node scripts/test-browser-fonts.mjs target/browser-view-sessions
node scripts/test-browser-permissions.mjs target/browser-view-sessions
node scripts/test-browser-permission-storage.mjs target/browser-view-sessions
node scripts/test-browser-consent.mjs target/browser-view-sessions
node scripts/test-browser-host-events.mjs target/browser-view-sessions
node scripts/test-browser-navigation.mjs target/browser-view-sessions
cargo test -p varta --test smoke browser_build --locked
cargo test -p varta-runtime --lib exact_url_scope --locked
```

The session probe exercises trusted CDP click/input, persistent closure state,
Promise/checkpoint/timer turns and exact native usage after events. It checks
bounded rendering, close/remount, idle versus active deadlines, invalid/second
commit traps before later logging, passive imports, denied link/clipboard calls
with zero platform effects, allowed calls through test-only platform spies, and
frozen-view navigation prevention. The spies never write the actual clipboard or
open an external page. Re-run the normal console/timer/network/integrity bundle
probe when changing the shared Worker/loader; its previous results do not cover
new execution-loop code. The resource probe checks real PNG decoding and Blob
URLs, exact native accounting, hash/length tampering, malformed compressed data,
missing inventory entries before later guest effects, and URL cleanup on close.
CLI checks cover project-relative inputs and leaf/directory symlink rejection.
The font probe decodes original TTF/OTF/WOFF/WOFF2 fixtures, compares native
accounting and actual glyph advances, exercises inline/quoted/variable/media
families, and runs simultaneous sessions with different bytes under one logical
family plus an ambient host face. It checks cleanup after close, aborted/deadline
loads and late completion, malformed/tampered data before the first guest log, and
rejection of a font name used as an image. Tests consume checked-in tiny fixtures;
fontTools/Brotli is required only for optional fixture regeneration.
Full browser-engine parity, richer font face collections and other image formats
remain separate acceptance work.

The permission probe compares native query/request logs, exact usage and audit
rows (excluding timestamps), including optional/stored/required/identity/ceiling
cases and malformed UTF-16/NUL/UTF-8-limit inputs. It also checks browser clipboard
state narrowing and live updates, listener cleanup, zero clipboard writes under
platform denial or observer failure, immutable snapshots and invalid decisions
rejected before artifact fetch. Node lifecycle tests use a controlled Wasm boundary
to verify startup-state coalescing, pre-entry faults, checkpoint backpressure and
transfer progress ahead of overdue intervals. Those tests complement real Wasm in
Chrome; they do not replace it. Native source-id exhaustion and generated embedding
limits have separate focused checks. The storage probe uses real IndexedDB with
concurrent connections, aborts, corrupt-file recovery, capacity failure without
eviction and connection cleanup. Live sessions check acknowledged/coalesced
revocation, zero subsequent clipboard calls, a persisted required denial across
page reload and fatal store/observer failures. Worker lifecycle tests separately
verify that revocation does not cancel an authorized transfer and blocks a new
request in its completion turn; malformed controls fail. Delayed audit observations
retain their original decision without restoring a grant.
This increment remains outside the earlier
consolidated browser-core gate until the broader permission batch is complete.

The completed consent/event/navigation package additionally passes the full gate:
**2921 passed, 0 failed, 16 ignored**, all eight commands, unchanged source hash
`52bcc1b91035af6a65a2d8688f43e22435df8e40c167ed0dff00618393dd19df`.
Report: `target/verification/20261102-072941-39498ymp/report.json`. Conformance is
123/127, four known unsupported, zero regressions, eight untested feature rows.
Only checkpoint documentation changed afterward. This is macOS arm64 evidence;
the browser probes above/below remain separate from Cargo's full gate.

The consent probe compares real generated Wasm against native scripted views for
required/optional/parallel requests with Allow/Deny/Dismiss: logs, exact cumulative
fuel/calls/jobs/turns and successful-session audit rows. It waits beyond a narrowed
700ms turn deadline both before first-run approval and between turns, verifies no
guest progress while waiting, and tests abort/revocation races including delayed
storage. The owned modal is exercised under CSP with trusted clicks and Escape;
synthetic approval is ignored, package markup stays text, and abort removes it.
Generated page tests cover initial consent and persisted allow/deny across reload.
The probe saves `permission-dialog.png` for visual inspection. Other browser engines
and OS effects still need their own acceptance evidence.

The named-event probe compares generic non-view applications with native instances:
FIFO/coalescing, close/reopen generations, listener failure, exact fuel/calls/jobs/
turns, and host gesture consent. Background records cannot prompt. It also checks
record ownership before readiness, NUL/BOM/supplementary text, recoverable size/name/
encoding rejection, terminal cleanup and acknowledgement backpressure. Node tests
separately fill the Worker queue, preserve arrival/gesture through coalescing,
reject stale source generations and malformed batch controls, and discard closed
source tasks. Browser acknowledgement capacity and native-equivalent Worker queue
capacity are distinct bounded surfaces; the probe does not claim synchronous
native admission timing for arbitrary concurrent browser host calls.

The navigation probe compares 15 native scripted-view scenarios (wide/explicit
requests, wide/explicit/empty host ceilings, Allow/Deny/Dismiss) with Chrome logs,
exact accounting and audit rows. Three repeated activations prove per-address
answers are ephemeral. It also tests a checked pause without an extra turn,
waiting beyond a narrowed deadline, revocation/abort/observer failures, platform
exceptions, owned full-URL display, synthetic clicks, headless denial and overlapping
activations. Opens are spies only. Worker lifecycle tests reject malformed pause/
resume controls and prove queued tasks remain parked until matching resume.

## Radio reconciliation transaction work

The radio allowance is independent of select work and guest fuel. For this
increment, check owner/form semantics, repeated checked/type/name writes, stale
edits, unchecked moved subtrees, UTF-8 name sizes, exact allowance boundaries and
fatal guest behavior. Rebuild the native oracle after changing its fixtures:

```sh
cargo test -p varta-view --test checkables --locked
cargo test -p varta-runtime --lib radio_work_exhaustion_is_fatal_and_retains_the_committed_group --locked
cargo run -p varta-view --example browser_contract_probe --locked -- target/browser-view-oracle.json
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node --test crates/varta-project/src/browser/*.test.mjs
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node scripts/test-browser-view.mjs
cargo test -p varta-common --lib browser::tests --locked
cargo test -p varta-common --test embedding_documentation --locked
```

The Chrome probe compares native admission, failure operation and DOM rollback;
the Worker test compares admission before/after a deferred local radio edit. The
application-owned mockup driver also exercises radio work refusal and a later
successful patch on each selected engine:

```sh
node --test examples/uarp-console/scripts/serve-mockups.test.mjs
node examples/uarp-console/scripts/test-mockups.mjs /path/to/playwright/index.mjs chrome
node examples/uarp-console/scripts/test-mockups.mjs /path/to/playwright/index.mjs webkit
```

Use `firefox` or omit the final selector for the complete three-engine attempt.
A browser launch failure is an unverified engine, not a passing matrix. These
focused checks do not establish local-input radio work limits, aggregate
retained-text/cascade limits, complete
cross-target radio behavior, compiled pilot acceptance or release readiness.

## Retained view text and native radio navigation

This scope covers source-byte accounting, sticky live-value capacity, exact
acceptance/rejection, deferred parent edits, and native focus order across many
radio names and membership changes:

```sh
cargo test -p varta-view --test retained_text --test checkables --locked
cargo test -p varta-runtime --lib retained_view_text_exhaustion_is_fatal_and_keeps_the_prior_payload --locked
cargo run -p varta-view --example browser_contract_probe --locked -- target/browser-view-oracle.json
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node --test crates/varta-project/src/browser/*.test.mjs
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node scripts/test-browser-view.mjs
cargo test -p varta-common --lib browser::tests --locked
cargo test -p varta-common --test embedding_documentation --locked
cargo check -p varta-common --no-default-features --lib --locked
cargo clippy -p varta-common -p varta-view -p varta-project -p varta-runtime --all-targets --locked -- -D warnings
```

The retained-text fixtures cover every canonical source slot, detached nodes,
accumulation across batches, replacement/deletion, UTF-8 capacity, stale writes
and same-batch type transitions. Empty commits after rejected JS transactions
also check that hidden source-size caches cannot poison future admission.
Native and browser payload accounting excludes actual live values already
covered by sticky reservations, preserving admission while their event is queued.
This is a payload contract, not a measurement of process RSS or complete CSS work.

For normal-stack application acceptance after rebuilding the CLI/companion and
projection example, run from the repository root:

```sh
cargo build -p varta -p varta-launcher --locked
cargo build -p varta-view --example project_flex_snapshot --locked
VARTA_BIN="$PWD/target/debug/varta" examples/uarp-console/scripts/test.sh
node examples/uarp-console/scripts/test-browser-chat-matrix.mjs target/debug/varta /path/to/playwright/index.mjs chrome
node examples/uarp-console/scripts/test-browser-chat-matrix.mjs target/debug/varta /path/to/playwright/index.mjs webkit
```

The optional engine selector keeps a focused run local; omit it for all three
engines. A selected engine pass does not establish Firefox or the full matrix.
The mockup driver separately checks retained-capacity refusal, unchanged DOM and
successful reclamation; the live chat probe checks trusted interactions, the
native ordered tree, selected accessibility and sampled style declarations.


## Stylesheet expansion and packaged resource failures

Use a fresh native oracle for work-unit, grammar, operation-position and rollback
parity. The generated-bundle probe builds a real browser artifact, removes its
sources/configuration, delivers a trusted event and checks fatal CSS admission
through the Worker while the prior DOM/title remain intact:

```sh
cargo test -p varta-common --lib view:: --locked
cargo test -p varta-view --test style_work --locked
cargo test -p varta-runtime --lib stylesheet_expansion_work_is_fatal_before_effects_and_keeps_the_previous_view --locked
cargo run -p varta-view --example browser_contract_probe --locked -- target/browser-view-oracle.json
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node --test crates/varta-project/src/browser/*.test.mjs
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node scripts/test-browser-view.mjs
cargo build -p varta -p varta-launcher --locked
node scripts/test-browser-style-work-bundle.mjs target/debug/varta
```

The source/parsed-work ceiling and selector indexing do not establish a complete
bound for per-frame cascade, derived output or accessibility work. Reuse the
selected-engine mockup/live-chat checks above for application coverage; a
Chrome/WebKit pass does not establish the full three-engine matrix.


## Component depth and potential cascade reads

Exercise attached and detached depth boundaries, moved text leaves, iterative
snapshot/accessibility serialization on a normal stack, command/final cascade
failure positions, cached index rollback and successful smaller retries:

```sh
cargo test -p varta-view --test tree_depth --test cascade_work --locked
cargo run -p varta-view --example browser_contract_probe --locked -- target/browser-view-oracle.json
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node --test crates/varta-project/src/browser/*.test.mjs
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node scripts/test-browser-view.mjs
node scripts/test-browser-style-work-bundle.mjs target/debug/varta --tree-depth
node scripts/test-browser-style-work-bundle.mjs target/debug/varta --cascade-work
```

Rebuild the CLI/companion before generated-artifact probes. These allowances cover
structural depth and potential matching/inherited-map discovery, rather than a
complete derived-output, accessibility, CPU or RSS budget.

## Computed CSS-variable scopes

Use a fresh native oracle to compare computed inheritance, alias freezing, invalid
local masks, active cycles, selected/empty fallbacks, token boundaries and typed
post-substitution validation. The native resolver tests also cover dependency depth
5000 on a 2 MiB stack and exact UTF-16 expansion limits. Browser scope tests cover
the same dependency depth and UTF-16 boundaries; empty fan-out runs in a bounded
Node subprocess so a regression cannot hang the entire suite.

The empty-reference fan-out regression runs a legal stylesheet with 2048 forward
dependencies in a bounded subprocess. Snapshot, style-reading focus, layout,
paint and stylesheet replacement must complete while the result remains valid
empty rather than choosing its fallback. The subprocess watchdog stops a
regression from hanging the whole suite; it does not raise a runtime allowance.
Small resolver checks separately retain token boundaries, inherited keywords,
quoted empty strings and the distinction between empty and missing values.

```sh
cargo test -p varta-view --lib variables:: --locked
cargo test -p varta-view --test variables --locked
cargo test -p varta-runtime --lib compiled_variable_scopes --locked
cargo test -p varta-common --lib view:: --locked
cargo run -p varta-view --example browser_contract_probe --locked -- target/browser-view-oracle.json
VARTA_VIEW_ORACLE=target/browser-view-oracle.json node --test crates/varta-project/src/browser/*.test.mjs
node scripts/test-browser-variables.mjs target/browser-view-oracle.json /path/to/playwright/index.mjs chrome
node scripts/test-browser-variables.mjs target/browser-view-oracle.json /path/to/playwright/index.mjs webkit
```

After rebuilding the CLI/companion, verify the production generated loader and
Worker separately from the direct DOM fixture harness:

```sh
node scripts/test-browser-variable-bundle.mjs target/debug/varta /path/to/playwright/index.mjs chrome
node scripts/test-browser-variable-bundle.mjs target/debug/varta /path/to/playwright/index.mjs webkit
```

This probe compiles a real v38 application, removes its source/configuration and
serves only the generated browser directory under its original CSP. A test-owned
inspection handle retains the closed shadow root without changing its mode.
Initial computed styles and two trusted mouse clicks cover frozen inherited
aliases, replacement/removal, empty component splices, selected numeric fallbacks,
ordinary cycles, outside-cycle consumers, unused self-fallbacks and private font names. The second turn
checks that removing the stylesheet discards the old alias binding. Network
requests outside the owned origin and page errors fail the probe. This positive
fixture does not replace the separate adversarial cycle comparison below.

The compiled fixture also includes 32 levels of doubled empty references. Its
margin selects the valid empty result, rather than the nonempty fallback; native
compiled-Wasm coverage and the independent native/browser oracle exercise the
same fan-out behavior. This is distinct from expansion of nonempty output.

The browser probe compares the browser host's computed scopes with independently
produced native literal declarations. Separate platform-only controls retain the
browser-native cycle results without the host's normalization. Chrome and WebKit
must agree with the native fixtures, including the formerly divergent outside-cycle
consumer and oversized UTF-16 custom values. The live part checks pseudo/media
changes, local and committed check state, focus effects, reparenting and detached
components, rollback/removal, ambient-variable masking and quoted packaged font
names. Media-listener counts must remain stable on replacement/rejection and fall
to zero on removal/close. A selected-engine pass is not a complete browser matrix.
Class rules must also leave the implicit root's style and custom bindings
unchanged when guest classes are assigned to it.

Repeat selected-engine mockup and live-chat acceptance after rebuilding the
CLI/companion. When renderer imports change, update only the named public module
allowlist in the mockup server and retain its private-path/method checks. Reuse the
CSS/depth/cascade generated failure probes to retain fatal Worker behavior and
prior-DOM rollback. Shared per-value custom invalidity does not establish aggregate
output/work bounds, complete late-value subset/default/inheritance parity or full
platform acceptance.

### Potential expanded-component admission

The native `expansion_work` integration suite and browser `view-expansion` suite
exercise exact UTF-16 inventory boundaries, repeated style-reading commands,
operation attribution, transient repair, alias reuse, inactive candidates,
detached components and standard literals exceeding the custom-value cap. Common
estimator unit tests cover iterative dependency components and scope restoration.
The Node suite also compares bounds with actual resolution across 1458 candidate
worlds. The shared native oracle includes four expansion cases (17 batches).

```sh
cargo test -p varta-common --lib expansion --locked
cargo test -p varta-view --test expansion_work --locked
node --test crates/varta-project/src/browser/view-expansion.test.mjs
node scripts/test-browser-style-work-bundle.mjs target/debug/varta --expanded-values
```

The compiled source-free failure probe checks prior DOM/title retention and fatal
Worker rejection before guest catch/logging. These checks bound potential
component text, not serialized output, retained history, backend allocation or
process RSS; selected-engine observations do not establish full platform coverage.
