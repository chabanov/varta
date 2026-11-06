# Scoped development verification

Develop coherent batches of related behavior. Run focused checks while implementing
and fixing them, cover their callers when a feature is ready, then run the full
gate once on the completed code batch before committing. A full pass does not need
to be repeated after a documentation-only edit. New code/build inputs invalidate
any previous check whose scope includes those inputs.

Python 3, the pinned Rust toolchain and Node.js 24 (only its built-in test
runner; no npm packages) are required. The helper resolves the workspace from
its own location; these examples start at the workspace root:

```sh
python3 scripts/verify.py local varta-bigint
python3 scripts/verify.py local varta-runtime --filter array_inference
python3 scripts/verify.py feature array_inference
python3 scripts/verify.py full
python3 scripts/verify.py --plan full
python3 scripts/verify.py browser --playwright /path/to/playwright/index.mjs
```

`--plan` prints commands without running or verifying anything. Local checks run
Cargo check for all targets of one crate, then that crate's tests, optionally
filtered. Project, CLI (package `varta`) and launcher checks build the companion after
the Cargo check and before their tests. Unfiltered `varta-common` and
`varta-project` checks then run the browser host's Node module tests (the same
`node --test` step as the full gate); a Cargo filter does not select Node tests,
so a filtered check omits them. Feature checks
build the companion, then select matching runtime unit tests, public engine API
tests and CLI smoke tests. Each selected suite, Cargo or Node, must execute
passing tests; a typo that matches zero tests fails. Where names differ, run explicit Cargo commands
with the appropriate filters instead of widening a filter until it happens to pass.

## View integration cases

`varta-view` now declares one `integration` target with the original case files as
modules. Select a family by its module prefix:

```sh
cargo test -p varta-view --test integration --locked clipboard::
cargo test -p varta-view --test integration --locked renderer::
cargo test -p varta-view --test integration --locked every_case_file_is_registered
```

Full workspace tests still run every original case. The registration guard fails
when a Rust case file is missing from the module list. Earlier dated checkpoint
commands naming separate `--test` targets describe the original layout; use the
module-prefix form for the current tree. Grouping preserves all 233 original
integration cases from the 3323-test clipboard gate. The standalone preparation
passes all 233, and its inventory exactly matches those original cases
(`target/view-test-grouping/inventory-comparison.json`); the applied Cargo target
passes 234 cases before painting changes (`target/view-test-grouping/applied-tests.log`).
The subsequent corner integration passes 50 library cases and 238 integration
cases (`target/paint-next-stage/applied-view-tests-fixed.log`). Ordinary module
declarations keep recursive formatting coverage; the registration guard checks
those declarations against all Rust case files. No timing ratio is inferred from
overlapping runs. This is scoped verification of the open painting batch, not a
new full gate.

The following border/outline checkpoint passes 52 library + 242 integration cases
(`target/paint-next-stage/border-outline-view-tests-coverage.log`). After bounded
large-radius arc refinement, final affected cases pass:

```sh
cargo test -p varta-view --lib --locked paint::corners::tests
cargo test -p varta-view --test integration --locked rounded::
```

Nine unit and eight integration cases pass in
`target/paint-next-stage/adaptive-corner-unit.log` and
`target/paint-next-stage/adaptive-rounded-integration.log`. These overlap the
preceding cases; a full run on these newer inputs is still pending at the open
painting-batch boundary. No actual GPU/native-window result is inferred.

The rounded clip-chain step then passes 57 library + 246 integration cases
(`target/paint-next-stage/clip-pipeline-view-tests-final.log`). Coverage includes
SDK intersection parity, mask reuse/reset, maximum depth, empty clips, interior
mask elision, post-overflow ordinary raster, nested hits, sibling/popup scope and
style transitions. The failed earlier style-transition assertion is retained in
`target/paint-next-stage/clip-pipeline-view-tests.log`; corrected expectations do
not change auto/hidden scrolling behavior.

The new Debug GPU fixture probe passes 28 actual Metal frames, including visible
mutations and partial damage. Run it with the command in the desktop README.
The original mean RGB threshold2/255 and GPU resource limits stay unchanged;
max per-channel differences remain reported rather than hidden. Exact code,
binary/image hashes, samples and scope are in
`target/paint-next-stage/clip-pipeline-checkpoint.json`. Earlier 28-frame output
had an invisible nested mutation, so the fresh probe adds visibility/damage
assertions and retains its own images/log. This is not a full gate or native
window/VoiceOver/field-text/Release/cross-target acceptance.


The text/control and stack-safety checkpoint then passes **64 library + 249
integration** View cases and **78 desktop GPU-enabled unit** cases:

```sh
cargo test -p varta-view --lib --test integration --locked
cargo test -p varta-desktop --bin varta-preview --features gpu --locked
cargo run -p varta-desktop --features gpu --example gpu_clip_probe --locked -- target/paint-next-stage/gpu-controls-layout
```

The output prefix's directory must exist. Logs are
`target/paint-next-stage/text-control-layout-view-tests.log`,
`target/paint-next-stage/text-control-desktop-gpu-tests.log` and
`target/paint-next-stage/gpu-controls-layout.log`. The actual Metal probe passes
52 frames, now including six native-control scenes at both scales and visible
mutations. `text-control-checkpoint.json` in that directory records exact source,
retained binary/image hashes, samples and failure scope. Earlier 28/52 probes
remain distinct observations, not cumulative independent frame counts.

Coverage includes analytic curved text/caret visibility, full source advances,
revision/IME guards, shared preflight/geometry work refusal and action recovery,
all explicit controls, repeated-clip idempotence and iterative native layout.
The deep stress fixture first exceeded the unchanged native depth bound, then
revealed a real recursive style-frame stack abort at an admitted depth. The
explicit enter/exit construction fixes it; a normal2MiB subprocess verifies
native depth128 success and depth129 refusal. Two pre-existing grouped subprocess
selectors are qualified and now require one executed case; earlier outer test
counts did not prove those child bodies ran. The final affected run executes all
three subprocess tests successfully. Fixture failures are retained in the
checkpoint, including missing explicit button alignment and a field-only setter
on a non-field. No guest budget, stack size, GPU threshold or resource cap was
raised. Full gate, Clippy consolidation and final application/native acceptance
remain at the still-open painting family boundary.


The diagonal contour foundation subsequently passes **69 View library + 249
integration + 78 desktop GPU-enabled** tests with:

```sh
cargo test -p varta-view -p varta-desktop --features varta-desktop/gpu --locked
```

`target/paint-next-stage/diagonal-clip-affected-tests.log` records that affected
run. A final validation-only refinement and stronger two-crossing assertions
pass five overlapping helper cases in `diagonal-clip-final-tests.log`:

```sh
cargo test -p varta-view --lib --locked paint::clip_curve::
```

The final actual Metal probe passes60 frames (15 common-color box/control
scenes at1×/2× and cold/change), adding asymmetric oversized inner arcs and
fractional geometry. `gpu-diagonal-final.log` and `diagonal-clip-checkpoint.json`
in the same directory retain source/binary/image fingerprints, all samples and
unchanged limits/threshold. Mean RGB error is0–0.1472/255; maximum single-channel
error77 stays reported. Earlier52/60 runs are separate evidence, not cumulative
new cases. Diagonal cutting itself has software geometry coverage; the live GPU
fixtures still exercise common-color padding/border/control paths. Mixed-color
corner painting, correct isolated seam/subtree opacity, inline rounding and the
final family/full gate remain open. The independent nearest-side counterexample
is design evidence against a rejected algorithm, not a current passing mixed
border raster case.

The isolated box-opacity implementation now has a final affected selection:

```sh
cargo test -p varta-view -p varta-desktop --features varta-desktop/gpu --locked
cargo clippy -p varta-view -p varta-desktop --all-targets --features varta-desktop/gpu --locked -- -D warnings
cargo run -p varta-desktop --features gpu --example gpu_clip_probe --locked -- target/paint-next-stage/group-final-debug
cargo run --release -p varta-desktop --features gpu --example gpu_clip_probe --locked -- target/paint-next-stage/group-final-release
```

The output directory must exist. Logs `group-final-affected-tests.log`,
`group-affected-clippy-final.log`, `group-final-debug.log` and
`group-final-release.log` live under `target/paint-next-stage/`;
`group-opacity-checkpoint.json` records their exact inputs, binaries, images and
scope. The final selection passes74 View library +260 integration +78 desktop
GPU-enabled cases. The affected cases cover overlap/root/nested/zero alpha, rounded/fractional
clip application, selection/IME geometry, retained damage, vector/raster switches,
both cache-overflow forms, late capture after a streamed prefix, and maximum native
opacity depth on a normal2MiB stack. A source-ownership unit case retains temporary
control-scope pointer keys until their memo entries drop. The independent full-layer
comparison keeps its maximum2-channel tolerance after a real tile-edge AA failure;
the tolerance was not relaxed. Two old opacity-refusal tests were updated for the
new painter, retaining positioned-paint refusal/recovery coverage. Initial failure
and intermediate logs remain alongside the final results.

Each final Debug/Release probe contains **60 actual Metal observations +8 explicit
CPU-fallback observations**. Those are separate profiles, not136 GPU frames.
The unchanged mean RGB threshold is2/255; observed maximum mean error0.1472/255
and maximum channel difference77 are reported. CPU-fallback scenes compare against
both retained and fresh stateless pixels, then require zero dirty pixels for an
unchanged preparation. Small local changes retain partial damage. Final single-sample
group preparation spans0.701–7.105ms Debug and0.259–2.044ms Release; unchanged
preparation spans0.098–0.204ms and0.015–0.050ms respectively. These host-fixture
times omit guest execution, events and physical presentation; they establish no
whole-application latency or before/after speed ratio. Mixed border colors, own
inline opacity/rounding, direct GPU groups, actual windows/VoiceOver/IME and the
target matrix remain open. The last full3323 gate still verifies only the older
clipboard inputs; no full family gate/commit is claimed here.

The following mixed-side increment adds independent inverse-inset, zero-width,
rotation and uniform-block ownership checks, plus actual colored-ring integration
cases for transparent holes, common-contour alpha at1×/1.25×/2×, distinct side
alpha at a diagonal seam, retained mutations and vector recovery. The initial
far-edge failure is retained in `border-alpha-diagnostic.log`; a unit regression
also checks shared mask/color coverage of that last partial row. The source fix
encloses four edges separately instead of trusting origin-floor/size-ceil.

```sh
cargo test -p varta-view --lib --test integration --locked border_
cargo test -p varta-view --lib --locked fractional_ring_coverage
```

The broader affected selection first found one stale distinct-color refusal test
(`mixed-border-affected-final.log`); its desktop78 and View80 library cases passed,
but its integration selection was not a passing checkpoint. After updating that
assertion to real side-color/hit checks, the final complete View selection passes
80 library +265 integration cases (`native-painting-precommit-view.log`). Final
all-target affected GPU Clippy is in `native-painting-precommit-clippy.log`. The expanded probe uses the same command with
new output prefixes:20 scenes produce60 actual GPU observations and20 explicit
CPU-fallback observations per profile, not80 GPU frames. Host preparation time for
complex mixed corners remains a performance gap, independently of pixel correctness.
Do not infer application/window, platform or full-plan completion from these checks.

The final pre-commit full gate passes3393 cases,0 failures,16 ignored and all eight
commands with unchanged source inputs. Authoritative report:
`target/verification/20261104-083445-nlyqugwi/report.json`; source SHA-256
`4bfd91cb3b2fe8c9b96ed662c3bf443fb38476af01c9b4f0ac465a6f21edfe92`.
`target/paint-next-stage/native-painting-precommit-checkpoint.json` records the final
Debug/Release probes and their source/binary/image hashes. The full run supersedes
the older clipboard-only gate for this code. Interrupted pre-commit attempts remain
in their logs; only this final eight-command report is full-pass evidence. Subsequent
documentation annotations do not change engine/build inputs.

The subsequent CPU performance follow-up has its own scoped evidence under
`target/paint-next-stage/border-speed-profiled/`. Its `checks.json` records frozen
View/Desktop/build inputs, 83 library +265 integration cases, strict View
all-target Clippy and Debug/Release GPU example builds. The completed Debug build
is reused only with unchanged inputs and its retained binary; it is not rebuilt
after Clippy just to repeat the same source verification. The final checks do not
relabel the earlier intermediate View/Desktop GPU Clippy as a new full pass.

`comparison.json` retains three alternating before/current runs per profile,
per-scene samples and medians, binary hashes and all three rounds of exact image
comparisons. Every round compares 80 RGBA images byte-for-byte (480 comparisons
across Debug/Release); each probe independently checks stateless/retained pixels,
input geometry, unchanged-frame reuse and local damage. Each run has 60 actual
GPU observations +20 explicit CPU fallbacks, with the existing GPU error bound.
Cold 2× opposing-corner preparation moves from 66.357 to 12.866 ms Debug and
7.517 to 2.798 ms Release (medians). All sampled mixed scenes fit 16.7 ms Debug on
this host; this does not establish window/input/guest/platform latency. The
repeated CPU profiling fixture and sampled stacks in `border-speed-rows/`
identified the owning array-iterator hotspot; its sampling run is not a timing
benchmark.

New geometry tests exercise both sides of near-tied arrivals, whole-block proof
requirements and independent translated-ellipse inset membership. Do not weaken
the existing alpha/pixel thresholds, alter build profiles, remove assertions or
raise cache/work limits to improve performance results. Earlier experiments remain
in their directories; `border-speed-final/` contains a circular-root proposal that
was measured and removed. The current follow-up has no new full-workspace pass;
the 3393-test report above verifies the preceding committed code. A full gate is
still required at the completed delivery/commit boundary.

The native inline background-radius follow-up has frozen-source scoped evidence
under `target/paint-next-stage/inline-next/final/`: 85 library +272 integration
cases, strict View/Desktop all-target GPU Clippy and both optional GPU probes.
The input map includes the new untracked source/test files as well as tracked
inputs. Its21-scene probe has64 actual GPU +20 CPU-fallback observations per build;
all80 legacy RGBA images per profile match the preceding checkpoint exactly.
The new inline case exercises actual GPU paths, not raster-fallback inference.

`native-probe.json` and `chrome.log` record21 browser cases/4344 checks. Eight new
scenes compare every pixel in the decoration ROI with the same2/255 RGB mean
bound; the original exact ink checks remain. A one-pixel boundary margin extends
the existing rectangle-edge convention to rounded hit regions. Deliberately
corrupted images, malformed raster lengths and a wrong legacy ink sample all
produce the expected nonzero failures (`negative-checks.json`). The actual initial
percentage mismatch remains in the earlier `chrome.log` outside `final`; it was
fixed by fragment-relative normalization, not by relaxing the bound.

Independent Chrome/WebKit diagnostics in `fragment-basis-standards.json` confirm
the percentage basis and record their own-font-box pointer difference. Firefox
launch/profile failures are retained separately and are not passing acceptance.
`opacity-next-diagnostic.json` and `opacity-native-gap.json` pin the next native
stacking-order discrepancy. These diagnostics do not close own inline opacity,
other native styles, application windows or platform/pilot/release requirements.
The prior3393-test full gate remains historical; consolidate the next full gate
at the completed delivery/commit boundary.

The subsequent static-box opacity context-placement correction has its own
frozen-source record at `target/paint-next-stage/opacity-context/checks.json`:
formatting, **85 library +275 integration cases**, strict View all-target Clippy
and the11-scene `native_opacity_probe`/Chrome oracle all pass. The latter reuses
`scripts/test-native-inline.mjs`: **1971 checks**, ROI means0–0.4325/255 and
maximum channel delta16, within its unchanged2/255 mean bound. Scenarios cover
ordinary/zero/nested/peer contexts, lifting through block/flex/grid containers,
curved ancestor clips, overlapping pointer targets and retained vector recovery.
The existing cache-overflow regression now compares every pixel-position target
and all node bounds instead of requiring identical storage order across a new
context. The overlap regression explicitly requires changed priority when opacity
returns to1. These are engine-local/scoped browser checks; the prior actual-GPU
and full-workspace checkpoints retain their earlier input maps. Own inline
opacity and broader CSS phase/platform acceptance remain open.

The next inline-opacity source capture has its current input map and results at
`target/paint-next-stage/inline-own-opacity/checks.json`. Native verification now
includes per-line source ordering, cross-span ligatures under opacity changes,
an independent curve mask applied to already composed ink, shared contour denial,
zero-alpha targets and a65,536-glyph capture crossing the retained command limit
without increasing it. The stress fixture uses several admitted text operands;
its initial oversized wire string refusal is retained in `inline-targeted.log`.

At the earlier inline-opacity checkpoint, the33-scene Chrome command was **not a passing acceptance gate**:
fractional curved backgrounds/ink still exceed the existing2/255 mean bound.
`chrome-strict.log` retains the actual failure. A separate diagnostic copy runs
every scene with the same assertions, records each failure in `each-results.json`
and also exits nonzero; it does not turn those failures into passes. Earlier
21-scene acceptance remains evidence only for its earlier fixtures/inputs.
The independent light-background snapping diagnostic and Blink's primary source
pin the physical-paint policy gap. Keep these failures and original thresholds
while aligning snapping; a successful native suite is not full browser parity.
The final frozen-source record passes85 library +283 integration cases and strict
View all-target Clippy. The diagnostic records6765 successful assertions across
all33 cases, with two additional failed pixel-mean assertions;31 cases complete
successfully. The unchanged static-box oracle passes1971 checks again. No
current-source actual GPU run or full-workspace acceptance is inferred from these
results. Earlier full/GPU checkpoints keep their own source fingerprints.

The subsequent paint-rounding input map and evidence live under
`target/paint-next-stage/pixel-snapping/`. The original33-scene1× command now
passes; the20-scene decoration selection passes at2×. The1.25× strict command
and its all-case diagnostic still exit1 at the ink/curve scene, retaining the
same2/255 bound. ROI coverage includes both logical ink bounds and snapped
paint bounds so a partial device pixel exposed by rounding cannot be omitted.
The original exact1× glyph assertions remain. Independent before-DPI browser
snapping and subpixel-glyph diagnostics explain which coordinates may round;
input/text visibility tests specifically retain logical geometry when a painted
clip has no pixel. The normal block-phase overlap was a separate pixel/target
failure at this historical checkpoint; the following increment fixes it. Do not infer full painting or
platform acceptance from either the native or1× browser suite.
Current scoped checks pass86 View library +287 integration +78 Desktop tests
with GPU enabled and strict affected all-target Clippy. The22-scene GPU probe
passes in Debug and Release:64 actual GPU +24 CPU-fallback observations per build.
The new inline-opacity fixture's fresh stateless reference requires its explicit
font; the initial omission/failure remains in `gpu-debug-initial.log`. Only that
probe file changed before `followup-checks.json`; completed engine/browser tests
are reused against unchanged relevant hashes, then Clippy and both probes rerun.
Four bad-image/bad-length/legacy-ink/invalid-scale inputs fail as expected. The
1×/2× browser suites pass6767/4043 checks; the125% diagnostic records4042 successful
assertions and one retained failure (mean2.248197,max98). No threshold or existing
assertion is weakened, and no new full-workspace gate is claimed.

The subsequent paint-phase checkpoint lives under
`target/paint-next-stage/paint-phases/`. Source-pinned checks pass86 View library,
291 integration and78 Desktop GPU-feature tests plus strict affected all-target
Clippy. The first broad run preserved its metadata-order failure; the corrected
regression checks unchanged bounds and every viewport hit target across context
creation, and all291 integration cases then pass. Phase tests also cover clipped
ancestors, zero-opacity hits, retained/vector mutation and overlapping decorated
lines without isolation. Native metadata equality may conservatively invalidate
reuse when the real paint context changes. Current Debug and Release Metal
probes each pass64 actual GPU +24 explicit CPU-fallback observations over22 scenes;
maximum mean RGB delta0.1472. Opacity composition is still CPU fallback.

The browser corpus now has59 cases. All24 block/flex/grid cases pass at all three
scales. Strict browser commands and all-case diagnostics remain nonzero because
of explicit text raster gaps:58/59 complete at1× and2×,57/59 at1.25×. Diagnostics
retain10104/10104/10103 successful assertions, respectively. The new dense
undecorated overlapping-text case fails means5.388889/5.369444/3.940351; the
previous125% curved-text case remains mean2.248197. Pixel bounds stay2/255.
Axis-only browser measurements are saved in `glyph-axis.json`; a horizontal
fractional change is not evidence that the vertical origin must also stay
fractional. No full-workspace/window/platform pass follows from these results.

The next vertical-paragraph-origin increment has evidence under
`target/paint-next-stage/text-origin/`. It rounds the containing vertical paint
origin before DPI, retaining horizontal subpixels and logical text geometry.
The new axis-specific native regression covers plain/rich, unchanged raw
snapshots, retained/vector parity and fractional logical bounds at all3 scales.
Current scoped checks pass86 library +292 integration +78 Desktop GPU-feature
tests (456 total) and strict affected all-target Clippy. Current Debug/Release
Metal probes each pass64 actual GPU +24 explicit CPU fallbacks (22 scenes),
maximum mean0.1472; opacity remains CPU fallback.
The125% opacity/curve scene improves from2.248197 to1.925481 and passes the same
2/255 threshold. All-case diagnostics now pass58/59 at each scale, with10104
successful assertions each. Dense overlapping text remains a failing raster
case; strict commands/diagnostics still exit1. Keep that failure visible and do
not infer full font, native window or platform acceptance.

The native-glyph increment uses `target/paint-next-stage/glyph-raster/`. Its
shared source probe is used by both the portable engine reference and the desktop
native adapter. Run `cargo run -p varta-desktop --example native_text_probe
--locked -- 1.25` to emit native data, then pass it to the unchanged
`scripts/test-native-inline.mjs`. Native data identifies its rasterizer and per-case
coverage/outline preparation counts. The strict59-case native corpus passes10105
assertions at1×/1.25×/2× with the original2/255 bound; the portable command retains
the explicit smoothing mismatch. Standard/antialiased browser and raw CoreText
controls explain the difference without changing the target oracle. Pixel masks
must be compared through retained/vector paths and clipped actual GPU observations,
not just a direct CoreText bitmap. GPU corpus cases `system-text`/`system-field`
require native coverage and no silent outline substitution on macOS. Other targets
do not count those macOS-only cases. Initial compile/lint failures remain separate
from the source-pinned final checks; an equivalent byte-chunk iteration lint fix
is followed by fresh native/browser/GPU checks, while unchanged engine tests are
reused by their exact input hashes.

Final native-glyph/accumulated-painting delivery gate:
`target/verification/20261104-131013-v4eh1yvu/report.json` passes all8 commands,
3434 tests/0 failures/16 ignored, with identical source SHA-256
`cf959f7810f40b54dbd95b106e635976078894c5423eaab4da30c97c03cdd3f8`
before/after. The conformance baseline has123 passed/4 known unsupported of127,
no regressions or infrastructure failures. Scoped evidence has90 library +296
integration +78 Desktop tests, strict affected GPU Clippy,59 native Chrome scenes
at3DPI (10105 assertions each), and24-scene Debug/Release Metal probes with72
actual GPU +24 CPU fallbacks each (max mean0.1472). Only result annotations follow.
The standalone portable reference keeps its smoothing difference. No actual
application-window/system-input, other-target or full-plan acceptance is inferred.

The solid-underline increment adds View `portable_decoration_probe` (portable
outlines) and Desktop `native_decoration_probe` (macOS native coverage). Their
output names are distinct in whole-workspace builds. It uses an original,
reproducible fixture, disjoint/curved descenders and Han/Hebrew aliases. Run:

```sh
cargo test -p varta-view --locked --test integration underline
cargo run -p varta-desktop --example native_decoration_probe --locked -- 1.25 > target/native-decoration-125.json
node scripts/test-native-inline.mjs target/native-decoration-125.json
```

Repeat the oracle at1×/1.25×/2×. The26-scene corpus passes222 checks per scale
with the same2/255 mean bound; its ink ROI encloses the occupied text/decorations,
not an enlarged blank viewport. Keep the initial geometry/16px-baseline failures
and diagnostic-only runs separate from passing acceptance runs. Test inherited
`none`/color/size, nested decorations, descender/curve gaps, CJK exclusions, bidi,
wrap, source-overhang/overlapping-line order, opacity, clips and retained/vector
output. Field tests separately verify own decoration and replaced-boundary stops
without changing editing geometry. GPU corpus also includes a clipped underlined
paragraph and a local decoration mutation; this requires actual vector GPU output,
without adding texture/layer capacity. The completed underline gate is
`target/verification/20261104-142517-oo4etf6x/report.json`:8 commands pass,
3448 tests/0 failures/16 ignored; before/after source SHA-256 is
`fbd84faf854f41f0802aec9f2dc6525c82be4fe87fc6ad843e54fb96b10af02d`.
The baseline remains123 passed/4 known unsupported of127, with no regressions or
infrastructure failures. Current View coverage is93 library +307 integration
cases, plus78 Desktop GPU-feature tests. Fresh post-gate native probes pass both
26/222 and59/10105 corpora at all3DPI with native coverage/no outline substitution.
Debug and Release GPU each pass25 scenes/100 observations:76 actual GPU,24
explicit CPU fallbacks, max mean0.1472. The final Debug artifact is rebuilt after
the last finite-coordinate guard; Release already follows that code edit.
The intermediate gate stopped before workspace build/tests to separate the
portable/native example output names; it is not counted as a pass. The metadata
check proves17 unique workspace example outputs. Only result annotations follow
the final frozen-source gate; further outline implementation is kept separately.

## Change-to-check map

| Change | During implementation | At the feature/batch boundary |
| --- | --- | --- |
| Isolated numeric algorithm | Local numeric crate and edge/error/resource cases | Relevant runtime conversion/operator tests once integrated; full before code commit |
| Parsing, lowering, inference or emission | Compiler check/tests plus focused runtime execution tests | Public API/module diagnostics and CLI/source-free artifact cases; full |
| Browser fuel transform | `cargo test -p varta-compiler --lib browser_meter` compares private images with locked native Wasmtime: results, remaining fuel, memory and selected fair host charges | Compiler suite/Clippy; after host integration, all shared charge families and Node/Chromium/Firefox/WebKit observations, permission denials, Worker termination and measured overhead; full before code commit |
| Compiled JSON decoders or their `simd128` scanner | `cargo test -p varta-compiler --lib decoders`, `cargo test -p varta-runtime --lib json_simd_tests` (vector, scalar and linked decoders on specials at every lane, whitespace runs, truncations and the corpora) and `json_decoder_tests` | `fuel_determinism_tests`, `collector_stress_tests`, `browser_meter`, `validate::simd`; the ignored `json_scan_fuel_per_string_byte` for the measurement; full |
| Runtime semantics | Focused runtime tests with positive, error and resource cases | Affected public API and artifact paths; full |
| ABI, value layout, GC, roots or exception channels | Common/ABI validation and affected runtime suites | Historical-profile rejection, canonical payloads, collections/roots, host-free helper checks, public API/packaged execution; full |
| Permissions or host boundaries | Allowed/denied calls, exact grants and host ceilings | Malformed inputs, fatal limits, status and finalizer behavior through public API/CLI; full |
| Artifact, launcher or packaging | Artifact/launcher/project tests and focused CLI paths | Full, plus separate `cargo test -p varta --test smoke --locked` after the companion build |
| Linked intrinsics (`varta-intrinsics`, `varta-intrinsics-wasm`) | `cargo test -p varta-intrinsics`, then `python3 scripts/build-intrinsics.py` (offline, locked; commit `intrinsics.wasm` and `intrinsics.json` together) and the compiler `link::` tests | Runtime `linked_intrinsic` corpus tests and the host suites of every family whose code moved; the v38 CLI smoke test; full |
| KV value layer (`varta-kv-codec`, `varta-kv-codec-wasm`) | `python3 scripts/verify.py local varta-kv-codec` (its tests and `scripts/build-kv-codec.py --check`); after a source change `python3 scripts/build-kv-codec.py` (offline, locked; commit `kv-codec.wasm` and `kv-codec.json` together) | Runtime `kv` and `json` tests, project `browser::` and Node KV tests, `node scripts/test-browser-kv.mjs <varta>` in Chrome (WebKit through Playwright); full (no_std check and fingerprint) |
| Dependency/build changes | Relevant isolated builds and engine dependency-boundary test | Full; verify lockfile and runtime distribution boundaries |
| Conformance inventory/runner | Local `varta-conformance` tests and reviewed case changes | Baseline JSON, strict JSON with expected exit status and explicit gap review; full for code/inventory changes |
| Documentation only | Diff, links, documented commands and consistency | No repeated engine suite if engine/build inputs are unchanged |
| Browser host modules, registry output, bundle generation or browser drivers | The affected `node --test` files and the named driver | `python3 scripts/verify.py browser` and the Node suites (unfiltered `local varta-common`/`varta-project`, or full); full for changed Rust inputs |
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
node --test crates/varta-common/browser/fuel.test.mjs crates/varta-project/src/browser/*.test.mjs
cargo run -p varta-conformance --locked -- --json
```

The helper expands the Node test pattern itself into sorted explicit paths
(`--plan` prints them) and fails if a pattern matches nothing; CI uses the
shell's expansion of the same pattern after installing Node.js 24.
Workspace tests already include `varta`'s `smoke` integration suite. A separate
repeat is reserved for launcher/packaging changes or a named flakiness investigation.
CI retains complete Linux/macOS coverage. A local full pass proves only that host.
Known unsupported conformance cases remain gaps, even when the baseline matches.
The full gate starts no browser. The browser oracle gate below is a separate
required gate for browser host, registry, bundle-generation and browser-driver
changes; CI runs it as its own job.

## Browser oracle gate

```sh
python3 scripts/verify.py browser --playwright /path/to/playwright/index.mjs
python3 scripts/verify.py --plan browser
```

The Playwright module may instead come from `VARTA_PLAYWRIGHT_MODULE`. Chrome is
chosen as `scripts/browser-probe.mjs` chooses it: `VARTA_CHROME_BIN`, otherwise
the installed Google Chrome on macOS and `chromium` elsewhere. The scope runs, in
order and stopping on the first failure:

1. Registry drift: `generate_browser_registry` writes into the run directory and
   `git diff --no-index --exit-code` compares it with the checked-in
   `crates/varta-common/browser/registry.mjs`.
2. `browser_contract_probe` writes the native view oracle. The one Node suite
   that reads it, `view-model.test.mjs`, runs first without and then with
   `VARTA_VIEW_ORACLE` set; the oracle run must pass more tests than the run
   without it, so a suite that stopped reading the oracle fails the scope. The
   other Node suites belong to the full gate (and to unfiltered `local
   varta-common`/`varta-project` runs), not to this scope.
3. Builds `varta` and the companion, then generates the `browser_bundle_probe`
   fixtures.
4. Real Chrome over the DevTools protocol: the view DOM oracle, the bundle,
   network and CORS acceptance (`test-browser-bundle.mjs --generate-network`
   generates its own network fixtures), the eight generated-session drivers over
   the fixtures, the dev, effects, KV, environment and streaming drivers with
   `target/debug/varta`, and all five style-work modes.
5. Playwright in Firefox, WebKit and the installed Chrome channel: the execution
   matrix with `--effects` and `--kv`, the integrity and dev matrices, the
   direct-DOM variables driver once per engine and the compiled variable bundle.

Each run writes the regenerated registry, the oracle, both fixture directories
and isolated Firefox `MOZ_APP_DATA`/`MOZ_LOCAL_APP_DATA` directories into its
own `target/verification/<run>/`, next to the logs and `report.json`. The report
also records the DevTools drivers' Chrome executable (`devtoolsChrome`) and the
version of the Playwright package that owns the supplied module. The Playwright
drivers do not use `VARTA_CHROME_BIN`: they launch Playwright's own `chrome`
channel lookup, so with that variable set the two Chromes can differ. Each
Playwright driver's log prints the browser version it launched.

When Chrome is not found, the Chrome and Playwright commands are not run; without
a Playwright module, the Playwright commands are not run. Each such command is
printed and recorded as skipped with its reason, and a run without failures then
ends `INCOMPLETE` with exit status 3, never `PASS`. An engine that Playwright
cannot launch fails its driver and so the scope. Known gaps are engine
selections named in the helper's `KNOWN_GAPS` table: they are never run or
counted as passed, and the final line states how many were left out. An entry
needs a reproducible failure; a selection that fails only on some runs is not
waived, so a failing run fails the scope. The table is currently empty, and all
three engine selections of `test-browser-variables.mjs` run.

Earlier Firefox `too much recursion` failures of that selection came from a
regex in `rewriteRootSelector` that was quadratic in the longest run without
braces or quotes; it is now a linear scan
([history](history.md#browser-oracle-gate-wave-3--2026-11-05)). Its worst
cases at the stylesheet byte limit are checked by `view-stylesheet.test.mjs`
(one of the Node suites above that this scope does not run), which counts the
code units the rewrite reads against a linear bound instead of timing it, and
stops a rewrite at its first uncounted access or read past that bound. The DevTools drivers'
shared `scripts/browser-probe.mjs` waits for Chrome to be reaped and retries the
profile removal. This mitigates one observed `ENOTEMPTY` teardown failure whose
race was never reproduced, so it is not a verified fix.

The gate does not cover `test-browser-select-bundle.mjs` (its `examples/select`
fixture is not in the repository), the native oracles (`test-native-*.mjs`,
`test-headless-geometry.mjs`, `test-select-work-runtime.mjs`), the fuel probe
page or the `examples/uarp-console` drivers.

The CI `browser` job runs this scope on Linux and macOS with Node.js 24, Playwright
1.64.1 and its Firefox/WebKit builds installed outside the repository, and the
runner's installed Google Chrome. It keeps `target/verification/` as a workflow
artifact. Local runs are logged in
[history](history.md#browser-oracle-gate-wave-3--2026-11-05).

## Optional desktop GPU backend

The default workspace gate does not enable the optional GPU dependency graph.
For a renderer/backend batch, also run:

```sh
cargo clippy -p varta-view -p varta-runtime -p varta-project -p varta-desktop --all-targets --features varta-desktop/gpu --locked -- -D warnings
cargo test -p varta-desktop --features gpu --locked
cargo test -p varta-view --test renderer --test renderer_inputs --locked
cargo build -p varta-desktop --features gpu --bin varta-preview --example gpu_probe --locked
```

On a declared GPU target, run the diagnostic `gpu_probe` against the same Wasm
and explicit font as the CPU reference, preserve its raw RGBA outputs, and record
adapter/driver/build/input hashes. Check Debug and Release windows, input geometry,
text/selection/popup clips, hover, scrolling, resize/visibility, color management
and device-loss recovery. Host timing ends at presentation return; offscreen
readback includes a GPU wait/copy. Neither proves physical display latency.
The probe's mean RGB tolerance is an antialiasing smoke check, not complete pixel
or cross-platform acceptance. Resource caps must be reported with their actual
scope; individual buffer caps are not a total GPU-memory quota.

The CI `gpu` job runs the Clippy command above and
`cargo build -p varta-desktop --features gpu --all-targets --locked` on Linux and
macOS. It runs no GPU tests or probes, so it proves only that the optional graph
compiles and lints on those hosts.

## Native accessibility

For a native accessibility increment, start with focused semantic/action and
queue tests, then cover the runtime turn boundary and dependency graph once the
package is ready:

```sh
cargo test -p varta-view --test accessibility --locked
cargo test -p varta-view --test accessibility_text --locked
cargo test -p varta-view --test accessibility_scroll --test accessibility_reveal --test text_geometry --locked
cargo test -p varta-runtime --lib assistive_ --locked
cargo test -p varta-desktop --locked
cargo test -p varta-desktop --features gpu --locked
cargo test -p varta-project --test boundaries --locked
```

Exercise the platform accessibility API against the packaged CPU Debug and GPU
Release hosts. Record actual backend, inputs, names/states, edits, option actions
and resize, and keep screen-reader/text-range acceptance separate. Put all
generated fixture artifacts under ignored `target` before the final full run;
an untracked generated manifest is still a source-fingerprint change. Do not
edit observation records while that run is active.

After shared name-summary or headless-projection changes, run the complete
`varta-view` suite once for the finished package and retain the exact-budget
`naming_work` cases. Native descriptions also need direct platform `Help`
observations and an over-limit/refusal/recovery scenario: the guest continues
ordinary input, stale assistive actions disappear, an explicit status is readable
and a valid later tree returns. Scoped results apply only to their changed-input
scope; preserve the full gate for the completed code batch before committing.

## Evidence and failure handling

Logical field text changes also cover Unicode byte/UTF-16 correspondence, CRLF,
empty trailing lines, long-run chunk boundaries, reversed selection, password
masks, readonly/disabled/composing controls, content revisions through edits and
Undo/type changes, foreign/stale native endpoints and selection-then-replace FIFO
across acknowledgement frames. Exercise actual platform range selection before
claiming native acceptance; logical metadata alone does not prove wrapping,
character geometry, VoiceOver or cross-target behavior.

For visual field geometry, compare source-byte coverage, bidi/soft lines,
physical scaling, clipping, grapheme password masks and CRLF content sizing with
the actual paint/input metadata. Cover endpoint affinity and collapsed selections,
range-ID reuse across reflow, already decoded selections, unknown old run IDs,
and the exact actual-visual-span ceiling plus recovery. The pure desktop checks
must not be relabeled as platform range or VoiceOver observations.

Live updates additionally cover implicit alert/status defaults, explicit inherited
priority/off settings, roleless/root busy state, atomic text across descendants,
multiple busy publications and final release, initially busy regions, cancellation
through empty content, empty/removal/replacement changes and rendered password
masks without duplicate run text. An outer atomic message must include nested
live/off content, while off changes alone remain silent; an unchanged assertive
child must not escalate a polite update. Cover complete-message work overflow
and recovery without changing guest admission. Effective-role text folding and
partial removals alone must stay silent (including during busy cancellation),
and reorders use only participating parts' priority. Same-content child
replacement in one atomic owner/presentation coalesces without replay.
Mixed semantic children must neither lose visible text nor repeat a derived name;
author labels, valid/empty relations and placeholder fallback remain distinct. The desktop live tests compile the exact safe planner used by the pinned
macOS adapter patch and process real publications through the consumer tree.
Check its release archive checksum, unchanged files, exact patch diff and supplied
licenses using `vendor/accesskit_macos/VARTA-PATCH.md`. Planner tests establish
message selection, not actual VoiceOver delivery. Observe announcements and
absence of repaint duplicates through the packaged host separately.

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

Native modal dialogs (top layer, backdrop, centring and the viewport reserve)
against local Chrome, at each compared scale:

```sh
for scale in 1 1.25 2; do
  cargo run -p varta-view --example native_dialog_probe --locked -- "$scale" > "target/native-dialog-$scale.json"
  node scripts/test-native-dialog.mjs "target/native-dialog-$scale.json"
done
```

Native CSS transitions are compared with Chrome on the same clock: Chrome keeps
every transition paused at the probe's time since its style change.

```sh
cargo run -p varta-view --example native_transition_probe --locked > target/native-transitions.json
node scripts/test-native-transitions.mjs target/native-transitions.json
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

Browser pilot effects (clock, entropy, environment snapshot, exit) and idle
collection between turns:

```sh
cargo test -p varta-common --lib browser::
cargo test -p varta-project --lib browser::
node --test crates/varta-common/browser/fuel.test.mjs crates/varta-project/src/browser/*.test.mjs
cargo build -p varta --bin varta --locked
node scripts/test-browser-effects.mjs target/debug/varta
node scripts/test-browser-matrix.mjs - <playwright-module-path> --effects target/debug/varta
```

`worker-effects.test.mjs` covers the Worker handlers and their denials
(missing `time.now` grant, empty host ceiling, `env.get` filtering, exit
denial and invalid codes) and the maintenance loop, including fuel, deadline
and invalid-status failures. `loader.test.mjs` drives the loader with a fake
Worker: `maintenance_begin` arms the `maintenanceDeadlineMs` watchdog with the
native exhaustion text and terminates the Worker, `maintenance_end` clears it,
a malformed or bounded-run maintenance message is a protocol failure, and an
exit before main's checkpoint rejects `mount().ready` with its code. The Node
suites run in the full gate and CI. Native comparisons of the registry's
diagnostic texts: `maintenance_exhaustion_texts_equal_the_native_runtime_errors`
and `invalid_input_failure_text_equals_the_native_runtime_diagnostic`
(varta-project), `env_snapshot_too_large_text_equals_the_browser_registry`
(CLI smoke) and
`an_invalid_idle_status_terminates_the_instance_with_the_browser_registry_text`
(varta-runtime, with a rewritten idle export, since compiled modules only
return valid statuses). `effect_admission_follows_the_native_profile_imports`
checks that each pilot effect is admitted exactly under the profiles whose
native host imports it. `test-browser-effects.mjs` builds temporary
projects and compares native `varta dev --budget-report` (`--app` for the
long session) with the browser dev host in local Chrome: logs, exit codes,
exact host calls, microtasks, output and fuel, and the listed idle periods.
Its cases include the recoverable failures (a too-large snapshot and an
invalid exit code, whose native diagnostics are the registry's texts) and the
older profiles that import a pilot effect natively (v37 clock, entropy and
exit; v20 entropy and exit).
Random values are checked by range and shape only; the `randomUUID` case
compares everything except fuel, because the guest prelude's cost depends on
the drawn bytes natively too. The matrix's `--effects` option runs the same
cases in Firefox, WebKit and Chrome (`-` skips the generated fixtures); an
engine that cannot launch is reported as failed, never as a pass.

The official registry has a Rust regeneration drift test
(`checked_in_browser_module_matches_registries`). Besides profiles and operations it
generates the value-layout, scheduler, network, view, permission and effect facts the
official host reads (`varta_common::browser`'s `layout`, `network`, `view`,
`permissions` and `effects` modules); it is not a copy of every generated table of
the embedding contract (the root-stack bounds, for example, are absent).
Regenerate it explicitly after a registry change:

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
cargo build -p varta --locked
node scripts/test-browser-streaming.mjs [varta-executable]
```

`test-browser-streaming.mjs` (default executable `target/debug/varta`) runs the
v38 long-lived-transfer programs as application instances with `varta dev --app`
and in local Chrome (its own bundle, a temporary profile and a random debugging
port) against one loopback server, and requires the same log lines: a server-sent
event stream longer than several test-only inactivity windows, a stalled read,
host-owned `User-Agent`, a decoded compressed body, and a burst of events larger
than the per-read delivery cap under the production budgets.

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
node examples/uarp-console/scripts/test-browser-sdk.mjs target/debug/varta
node examples/uarp-console/scripts/test-browser-settings.mjs target/debug/varta
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

### Potential accessible-name work

The native `naming_work` integration suite and browser `view-naming` suite cover
the exact UTF-8 allowance, one-byte overflow, final-inventory repair/rollback,
detached and hidden candidates, native host labels, repeated/cyclic relations,
field reservations and changes in naming-capable roles. A maximum-depth tree with
a full-size text leaf remains accepted for author-only roles; making every role
content-naming rejects, while making only one does not. Native snapshot tests
also verify label document order and index refresh on rename/reparent/removal.
A native unit matrix checks computed element-name sizes against the bound across
144 worlds. Raw text-leaf publication is covered by retained-source limits.

```sh
cargo test -p varta-view --test naming_work --locked
cargo test -p varta-view --lib naming_work --locked
cargo test -p varta-runtime --lib accessible_name_budget --locked
node --test crates/varta-project/src/browser/view-naming.test.mjs crates/varta-project/src/browser/view-session.test.mjs
node scripts/test-browser-style-work-bundle.mjs target/debug/varta --naming-work
```

Use the refreshed native view oracle for Node and the Chrome DOM boundary probe.
Its five naming cases cover 25 batches, including the author-only/content-role
transition. The official-host mockup probe includes final-inventory refusal,
DOM/model/title rollback, preserved node identity and smaller retry in each
selected engine. The compiled source-free probe verifies fatal Worker refusal
before guest catch/logging. These observations do not measure platform
accessibility-engine CPU or establish a complete browser/platform memory quota.

### Bounded native view diagnostics

Snapshot encoder units verify encoded UTF-8 boundaries, escaping/keys in compact
and pretty output, oversized-fragment refusal before allocation, bounded requested
capacity and the public 16 MiB allowance. Native integration checks retain newest
complete opened URLs by both entry and UTF-8 byte limits, preserve action results
and rollback, and ignore platform refusals. A 500-level/3000-leaf view is built
under the existing per-transaction work limits; pretty export refuses while
compact retry and later cleanup succeed. The compiled TSX fixture builds in
bounded timer turns with its explicit timer grant and unchanged guest budgets.

```sh
cargo test -p varta-view --lib snapshot_json --locked
cargo test -p varta-view --test snapshot_export --locked
cargo test -p varta-runtime --lib snapshot_export_limits --locked
cargo test -p varta --test smoke view_snapshot_export_is_bounded --locked
```

The CLI regression also checks that requested budget diagnostics survive export
refusal and that no partial JSON/completion marker is printed. Its cleanup script
then succeeds through the ordinary output path. Structured inspection/caller-owned
copies, external serializers and complete RSS accounting are outside this byte
buffer check. The desktop worker's one-outstanding-frame acknowledgement gate and
raster dimension checks are source-inspected separately; no new GUI/backpressure
or cross-platform observation is inferred from these serializer tests.

### Native GUI input backlog

```sh
cargo test -p varta-desktop --locked
cargo clippy -p varta-desktop --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
```

Queue tests cover exact aggregate owned text, one-byte overflow, count capacity,
atomic preedit replacement, FIFO/binding boundaries, full Unicode commit values
and capacity release on pop/filter/clear. Preview-method tests use bounded real
channels and synthetic CPU frames without an OS window to exercise first-frame
deferral, one-key backpressure, stale-binding release and spare-buffer compaction.
The normal native IME value/cursor/focus guards remain exercised. These checks do
not substitute for real-window, OS composition, slow-consumer or cross-platform
acceptance. Pinned shaping-cache source inspection is recorded separately from
runtime memory measurements; cache entry counts are not an aggregate RSS quota.


### Native field layout reuse

```sh
cargo test -p varta-view --locked
cargo clippy -p varta-view --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
cargo fmt --all -- --check
```

The view suite covers plain/password glyph equivalence, UTF-8 hit positions,
composition, selection, editing, multiline scroll/caret geometry and controls
sharing the glyph painter. Source inspection additionally verifies that a
nonempty field passes its existing caret/hit layout to painting and borrows
ordinary values/placeholders. No allocator/RSS or frame-timing claim follows
from these regressions. At the reuse checkpoint, full-size field values/masks
could still exceed the unchanged general UTF-8 allowance. The subsequent Unicode field package below
uses a separate semantic field bound and a wide-offset dependency patch;
complete backend/platform acceptance remains separate.


### Maximum Unicode native field display

```sh
cargo test -p varta-view --test fields --test text --locked
cargo test -p varta-view -p varta-desktop --locked
cargo clippy -p varta-view -p varta-desktop --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
cargo fmt --all -- --check
```

The new field checks exercise maximum UTF-16 values occupying up to 98304 UTF-8
bytes: plain/search Unicode, password masks, astral/combining text, exact source
hits/caret, grapheme deletion and one-unit excess rollback. Full-size IME replaces
an existing selection without mutating the value until commit; a composition at
the full value's end still cancels under the unchanged combined-value guard.
Long placeholders and content-sized textarea measurement/wrapping use the same
semantic field classification. Mixed-direction suffix pixels, caret and shifted
source hits are compared directly, and reuse of the shaper through the public
API must still reject oversized general text.

The initial local Parley 0.11.1 offset patch widened the cluster text offset and
its construction cast; the subsequent large-cluster patch also fixes glyph/
coverage counts and full font mapping. See `vendor/parley/VARTA-PATCH.md` and
`VARTA-PATCH.diff` for its current scope. Verify the vendored
source against the checksum-pinned original release when changing that patch,
retaining upstream licenses and explicit dependency versions. The initial new
regressions exposed wrapping offsets and UTF-8 slice panics without it; passing
only a looser Varta input check is insufficient. The broader native suite checks
other controls and text/layout callers sharing the patched dependency. These
fixtures do not prove arbitrary font expansion, full bidi navigation, bounded
aggregate caches/RSS, real-window IME or a complete platform matrix.


### Large native glyph clusters

```sh
cargo test -p varta-view --lib text::tests --locked
cargo test -p varta-view --test fields expanded_combining_cluster --locked
cargo test -p varta-view -p varta-desktop --locked
cargo clippy -p varta-view -p varta-desktop --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
python3 scripts/generate-view-cluster-font.py
```

Only fixture regeneration needs fontTools; the committed original fonts are
used directly by Cargo tests. Repeated generation must leave both font files
byte-identical. One mark produces a single glyph, another expands through `ccmp`
to three, and a partial face lacks the trailing character of a long grapheme.
Checks observe actual glyph counts around the old inline sentinel, complete
fallback selection past 32 characters and the existing 65536-glyph paint boundary
with excessive refusal and subsequent recovery. Dependency source/manifest/
license comparison against the checksum-pinned release must agree with the
four-file patch diff. Neither these tests nor wider counters establish an
aggregate bound on pre-paint allocation, shaping CPU, caches or RSS.


### Bounded native shaping requests

```sh
cargo test -p varta-view --lib --test fields --test text --locked -- --nocapture
cargo test -p varta-view --lib shaping_ --locked -- --nocapture
cargo test -p varta-view -p varta-desktop --locked
cargo clippy -p varta-view -p varta-desktop --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
cargo fmt --all -- --check
```

Native request limits are 65536 completed glyphs, 262144 intermediate slots per
glyph-info/position array, an initial allowance of one million upstream-charged
operations across runs and 65536 retained slots per array. Tests cover a smaller
exact array boundary/overflow, total output and operation exhaustion across
scripts, configured policy surviving context clone, unchanged maximum ASCII/
Unicode input, early refusal of strong 16× expansion and successful smaller
reuse. Refused layouts never publish partial glyphs/metrics; refused glyph arrays
are discarded before later calls. Array capacities and charged operations are
read from the actual dependency pipeline rather than inferred from wall time.

The 16× original synthetic font is generated alongside the earlier cluster
fixtures; all three files must regenerate identically. The pinned HarfRust and
Parley archives/manifests/licenses must match their original checksums except
for the documented two- and six-file diffs, respectively. See both
`vendor/*/VARTA-PATCH.md` records.

Capacity bytes exclude allocator metadata/reallocation overlap and other
analysis/font/plan/layout buffers. The initial request implementation could
overshoot operation thresholds; the strict pre-charge checks below address that.
The counter still omits some parser/analysis/plan work. These tests establish
neither total RSS/CPU nor an execution deadline; complete backend/cache/platform
acceptance remains open.


### Strict native shaping pre-charge

```sh
cargo test -p varta-view --lib text::tests --locked -- --nocapture
cargo test -p varta-view --lib aat_ --locked -- --nocapture
cargo test -p varta-view -p varta-desktop --locked
cargo clippy -p varta-view -p varta-desktop --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
python3 scripts/generate-view-aat-fonts.py
```

The multi-script check must succeed with its exact observed allowance and refuse
one unit below without overspending. Original AAT fonts cover finite current/
marked insertion, legacy/extended kerning state machines and a nonadvancing morx
loop. All smaller budgets refuse and drop scratch, exact budgets reproduce
completed metrics/output, and a normal retry succeeds. Source inspection must
find only one direct counter subtraction inside the checked charge helper and
no counter-dependent forced advancement. All five AAT files regenerate
byte-for-byte; archive comparison matches the current six-file HarfRust patch.
These observations concern existing charged work, with some newly charged state
transitions; they do not meter all parser, analysis, plan, outline or frame work.


### Shared native view shaping scope

```sh
cargo test -p varta-view --lib view_shaping_scope --locked -- --nocapture
cargo test -p varta-view -p varta-desktop --locked
cargo clippy -p varta-view -p varta-desktop --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
cargo fmt --all -- --check
```

The adapter's outer allowance is 16777216 charged operations, shared by nested
measurement/paint calls; all per-layout/storage/paint limits remain independent.
Real content-sized textarea probes verify intrinsic and Taffy remeasurement,
geometry-to-paint exhaustion, exact funded output/pixel parity, scaled raster,
box queries, and two layout commands in one batch. The failure preserves both
the snapshot and deferred clipboard effects; a funded retry executes once.
Nested/caught failures cannot refill quota. Standalone measurements and later
independent requests recover normally; the diagnostic reports only the last
outer view request. The deliberate unwind probe prints an expected panic with
`--nocapture`, catches it, and verifies scope-state cleanup; it is not an uncaught
test failure or a general dependency-cache unwind-safety claim. Tests use small
injected allowances through the same scope path as the default view allowance.

Regenerate the six-file Parley patch diff after the temporary-ceiling/context
changes and compare source/manifest/licenses with the checksum-pinned release.
No browser/Node/ABI claim follows from this native-only counter adapter. Unmetered
work, full frame CPU/RSS, other buffers/caches and platform acceptance remain
separate tasks.

### Native layout measurement failure

```sh
cargo test -p varta-view --lib view_shaping --locked -- --nocapture
cargo test -p varta-view -p varta-desktop --locked
cargo clippy -p varta-view -p varta-desktop --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
cargo fmt --all -- --check
```

Eight actual content-sized textareas use the original marked-insertion AAT
fixture. Their intrinsic measurements succeed; the first width-constrained
measurement cannot complete within the injected shared allowance. A test-only
builder counter must observe exactly nine builds (eight intrinsic and one
refused), through geometry, box, raster and scaled-raster entry points. Subsequent Taffy callbacks
must use fallback metrics without another builder. Failure retains its text
diagnostic, publishes no geometry/image and leaves the snapshot unchanged; a
funded independent retry reproduces the original pixels. A layout-command batch
must stop at its first failing command, preserve its title/commit count/snapshot,
and perform no deferred clipboard write. Its funded retry executes that write
once. The builder count observes real dependency requests; it is not a timing,
allocation or complete CPU measurement. Taffy's remaining traversal still runs.

### Native Unicode paragraphs

```sh
cargo test -p varta-view --test paragraphs --locked -- --nocapture
cargo run -p varta-view --example native_paragraph_probe --locked -- target/native-paragraph-oracle.json
node scripts/test-native-paragraphs.mjs target/native-paragraph-oracle.json
cargo test -p varta-view -p varta-desktop --locked
cargo clippy -p varta-view -p varta-desktop --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --locked
cargo fmt --all -- --check
```

The original `unicode.ttf` covers distinct bar/square glyphs and a zero-advance
mark. Three native tests check symbols, supplementary scalars, accented Latin,
mixed Hebrew/Arabic visual ordering, combining ink, CJK wrapping, nonbreaking
space and CSS whitespace across block/flex/grid. The source remains unchanged.
NUL still rejects atomically at wire admission; other unsupported controls and
the existing `pre-wrap` tab/edge-space restrictions remain errors. The general
64 KiB UTF-8 boundary refuses excess and a smaller independent render recovers.

The probe emits 33 original-font cases. Real Chrome compares container geometry
and source text, then independently shapes the source and its glyph aliases in
a canvas for the applicable cases: 81 checks. Native tests compare the equivalent
glyph sequences within the native backend. This is not a claim of identical
pixels between different rasterizers or complete script typography/inline layout.
Regenerate `unicode.ttf` with `python3 scripts/generate-view-unicode-font.py` and
compare its bytes; fontTools is used only for regeneration.

The generic [headless-frame example](../crates/varta-desktop/README.md) runs an
actual compiled module and validated manifest through the public application
and raster APIs. Check both successful initial RGBA output and nonzero failure
diagnostics; it does not run the rest of an interactive session. The unchanged
UARP console view artifact initially refused node 44's `↑`. With Unicode text
admission it reaches node 2's `max-width: 48rem` limitation instead. Keep those
failures distinct from the passing synthetic Unicode artifact. This does not
close the console/native platform acceptance or native relative-length support.

### Native rem reference metrics

```sh
cargo test -p varta-view --test lengths --locked -- --nocapture
cargo test -p varta-runtime --lib native_layout_ --locked -- --nocapture
cargo run -p varta-view --example native_lengths_probe --locked -- target/native-lengths-oracle.json
node scripts/test-native-lengths.mjs target/native-lengths-oracle.json
cargo test -p varta-view -p varta-desktop --locked
cargo clippy -p varta-view -p varta-desktop -p varta-runtime --all-targets --locked -- -D warnings
cargo build -p varta-desktop --bin varta-preview --example headless_frame --locked
cargo fmt --all -- --check
```

Six native cases compare rem/pixel geometry and native pixels for dimensions,
edges, nested grid tracks, signed positioned edges, border/outline widths and
offsets, font size and absolute line height. They cover raw-style preservation,
reference changes/zero, invalid host metrics, separate view/default/media font
meanings, variable inheritance, coefficient/resolved limits and rollback before
deferred clipboard effects. The inherited text size intentionally differs from
the document reference, so a font-relative line-height multiplier cannot stand
in for a rem length. The runtime selection proves initial-turn scroll geometry,
reference preservation through resize/environment/DPI updates and earlier theme/
scroll behavior. Neither default budgets nor the guest ABI change.

The independent Chrome probe creates a shadow tree and explicitly sets the
containing document's font to 16/20/24 px while its view root uses 2rem. Eighteen
fixtures compare native and DOM boxes plus document/view font identities (168
checks), including media rem's independent initial-font reference. Anonymous
native text boxes are compared through their container, not DOM glyph ranges.
This is scoped geometry evidence, not complete fractional-border rounding,
typography, real-window or platform acceptance. Native standalone hosts keep the
default reference; embedding hosts explicitly supply other root metrics.

### Native inline formatting

```sh
cargo test -p varta-view --test integration inline --locked
cargo test -p varta-view --lib inline_work_scope --locked
cargo run -p varta-view --example native_inline_probe --locked > target/native-inline-oracle.json
# Default1x probe passes after paint rounding;125% ink/curve acceptance remains open.
node scripts/test-native-inline.mjs target/native-inline-oracle.json
cargo run -p varta-view --example native_inline_probe --locked -- 1.25 decorations > target/native-inline-125.json
# Still fails at the recorded125% ink/curve scene; keep the diagnostic visible.
node scripts/test-native-inline.mjs target/native-inline-125.json
cargo run -p varta-view --example native_inline_probe --locked -- 2 decorations > target/native-inline-2.json
node scripts/test-native-inline.mjs target/native-inline-2.json
```

The original-font probe compares a standards-mode real Chrome DOM with native
fragment unions, interior hits and selected solid/background screenshot pixels.
Boundary pixel footprints differ from native point hits: the browser probe
excludes points within one CSS pixel of a fragment edge, while native regression
tests check fragment gaps and overflow targets directly. Text-node anonymous
boxes, `br` rectangles and implicit/document root hits are not conflated with
DOM element rectangles. The cases include wrapping, no-wrap overflow, nested
font/line struts, ligatures, combining marks, Arabic context, empty anchors,
trailing breaks and clipping. This is not full browser or platform typography
parity. The work-scope test exercises real failed layout commands, nested calls,
model/clipboard atomicity and independent retry. Before committing, run the full
workspace gate; scoped/browser success alone is insufficient.

Consolidated 2026-11-03 result: **3174 passed, 0 failed, 16 ignored**, all eight
full-gate commands and unchanged sources, recorded at `target/verification/20261103-122544-p42wl8c3/report.json` (local,
ignored). The batch includes **217 native tests**, the separate **2724-check
Chrome inline oracle**, and the earlier Unicode/rem scoped probes. Only result
and status documentation was updated after the full gate; code/build inputs
were unchanged. This does not close native console/platform/release acceptance.

### UI-2 consolidated accessibility gate — 2026-11-04

`python3 scripts/verify.py full` passes all eight commands with unchanged inputs:
**3281 tests passed, 0 failed, 16 ignored**. Report:
`target/verification/20261104-011619-8_yzsx6c/report.json`; source SHA-256
`2ab8cfa1489dc188cae26237c3f9ea5cf2d9b9332e48977199448f47176e62df`. This supersedes the failed boundary report
`target/verification/20261104-005142-wa0_orw4/report.json` for the corrected
inputs; that failed report is retained. Documentation-only annotations follow
verification. The separate optional GPU, Showcase and headless checks above
retain their own scopes. Native window/VoiceOver, remaining range/scroll routes
and other target acceptance are not inferred from the workspace result.

A read-only sample of a delayed test process observed 800/800 samples at
`_dyld_start + 0` with a 112 KiB footprint before the Rust harness started.
Evidence: `target/a11y9-test-startup-sample.txt` and
`target/a11y9-test-startup-diagnostic.json`. The cause of this host startup
delay is unconfirmed; no system protections were changed. A common cause with
the earlier UI-tool timeouts is only a hypothesis. Documentation retrieval later
succeeded, but selecting the current Debug bundle timed out again after 60 s
with kernel reset (`target/a11y9-native-retry.json`); no native acceptance
is inferred from this attempt.

### Native macOS range package after `5a52125` — scoped checkpoint

The applied package passes 66 desktop tests with GPU enabled and strict
all-target desktop GPU Clippy (`target/a11y10-desktop-gpu-tests.log`,
`target/a11y10-clippy.log`). The exact prepared source also passed 66 CPU desktop
tests and macOS SDK metadata compilation; those overlap the Cargo cases and are
not additional independent test counts. Eleven new cases cover exact range
validation, overflow, Unicode/CRLF, password masks, visible whole lines and
guarded scroll routing without selection/focus/value mutation.

Formatting passes. The regenerated vendor patch reverses cleanly in check-only
mode, and provenance is recorded in `target/a11y10-vendor-provenance.json`.
Whitespace checks exclude the generated nested diff's required context-marker
spaces; they remain enabled for implementation and documentation. The unchanged
upstream overflow is reproduced separately by the expected failing extracted
probe (`target/a11y10-stage/overflow-probe.log`).

The completed range package then passes **3292 tests, 0 failures, 16 ignored**
in all eight full-gate commands with unchanged inputs. Report:
`target/verification/20261104-015702-6g3mx2fb/report.json`; source SHA-256
`6294ded534da348dc9dc43516e10f0ecd1159e367184dc4462cc79c313823653`. This supersedes the preceding
3281-test checkpoint for these newer code inputs. Only documentation annotations
follow this gate before the package commit. Actual native/VoiceOver, remaining
page/item/absolute routes and other-platform acceptance remain open.

### Fragmented inline outlines — scoped family checkpoint

`cargo test -p varta-view --locked` passes104 library +315 integration tests.
The `portable_outline_probe` example emits33 independent original-font fixtures;
run it at1,1.25 and2, then pass each JSON to
`node scripts/test-native-inline.mjs <json>`. All33 pass at every scale under
the existing2/255 mean bound (304/300/300 assertions). The verifier accepts an
explicit bounded integer `viewport`, defaults to100×100, and rejects invalid
viewport/raster dimensions. Existing59 inline and26 decoration JSONs also pass
the updated verifier; that replay checks the verifier, not a new engine render.

Strict all-target View/Desktop GPU Clippy and actual Debug/Release
`gpu_clip_probe` runs pass. Each build has28 scenes,84 actual GPU observations
and28 explicit CPU fallbacks; maximum mean delta0.1472. Square/rounded outlines
require actual vector GPU rendering; the opacity case explicitly falls back.
Logs, source hashes and guarded merge evidence are under
`target/paint-next-stage/outline/`. This is scoped verification of an open family
batch. The preceding3448-test full gate covers older source; do not relabel it
as a current outline/spacing/full-platform pass.

### Vertical inline spacing — same open family batch

`cargo test -p varta-view --locked` passes104 library +321 integration tests.
Six new integration cases cover independent line/content geometry, padding hits,
percentages, cascade side override/removal, nested boxes, retained/vector changes
at three DPIs with opacity/radii/outlines, and explicit horizontal/length refusal.

```sh
cargo run -p varta-view --locked --example portable_spacing_probe -- 1.25 > target/native-spacing-125.json
node scripts/test-native-inline.mjs target/native-spacing-125.json
```

Run at1,1.25 and2: each passes21 scenes/10131 assertions, maximum observed mean
RGB delta0.579711 under the unchanged2/255 bound. The probe marks one-CSS-pixel
neighbourhoods of hit boundaries, including curves, using adjacent exact-point
samples; the verifier already excludes rectangle edges. This separates Chrome's
pixel-footprint query from native exact-point hits. All box/pixel comparisons
and interior hit checks remain active. Failure/discovery logs (including initial
fixture errors and the curved-edge mismatch) are retained under
`target/paint-next-stage/spacing/`. No current full gate or application/platform
acceptance is inferred from these scoped results.

The final owned-length correction passes a fresh425-test View run and strict
all-target View/Desktop GPU Clippy. Fresh21-scene native JSONs are byte-identical
at all three scales to the independent Chrome-checked inputs above. Final
Debug/Release GPU runs each cover30 scenes with92 actual GPU observations and28
explicit CPU fallbacks; both new square/rounded padding scenes render on GPU.
Maximum mean delta remains0.1472. The correction preserves `Send`/`Sync` for the
shared content flow without retaining Taffy's opaque-pointer-capable length type.
The172 pinned inputs remain unchanged; exact scope, hashes and logs are in
`target/paint-next-stage/spacing/checkpoint.json`. The earlier3448-test full gate
still covers its older snapshot, not this open outline/spacing family.

### Horizontal inline edges and borders — open family batch

`cargo test -p varta-view --locked` passes 111 library + 336 integration tests;
`cargo test -p varta-desktop --features gpu --locked` passes 78. Edge unit
tests cover Parley break/width/ligature/memo/limit behavior; `inline_edges`
and `inline_border` cover geometry, pixels and hits of padding, margins and
borders, slices across lines, empty and rounded boxes, margin-gap line hits,
slice radius normalization, content boxes, retained/stateless/vector agreement
at three DPIs and the explicit refusals.

```sh
cargo run -p varta-view --locked --example portable_edge_probe -- 1.25 > target/native-edges-125.json
node scripts/test-native-inline.mjs target/native-edges-125.json
```

Run at 1, 1.25 and 2: each passes 39 scenes/68837 checks, maximum mean RGB delta
0.198 under the unchanged 2/255 bound. The verifier now also skips hit samples
in the one-pixel band just above each line box (hull of its text fragments),
where Chrome's footprint depends on overlapping fragments; box and pixel checks
are unchanged. Rerun the earlier inline oracles after any change to fragments
or hit order; `target/edge-probe/regress/run.sh` records the commands used for
this batch. `native_opacity_probe` takes no scale argument. Debug and Release
`gpu_clip_probe` each pass 34 scenes (108 actual GPU + 28 explicit CPU-fallback
observations). The consolidated full gate passes all eight commands: 3495
tests, 0 failed, 16 ignored, conformance baseline matched
(`target/verification/20261104-175437-p30mj15f/report.json`).

### Consolidated native inline painting commit gate — 2026-11-04

`python3 scripts/verify.py full` passes all eight commands: **3473 tests passed,
0 failed,16 ignored**, with unchanged inputs. Report: `target/verification/20261104-161029-qyx08oka/report.json`;
source SHA-256 `22ba2807b47b0ab666df1b89bd0bc57a6b1d96ed5a55b2f9650d474b7478ad71`.
Conformance reports123 passed/4 known unsupported of127, baseline matched,
zero regressions and zero infrastructure failures. Current affected GPU Clippy,
30-scene Debug/Release Metal observations (92 actual GPU +28 explicit CPU
fallbacks per build) and21-scene Chrome comparisons retain their separately
pinned scope above. All172 relevant code/build/fixture/tool inputs still match.
Only documentation result/status annotations follow the full gate. The incomplete
horizontal-edge prototype is kept only on the local branch `wip/inline-edge-shaping` and excluded from commit `ed77739e`;
no horizontal spacing/border, real-window, other-platform or whole-plan acceptance
is inferred. Evidence: `target/paint-next-stage/spacing/full-gate-checkpoint.json`.

### Host-owned native clipboard — package after `2400dbf`

Scoped core tests use tasks and injected results, without OS I/O:
`cargo test -p varta-view --test clipboard --locked` (15 pass,
`target/clipboard-core-final.log`). Real Wasm/Application coverage checks later
input delivery, absence of fresh gesture authority, guest changes during I/O,
termination, failure recovery, prevention and application isolation:
`cargo test -p varta-runtime --lib --locked view_tests::native_clipboard`
(six pass, `target/clipboard-runtime-tests.log`).

The desktop GPU regression run passes 76 cases
(`target/clipboard-desktop-gpu-updated.log`), including nine clipboard/queue
cases and the deferred-timing case alongside previous regressions. Providers
are injected; these tests never access the user's clipboard. The initial scoped
Clippy failure was `items_after_test_module` in the new desktop helper; helpers
were moved above the tests. Final strict all-target Clippy passes
(`target/clipboard-clippy-final.log`), as do both direct/transitive dependency
boundary cases (`target/clipboard-boundaries.log`). The fresh CPU Debug bundle
builds and the Showcase passes its 12 trusted-input script
(`target/clipboard-showcase.log`). App selection through Cua times out after
60 s with kernel reset; current launch and actual system clipboard behavior
remain unconfirmed. Bundle hashes and scope are recorded in
`examples/native-showcase/clipboard-checkpoint-2026-11-04.json`. These scoped
results do not constitute the final package gate.

The first full-gate attempt stopped at formatting before engine checks
(`target/verification/20261104-032513-0aqoes7o/report.json`); ordering two test
module declarations fixed it. The final full gate records **3323 passed, 0 failures, 16 ignored**, all eight commands with unchanged inputs. Report: `target/verification/20261104-032530-xtwjknm6/report.json`; source SHA-256 `011b1246d98baaba320d4a7e85bf0b676c75779b3e03099e3cf279d2659d7ced`.
This supersedes the preceding 3292-test range checkpoint for the newer clipboard
inputs. Documentation-only status annotations and a runtime comment correction
("queue keydown", rather than implying immediate callback delivery) follow the
gate. Actual system clipboard behavior and the remaining platform matrix stay
open.
