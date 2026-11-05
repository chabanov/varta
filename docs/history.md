# Varta development history

Completed increment logs, earlier status checkpoints and superseded direction text,
moved verbatim from the development plan and README on 2026-10-28. These records
describe the state at the time each entry was written; counts, versions and "next"
statements in them are historical. Current status is in the
[development plan](development-plan.md#status); current contracts are in
[architecture.md](architecture.md).

## 2026-11-05 — Browser KV storage on OPFS (wave 3, `w3-browser-kv-opfs`, scoped)

The official browser host stores KV databases instead of denying every open,
on the agreed design. The value layer moved out of `varta-runtime/src/kv`
into the safe `no_std` crate `varta-kv-codec` (staging, the V8 codec, a Deno
key codec replacing the `denokv_proto` calls, descriptors, cursors,
versionstamp text, fair fuel, the decode classifier and the top-down
construction of the result and of checked JSON's recoverable errors); the
native runtime calls it, and `varta-kv-codec-wasm` compiles it to a
checked-in wasm32 module with a source fingerprint
(`scripts/build-kv-codec.py`). The Worker acquires one OPFS container per
artifact through a sync access handle before instantiation and commits
through a write-ahead record and two checksummed header slots; the loader
validates KV ceilings; the browser build admits KV grants. Lead decisions on
the design's open questions are recorded in the
[development plan](development-plan.md#browser-kv-storage-agreed-design-wave-3).

Scoped evidence on `wave3/w3-browser-kv-opfs`: `verify.py local varta-runtime
--filter kv` (25, the fuel goldens included, unchanged) and the runtime
`json` tests (160) whose shared builder and error publication moved; a
differential test of the key codec against `denokv_proto` and of the
expiration bound against `chrono`; `varta-kv-codec` (18), common `browser::`
(7) and project `browser::` (49) tests, the boundary test, the documentation
tests and 201 Node tests (container crash points at every write, flush and
truncate, compaction included, quota, corruption, lock, WebKit's lock error,
the Worker's refusal without sync access handles); the codec build
reproduces its fingerprint; strict Clippy of the touched crates and of the
wasm32 shim. `scripts/test-browser-kv.mjs` passed in local Chrome three
times (logs and exact fuel, host calls, microtasks, output bytes and turns
equal to native for the basic, reload, isolation, missing-grant and ceiling
cases; two tabs, corrupt, full and aborted storage) and in Playwright WebKit
(all but the quota case, which needs Chrome's DevTools quota override);
Playwright Firefox did not launch here. The full gate is the wave lead's.

### Review fixes (2026-11-05)

A shared KV value depth limit (`varta_kv_codec::MAX_VALUE_DEPTH`) now
bounds the stage walk, the V8 encoder and decoder and the classifier on both
hosts with one recoverable `TypeError`; before it Chrome trapped from about
750 levels, WebKit from about 170, and a native debug `varta dev` aborted at
1500. The browser refuses database paths below a directory of their root by
name, opens databases writable as native does, and refuses reads from an
uncertain container; the whole-artifact lock is an accepted deviation. A
native test pins `denokv_sqlite`'s partial commit; a project test runs the
checked-in wasm32 codec beside the native build over every Deno fixture and
probe value; `test-browser-matrix.mjs --kv` runs through a persistent
Playwright context; `verify.py` knows `varta-kv-codec` and checks the codec
fingerprint. Scoped evidence: `verify.py local varta-runtime --filter kv`
(27), `varta-kv-codec` (20), common `browser::` (7), project `browser::`
(51), 204 Node tests, a clean-room codec build byte-identical to the
checked-in module, and `scripts/test-browser-kv.mjs` in Chrome 156 (11
cases) and Playwright WebKit 26.5 (10, no quota override).

### Second re-check fixes (2026-11-05)

A part shared within a KV value now counts at its deepest reach: the stage
walk, the V8 writer and the V8 reader keep each aggregate's height and check
it again at every later reach. Before, `{ a: x, b: y }` with `y` wrapping the
same `x` deeper than the limit stored ("set ok") and never read back, while
`{ a: y, b: x }` was refused. Native, the wasm32 codec (a new
wasm-vs-native stage test over a guest with a shared cell) and Chrome agree
on the shared-chain cases in both orders and on a cycle (new `dag` case).
The KV acceptance's server closes idle browser connections, so
`test-browser-matrix.mjs --kv` finishes in Chrome; CI checks
`varta-kv-codec` without `std` and the codec fingerprint.

### Third re-check fixes (2026-11-05)

A KV value's extent (`varta_kv_codec::Extent`: values and code units of
text, a shared part at every reach, absent optional fields as values) is
bounded by `kv::MAX_VALUE_NODES` and `kv::MAX_VALUE_UNITS` in the stage
walk, the V8 writer and reader and the classifier on both hosts, with one
named recoverable `TypeError`; the classifier pays fuel every deadline
stride. Before, a diamond `x = { a: x, b: x }` stored after a dozen levels
and its `get` exhausted the turn's fuel; after about twenty, a native `get`
ran 5.8 s past the deadline at about 980 MB of host memory. A `get` at the
bound now takes about 3.4 million fuel and 1.8 MiB of guest memory in its
turn. Native (`shared_parts_keep_the_value_bound_on_set_and_get`, which
exhausted the turn's fuel on the old codec), the wasm32 codec against the
native build (stored and staged diamonds, both failing against the old
module) and Chrome (`diamond`, with planted Deno-format values and the
Chrome process tree's memory growing by about 37 MB) agree.
`build-kv-codec.py` takes Cargo's target directory from `cargo metadata`, so
a clean-room build with `CARGO_TARGET_DIR` completes.

## 2026-11-04 — Aborted fetch closes its connection (wave 2, `w2-net-abort`, scoped)

Pilot finding F2 fixed. An abort while the head was pending only
marked the transfer task cancelled; the request runtime dropped it, and the
connection task owning the socket, when the event loop next drove that
runtime, so a guest waiting on timers kept the connection open until it woke.
`net_request_abort` now cancels both tasks and runs the runtime until they are
dropped. A v2 HTTP server whose client left kept its one worker permit until
the cancelled worker stopped and refused a request arriving meanwhile with
503; its accept loop now waits for the permit first (v1 frozen with its
immediate 503). After review, a transfer that ends on its own also closes its
connection before its terminal completion is delivered, the abort tests order
their abort by a loopback gate instead of a timer, and the v2 disconnect tests
reject any 503.

Scoped evidence on `wave2/w2-net-abort`: two new `net_tests` regressions, each
failing before its half of the fix (close 602 ms after the abort; `hello: 503`)
and passing after, `verify.py local varta-runtime --filter net` (55) and
`--filter server` (32), related runtime groups, strict Clippy of
`varta-runtime` and formatting. The full gate is the wave lead's.

## 2026-11-04 — Opt-in headless geometry (wave 2, scoped)

Headless view snapshots gained an opt-in `geometry` section: the viewport, the
base text size and, per displayed element in document order, its id, used
outer display and border box, computed by the existing explicit-font layout
from the final view of a scripted session. `varta dev --view-script` takes
`--view-geometry` with repeatable `--view-font PATH FAMILY`; the runtime adds
`run_view_with_geometry` and `Application::view_snapshot_with_geometry`, and
`varta-view` adds `View::snapshot_with_geometry` with `GeometryRequest`.
Without a request the output is byte-identical: both UARP console view scripts
still match `chat.expected.json` and `stream.expected.json`. Blockified flex
and grid items keep their cascaded `display` in the tree and report the used
`block` in the section. Unsupported layout fails the run without a partial
section; the console chat view is refused at present (node 43 with the
structure fixture face), so its geometry stays open.

Scoped evidence on `wave2/w2-headless-geometry`: `scripts/verify.py local
varta-view`, `local varta-runtime --filter view` and `local varta-project`; the
full `varta` smoke suite (353 tests); the documentation tests; and
`node scripts/test-headless-geometry.mjs` with Chrome 156.0.8078.80 on macOS
arm64. That oracle ran counter, list and form fixtures, 44 boxes and 223 checks,
all within ±1 px (largest observed difference 0.0275 px). It rebuilds the
snapshot tree in Chrome rather than replaying the patch stream through the
official applier. The full gate is the wave lead's.

## 2026-11-04 — Native positioned painting and z-index (wave-2 branch)

The wave-2 package `w2-native-positioned-zindex` removed the native paint
refusal of positioned boxes and z-index. A pre-pass assigns each layer to its
stacking context and builds clip scopes in tree order; contexts paint negative
layers, in-flow blocks, content, then zero and positive layers, with hit order
following paint order. Clips, wheel chaining and scroll-into-view follow the
containing block chain, so fixed boxes stay with the viewport and absolute boxes
ignore scrollers outside their containing block. Out-of-flow boxes escaping an
opacity group's clip are refused. The same branch models Chrome's minimum
logical font size for non-absolute sizes. Evidence and open items are in the
[development plan](development-plan.md#native-positioned-painting-and-z-index--wave-2-package-w2-native-positioned-zindex-scoped-2026-11-04).

## 2026-11-04 — Native preserved white space (wave 2, scoped)

The wave-2 package `w2-native-whitespace` made `white-space: pre-wrap` and `pre`
native in full: preserved tabs advance to tab stops at `tab-size: 8` from their
line start (Blink's half-space rule included), a run of preserved spaces or tabs
at a soft wrap hangs as a whole, preserved spaces before a forced break or the
end conditionally hang (they count in max-content, not in min-content, and in
center/right alignment while they fit) and `pre` spaces always count. Text nodes
with different `white-space` modes now share one inline flow. The model drives
Parley's line breaker (its public API plus the hidden `set_prior_line_width`
hook for alignment) and checks every realized line against it, refusing any
disagreement. Review fixes then put tab stops on the flow root's space grid, let
a wrapping node inside a `pre`/`nowrap` root wrap at min-content, kept trailing
`pre` spaces in underlines and hit fragments, and reported the model's refusals
as unsupported content on the flow's node. Chrome observations fixed the details, including a collapsible
space after preserved spaces staying uncollapsed and a hanging tab run keeping
both tabs on its line. Scoped evidence and open items are in the
[development plan](development-plan.md#native-preserved-white-space--wave-2-package-w2-native-whitespace-scoped-2026-11-04).

## 2026-11-04 — Browser pilot effects and idle collection (wave 1, scoped)

The official browser host admitted `date_now`, `number_random`, `env_snapshot`
and `process_exit_f64`, with Worker handlers equal to the native ones: an
audited `time.now` decision before `Date.now` (denial logs the native text and
returns NaN), ungated `crypto.getRandomValues` entropy converted like native,
an `env.get`-filtered snapshot of the embedder's environment inputs in native
member order, and an exit that ends the run or `mount` session with its code.
`mount` sessions run `varta.collector.idle` once per idle period under the
maintenance budget, restore the closed turn's fuel and report listed periods
with native rules; the parent watchdog bounds the maintenance deadline. A new
`effects` section of the generated browser registry carries the operation
names, capability ids, entropy widths, failure texts and collector constants.

Scoped evidence on `wave1/w1-browser-pilot-effects`: `varta-common` and
`varta-project` browser unit tests, the boundaries test, regeneration without a
diff, 177 Node tests, `scripts/test-browser-effects.mjs` in Chrome (six native
comparisons, including a 13-turn session with idle collection) and Firefox 157
through the matrix (six effect cases and 13 generated fixtures). WebKit was not
run. The full gate is the wave lead's.

## 2026-11-04 — Native structure, alignment and ellipsis (wave-1 branch)

The wave-1 package `w1-native-structure-text` added the versioned native
user-agent table and with it native `h1`–`h3`, `ul`/`ol`/`li` outside markers,
`pre` (`white-space: pre`, generic monospace sized by Chrome's family switch)
and inline `label`, inherited `text-align` through Parley alignment and one-line
ellipsis that hides glyphs while keeping untruncated geometry and hits. Chrome
observations fixed the details: symbol markers are Blink's integer-ascent shapes
in CSS pixels at every device scale, the marker box (line box united with the
font content area) is the item's pointer target, and truncated text still hits
its element. Scoped results: 30246 Chrome checks per scale for the new
`native_structure_probe`; earlier probes' native output unchanged. The console
triage before/after and the open items are recorded in the
[development plan](development-plan.md#native-structure-alignment-and-ellipsis--wave-1-package-w1-native-structure-text-scoped-2026-11-04).

## 2026-11-03 — Painted field geometry and visual native text ranges

UI-2.3 exposes borrowed cluster/line geometry from the exact painted frame,
guarded by source identity and content revision, without reshaping for observers.
The same cluster rectangles drive pointer navigation. Original source ranges,
direction, visual wrapping, content clipping and both endpoint carets retain
their physical-pixel coordinates through scale changes. Transient IME geometry
is refused. Native password units now follow the renderer's grapheme masks;
legacy headless JSON retains its original mask count.

A new regression caught CRLF creating a separate extra visual line. Shared
display normalization fixes painting and content sizing while retaining original
guest bytes, selection offsets and End-before-pair behavior. Password masking
and CRLF display transformations keep source maps under the existing budgets.

Native runs now split by visual line/direction and character-index limits, with
clipped bounds, positions/widths and logical same-line links. Password metrics
split when clipping makes their widths/steps nonuniform. Run IDs are reused only
for equal content revisions and exact source ranges. Reflow cannot reassign an
old native character index to different bytes. Endpoint selection follows the
painted caret's affinity; collapsed upstream selections remain collapsed.
Actual visual spans share the observer-only run ceiling and explicit refusal/
fresh-ID recovery behavior, independently of the logical model's run count.

The 90-case field/editor run passes before final caret refinement; final 16 core
cases and 35 desktop GPU cases pass afterward, alongside strict affected-package
GPU all-target Clippy, formatting and a GPU-capable Debug build. Overlaps are not
additional unique cases. The new Debug bundle's UI query timed out after 20
seconds. Launch and OS range behavior remain unconfirmed; VoiceOver, assistive
scrolling/announcements and other targets are still open. No new full gate is
claimed. [Pinned inputs and evidence](../examples/native-showcase/accessibility-text-geometry-2026-11-03.json).

## 2026-11-03 — Native logical text runs and checked directional selection

UI-2.2 adds host-owned field text metadata: scalar units with CRLF grouped,
Unicode word starts, hard lines, trailing empty lines and directional UTF-8
selection. AccessKit runs contain at most 255 units so word-navigation end
positions also fit u8. Long logical lines link adjacent chunks; original UTF-8
offset maps remain in the host. Password runs contain only scalar masks and
reveal no original word/line structure.

Opaque content revisions are separate from wrapping field edit sequences.
Actual edits, guest value changes, Undo/Redo and input-type changes invalidate
earlier ranges, including when old bytes or types return. Equal value echoes
and selection-only frames retain run identity. Native callbacks reject unknown,
foreign-owner, invalid-index and stale endpoints before queueing or effects.
Selection-only acknowledgement frames preserve queued replacement ordering;
readonly selection remains available while editing is denied. Disabled and
composing fields do not advertise selection. A legacy selection inside CRLF is
left unchanged and omitted from the native selection snapshot.

A separate observer cap bounds auxiliary text nodes; text/metadata work also
uses the existing observer allowance. Refusal leaves guest admission, rendering
and ordinary input unchanged. Wasm profiles, imports and headless JSON retain
their previous meaning. Visual wrapping/bidi/character geometry, assistive
scrolling/announcements, VoiceOver and other targets remain open.

Scoped verification passes 244 view cases, three runtime assistive cases and
29 desktop GPU cases, strict affected-package GPU all-target Clippy, formatting
and a GPU-capable Debug build. Seven final text cases strengthen Undo/Redo and
type-transition assertions; they are not additional unique cases. The prepared
CPU Debug bundle and the preceding Release bundle both timed out while being
queried through the UI tool. Launch and current native range behavior remain
unconfirmed; no platform success is inferred from the unit tests.
[Pinned scope and source digest](../examples/native-showcase/accessibility-text-ranges-2026-11-03.json).

## 2026-11-03 — Native descriptions and bounded text-alternative observer

The next working increment adds a separate `description` to typed native nodes
and AccessKit from `aria-describedby`, using the existing finite name traversal.
Reference order, hidden-target exceptions, nested label relations, image text
alternatives and embedded password masks share that implementation. Describedby
cycles do not recurse. The Showcase connects its three existing help texts with
explicit ARIA references and keeps names separate from descriptions.

`accessibility_tree` now returns a typed observer result. Numeric name summaries
are shared with the unchanged guest admission calculation; the native observer
separately preflights all semantic/generic names and descriptions within one
8 MiB potential-work allowance. This closes the earlier actionable-generic name
expansion gap without rejecting previously admitted v38 guest batches. Action
validation uses interaction flags and visibility without computing text names.
Observer refusal leaves the instance running, clears stale native actions and
publishes a host-owned static-text explanation. Recovery allocates fresh IDs.
An adjacent semantic-queue fix preserves focus-then-value bursts across a focus
acknowledgement by rebinding only the same native/source identity and still-valid
action to the latest presented frame; worker frame guards remain in force.

Mac API observations cover CPU Debug and Metal Release descriptions, including
reactive `Help` changes. A separate CPU fixture traverses valid → refused → valid
states using ordinary keyboard input for recovery, with an explicit static-text
refusal and unchanged process identity. Scoped checks pass 237 view cases,
2 runtime assistive cases and 25 final desktop cases, affected/final desktop
GPU all-target Clippy and the 12-input Showcase scenario. Native observations
precede the final semantic FIFO refinement; the repeated native retry timed out
in the UI tool, so no passing post-refinement native result is claimed.
[Exact scope and hosts](../examples/native-showcase/accessibility-descriptions-2026-11-03.json).
The earlier
3215-test source reconciliation belongs to the prior code inputs; this increment
uses scoped checks, with the full workspace gate reserved for the code-commit
batch boundary. Full text ranges/line data, assistive scrolling/announcements,
VoiceOver behavior and other targets remain open.

## 2026-11-03 — Native accessibility foundation after `be85c981`

The next UI package adds a shared flat semantic snapshot and desktop-only exact
AccessKit 0.25.2/accesskit_winit 0.34.2 pins. Existing locked versions remain;
48 platform/transitive packages are added, with dependency-boundary regression
checks keeping AccessKit out of engine, runtime, tooling and distribution.
Existing headless JSON, Wasm imports/profile/ABI and guest budgets are unchanged.
Native IDs follow opaque source identity and are never recycled. Lazy snapshots
are published only after the corresponding frame presents. Focus, Click,
string SetValue and ReplaceSelectedText use bounded callback ingress, coalesced
wakes, frame/identity checks and ordinary input/broker/later guest event turns.

Scoped checks pass six core semantic/action cases, one runtime event-turn case,
22 desktop tests in both default/GPU builds and affected GPU all-target strict
Clippy. Real macOS accessibility API observations pass on CPU Debug and Metal GPU
Release, including reactive edits and option actions; Debug also covers Undo,
radio/checkbox and zoom/restore. Release guest JSON/generator/await output is
observed. [Pinned record](../examples/native-showcase/accessibility-checkpoint-2026-11-03.json).
All eight consolidated verification commands pass, including **3215 tests,
0 failures, 16 ignored** and the unchanged conformance baseline. The wrapper
report remains failed: a separate native password-fixture build created an
untracked generated manifest during the run. Moving it under `target` restores
the exact initial source fingerprint; reintroducing only that artifact exactly
reproduces the reported final fingerprint. Completed command results are reused
for identical inputs, with an explicit reconciliation rather than relabeling the
original report. Evidence: `target/verification/20261103-192156-v_2wjssj/report.json`
and `source-reconciliation.json`. Later edits are documentation/observation
records only. The secure native field is also observed masking its value before
and after editing. Full text ranges, descriptions, assistive
scrolling/announcements, complete geometry, screen-reader and other-target
acceptance remain open.

## 2026-11-03 — UI geometry/timing and optional Metal GPU prototype

Working tree after `1259eb8`: standalone browser root/control geometry normalized
under an exact inline-style CSP hash; bounded opt-in timing reports Worker/DOM/frame
opportunity phases. Native input/frame acknowledgement wakes the worker without
lost notifications; timing ends at host presentation return, excluding scanout.
Shared bounded vector commands and frame metadata feed an optional desktop-only
wgpu 30.0.0/Vello GPU 0.3.0 backend; pollster 0.4.0 supplies initialization.
No guest ABI/import change. Normal window presentation has no CPU rasterization
or readback. One outstanding submission, completion timeout and CPU recovery
preserve Wasm state. Metal device-loss recovery and explicit Display P3/sRGB
conversion were observed on the development Mac.

Debug/Release each pass 26 GPU offscreen cases at 1×/2×, including text, selection,
preedit/commit and select popup states, with exact input geometry and mean RGB
delta 0.187–0.477/255. Real GPU Debug checks cover editing/Undo/Redo, scrolling,
select and JSON/generator/async output. A 56-sample Release window observation
records preparation median/p95 1.65/4.46 ms and queue-to-present-return
6.13/19.80 ms. One first-presentation sample is 1.89 s. Release bundles are
24,065,252 bytes CPU and 28,574,180 bytes GPU. These local measurements are
[hash-pinned](../examples/native-showcase/gpu-ui-checkpoint-2026-11-03.json);
readback timings are diagnostic and not window latency. Default backend remains
CPU; broader target/device acceptance, total GPU memory and guest graphics remain
open. Focused checks include 162 browser Node tests, 12 Showcase assertions,
13 renderer/control tests and optional GPU all-target Clippy; the final consolidated
gate passes **3202 tests, 0 failures, 16 ignored**, all eight commands with
unchanged sources. Report: `target/verification/20261103-182413-ec8q70t8/report.json`;
source SHA-256 `009e775316e7cb5c20b627ba05cc95b75b780efe383caffdce10c1191f45482d`. Conformance
remains 123 passed/4 known unsupported, baseline matched and zero regressions.
Only documentation/measurement annotations changed after the gate. The first consolidated attempt stopped on an
HTTP lifecycle test race: request reporting precedes release of the worker permit.
The test now waits for a completed health response after every cancellation;
production limits, timeouts, 48 cancellation samples and memory/descriptor
assertions are unchanged. The original test passes when isolated. Retained-input
checks now keep CPU and vector renderers separate and hold viewport dimensions
fixed through each input sequence, so validation also exercises CPU damage replay.

## 2026-11-03 — CPU renderer, pointer states and TSX Showcase checkpoint

Working tree after `1259eb8`; these changes are not committed. The retained
CPU renderer caches style/geometry preparation, shaping, glyph outlines and
bounded draw commands, replays damage and reuses RGB pixels. The desktop host
coalesces pointer samples separately from activations/keys, supports hover/active
and cursors, preserves stale-input rejection and waits for the focus frame before
typing. Targeted graphics/host dev optimization keeps debug assertions enabled.

Recorded scoped selections pass **43 tests** across common/view/desktop/runtime;
affected-package all-target Clippy passes. Local logs are
`target/hover-{view,matcher,desktop,runtime}-tests.log` and
`target/hover-clippy.log`. Later example-only edits have a successful external-CSS
build and **12 trusted-input assertions** in
`target/showcase-css-dev-{build,verify,package}.log`. These results do not replace
the earlier 3174-test full gate or cover all current inputs as a consolidated gate.

The example uses typed TSX, reactive signals, external `styles.css` and explicit
`.ts` imports. Local macOS Debug/Release bundles exist; the newer Dev bundle uses
the external-CSS artifact. Two pinned benchmark reports retain distinct scopes:
the pre-TSX CPU baseline has 384 reference-equal frames; the later TSX/pointer run
has 192. Both reports predate subsequent example changes and exclude presentation.
At 2× the latter's Debug/Release medians are 8.58/1.66 ms for pointer feedback,
6.68/1.38 ms for the counter and 16.85/10.23 ms for scrolling. Original hashes and
limitations are retained in the [Showcase](../examples/native-showcase/README.md).

The official browser dev host was opened alongside native preview. Chrome
interaction checks cover Ukrainian editing/Undo/Redo, Tab, counter/radio/checkbox,
select and JSON/generator/async output; the native result agrees. Explicit Arial
registration resolves the browser fallback font mismatch. Browser root margins,
height/scroll ownership and field box sizing still differ; native controls lack
OS accessibility exposure. This is a local comparison, without a comparable
input-to-present performance result or broader platform acceptance.

P13a–P13c record future native GPU UI, typed application graphics and 3D-library
port work. They add no implementation, dependency or Three.js compatibility.
Current proposed continuation and acceptance are in the
[UI checkpoint](development-plan.md#current-ui-checkpoint-and-proposed-continuation).

## 2026-11-02 — Browser dev/reload and scoped browser matrix verified

Working tree after `abea3ac`: memory-only loopback browser development now includes
bounded content/probe observations, revision-pinned asset delivery, serialized
off-thread checks/rebuilds, failed-input recovery, bounded text diagnostics and
owned page/Worker cleanup before reload or on server loss. Runtime ceilings and
decisions remain captured inputs; no native guest execution or distribution writes.
Initial-invalid projects still reject startup and synchronous compilation is not
interruptible. Reconnect and full browser/pilot/platform acceptance remain open.

The consolidated package passes **2946 tests, 0 failures, 16 ignored**, all eight
commands with unchanged sources. Report:
`target/verification/20261102-093345-4pi5qr3n/report.json`; source SHA-256
`ababac54e03c6070e9a05140d1d02fdfb6b1d312d607ec600067ef3b1d691a7b`.
Conformance remains 123/127, four known unsupported, zero regressions, matching
baseline and eight untested feature rows. Only result documentation changed after
the gate. Separate evidence includes 85 oracle-enabled Node tests, Chrome's
expanded dev probe and the scoped **63-check** matrix on Chrome 156.0.8078.74,
Firefox 157.0 and Playwright WebKit 27.2. Matrix report/drivers:
`target/browser-matrix-20261102/`. Each engine passes 13 native-accounting fixture
executions, five integrity boundaries and three live dev sequences. Firefox uses
owned app/local-data directories to avoid personal profile state. These are local
engine observations, not installed-Safari or complete pilot acceptance.

## 2026-11-02 — Browser consent, events and navigation verified

Working tree after `abea3ac`: the consolidated package adds native-schema decisions,
bounded audits, platform restrictions, IndexedDB persistence, acknowledged runtime
revocation, first-run/gesture consent, an owned modal, generic named event instances
and per-activation any-HTTPS consent through a checked pause/resume barrier. It
passes **2921 tests, 0 failures, 16 ignored**, all eight full-gate commands with
unchanged sources. Report: `target/verification/20261102-072941-39498ymp/report.json`;
source SHA-256 `52bcc1b91035af6a65a2d8688f43e22435df8e40c167ed0dff00618393dd19df`.
Conformance remains 123/127, four known unsupported, zero regressions and eight
untested feature rows. Only checkpoint documentation changed after the gate.

Independent evidence: 81 oracle-enabled Node tests, real Chrome/native comparisons
of output/accounting/audits, required/optional/parallel consent, persistence and
revocation races, generic host records and generations, and 15 per-address consent
scenarios. Console/network, view, storage, PNG and font regressions also pass.
Navigation opens are spies, never external OS windows. Fixtures:
`target/browser-navigation-final-20261102`; earlier permission/event fixtures retain
their own focused reports in the development plan. This supersedes the prior
2919-test core checkpoint for current implementation, without closing browser dev,
broader control/layout/effects, Firefox/WebKit, native GUI or other OS acceptance.

## 2026-11-02 — Consolidated browser-core verification

Working tree after `abea3ac`: compiler fuel instrumentation and original admission,
verified source-free browser bundles/CLI distribution, Promise/timer/network
adapters, application/view turns, transactional DOM and verified PNG/font resources
have completed their first consolidated workspace gate. It passes **2919 tests,
0 failures, 16 ignored**, with all eight commands and identical before/after source
fingerprints. Conformance remains 123/127, four known unsupported, no regressions,
with eight feature rows still untested. Report:
`target/verification/20261102-031538-rh756ebf/report.json` (local, ignored), source
SHA-256 `17809511c875a150a6695cf3c6c1c0765174bae173c931b935308ba2ee632dbe`.
The earlier attempt stopped at Clippy before workspace tests; the final run follows
the alignment-check style repair and covers the final code. Checkpoint documentation
was updated afterward.

Separate oracle-enabled Node checks pass 46. Real Chrome probes cover console,
async/timers/network/CORS and integrity/termination; view sessions, native accounting,
permissions and cleanup; PNG decoding/pixels and tampering; and original
TTF/OTF/WOFF/WOFF2 fixtures, real glyph metrics, variable/media declarations,
simultaneous/ambient font isolation and abort/deadline/late-load cleanup. These
results do not close Firefox/WebKit, browser dev, permission prompt/audit integration,
full control/layout/accessibility acceptance, native GUI or Linux/Windows/Pi releases.

## 2026-11-01 — Stabilization batch 2

Working tree after `abea3ac`: direct literal-constrained JSON assertions keep
semantic constraints before erasure, scan tags once per field/scalar kind,
validate one selected record and build without an intermediate guest unknown
graph. Conservative misses keep the original parse/assertion and diagnostics.
Literal/unknown tag overlaps use strict matching, retaining valid alternatives
and rejecting ambiguity. Non-schema target fields retain their prior conversion
or diagnostic. Public schema/host/value ABIs and old profiles remain unchanged.

Final full gate: **2825 passed, 0 failed, 16 ignored**, all eight commands passed,
source unchanged during verification. Conformance remains 123 passed and four
known unsupported, eight untested feature rows, baseline matched. Evidence:
`target/verification/20261101-192403-a66359j5/report.json` (ignored, local macOS
arm64). Fresh Node host: 117 passed; UARP console: 107 passed in 28 groups and
two view checks. Nine new runtime tests plus one CLI test cover literals,
fallback/errors, GC/tape relocation, async single evaluation, non-schema
admission, unknown overlap and imported types in source-free Wasm/native files.
The existing 24/200-member dispatch ceiling passes unchanged (12/7 fuel per
preceding candidate). Selected last-tag cases save roughly 27–35% fuel against
only direct assertion decoding disabled; no general throughput claim follows.
Only result documentation followed the gate, with code/build inputs unchanged.
Both stabilization batches remain local and uncommitted; server, platform and
release acceptance remain open.

## 2026-11-01 — Stabilization batch 1

Working tree after `abea3ac`: checked JSON records now keep document order for
ordinary names and numeric array-index order, across generated/shared/host
builders and wide/escaped/recursive inputs. Required-boolean truth guards narrow
record/class result unions under v38; general boolean alternatives remain on
both branches. Source-free Wasm and single-native packaging are covered.

Final local gate: **2815 passed, 0 failed, 16 ignored**, all eight commands
passed, source unchanged during verification. Checked conformance: 123 passed,
4 known unsupported, baseline matched and no regressions. Evidence:
`target/verification/20261101-174930-muu4a2lf/report.json` (ignored, local macOS
arm64). Node host: 117 passed; normal-stack UARP console: 107 passed in 28 groups
and two view checks. Existing fuel goldens remain unchanged. Two older comparison
expectations were corrected for the deliberately different v38 JSON order;
old-profile bytes/behavior are retained. Result-only documentation edits followed
the final gate with code/build inputs unchanged; this batch is not yet committed.
Direct discriminated-union decoding and the rest of the plan remain open.

## 2026-11-01 — V38 review and planning checkpoint

The four-defect review correction batch was committed as `abea3ac`. Its final
full local gate passed all eight commands on macOS arm64: 2809 tests passed,
none failed and 16 were ignored. Checked-v37 conformance matched the baseline
(123 passed, 4 known unsupported, 8 feature rows without cases). The verified
source was committed without further code changes. Local evidence is under
`target/verification/20261101-140907-oznfamiy/`; ignored reports are not shipped
release evidence. Node differential checks passed 117 cases; the normal-stack
UARP console run passed 107 tests in 28 groups and two headless view checks.
No fresh Linux, Windows, real-browser or Pi result is claimed for this batch.

The subsequent documentation-only reconciliation updates the CORE/platform
ledgers and delivery queue; it does not add runtime functionality or close
whole CORE work streams. The superseded dashboard follows to preserve earlier
branch/test checkpoints. Its “next” statements are historical.

### Superseded development-plan dashboard (recorded before reconciliation)

Updated 2026-10-29. Varta is an experimental research project: a compiler and
capability-checked runtime core with K1–K3 implemented in their initial checked
profiles and K4 language/value work in progress. It is not a production release
and does not claim full ECMAScript, Node or npm compatibility.

| Item | Current state |
| --- | --- |
| Profiles | Default `checked-i32-utf8-v1`; latest released opt-in `checked-f64-utf16-v31`; `checked-f64-utf16-v32` is the [pilot batch](development-plan.md#pilot-batch-v32-in-development), merged on the `v32-integration` branch and not released. Versions of every profile: [version matrix](architecture.md#version-matrix) |
| Last full gate | `main`: commit `bf8c6c2` (2026-10-28), 1750 tests. `v32-integration` with both pilot waves: commit `45a6af1` (2026-10-29), 1908 tests. `v34-integration-merge` with the records, unknown and async streams: commit `02d1a75` (2026-10-29), 2018 tests, 0 failures. `v36-merge` with the declared-fields, function-fields, timers and async-generator streams: commit `765743e` (2026-10-29), 2196 tests, 0 failures, conformance 101 passed and 9 known gaps. `v38-wave2-merge` with the seven wave-2 streams: code at commit `911e9ed` (2026-10-30), 2662 tests, 0 failures, 3 ignored, conformance 123 passed and 4 known gaps, baseline matched, including the `no_std` core check. `v38-wave3a-merge` with method subtyping, linked conversions and readers, linked JSON with fair host fuel, KV storage with its fair rates and the 8M v38 per-turn fuel: code at commit `f3649a7` (2026-10-30), 2737 tests, 0 failures, 12 ignored, conformance 123 passed and 4 known gaps, baseline matched, including the `no_std` core check. `v38-wave3b-merge` with exact fuel determinism, compiled JSON decoders, linked serialization and logged text, idle and incremental collection and declared response headers: code at commit `443e13a` (2026-10-31), 2800 tests, 0 failures, 16 ignored, conformance 123 passed and 4 known gaps, on macOS arm64 and, with the same commands (`cargo test --workspace --locked --no-fail-fast`), in a Linux aarch64 container (`rust:1.98.0-slim-bookworm`), identical golden fuel numbers on both. All are the seven-command gate in [verification.md](verification.md#full-gate) on a local host, not cross-platform CI results |
| Compatibility inventory | Current counts and gaps: [conformance/README.md](../conformance/README.md) |
| Completion | Measured by the [core completion ledger](development-plan.md#core-completion-ledger); no work stream is closed by this snapshot |
| Pilot | [uarp-sdk 0.13.2](development-plan.md#pilot-acceptance-package-uarp-sdk) (0.13.1 for the wave-2 integration and wave-3a streams, 0.13.0 for the wave-2 streams, 0.12.0 until the v38 integration); wave-3a integration probe under v38: **65 of 65** files check unmodified (0.13.1 with `typeof` of engine globals: 65 of 65; wave-2 integration: 7, stopped at the `typeof Blob` guard of `core/transport.ts`) ([probe](development-plan.md#pilot-acceptance-package-uarp-sdk)); earlier: M1 in progress: on the `v36-merge` branch `generated/meta.ts`, `generated/models.ts`, `core/json.ts`, `core/pagination.ts` and `core/errors.ts` compile and validate; the other files stop at the Web streams of `core/sse.ts` or a closure capture before initialization in `core/util.ts` ([measurement](development-plan.md#pilot-acceptance-package-uarp-sdk)) |
| Platform targets | Third-party embedding contract published in [embedding.md](embedding.md); browser, desktop and Raspberry Pi 5 targets, application lifecycle and view layer follow the [accepted design](development-plan.md#platform-targets-and-application-model) (2026-10-29), not implemented yet |

Next work, in order:

1. **Suspension foundation** ([K4.4](development-plan.md#k44--suspension-scheduling-and-async-planned)
   block 1): expression normalization, continuation frames over closure cells,
   exception and GC invariants and the closed resume dispatcher are implemented
   compiler-internally and tested in Wasmtime
   ([contract](architecture.md#suspension-foundation-k44-block-1)) and merged
   on the v32 integration branch. Runtime admission under profile v33
   (`resumable_frames`) is implemented; the runtime does not schedule frames
   yet. Synchronous generators (K4.4 step 2, `generators`) are implemented in
   v33 on the `v33-generators` branch
   ([contract](architecture.md#synchronous-generators-generators)).
2. **Pilot M1–M3**, alongside block 1: review and release the v32 pilot batch
   (both waves are merged on the integration branch), then index signatures and
   wider records, Web types (`Headers`, `AbortSignal`) and the 65-file module
   graph budgets.
3. Promise jobs, then async/await (K4.4 steps 3–4): implemented under v34
   and merged with the records and unknown streams on `v34-integration-merge`
   ([scheduler](architecture.md#microtask-scheduler-promises),
   [async functions](architecture.md#async-functions-async_functions));
   step 5's timers and step 6's async generators and `for await` are
   implemented under v36 and integrated on `v36-merge`
   ([timers](architecture.md#timers-timers),
   [async generators](architecture.md#async-generators-async_generators));
   I/O and async HTTP are next.

Completed increment logs, earlier checkpoints and verification records are in
[history.md](history.md).


## Superseded direction text (2026-10-28)

The project direction was restated as a [research thesis](development-plan.md#research-thesis).
The earlier wording follows.

### Former development-plan runtime target paragraph

The product target is a self-contained JS/TS runtime in the same application
category as Deno/Bun, retaining Varta's TS/JS → Wasm → controlled-runtime path.
The current deliberately small checked subset is an implementation stage, not
the permanent product boundary. A JSON server is the first vertical acceptance
scenario, not the definition of a complete runtime.

### Former development-plan active priority

The immediate priority is engine capability, not an editor example or MCP.
The active order is **K1 structured values/JSON → K2 server lifecycle/inbound
HTTP → K3 runtime packaging and end-to-end validation**, now implemented in their
initial checked profiles. K4.1 and K4.2's binary64/aggregate/JSON increments are
implemented; separate undefined-capable and UTF-16 profiles are also available.
K4.2 adds primitive unions and checked optional/missing values, extends unions to references and continues with broader dynamic values and external adapters. JavaScript semantic
compatibility is a cross-cutting requirement, followed by the wider language,
scheduler, standard-library and module/package work below. Generic application
state and AI integration remain later work. Existing C1/C2.1 achievements below are preserved;
they do not dictate the priority of the remaining core work.

A native TS/JS program serving a real JSON object is the acceptance scenario for
these generic capabilities. It is first exercised through compiler/runtime/CLI
regression fixtures, not a domain-specific server implementation inside Varta.
Only after the APIs actually work should a separate application project use them.
An external wrapper around `varta invoke`, or a guest returning a string that a
separate host parses as JSON, does not satisfy this acceptance scenario.

### Former README platform direction

Varta targets typed, contract-driven TS/JS applications for desktop, server and
robot environments. The intended architecture compiles supported source into
portable Wasm and exposes explicitly selected application operations to external
AI agents through MCP. UI, API and agent callers share the application's command
and query layer, business rules and state; connecting another agent does not
create another application state.

The target is a self-contained JS/TS runtime in the Deno/Bun category. The current
small language subset is an implementation stage, not the product's final scope.
Language semantics, memory management, scheduling, standard APIs, modules/packages
and distribution all belong to the core roadmap. Full compatibility is not yet
implemented; the current i32 numeric profile in particular is not standard JS
Number behavior. See the [runtime target](development-plan.md#runtime-target-and-compatibility).

New modules explicitly declare their selected language profile (default `checked-i32-utf8-v1`).
`inspect` reports that profile and whether it was declared or inferred from an
older host ABI v1 artifact. Unknown profiles fail before execution. Generic Wasm
without language/ABI metadata reports an unspecified source language. An opt-in
binary64 profile with typed aggregates and JSON is implemented below. A further
opt-in profile adds distinct `undefined`; `checked-f64-utf16-v1` adds UTF-16 strings.
`checked-f64-utf16-v2` adds closed primitive unions and runtime type checks.
`checked-f64-utf16-v3` adds presence-aware optional fields and missing primitive reads.
`checked-f64-utf16-v4` extends unions to closed records/arrays and adds Array.isArray.
`checked-f64-utf16-v5` adds strict JSON parsing for optional fields and unions.
`checked-f64-utf16-v6` adds bounded resizable arrays with push/pop, at/slice and includes/indexOf.
`checked-f64-utf16-v7` adds explicit primitive throw/try-catch across calls.
`checked-f64-utf16-v8` adds nominal Error objects with checked messages and guards.
`checked-f64-utf16-v9` adds typed TypeError, RangeError and SyntaxError families.
`checked-f64-utf16-v10` makes checked JSON syntax/type failures catchable.
`checked-f64-utf16-v11` adds typed function values and capture-free arrows.
`checked-f64-utf16-v12` adds escaped closures over initialized lexical bindings,
plus map/filter/forEach/find/reduce/some/every as additive compiler operations.
`checked-f64-utf16-v13` adds sparse array presence and hole-preserving map results.
`checked-f64-utf16-v14` adds bounded indexed extension, delete and writable length.
`checked-f64-utf16-v15` adds checked Number exponentiation (`**`, `**=`), using
host ABI v18 with the existing value/schema v12 layouts. It also admits the
documented direct Math intrinsics and non-coercing Number predicates below.
`checked-f64-utf16-v16` adds the twenty deterministic native Math operations
described below, with host ABI v19 and the same value/schema v12 layouts.
`checked-f64-utf16-v17` adds Math.random through host ABI v20, inheriting v16
without changing the value/schema v12 layouts.
`checked-f64-utf16-v18` adds ECMAScript decimal Number formatting in scalar and
union logging through host ABI v21, retaining value/schema v12.
`checked-f64-utf16-v19` adds primitive String(...) conversion through ABI v22
with the same value/schema v12 layouts.
`checked-f64-utf16-v20` adds primitive Number(...) conversion and checked numeric
String parsing through ABI v23, retaining value/schema v12.
`checked-f64-utf16-v21` adds checked direct Number.toString calls in bases 2–36
through ABI v24 with value/schema v12.
`checked-f64-utf16-v22` adds direct Number.toFixed/toExponential/toPrecision
and guest-only Number.valueOf through ABI v25 with value/schema v12.
`checked-f64-utf16-v23` adds global and Number.parseInt/parseFloat calls through
ABI v26, retaining value/schema v12. It also admits direct Boolean(...) and
coercing global isFinite/isNaN as described below.
`checked-f64-utf16-v24` adds truth-based local guards and short-circuit RHS
refinement, sharing host ABI26 / value/schema12 with v23.
`checked-f64-utf16-v25` adds checked Array.findIndex/findLast/findLastIndex,
sharing host ABI26 / value/schema12 and inheriting v24. Rebuild both native
companions for the expanded protocol2 profile list.
`checked-f64-utf16-v26` adds checked Array.reduceRight with optional typed
initial values, inheriting v25 with the same host ABI26 / value/schema12.
`checked-f64-utf16-v27` adds uncontextual array element-union inference and
peer typing for empty nested arrays, retaining host ABI26 / value/schema12.
General dynamic objects and external adapters remain pending. The
[migration contract](development-plan.md#k4--ecmascript-value-profile-and-language-expansion)
preserves the existing artifact and tool semantics.

Primitive Tool Contract v1, offline discovery and checked JSON invocation are
implemented. A broader Application Contract and a long-lived application
service, MCP bridges, package signatures and desktop/database/hardware adapters
remain planned. Closed typed objects, `null` and bounded `JSON.stringify` are
implemented. Dense typed arrays and `varta.json.parse<T>` are also implemented.
Inbound HTTP/1 now supports checked synchronous handlers with isolated request
instances. General dynamic JSON parsing and broader web APIs remain pending;
`net.fetch`/`net.get` are outbound clients.
The [active delivery sequence](development-plan.md#delivery-sequence) now
has delivered the first structured values/JSON, HTTP and independent runtime
packaging profiles. Broader language semantics and standard APIs remain core work;
generic application state and MCP follow those foundations.
A developer-written external server wrapper is an integration option, not the
acceptance target for an HTTP server on Varta.

Portable Wasm requires a compatible host; native launchers target a supported
OS/CPU. Source-free compilation does not guarantee that business logic cannot be
analyzed or modified. Authenticity requires verification by a trusted loader,
while concrete actions always remain subject to host and caller permissions.

### Former README introduction

Varta compiles a small, checked TypeScript subset to WebAssembly and runs it in a
capability-checked Wasmtime host. System access is denied unless the exact
operation and resource are declared. Applications can be distributed as a single
native executable containing portable Wasm and its runtime.

Status: MVP / active development. The [architecture contract](architecture.md)
describes the implemented language, trust boundaries, ABI and limits. The
[development plan](development-plan.md) tracks audit fixes and next milestones.
The [2026-10-28 checkpoint](development-plan.md#status)
records completed core increments, verified test counts, remaining limits and the
next steps for checked record access.

### Former architecture introduction

This document describes the implemented MVP. Future work is tracked separately in
[development-plan.md](development-plan.md). Varta compiles a deliberately small
TypeScript subset to portable WebAssembly and executes it in a capability-checked
Wasmtime host. It does not provide general JavaScript, Node or npm compatibility.
The [2026-10-28 development checkpoint](development-plan.md#status)
records the current delivery state and local verification; the contracts below
remain the source of truth for individual features and profile boundaries.

## Status checkpoints

### Development checkpoint — 2026-10-28

Stage: experimental compiler/runtime core, with K1–K3 implemented in their initial
checked profiles and K4 language/value work in progress. This is a development
snapshot, not a production release or full Deno/Bun/ECMAScript compatibility.
Engine work remains the priority; applications and MCP adapters stay separate.

Completed since baseline commit `526f9ab`:

| Core increment | Implemented boundary |
| --- | --- |
| Object.values for records | Exact/admitted union result types, present-own ordering, shallow identity and rooted cached payloads |
| Object.keys/Object.values for arrays | Ascending own indices, dense results, skipped holes and exact value element types |
| Catch object bindings | Static name/message keys, aliases/defaults, TDZ, fresh captures and cleanup through outer handlers/finalizers |
| String.slice/substring | Optional Number/Undefined bounds, exact UTF-16 units, owned result strings and unchanged Array.slice |
| String.includes/startsWith/endsWith | Required String needle, bounded optional positions, allocation-free unit comparison and unchanged Array.includes |
| Array.findIndex/findLast/findLastIndex (v25) | Typed forward/reverse predicate search, live sparse reads, snapshot bounds and rooted cached matched values |
| Array.reduceRight (v26) | Reverse folds with typed seeded/unseeded accumulators, live sparse presence and rooted values |
| Array literal inference (v27) | One admitted element union, rooted once-only initializers, typed peer empties and unchanged mutable-array invariance |
| BigInt scalars (v28) | Canonical managed leaves, exact arithmetic/conversions/comparisons, recoverable semantic errors and typed width/radix methods; aggregate/union/throw/JSON integration pending |
| BigInt graphs (v29) | Value/schema13, exact reference masks, primitive-union arithmetic, native JSON TypeError and owned BigInt exceptions; BigInt compound assignments and numeric updates; primitive width/radix, update, compound and ordinary arithmetic/unary coercion and six-kind comparison/equality; object ToPrimitive pending |
| Object operations (v30) | Optional-record deletion and dense Object.entries snapshots for closed records/typed arrays; String/primitive own-index enumeration, Object.is/SameValue and checked Object.assign with fresh-record shape preparation; descriptors and general objects remain pending |
| Function codec groundwork | Common value14 tag12, catalog-required function cells, typed graph checks, JSON omission/null rules and immutable per-module/Store catalog ownership; source profile and versioned host operations remain pending |
| Checked compatibility inventory | Pinned v30/ECMAScript 2025 sources, isolated executor, explicit supported/gap/untested counts and reviewed-baseline CI gate |

Earlier additive operations require v12+; sparse/mutation rules remain profile-specific.
The default remains `checked-i32-utf8-v1`. The latest opt-in profile is
`checked-f64-utf16-v30`, using host ABI v28 with value ABI v13 and schema v13.
It adds optional-record deletion and Object.entries for closed records/typed arrays,
inheriting v29 BigInt graphs/primitive unions, JSON TypeError and BigInt exceptions,
v28 checked BigInt scalars and v27 uncontextual array element-union
inference, inheriting v26 Array.reduceRight and v25 Array.findIndex/findLast/findLastIndex and v24
truth-based local narrowing, post-store/targeted-exit joins, proven call effects
and stable callback-input specialization,
write/capture-aware invalidation and short-circuit
RHS guards, inheriting v23
global and Number.parseInt/parseFloat prefix parsers, Boolean conversion and
coercing global isFinite/isNaN, guest truth conversion/control flow and
operand-valued logical operators, inheriting v22
Number.toFixed/toExponential/toPrecision and guest-only valueOf, plus
v21 checked Number.toString in bases2–36 and v20 primitive Number/numeric
String parsing, v19 String conversion and v18 canonical Number output,
v17 OS-backed Math.random, twenty
v16 native Math imports and
v15 Number exponentiation, with no new permission/helper export/value/schema/
container version or limit change.
Fourteen checked Math intrinsics execute as guest instructions in the same profile,
with strict Number arguments and no additional host imports or version change.
Math.pow shares the existing versioned power host operation and its budgets.
Number predicates accept admitted values/unnarrowed unions without coercion,
retaining missing/ignored-argument semantics and the same guest-only boundary.
The source/artifact paths and public engine API are covered by regression tests.

The v28 BigInt scalar increment uses host ABI27 with unchanged value/schema12.
It connects parsed literals, rooted locals/parameters/returns/closures and exact
arithmetic/bitwise/shift operations to the binary mathematical core. BigInt(),
String/Number/Boolean/typeof, primitive comparisons, Number/Undefined-radix
toString, valueOf and explicit Number-width/BigInt-value asIntN/asUintN are covered.
Declared semantic errors are recoverable; malformed transport and resource limits
stay fatal. Structured BigInt tags, arrays/records/unions/throws, JSON behavior,
compound updates and complete coercive/method forms remain pending. CORE-01/07
remain Partial; scalar execution does not close the BigInt work stream.

The v29 increment admits BigInt tag11 only under value13. Managed records, sparse
arrays and reference cells retain exact child masks, including mathematical zero.
Primitive-union arithmetic dispatches cached values without replaying source
expressions; mixed BigInt/Number pairs throw TypeError. Versioned observation and
conversion imports validate complete BigInt blocks and account native work.
JSON validates the bounded graph before a recoverable BigInt TypeError, keeping
malformed metadata/cycles/resource failures fatal. JSON tokens never create a
BigInt; checked unions select a compatible JSON-kind member. Exception channel4
owns uncaught binary payloads and preserves BigInt through catch/finally/GC.
V28 keeps its scalar-only ABI27/value12 boundary. Primitive width/radix coercion
and primitive arithmetic/unary/update/compound coercion are supported in v29
within checked source/storage types.
Object ToPrimitive remains outstanding.
All CORE work streams retain their previous completion state.

The completed v29 comparison/equality block passed 1179 runtime tests at the broad
checkpoint, then seven focused comparison tests after preserving scalar-nullish
fast paths and the UTF16 iterator cleanup, plus a strengthened direct-operand GC
case. Common/compiler suites pass (55/36 tests). Strict common/compiler/runtime
Clippy, the cross-module API, rebuilt runtime companion and source-free CLI/native
cases pass. Current conformance reports 78 cases: 67 supported, 11 known gaps,
seven untested feature rows, with no regressions or infrastructure failures.
The full pre-commit gate is run once for this completed code batch; a local pass
does not prove cross-platform or complete ECMAScript readiness.

V28 scoped verification includes **1117 runtime unit tests** at the broad
checkpoint, followed by **15 targeted BigInt tests** after the final lexical
shadowing test/fix; **99 compiler/common/numeric tests**, **24 conformance runner
tests**, the dependency-boundary test, public virtual-module BigInt execution and
the CLI/source-free Wasm/native artifact BigInt case passed. Workspace all-target
Clippy and formatting passed. The v28 baseline and strict reports agree on
**46 cases: 34 supported, 12 known unsupported, 7 untested feature rows**; strict
exit 1 remains expected. This is scoped batch evidence, not a new full workspace,
full CLI repeat, cross-platform run or BigInt-completeness claim.

Last full checkpoint, before binary transport work, on macOS arm64 with Rust
1.98.0 and locked dependencies: **1582 workspace
tests passed**, followed by **268 CLI tests passed** in a separate repeat run.
Formatting, the core-only library check, runtime-companion build, workspace/all-target
build and Clippy with warnings denied also passed. This records local verification;
it does not claim a new cross-platform CI or release qualification.

Current boundaries remain explicit: 32 record fields, 1024 array elements and a
1 MiB encoded string ceiling, alongside source/type/frame/root/memory/fuel/deadline
budgets. Catch patterns do not admit arrays/rest/other properties or aggregate
throws. String search rejects coercion/regex/omitted needles; includes currently
uses candidate scanning with quadratic worst-case cost, bounded by existing fuel.
Maximum-string stress tests use explicit test fuel without changing runtime defaults.

Completed: **String.indexOf/lastIndexOf** and **Array.lastIndexOf**, including
UTF-16/code-unit index results and their method-specific position rules.
Checked compiler support and allocation-free helpers are available in the
relevant resizable/UTF-16 profiles.

Review follow-up restores the method and compatibility boundaries: Array.lastIndexOf
uses reverse dispatch, distinguishes an omitted position from explicit Undefined,
and clamps/rejects bounds before conversion; Array.reverse uses the complete i64
handle for sparse bitmap access. Trimming excludes U+0085 while retaining the full
ECMAScript WhiteSpace/LineTerminator set. Reinstated v13 regression checks continue
to reject writable length/deletion and trap indexed extension. New tests reproduce
the original failures, verify dense/sparse profiles and execute dependency modules
in source-free Wasm/native artifacts. No ABI, capability or resource-limit change.

Completed: **dynamic String own keys for closed records** in v12+, with exact
UTF-16 lookup, presence-aware results and rooted source/key evaluation.

Completed: **dynamic String own keys for arrays (v13+) and Error families (v12+)**.
Array keys preserve canonical index/length and sparse-presence rules; Error keys
read only own message presence. Key effects, original identity, roots and earlier
profile boundaries are covered without an ABI change.

Completed: **typed computed-key reads on closed records** in v12+. Narrowed String
keys return the admitted union of field types and Undefined through presence-aware
UTF-16 lookup. Key effects run once on the retained original receiver; scalar bits,
reference identity and GC/exception/finalizer lifetimes are preserved. Unsupported
inherited names throw TypeError unless declared own; no prototype access or coercion.
Literal exact types and older-profile rules remain unchanged, with no ABI change.

Completed: **typed computed-key writes on closed records** in v12+. Simple assignment
statements prepare source/key before RHS effects, then check full selected-field
types before storing. Preserve shapes, invariant arrays, Error families, optional
presence/order and root/exception/GC lifetimes. Unknown keys and mismatched values
throw TypeError after RHS success; no new properties, coercion or ABI change.

Completed: **computed record assignment expressions** in v12+, with original RHS
types/values, static computed-literal checks and prepared references through chains.
Saved temporary floors retain earlier operands/pairs and original results through
GC, calls and finalizers. Inner failure skips outer stores; outer failure retains
successful inner writes. No ABI/import/helper/layout change.

Completed: **ordinary local, dot-property and array assignment expressions** in
v12+, sharing prepared references/storage with destructuring. Preserve original
RHS types, local function/capture identity, full result pairs and mixed chains.
Numeric expressions validate after RHS against current backing/presence; v14 adds
checked length expressions, while earlier profile/mutability/type/budget rules persist.

Completed: **checked compound arithmetic assignments** (`+=`, `-=`, `*=`, `/=`, `%=`) in v12+
for locals, static record fields and numeric arrays, plus length in v14. Prepared
references cache old values before RHS, then validate matching Number/String
operands, compute and store with existing type/bounds/ownership rules. Preserve
RHS effects, checked absence errors, partial chains, GC/pairs and fatal limits.

Completed: **dynamic record compound references** in v12+, retaining receiver/key
identity and old-value snapshots across RHS effects. Reuse read-union admission,
strict arithmetic kinds and full selected-field checks, with explicit inherited/
missing timing, partial chains and rooted GC/finalizer lifetimes.

Completed: **short-circuit assignment operators** (`&&=`, `||=`, `??=`) in v12+,
retaining prepared references and evaluating/storing RHS only in the selected
branch. Preserve skipped values/presence, original RHS types/results, current
storage/profile rules, function captures, partial chains and rooted GC/finalizers.

Completed: **checked prefix/postfix increment and decrement** in v12+, preserving
prepared references and actual Number checks, then returning old/new values only
after successful stores. Captured/fresh loop cells, partial effects, exact numeric
bits, optional/sparse absence, length errors and rooted GC/finalizers are covered.

Completed: **checked Number bitwise and shift operations** in v12+, with explicit
full-range ToInt32/ToUint32 conversions, masked counts and signed/unsigned Number
results. Inline host-free conversion preserves source order, ordinary narrowing,
GC/pairs/status and profile/resource boundaries; exhaustive numeric checks pass.

Completed: **bitwise compound assignments** in v12+, retaining prepared references
and old values before RHS, strict runtime Number guards, computed results and
post-RHS selected-field/current-storage checks. Partial chains, absence timing,
GC/pairs/finalizers, unsigned results and fatal/profile boundaries are covered.

Completed: **checked Number exponentiation** (`**`, `**=`) in the opt-in v15/ABI18
profile, with explicit signed-zero/nonfinite/odd-integer rules and bounded pure
native power. Ordinary finite results are implementation-approximated; registry/
profile/signature checks, host-call/deadline budgets and old-profile gates persist.

Completed: **checked Math.abs/floor/ceil/trunc/round/sqrt** in v15, with one
narrowed Number argument, exact rounding/signed-zero rules and guest-only bounded
execution. All-exponent numeric checks, argument effects/roots, profile/type/
shadowing gates and source-free artifact paths are covered without an ABI change.

Completed: **Math.min/max, sign, fround, clz32 and imul** in v15, with explicit
arity, exact integer conversions/products, binary32 rounding and NaN/zero rules.
Variadic argument order, exceptions, roots and pure guest execution are covered.

Completed: **Number.isFinite/isNaN/isInteger/isSafeInteger** in v15, accepting
admitted values and unnarrowed unions without coercion. Missing/non-Numbers return
false; ignored extra arguments retain effects/errors. Exact numeric boundaries,
union tags, pairs/roots/finalizers and artifact/permission paths are covered.

Completed: **all Math and Number numeric constants** in v15: exact binary64 values,
dot/literal-key reads, ordinary lexical shadowing and read-only write diagnostics,
with no import/allocation/ABI change. Source-free and public-API paths are covered.

Completed: **Math.pow** in v15 as a two-Number intrinsic over the existing power
host operation, with operator/method edge-case bits, one-call accounting, argument
order and helper-purity checks.

Completed: **Math.f16round** in v15, with direct binary64-to-binary16 ties-to-even
rounding through exact guest rescaling. Every binary16 encoding and finite midpoint
neighborhood, both signs, all f64 exponents and a binary32 double-rounding example
are checked against an independent integer-significand oracle. No new host/ABI/limits.

Completed: **Math.hypot** in v15, with all-argument Number snapshots, exact
power-of-two scaling, compensated products/sums and root refinement in guest code.
Infinity wins over NaN after all argument effects; empty/all-zero calls return +0.
Exponent-wide comparisons, exact scaled triples/subnormals, high-precision overflow
goldens, compensation, roots/pairs/finalizers and source-free/grant paths are covered.
Finite results remain implementation-approximated; no new ABI/import/budget.

Completed: **nineteen unary native Math methods and atan2** in v16/ABI19,
with shared typed registry names/signatures, exact required edge cases and bounded
platform-approximated finite math. One-call/deadline/status rules, inherited values,
helper purity, hostile metadata/signatures, GC/pairs and source-free/public APIs are
covered. Value/schema12, default/older profiles and resource limits remain unchanged.

Completed: **Math.random** in v17/ABI20, using a fresh OS word per call and exact
53-bit sampling in [0,1). No shared PRNG/seed/cache is persisted; ignored argument
effects, entropy failures, limits, status, roots and source-free artifacts are
covered. The function remains separate from the deterministic native registry.

Completed: **ECMAScript decimal Number output** in v18/ABI21 for scalar and union
logging. Shortest decimal/scientific text and zero/nonfinite rules use the existing
pinned formatter; old profiles retain their text. Exponent/threshold/minimality,
registry/budget/GC/status and source-free paths are covered.

Completed: **primitive String(...) conversion** in v19/ABI22 for all admitted
primitives/unions, absent/ignored-argument rules, exact UTF-16 and owned results.
Status, roots, buffers, limits and old bindings/profiles retain their contracts.

Completed: **primitive Number(...) conversion** in v20/ABI23, providing the numeric
argument foundation for Number.prototype.toString. Strict numeric String grammar,
exact whitespace, decimal/prefixed rounding, signed zero/nonfinite/empty rules and
ignored effects/roots/status/budgets are covered without new limits or dependencies.

Completed: **checked direct Number.prototype.toString** in v21/ABI24, bases2–36,
primitive radix/default/ignored argument rules and catchable RangeError before
nonfinite cases. Exact midpoint intervals select shortest native text; decimal
uses the existing formatter. Bounds, independent rational checks, roots/status/
limits and source-free paths are covered; num-bigint0.4.8 gains direct runtime use.

Completed: **Number.toFixed/toExponential/toPrecision and valueOf** in v22/ABI25.
Exact bounded decimal rounding, missing/Undefined arguments, method-specific
RangeError ordering, guest-only identity and ignored effects are covered alongside
roots/status/budgets, independent Decimal goldens and source-free/public paths.

Completed: **Number.parseInt/parseFloat and their global forms** in v23/ABI26.
Primitive String/radix conversion, exact longest-prefix grammar/whitespace, Int32
radix wrapping and bounded nearest/even integer rounding preserve status/buffers,
roots/pairs and fatal budgets. Independent integer goldens and source-free/public
paths cover this boundary.

Completed: **Boolean conversion and coercing global isFinite/isNaN** in v23,
without an ABI change. Guest-only Boolean accepts ordinary values/unions including
references; numeric predicates reuse primitive Number conversion after cached
argument effects. Numeric exponent, roots/pairs/status/budget, helper-purity and
source-free/public paths are covered.

Completed: **guest ToBoolean across conditions, loops, ternaries, ! and existing
predicate callbacks**, plus operand-valued &&/||/?? and logical assignment tests.
Cached operands, selected-branch type narrowing, numeric bits/reference identity,
roots/pairs/status/limits and source-free/public paths are covered without ABI changes.

Completed: **truth-based direct-local narrowing and RHS guards** in opt-in v24,
sharing ABI26/value12/schema12 with v23. False primitive possibilities, scoped
compound/loop facts, write/capture invalidation and redundant guards
have regression coverage alongside metadata, roots/status and source-free paths.
V23 keeps its earlier truth-branch mutation admission.

Completed: **write-aware refinement invalidation and capture dataflow** in v24.
Per-binding revisions preserve alternative branch input and discard written facts
at joins; loop/test/callback effects prevent stale narrowed reads. Mutable captures
retain declared cell types and const proofs propagate through nested closures.
Compiler-only swc_ecma_visit29.0.0 adds bounded effect traversal; runtime dependency,
ABI, capability and limit boundaries are unchanged. Assignment/loop/capture/GC/
status/diagnostic and source-free/public paths have regression coverage.

Completed: **post-store local type inference and normal branch joins** in v24.
Stores preserve observed types before widening into declared storage; if/ternary
joins combine normally completing paths, including subunions and surviving early
return/throw guards. Logical targets use selected result types. Skipped RHS/default
stores, loop continues, switch direct entries/fallthrough, named breaks and
exception prefixes cannot leak facts. No ABI/layout/production budget changes.

Completed: **completion-targeted loop/switch/label flow** in v24. Reachable breaks
and continues carry types to their actual targets; updates/do-while tests join
normal/continue inputs, and loop exits combine false tests with matching breaks.
Finalizers transform or cancel pending exit facts in unwind order. One-time for
initializers precede repeated-only mutation analysis, fixing stale second-test
assumptions. GC, nested targets, overrides, diagnostics and source-free/public
execution have regression coverage with unchanged ABI/layout/runtime budgets.

Completed: **proven call effects for local cells** in v24. Immutable function
provenance and transitive closure summaries preserve unaffected captured facts;
module calls account for supplied callback proofs. Unknown/mutable/returned/self/
parameter targets stay conservative. Argument writes, guards/loops, shadowing, GC,
denial status and source-free/public paths are covered without runtime/ABI changes.
This does not admit function-valued aggregates or prove property immutability.

Completed: **stable callback-input specialization** in v24 closure summaries.
Symbolic callback positions bind actual arguments through forwarding, const copies,
read-only captures and default/rest/receiver prologues. Direct/nested parameter
replacement retains unknown reentry. Separate committed writes from local flow
invalidation; shared-depth AST guard/loop effects, GC, diagnostics and source-free/
public paths preserve ABI/layout/resource contracts. Higher-order opaque inputs
and point-sensitive mutable/returned provenance remain conservative.

Completed: **Array.findIndex/findLast/findLastIndex** in opt-in v25, sharing
ABI26/value12/schema12. Typed callbacks receive live values (including Undefined
for holes), indices and the original array. Reverse searches, snapshot bounds,
mutation/deletion/truncation, cached matched values, thisArg and ignored argument
effects preserve roots, capabilities, system status and fatal limits. Existing
profiles reject the new methods. Generic array-like/prototype calls and other
array APIs remain unfinished. See the [predicate-search contract](architecture.md#checked-f64-utf16-v25--predicate-array-searches).

Completed: **Array.reduceRight** in opt-in v26, sharing ABI26/value12/schema12.
The fold visits present indices in descending order, uses an explicit typed seed
or the rightmost present element, and throws recoverable TypeError for unseeded
empty/all-hole arrays. Live mutation/presence, source/callback/seed/ignored-argument
ordering, accumulator inference/identity, callback roots and fatal/resource/capability
boundaries have regression coverage. V25 and earlier reject the new method;
existing reduce admission stays unchanged. See the
[reverse-fold contract](architecture.md#checked-f64-utf16-v26--reverse-array-folds).

Completed: **initial checked compatibility inventory and isolated runner**.
The pinned ECMAScript 2025/v30 corpus has 40 feature rows and 78 original checked
programs: 67 supported, 11 known unsupported, with 7 feature rows untested. Source
SHA-256 checks, complete corpus listing, strict metadata, default runtime budgets,
empty authority and per-process deadline/output bounds preserve explicit results.
Baseline CI distinguishes expected gaps from conformance passes; strict mode
currently fails. The suite exposed uncontextual array union inference as a
separate CORE-01/02 gap; v27 closes that case and adds four positive cases. This advances CORE-15 to Partial and does not close it;
upstream Test262 execution and broader coverage remain required. See the
[runner contract](../conformance/README.md).

Completed: **uncontextual array literal inference** in opt-in v27, sharing
ABI26/value12/schema12. Source-order expressions are lowered once, admitted union
members flatten into one final element type, and existing boxing/rooting preserves
Number bits, reference identity, holes and captured-cell effects. Empty/all-hole
array peers can use typed array evidence without side-effect reanalysis; existing
aggregate contexts and explicit annotations remain strict. Conflicting record/
array kinds, Function/opaque elements and untyped empty graphs remain unsupported.
The old profile still rejects the formerly unsupported `[1, undefined]` case;
the v27 compatibility baseline now records it as supported. Source/type/root/fuel
and function-body limits remain unchanged. See the
[inference contract](architecture.md#checked-f64-utf16-v27--array-literal-inference).

Next architectural block: the
[suspension foundation](development-plan.md#k44--suspension-scheduling-and-async-planned)
(expression normalization, continuation frames over closure cells, exception/GC
invariants and a closed resume dispatcher), after the prepared-catalog increment
is committed. Generators, Promise jobs and async/await build on it in that order.
Priorities are checked against the [uarp-sdk pilot](development-plan.md#pilot-acceptance-package-uarp-sdk);
its async-independent steps M1–M3 can proceed alongside that block.

Other open work: **higher-order callback-argument binding, property/exception effects, initializer
inference and loop fixed points**, then
the broader object/prototype and conversion foundation. Remaining
array APIs remain tracked under CORE-02; existing filter/find/some/every
callbacks use the new truth path. General
object-to-primitive String/Number conversion follows the object/prototype foundation.
General Math namespace objects, coercion and borrowed methods remain separate language work.
Remaining work also includes generalized record reads/writes/coercion, constructors and prototypes,
aggregate exception design, scheduler/promise jobs, standard/system API expansion
and package compatibility. Wider dynamic values remain explicit profile work.
Shared application state, MCP, signing and external adapters remain later
milestones. Detailed completed contracts and sequencing follow.

## Pilot measurements

Superseded uarp-sdk pilot measurements, newest first; the current one is in the
[development plan](development-plan.md#pilot-acceptance-package-uarp-sdk).

**Verified behavior with v32, second wave (2026-10-29).** The same probe on the
integration branch with all nine v32 selectors (default exports, interface
inheritance, generics and Error subclasses added): each of the 65 source files of
the pinned tarball was compiled with `varta check` under `checked-f64-utf16-v32`
as the only `export *` source of a probe entry, so the file and all its
dependencies form the graph; production budgets were unchanged. The first
diagnostic per file:

| First diagnostic | Files |
| --- | --- |
| Compiles and validates: `generated/meta.ts`, `generated/models.ts`, `core/json.ts` | 3 |
| `core/errors.ts:121` `unsupported type reference Headers` (reached through `core/resource.ts`, `core/transport.ts`, `core/sse.ts` and most resources) | 36 |
| `core/pagination.ts:27` `unsupported async or generator generic function` (`autoPaginate<T>`, an `async function*`, reached by paginated resources) | 23 |
| `core/util.ts:21` `unsupported type reference AbortSignal` (`sleep`) | 1 |
| `generated/resources/index.ts`, `index.ts`: module graph exceeds the import-declaration limit | 2 |

Compared with the first wave, `CursorPage<T>` and the uninstantiated generic
`pick<T, K extends keyof T>` now pass, so pagination stops at the async generator
(K4.4) and `core/util.ts` at Web types; `index.ts` passes its `export default` and
stops at the graph limits (65 modules and about 370 relative import/re-export
declarations against 64 modules and 256 declarations). With `Headers` stubbed,
`core/errors.ts` still stops first at the index signature in `ProblemDocument`,
so its `class … extends Error` hierarchy (now in the v32 subset, except
`unknown`-typed causes) is not yet reached in the file itself. Instantiating `pick` would next need `Record<string, unknown>`,
`unknown`, an `as` assertion and computed access with a `keyof` key, all still
rejected.

## Completed milestones and follow-ups

### Completed milestone: architectural hardening

The first milestone is a checked, bounded single-file TypeScript subset with
capability-controlled execution and one-file native packaging. The implemented
contract is in [architecture.md](architecture.md).

The October 2026 architecture audit is addressed as follows:

| Audit finding | Implemented change | Regression evidence |
| --- | --- | --- |
| Untrusted native deserialization | Native AOT entry points removed; portable Wasm only | Rejection through runtime and all artifact commands |
| Filesystem boundary and check/open race | Component-aware scopes and pinned no-follow descriptors | Allowed file, sibling, traversal, final/intermediate/internal symlink cases |
| Malformed manifest grants expand rights | One strict schema and operation-specific grant types | Wrong/missing fields and unknown operations rejected |
| HTTP request injection | Parsed authorized URLs and bounded HTTP library | CR/LF denied before I/O, normalization and authority tests |
| Unbounded resources | Store memory limits, fuel, epoch deadline, I/O/output/call budgets | Infinite loop, guest memory.grow, output and oversized file tests |
| Function type order mismatch | Signature interning shared with emission | Mixed-arity functions execute correct results |
| Silent semantic changes | Lexical scopes, typed IR, structured branch labels | Shadowing, nested switch, default/fallthrough, continue and top-level-effect rejection |
| Artifact validation misses invalid code | Full Wasm/ABI validation before publish and every artifact command | Invalid operand stack and missing ABI globals rejected |
| Divergent check/build/dev | Shared project pipeline and deterministic internal subset typing | Identical errors for all three commands |
| Stale string length and sentinel errors | Reusable result buffer, cleared result globals, status API | Success then denial, structured status, bounded output |
| Container marker collisions | Fixed versioned EOF footer with checked lengths | Marker literals survive native packaging; overflow lengths rejected |
| CLI reports success after failure | Central command error/exit handling | Run, inspect and validate return code 1 on bad input |

Additional hardening: source complexity is bounded before recursive parsing;
syntax errors return typed diagnostics instead of a dropped-diagnostic panic;
workspace code forbids unsafe; operation/signature metadata has one owner;
filesystem devices are not readable; proxies and redirects are disabled;
unsupported compat and HTTPS configurations fail explicitly.

Quality gates for a completed code batch (see the
[scoped verification map](verification.md) for the development loop):

```sh
python3 scripts/verify.py full
```

This runs formatting, isolated core/companion builds, strict workspace Clippy,
all-target build, workspace tests (including CLI once) and the reviewed conformance
baseline. Local and feature checks accompany intermediate implementation work;
launcher/packaging changes additionally repeat the CLI suite. Documentation-only
and verification-tooling changes use the checks specified in the verification map.

The CI workflow runs the gates on Linux and macOS. Tests are hermetic and use
synthetic sources, process-local environment values and temporary files; they do
not need a network service or external TypeScript checker.
### Completed follow-up: host policy ceilings

`run`, `dev`, `inspect`, `validate` and packaged launchers accept an explicit
`--policy` file. Runtime authorization requires both package and host filters;
empty ceilings deny all privileges and malformed files never fall back. File
access uses pinned host directory roots, including when a package supplies a
symlink as its own root. Inspection reports the two scopes without approximating
wildcard intersection. Host policies are not persisted into artifacts.

Regression coverage includes environment/path narrowing in both directions,
operation independence, scoped network authorization without network I/O, boolean
permissions, malformed/missing files, deny-all packaged execution and symlink-root
escapes. The public runtime and project services expose the same constrained-run
contract as the CLI.
### Completed follow-up: source diagnostics

The shared compiler source context preserves real entry filenames and precise
semantic ranges through expression/function IR. Human diagnostics show bounded
excerpts and carets; `check --json` exposes typed categories, UTF-8 byte offsets
and Unicode line/column ranges. Nested expression errors retain their original
locations. Virtual source paths are supported without file I/O, and naming a
source does not change its generated bytecode.

Regression coverage includes expected/actual type errors, nested calls, unsupported
annotations, Unicode/CRLF/BOM coordinates, parser/resource-limit errors, bounded
large-line excerpts, static-data overflow, config-relative entry paths and
consistent diagnostics across check/build/dev. Valid named-source modules are
instantiated and executed through Wasmtime as well.
### Completed follow-up: local modules

Named function imports/exports and aliases now link through an explicit,
deterministic dependency graph. Functions retain file-local scopes and source
diagnostics, imported calls retain their types, and dependencies are embedded
into portable/native builds. `check --json` exposes the graph for inspection.
Compiler embeddings supply a loader; project source loading stays within the
configuration directory through pinned no-follow descriptors. Cycles and
unsupported initialization/import forms fail explicitly. Module count, imports,
depth, total/per-file bytes and global function count are bounded.

Regression coverage includes shared dependencies, alias forwarding, recursive
library functions, private name isolation, imported type errors and static-data
diagnostics, deterministic bytecode, graph budgets, missing/private exports,
path escapes, symlinks, and execution after deleting sources. Imported host calls
obey both application grants and host ceilings. ABI and packaging are unchanged.
### Completed follow-up: retained string values

Strings now work in inferred/annotated local bindings, assignments, parameters,
explicitly annotated returns and dynamic host arguments across module boundaries.
Literal slices are static; host results are copied immediately into an immutable,
bounded execution arena. Saved values survive host-buffer reuse, nested calls and
reassignment. Full Wasm value types determine signature interning and local slots.
The raw host ABI, capability model and artifact versions remain unchanged.

Regression coverage includes Unicode/empty values, recursive and mixed-signature
functions, scope/control-flow slots, cross-module returns, native and Wasm runs,
multiple host arguments, dynamic environment/filesystem permission denials,
host ceilings, fuel exhaustion, and bounded heap behavior with additional
test fuel. String operators and reclamation are covered by the follow-ups below;
implicit string coercion remains unsupported.
### Completed follow-up: programmatic host errors

`varta.result.capture` now executes a direct privileged host call once and retains
an immutable typed `VartaResult<T>` containing `.ok`, `.status` and `.value`.
Values and failures stay paired across later host calls, aliases, reassignment,
recursive calls and module boundaries. String payloads keep their safe snapshot
ownership; the result itself uses a bounded two-value Wasm representation with no
record allocation. Function annotations, parameter limits, physical local slots
and multi-value returns are checked consistently. Shared editor declarations
document the API. Existing host imports, policies and artifacts remain compatible.

Regression coverage includes empty/false success, all five status codes, retained
Unicode payloads, nested call evaluation, readonly fields and type errors,
parameter budgets, identical check/build/dev rejection, Wasm/native execution,
host ceilings, denied network/process calls and authorized process exit. Resource
traps remain execution failures; capture does not introduce general exceptions.
### Completed follow-up: string comparison and concatenation

String `+` now creates immutable concatenated values, with empty-operand sharing,
left-to-right single evaluation and bounded allocation. Equality compares content;
ordering uses UTF-16 code units without normalization or locale collation.
Both operands must be strings. Host snapshots and concatenations share the same
checked arena; each string is bounded to 1 MiB of UTF-8 bytes. Comparisons require
no allocation, and pure string helpers do not affect saved or live host status.
Helper types/indices are resolved conditionally, including with the maximum user
function count. Host imports, capabilities, ABI and packaging are unchanged.

Regression coverage includes all eight comparisons across 484 pairs of empty,
NUL, ASCII, combining and BMP/supplementary Unicode samples against UTF-16 order;
nested/cross-module concatenation, retained host/captured values, operand evaluation
order, native/Wasm execution, diagnostic ranges, constructed resource denials,
per-value limits, bounded heap behavior, growth failures and comparison fuel exhaustion.
### Completed follow-up: string memory reclamation

Owned strings now use a bounded, nonmoving mark/sweep heap with block reuse,
splitting and adjacent-free coalescing. Collection runs before memory growth;
dead trailing blocks can lower the logical heap end. Committed Wasm
pages stay available for reuse until the instance is dropped. Static data and the
host result buffer remain separate. Host signatures, capabilities and artifacts
stay compatible.

Compiler-managed roots preserve bindings, aliases, string result payloads,
expression operands and call arguments. Scope exit, overwrite, `break`, `continue`
and return remove roots at the appropriate boundary. A fixed 64 KiB root stack
bounds frames and temporaries; root overflow, all-live heap exhaustion and failed
growth remain controlled execution failures. Collection and bookkeeping consume
guest fuel. Bindings are conservatively retained to scope exit; compaction and
last-use optimization are not implemented.

Regressions cover copying more bytes than fit in the heap with bounded committed
memory, nested expression/function transfers, recursive captured results,
middle-block aliases, free-block splitting/coalescing, loop/switch exits, static
and host buffer isolation, per-value limits, live-set exhaustion, root capacity,
fuel, and real CLI/Wasm/native behavior with unchanged permission denials.
### Completed follow-up: engine and tooling boundaries

- Core contracts no longer contain project path/output configuration.
  `VartaConfig` and `CapabilitySpec` now belong to `varta-project`.
- Runtime inspection returns backend-independent metadata; project tooling no
  longer imports Wasmtime, and CLI has no direct runtime dependency.
- Removed the production empty-module test helper and unsupported port-list
  permission types. Unsupported grant shapes continue to fail closed.
- Removed cancelled application-model files and empty abandoned directories.
  No domain models or application profiles were added to engine production code.
- A Cargo dependency-graph regression guards layer direction; a public API
  integration test uses the engine without project tooling or a CLI process.
  CI separately builds core libraries.

The repository contains engine libraries, their development tooling, tests,
documentation and CI. `target` is ignored build output. Native packages now use
the independent `varta-runtime` companion; dependency guards cover artifact I/O
and launcher crates as well as engine libraries.

## Increment logs

### BigInt compound assignment increment (v29)

BigInt targets and admitted primitive unions reuse checked scalar/union arithmetic
for compound assignments. Prepared receivers, keys and old values precede RHS
effects; failed operations do not store. Five new runtime cases cover all eleven
signed arithmetic/bitwise operators, unsigned-shift TypeError, division RangeError,
array-backing changes, sparse/optional fields, union dispatch and v28 rejection.
The public API and source-free artifact cases also exercise an array compound store.
At that checkpoint, full coercive built-ins and increment/decrement remained
pending; the numeric-update increment below advances the latter. CORE-01 stays Partial.

Scoped verification of this increment: 51 compound-assignment runtime tests and
19 BigInt graph/ABI/GC tests pass (overlapping selections), plus one cross-module
public API test and one CLI test covering development/module execution and
source-free Wasm/native artifacts; the launcher build passes. Strict compiler/runtime
all-target Clippy, formatting and diff checks pass. The reviewed conformance run reports 53/41/12 total/pass/known gaps,
no unexpected support, regressions or infrastructure failures, and seven untested
feature rows. These focused results do not replace the pre-commit full gate.
### BigInt numeric updates (v29)

Prefix/postfix increment and decrement preserve exact BigInt values and return
the old/new numeric value after a successful checked store. Number|BigInt unions
select the corresponding 1 or 1n unit after caching the input once. Prepared
receivers/keys, sparse and optional presence, mutable backing resolution and
rooted old/new results retain the existing update contract. V28 still rejects
BigInt updates. At this checkpoint primitive ToNumeric coercion remained pending;
the later primitive-update increment below adds it within checked storage types.
CORE-01 remains Partial, with coercive built-ins and broader primitive semantics
still outstanding.

Scoped verification of numeric updates: 23 update-related runtime tests and 25
BigInt graph/ABI/GC tests pass (overlapping selections), including six new numeric
update cases. The cross-module public API case and CLI development/source-free
Wasm/native case pass with both prefix and postfix array updates. Strict
compiler/runtime all-target Clippy, formatting and diff checks pass. The reviewed
corpus at that checkpoint had 54 cases: 42 supported, 12 known unsupported, zero
regressions, unexpected support or infrastructure failures, and seven untested
feature rows. Inventory and lock digests matched the files at that checkpoint.
This is scoped evidence; the
full pre-commit gate has not been repeated for this increment.
### BigInt primitive width/radix coercion (v29)

ABI28 adds pure bigint_width_v2 and bigint_to_string_v2 checked imports accepting
canonical v13 primitive cells. Source arguments, including ignored extras, are
evaluated once before conversions. Width methods apply ToIndex before ToBigInt:
String/Boolean values are admitted, Number values throw TypeError even at width
zero, malformed integer strings throw SyntaxError, and invalid widths throw
RangeError first. Radix defaults only for undefined; other primitives undergo
implicit ToNumber, so BigInt radix throws TypeError and out-of-range/NaN radix
throws RangeError. Both transports are validated before a recoverable failure;
malformed payloads/resources stay fatal, fuel is charged and pure status preserved.
V28 retains its Number-width/BigInt-value and Number/Undefined-radix admission.
At this checkpoint object coercion, Symbol and primitive update coercion remained
pending; primitive updates are implemented in the later increment below.

The unchanged coercive-width corpus gap was observed as unexpected support with
exact expected output before promotion. A new radix/error case extends the
inventory; broad ECMAScript compatibility and CORE-01 completion remain unproven.

Verification of primitive BigInt methods: 1149 runtime unit tests, 55 common
and 36 compiler tests pass, including five new source-coercion tests and the
new malformed-transport ABI test (31 focused BigInt graph cases). Three primitive
conversion/error scenarios also match normalized Node.js v24.20.0 output exactly.
The cross-module API and CLI development/source-free Wasm/native cases pass with
coercive widths and radices; the updated runtime companion builds. Strict
common/compiler/runtime all-target Clippy, formatting and diff checks pass.
Baseline and strict JSON reports are identical: 55 total, 44 supported, 11 known
unsupported, zero unexpected support/regressions/infrastructure errors and seven
untested feature rows. Strict mode exits 1 as expected; inventory and lock digests
match the current files. No new full workspace or cross-platform gate is claimed.
### Primitive update ToNumeric (v29)

Increment/decrement now converts String/Boolean/Null/Undefined to Number and
retains BigInt exactly, including the converted numeric old value for postfix
expressions. Each source read occurs once; metered PrimitiveNumberV2 handles
non-numeric primitives. Number-capable optional fields and sparse array entries
can become NaN with successful stores publishing presence/order only afterward.
Declared mutable storage types remain invariant: computed numeric union results
are checked before storing, and postfix old values are narrowed only after a
successful store proves their numeric kind. Missing BigInt-only entries still
throw TypeError without mutation; explicit String-only storage cannot accept a
Number update. Dynamic record guards preserve each selected field type.
V28 and older profiles retain noncoercive updates. Object ToPrimitive and general
inferred storage widening remain pending; CORE-01/02/03 stay Partial.

Verification of primitive update ToNumeric: seven new runtime tests pass,
covering all admitted primitive kinds, refined stores, sparse/present fields,
dynamic record field invariants, BigInt-only storage and v28/object rejection,
and fatal native parsing fuel exhaustion. The affected update selection passed
34 tests before the additional one-test fuel run; 31 BigInt graph/ABI/GC tests
and 36 compiler tests also pass. Cross-module public API and CLI source-free
Wasm/native cases exercise primitive updates through the updated module helper.
Strict compiler/runtime all-target Clippy, formatting and diff checks pass.
Two pinned conformance cases match normalized Node.js v24.20.0 output. Baseline
and strict reports match exactly: 57 total, 46 supported, 11 known unsupported,
zero unexpected support/regressions/infrastructure failures, seven untested rows;
strict exits 1 as expected and current inventory/lock hashes match. No new full
workspace/runtime/CLI or cross-platform pass is claimed.
### Primitive compound assignment coercion (v29)

All arithmetic/bitwise compound assignments now cache the old value and RHS
before primitive conversion. Addition chooses concatenation when an actual
operand is String; other cases normalize to Number/BigInt, retaining exact BigInt
and mixed-numeric TypeError. Six-member primitive operands normalize once before
a maximum two-by-two numeric dispatch, preserving existing body/root limits.
Both native String/Number conversions use the metered v2 imports. Always-throwing
mixed pairs do not invent a BigInt result type, so Number targets can propagate
the intended TypeError instead of failing static assignment admission.
A shared checked result guard narrows only successful values compatible with
declared storage. Dynamic record fields retain their individual types, and
failed stores preserve RHS effects without publishing a new target value.
Number-capable holes can become NaN, while a String RHS can concatenate an
undefined sparse read into a valid String result. Typed array/field invariance,
source order, UTF16/roots, resource ceilings and v28 rejection are preserved.
Ordinary unary/binary coercion, object ToPrimitive, Symbol and general inferred
storage widening remain outstanding; CORE-01/02/03 stay Partial.

Verification of compound primitive coercion: all 1165 runtime unit tests and
36 compiler tests pass, including nine new cases covering primitive operators,
six-member operands, checked sparse/dynamic stores, mixed-numeric failures,
GC across RHS replacement and fatal native conversion fuel exhaustion. The
cross-module API and CLI development/source-free Wasm/native cases pass with
coercive multiplication and String/BigInt concatenation. Strict compiler/runtime
all-target Clippy, formatting and diff checks pass. Three new pinned corpus cases
match normalized Node.js v24.20.0. Baseline and strict reports are identical:
60 total, 49 supported, 11 known unsupported, no unexpected support/regressions/
infrastructure errors, seven untested rows; strict exits 1 as expected. Current
inventory and lock hashes match. This affected-runtime pass replaces the older
1149-test runtime checkpoint for current lowering; full workspace/CLI repetition
and cross-platform verification have not been performed for this increment.
### Ordinary primitive arithmetic and unary operators (v29)

Ordinary +, -, *, /, %, ** and bitwise/shift expressions share the cached
primitive arithmetic engine with compound assignments. Addition chooses actual
String concatenation; otherwise operands become Number/BigInt, with checked
mixed-numeric TypeError and exact BigInt operations. Scalar Number and String
paths avoid redundant native conversions. Six-member primitive unions normalize
before bounded numeric dispatch, preserving source order and current limits.

Unary +, - and ~ admit primitive conversion. Minus/not retain BigInt; unary plus
uses the Number result domain and throws TypeError for BigInt. The native
BigInt unary operation still validates canonical leaves and meters work before
that recoverable error, so malformed/resource failures bypass guest handlers.
GC, sparse reads, signed zero/NaN/Infinity and once-only source effects are covered.
V28 and older source admission stays frozen. Comparison/equality semantics,
object ToPrimitive, Symbol and general inferred storage widening remain pending;
CORE-01/02/03 stay Partial.

Verification of ordinary primitive arithmetic/unary: eight new runtime cases
pass, including canonical BigInt unary-plus corruption bypassing catch/finally,
wide unions, once-only effects and GC after RHS replacement. Affected selections
pass 72 primitive-related, 50 BigInt-related and seven update-coercion tests
(overlapping), plus 36 compiler tests. The cross-module API and CLI development/
source-free Wasm/native cases cover arithmetic and unary conversion, including
Number-domain BigInt TypeError. Strict compiler/runtime all-target Clippy,
formatting and diff checks pass. Three new pinned cases match normalized
Node.js v24.20.0. Baseline and strict JSON reports agree: 63 total, 52 supported,
11 known unsupported, zero unexpected support/regressions/infrastructure errors,
seven untested rows. Strict exits 1 as expected; current inventory and lock hashes
match. The older 1165-test runtime pass remains historical evidence; no new
full runtime/workspace/CLI repeat or cross-platform pass is claimed.
### Primitive comparison/equality block (v29)

All eight equality/ordering operators support the six admitted primitive kinds.
Strict equality retains type distinctions, NaN inequality and signed-zero equality.
Abstract equality implements nullish pairs, Boolean conversion, String/Number
conversion and exact BigInt/String/Number rules. Ordering compares two Strings by
UTF16 units, treats NaN/invalid BigInt strings as unordered and never rounds BigInt
through Number. Source operands run once before canonical cells are boxed/rooted.
ABI28 pure primitive_equal/primitive_compare imports validate both transports,
charge native fuel and keep malformed/resource failures fatal. Scalar Number,
String, Boolean equality, BigInt and nullish paths retain suitable guest/existing
fast paths; a 1500-iteration numeric/nullish loop stays below host-call ceilings.

Six new runtime cases include a pinned Node.js v24.20.0 oracle of 2048 comparisons,
precision/infinity/UTF16 edge cases, source order, direct-operand GC, scalar-loop
imports and malformed/fuel failures bypassing handlers. Three pinned corpus cases
also match Node.js. V28 admission remains frozen; objects, Symbol and broader
mutable storage remain explicit unfinished CORE-01/02/03 work.
### Closed property deletion (v30)

V30 inherits v29 ABI28/value13/schema13/channel4 and adds optional-record deletion
and primitive computed delete keys for records/arrays. Base/key run once before
mutation. Deletion installs canonical undefined, compacts live order entries,
zeros the unused trailer and preserves static trace masks. Reinsertion appends
new string-key order; numeric enumeration stays numeric-first. Escaped values
retain identity through GC while deleted edges can be reclaimed. Absent/unknown
own keys return true; required checked record fields and array length throw
TypeError before mutation. Object/Symbol ToPropertyKey, arbitrary properties,
descriptors and general required-field deletion remain unfinished CORE-02 work.
V29 and older source admission remains frozen; native launcher protocol2 now
advertises 33 profiles. No ABI/layout/resource ceiling was raised.

Verification: seven focused runtime tests, 55 existing common tests plus one new
profile test, 36 compiler tests, strict common/compiler/runtime all-target Clippy,
one cross-module public API test and one CLI development/source-free Wasm/native
test pass. The runtime companion rebuild passes. Tests cover field31, order,
primitive keys, aliases and repeated large allocations/deletions beyond heap
capacity. Three new pinned cases match Node.js v24.20.0. Current inventory is
v30 with 78 cases: 67 supported, 11 known gaps, zero regressions/infrastructure
errors and seven untested feature rows. The previous full workspace gate (1678
tests, commit 040756c) is historical evidence, not a full pass for this increment.
### Object.entries snapshots (v30)

Closed records and typed arrays now produce dense mutable key/value pair arrays
in own-key order. Absent fields and holes are skipped; explicit undefined remains.
Pairs are fresh and nested objects retain identity. Source/ignored arguments run
once before snapshots; nullish/missing sources throw TypeError and primitive
Number/Boolean/BigInt wrappers return empty arrays. Pair types use the existing
String/value union; positional tuple typing, generic objects, Symbol and mixed aggregate source
shapes remain unfinished CORE-02/07 work. The String enumeration increment below
extends this operation to String inputs and primitive unions.
Seven focused runtime tests pass, including a 1024-element snapshot and collection
beyond heap capacity with explicitly increased test-only instruction budgets.
The cross-module public API and CLI development/source-free Wasm/native fixtures
pass with deletion followed by enumeration. Strict common/compiler/runtime
all-target Clippy passes. Two new pinned cases match Node.js v24.20.0. Baseline
and strict corpus reports agree: 78 total, 67 passes, 11 known gaps, no unexpected
support, regressions or infrastructure failures; seven feature rows remain
untested. Strict mode exits 1 for these declared gaps. Full workspace verification
remains pending for this uncommitted batch; the CORE work streams remain Partial.
### String own-property enumeration (v30)

Object.keys/values/entries now accept String, nullable String and unions of the
six admitted primitive kinds. String properties enumerate UTF16 units, retaining
surrogate pairs as two values and preserving lone surrogates. Primitive wrappers
without own enumerable properties produce empty arrays; nullish/missing inputs
throw TypeError. V30 keys/values evaluate ignored arguments before snapshots.
Guest-only slicing/array helpers avoid native calls per unit. More than 1024
units raises recoverable RangeError; ABI/layouts and production budgets remain
unchanged. Older-profile call rules and String rejection stay frozen.

Seven new targeted tests cover UTF16, primitive unions, ignored-argument order,
nullable errors, capacity and repeated collection. Capacity and collection stress
fixtures explicitly increase test-only fuel. Two pinned cases match Node.js
v24.20.0. The combined object-operation filter passes 53 runtime tests, and the
cross-module public API and CLI development/source-free Wasm/native fixture pass.
Strict common/compiler/runtime all-target Clippy and formatting checks pass.
The reviewed corpus reports 78 total, 67 passes and 11 known gaps, with no
unexpected support, regressions or infrastructure failures; seven feature rows
remain untested. Strict mode reports the same counts and exits 1 for declared
incomplete coverage. Seven property-deletion regression tests also pass after
the enumeration changes. General object conversion, mixed aggregate sources,
Symbol, descriptors and positional tuple inference remain unfinished. CORE-02/07
remain Partial; no full workspace or production readiness claim is made.
### Object.is / SameValue (v30)

Object.is now compares admitted primitives and reference identities without
coercion, including union dispatch across different checked aggregate shapes.
NaNs are equal, signed zeros distinct; Strings compare UTF16 contents and BigInts
retain exact mathematical equality. Argument expressions run once left-to-right,
missing/void arguments become Undefined, and ignored extras execute before the
comparison. Cached operands remain rooted across later allocations/collection.
Numeric comparisons are guest-only and no ABI/resource ceiling changes.

Seven focused tests pass, including a pinned Node.js v24.20.0 matrix of 324
primitive pairs, reference/error aliases, distinct shapes, ignored arguments,
collection stress and a 1500-iteration numeric loop at default budgets. The
collection-only fixture explicitly increases test fuel without changing memory
limits. Two original corpus cases match Node.js. Symbol, unimplemented kinds,
first-class intrinsic values and native result-pair object identity remain
unfinished; native results require explicit fields. CORE-01/02/03 remain Partial.

Object.is integration verification: cross-module public API, CLI development,
source-free Wasm/native execution, strict common/compiler/runtime all-target
Clippy and formatting checks pass. The reviewed corpus contains 40 feature rows
and 78 cases: 67 pass, 11 remain known unsupported, zero unexpected support,
regressions or infrastructure failures; seven feature rows remain untested.
This is scoped evidence for the increment, not a full workspace or readiness gate.
### Checked Object.assign (v30)

Record/array targets copy own enumerable record, array and UTF16 String properties
in source/key order, preserving target identity and shared children. All argument
effects precede target validation; per-source snapshots follow earlier copies so
later aliases observe prior writes. Holes are skipped, explicit Undefined copied,
and nullish sources ignored. Primitive/function/Error sources without enumerable
own fields are inert. Missing/nullish targets throw TypeError. Array length uses
Number validation; semantic failures retain earlier writes and checked field
storage remains sound. Numeric array copies require no per-index native call.

Plain fresh record literal targets reserve known record-source fields in their
initial descriptor; new fields are optional and existing literal field types may
widen to an admitted union. No escaped target changes layout or reference mask.
Record sources read fields separately, admitting unrelated nested shapes without
erased unions. General property creation, primitive target boxing, descriptors,
getters/setters, Symbol and unrestricted shape-changing storage remain unfinished
CORE-02/03/06 work. Indexed sources require declared target fields; unknown keys
retain existing checked-write guards. This is an explicit checked increment,
not general JavaScript Object.assign completion.

Fifteen focused tests pass, covering own order/presence, fresh shapes, nested
identity, record/array/source unions, argument order, partial errors and aliases.
A 1024-element array copy fits default production budgets. The collection stress
fixture alone raises test fuel and keeps the production memory limit. Three new
pinned cases match Node.js v24.20.0. ABI/layouts, the 33-profile list and all
production ceilings remain unchanged; CORE-02/03/06/07 remain Partial.

Checked Object.assign integration verification passes: cross-module public API,
CLI development and source-free Wasm/native execution, strict common/compiler/
runtime all-target Clippy, formatting and whitespace checks. The reviewed corpus
contains 40 feature rows and 78 cases: 67 passes, 11 known unsupported, no
unexpected support, regressions or infrastructure failures; seven feature rows
remain untested. This increment has scoped verification; the full workspace
pre-commit gate and wider readiness criteria remain outstanding.
### Managed function transport foundation (in progress)

Function-valued records/arrays/unions still require a versioned public value tag,
updated schema/native transport and receiver-preserving property calls. Current
v30 source admission and value13 stay frozen. The existing closure representation
has a distinct opaque heap flag, signature/code slots and an optional environment;
its binding type IDs are compiler-private and are not public ValueKind tags.

New allocation-free common views validate physical closure/environment/binding
envelopes before future structured-value admission. They check handle extents,
alignment, allocated heap flags, reserved words, signature/code budget ceilings,
exact trace masks, capture counts up to 32 and one/two-slot binding widths.
Duplicate captured cells and recursive function payloads remain legal; opaque
binding payloads are not recursively interpreted. Matching a binding envelope to
compiler-provided type ID/width/root metadata is explicit. These checks do not
prove code/signature membership in a module or validate a captured value's type;
those remain required at the future typed transport boundary.

Five malformed/boundary common tests pass. Two runtime tests compare these views
with real compiler-emitted mutable captures, String captures, capture-free
closures and captured native result pairs, and retain v30 rejection of function
aggregates. No public function value tag, new profile, host import, ABI/layout,
resource limit or language admission has changed. Function-valued properties,
method calls, JSON function rules and descriptors remain unfinished CORE-02/03/07
work; this foundation does not close any readiness stream.

Envelope-foundation verification also passes strict common/compiler/runtime
all-target Clippy, formatting and whitespace checks. Corpus inventory remains
78 cases / 67 supported / 11 known gaps with seven untested feature rows; no
source behavior changed and no new compatibility pass is claimed for this step.
### Typed private capture catalog (transport preparation)

V30 modules with function signatures now optionally carry a versioned
`varta.functions` catalog outside mutable guest memory. It maps private binding
IDs to exact value schemas/result widths and maps closure code/signature IDs to
capture layouts. The compiler derives it from the same IR tables used by checked
dispatch and closure prologues. Older V30 artifacts without the optional catalog
remain accepted; earlier profiles do not admit the new metadata. Preflight rejects
duplicate, malformed or incompatible catalog sections before execution.

Typed closure inspection validates captured primitive bits, String extents,
BigInt leaves, Error classes, records, sparse arrays and union member selection.
Record checks include field names, types, optionality and exact compiler slot
identity, not merely a structurally valid descriptor. Result pairs retain exact
width/root metadata and bounded status values. Private function captures check
catalog code/signature membership; shared bindings and recursive closures remain
legal through memoization. A work allowance plus existing 4096-node/64-depth
bounds prevents unbounded traversal. Catalog type DAGs enforce their depth bound
even when serialized bottom-up. No helper invokes guest code or grants authority;
Wasm checked dispatch remains responsible for actual invocation.

Ten common function/catalog tests and the 51-test runtime function filter pass.
After strengthening record slot checks, four focused runtime integration/security
tests and ten common tests pass again. These cover real emitted closures, nested
structured captures, native result pairs, recursive function payloads, corrupted
Boolean values/descriptors, and preflight metadata rejection. The public
cross-module and artifact fixtures now include an escaped numeric closure.

Value ABI13, schema13, host ABI28 and the 33-profile list are unchanged. A public
function value tag, native transport integration, function-valued aggregate source
admission and receiver-preserving property calls remain outstanding. The catalog
is preparation for that versioned increment, not a claim that function-valued
properties or accessor descriptors are complete. CORE-02/03/07 remain Partial.

Typed-catalog batch verification: strict common/compiler/runtime all-target
Clippy, cross-module public API, rebuilt runtime companion and CLI development /
source-free Wasm/native fixture pass. Formatting/whitespace checks pass. The
reviewed corpus still reports 78 cases: 67 supported, 11 known unsupported,
zero unexpected support/regressions/infrastructure failures, seven untested
feature rows. No new language compatibility pass or full workspace readiness
claim is made; public function-value admission remains the next required stage.
### Function value14 codec (not yet source-admitted)

Common value ABI14 adds Function tag12 with an exact reference mask. V13 and
older reject this tag. Record/presence, sparse-vector, Error and BigInt layouts
inherit their prior physical forms under the new explicit version. Function
reference cells require an immutable owner catalog and bounded typed closure
validation; context-free function reads fail. Catalogs allow function fields,
array elements and one function signature per reference union only under v14.
Private function captures in v13 catalogs retain their existing behavior.

The runtime exposes a Rust value14 graph encoder without granting invocation
rights. Ordinary function properties are omitted, array function values become
null, and a top-level function produces Undefined. Validation occurs before
omission and shares a caller-owned function work budget. Private capture cycles
through records or arrays remain legal and do not become observable JSON cycles.
Callable toJSON hooks are explicitly rejected until guest-side dispatch exists;
this encoder never silently treats such a hook as an ordinary omitted property.
Schema14 maps existing JSON input types to value14 layouts, but JSON tokens and
schemas cannot fabricate callable values. Existing schema/resource ceilings stay.

Five initial v14 data-level tests pass; final tests also cover toJSON rejection
and the public Rust codec on a real emitted scalar-closure snapshot after explicit
owner-side transcoding. All 67 common tests and 85 runtime JSON-filter tests pass.
The whole workspace compiles. This is codec groundwork, not new TS admission:
all 33 source profiles retain their current contracts (latest v30/value13/ABI28).
A new source profile, versioned host imports/context wiring, member invocation,
function-valued aggregates and their end-to-end source/artifact/conformance checks
remain outstanding. No additional language compatibility pass is claimed, and
CORE-02/03/07 remain Partial.

Final value14 codec verification: seven focused data/Rust-API tests and four
legacy typed-closure/catalog tests pass. Strict common/compiler/runtime all-target
Clippy passes, followed by final runtime all-target Clippy after the public API
fixture; formatting and whitespace checks pass. Node.js independently matches
ordinary function omission/null/Undefined and both private-cycle JSON outcomes.
No source/artifact/conformance pass is claimed for value14 host operations, which
are not registered yet; current language admission stays unchanged.
### Prepared function-catalog ownership increment

The preceding source snapshot was committed and pushed as `bebe8d4` after the
full seven-command verification gate passed 1746 tests with unchanged sources.
The reviewed compatibility inventory remains 78 cases: 67 supported, 11 known
gaps, plus seven untested feature rows; this is not complete compatibility.

The next runtime increment retains an immutable verified capture catalog beside
its validated module and transfers shared ownership into each execution Store.
Startup, tools and HTTP requests use this prepared path. One instantiation
attempt per execution prevents replacing a live module's schema, including after
a failure. Failed instantiation clears the owner. Legacy backend validation APIs
and optional-catalog admission remain available. Capture proofs on the sealed
common catalog retain graph/work checks without revalidating immutable schemas.
Source profile31, host ABI29, function-valued aggregates/member invocation and
end-to-end value14 source/artifact/conformance checks are still outstanding.
CORE completion states remain unchanged; this increment closes runtime metadata
ownership groundwork, not the full function transport requirement.

Verification for prepared ownership: the new common sealed-catalog test and
three runtime ownership tests pass (the prepared-name filter also ran nine
existing regressions). Eight tool-invocation tests, six HTTP tests and the final
58-test function filter pass, including real emitted capture graphs validated
through the Store's catalog and v30 artifacts with optional metadata removed.
The cross-module public API test and rebuilt runtime companion/source-free CLI
artifact test pass. Strict common/runtime all-target Clippy passed; final test
fixture additions passed compilation and focused behavior checks. Formatting and
whitespace checks pass. No new full-workspace gate or value14 source compatibility
pass is claimed for this increment.

## Native form controls — 2026-11-02

Working-tree additions after `870344d` extend the experimental native CPU preview
with password/search fields and font-independent checkbox/radio marks. The native
contract remains an explicitly styled, bounded subset, with the production shell,
full L0 styling and cross-platform acceptance still open.

Password painting shapes one mask per extended grapheme, including transient IME
preedit, with source UTF-8 offsets retained for selection and editing. Four public
view tests cover distinct mask/plaintext glyph pixels, Cyrillic, combining marks,
emoji/ZWJ, 2x coordinates, original input values, Undo, placeholders, type changes,
composition joining the preceding grapheme and the existing shaping limit. A Wasm
application test verifies original password/search values. The native field oracle
matches real Chrome in 28 box/caret checks with the same original font. A separate
WebKit sample uses UTF-16-unit masks, leaving full password parity open.
On macOS Retina, `field-types` showed five masks, three after selection/deletion,
then five after Undo. Search deletion/Undo restored its four plaintext glyphs.
Guest logs confirmed two edits per field without logging the values.

Checkbox/radio controls paint a check mark or circular checked dot inside their
explicit rectangular boxes without a font. Marks scale with DPI and respect the
content/ancestor clip. The shared model now groups radios by equal nonempty name,
tree and closest form owner, including checked subtree insertion and type/name
changes. Missing/empty names remain independent. Native and JS transactions roll
back group changes on rejection; browser input and Worker event synchronization
uncheck only the selected group. Native arrow navigation skips hidden/disabled
peers, wraps and honors keydown/click prevention. Tab uses one eligible group stop.
Click records observe preactivation; cancellation restores the previous group
before any input/change or counter increment. Browser checkable field-change
records now use the same empty value payload as native.

Verification: all **91 varta-view tests** passed, including four new control tests;
the focused Wasm checkable application test passed; all **97 browser Node tests**
passed with the native oracle. Real Chrome matched **35 radio group/patch checks**
and byte-identical click/keydown/input/change records plus state in **six activation
and cancellation scenarios**. Strict view/desktop all-target Clippy, desktop build,
formatting and whitespace checks passed. These are scoped results, not a new full
workspace/release gate. The browser probe commands are in
[verification.md](verification.md#browser-registry-and-fuel-probe-development).

The font-free `checkables` fixture was observed on macOS Retina: Space checked the
checkbox; Tab/Down moved the upper radio selection while the separate form stayed
selected; Shift+Tab returned directly to the checkbox; Space cleared it; a pointer
click selected the first upper radio. Guest logs confirmed the four edits and
field counters. The test window/process closed cleanly. Full control theming,
focus-modality matching, native accessibility, Pi and production packaging remain
open; no P10 or whole-CORE completion is claimed.


### Single-select state and native popup

Single-select controls now share option selectedness, dirty attribute behavior,
text-derived values, duplicate-value identity, reparenting and atomic rollback in
native and browser models. Equal guest echoes keep the concrete option. Browser
DOM/Worker envelopes retain a validated option ID with deferred events without
changing the Wasm wire. Derived option values retain the existing value bound.

The native CPU renderer adds a top-layer scrolling popup, physical hit targets,
keyboard preview/accept/cancel, focus-loss dismissal and character cycling. Option
subtrees have no ordinary layout boxes. The desktop fixture bundles its own font.
TSX applies initial values after options, retaining one getter evaluation in source
order, reactive subscriptions and ref ordering; a Wasm regression covers these.

The generated browser select fixture passed trusted character input, duplicate
identity, input/change delivery and guest echo through its packaged Worker. Native
popup GUI acceptance, aggregate selection-scan budgeting, timed multi-character
search, hover feedback and broader control styling remain open. These additions
do not close P10 or claim cross-platform/release acceptance.

At the commit boundary, all 103 browser Node tests passed with a freshly generated
native oracle. Real Chrome matched 11 select identity/event/mutation scenarios,
35 radio ownership/rollback checks and six checkable activation/cancellation
scenarios. The password/search probe passed 28 geometry/caret comparisons.


### Single-select transaction work and native acceptance

A shared draft allowance now meters ownership walks, affected option source bytes
and final inventory in native/browser transactions. It includes detached/disabled
options and deep walks without a select owner; existing wire/value budgets stay
unchanged. UTF-8 byte sizes agree for ASCII and emoji. The JS model caches only
host-owned source sizes, copying and rolling them back with the tree. Exceeding
work is a typed fatal view failure that keeps the previous successful commit.

A regression reproduced different admission in the main thread and Worker while
an edit was awaiting delivery. Admission now ignores live field value/sequence;
consecutive identical select writes share one lookup, including unmatched values,
and any intervening operation clears that shortcut. DOM replay skips duplicate
writes while keeping original indices for prepared fonts/resources/stylesheets.
Another regression covers native logical character input without committed text;
select typeahead uses that logical key, while text fields retain insertion rules.

Scoped verification: **105 varta-view tests**, **42 Wasm/view application tests**,
**108 browser Node tests** with a freshly generated native oracle, five browser
registry checks and **14 documentation/registry checks** passed. Real Chrome
matched **33 select-work batches**, including failure positions, rollback, mutated
fallback text, deep owners and stylesheet preparation after duplicate writes;
existing 35 radio ownership checks, six checkable activation scenarios and eleven
select identity/event scenarios also passed. Strict affected all-target Clippy,
formatting, CLI/companion/desktop builds and whitespace checks passed. These are
scoped results, not a new full-workspace/release gate.

A freshly packaged browser select passed trusted character input, duplicate
identity, input/change and guest echo through its actual Worker. The new native
artifact probe removes source/config before execution; both the portable runtime
and the source-free single binary produced the exact work-limit failure, retained
prior output and reached neither guest catch nor following logs. The single binary
was moved alone into a new directory and the portable bundle removed before its
run. A fresh CLI view
script confirmed empty-text logical-key choice and guest echo.

On macOS Retina the native fixture passed Tab/Space opening, Down preview without
changing the closed label, Enter choosing between equal-valued options, Escape
cancellation, wheel scrolling, pointer choice and outside-click dismissal. Guest
logs reported `input/change same 1`, then scrolled-row value `4` at sequence `2`;
character input also changed the selection and its label. Both test windows and
processes closed cleanly. Empty-text logical-key behavior is separately verified
through the native API, Wasm and CLI; GUI character input was observed directly.
Timed multi-character search, hover feedback, complete input methods, native
accessibility, Pi/other platforms, broader L0 controls/styling and release metrics
remain open. Radio/cascade work and aggregate retained-text budgeting need their
own audit; this allowance is specific to selection operations and option inventory.
The independent prototype applier is outside these official-host checks; any
remaining pilot use needs a migration/alignment audit before release acceptance.

## Official-host pilot mockups — 2026-11-02

Committed as `a71d281`: UARP console mockup encoding, linting and projection now
consume the common registry and official browser host modules, replacing their
prototype dependency. Lint checks real attribute/style semantics and complete
transactions, including packaged-resource denial and aggregate select work.
Projection checks live values, checked state, duplicate selection identity and
open dialogs in addition to structure and the existing bounded geometry sample.
A loopback-only named-asset server and its allowlist/Host/path/resource tests
replace serving the repository tree for these pages.

The real WebKit probe exposed automatic selection of the first disabled option.
DOM replay now reconciles selected indices against the shared model, retaining
explicit no-selection through reinsertion and dirty attributes. Equal/no-op
commits avoid a setter write and reconciliation creates no field edits. A native
oracle supplies the disabled/select-state expectations independently.

Focused evidence: 15 native select tests, 109 oracle-enabled Node browser tests,
one server test, eight new native/Chrome select-state batches and the existing
select/radio/work/event probes passed. Chrome and WebKit each passed five mockups,
15 projections and 41 regression checks. Firefox failed at launch in that run
(timeout with local process/graphics diagnostics), so the newer package has no
passing complete three-engine matrix. The final full gate passed all eight
commands and 3048 tests with unchanged sources, report
`target/verification/20261102-192654-7q8tkmi1/report.json`. This is macOS arm64
package evidence, not complete pilot/native-platform or release acceptance.

## Radio transaction work — scoped verification 2026-11-02

Working tree after `a71d281`: the shared view registry now declares a separate
8,388,608-unit radio-reconciliation allowance and 16 units per visited node.
Transactions charge owner/group walks, compared UTF-8 name bytes and insertion
subtrees. Checked/type/name writes, stale patches and unchecked moved radios
have the same admission regardless of deferred parent edits. The JS model caches
host-owned name byte sizes; native insertion visits stop incrementally at the
limit. Rejection retains the previous tree, checked membership, sequence counters,
title and effects. Local input/navigation scans are outside this transaction
allowance and remain an explicit follow-up, with aggregate retained text/cascade.

Final scoped evidence: **109 varta-view tests**, **43 runtime/view tests** and
**113 oracle-enabled Node browser tests** passed. The native test admits exactly
8192 scans at the allowance and rejects the next scan at its original operation
index. The real Wasm case terminates before guest catch/later logging and keeps
its prior view. A queued Worker edit cannot change work admission or be lost on
rejection. Registry drift (five tests), embedding documentation (eleven tests),
the no-std common-core check and strict all-target Clippy for common/view/project/
runtime passed; formatting and whitespace checks passed.

Real Chrome matches the fresh native oracle on **55 select/radio work batches**,
including exact failure positions and rollback, plus **1839 radio DOM/state
observations**, the eight disabled-select state batches, six checkable activation
scenarios and eleven select identity/event scenarios. Chrome and WebKit each pass
five pilot mockups at three sizes and **44 regression checks**, including radio
work refusal, unchanged DOM/model and a later successful small transaction. The
named-asset mockup server test passes. This is scoped macOS/engine evidence,
not a full-workspace gate or complete Firefox/pilot/native-platform acceptance.

Local logs: `target/radio-view-component-final.log`,
`target/radio-runtime-view-final.log`, `target/radio-oracle-node-final.log`,
`target/radio-chrome-final.log`, `target/radio-clippy-final.log`,
`target/radio-pilot-chrome.log` and `target/radio-pilot-webkit.log`. Regenerate the
native oracle with the verification recipe; these ignored files are development
evidence, not distributed artifacts.

## Native radio navigation and retained text — scoped verification 2026-11-02

Working tree after `a71d281`, following the radio-transaction batch: native Tab
builds one transient document/form/name partition instead of scanning the full
tree per distinct radio name. Current-group membership uses a node set and click
preactivation reuses its captured peers. Many-group forward/backward order,
checked stops and a later membership rename pass without a persistent index.
CSS/display work remains a separate audit.

The registry declares 8,388,608 bytes of canonical retained text/capacity and
98,304 bytes per live-value reservation. Source bytes cover text, attributes,
classes, inline styles, title and raw stylesheets, including detached nodes.
Reservations cover worst-case UTF-8 values under the existing UTF-16 bound and
remain sticky across type changes, including stale set-value operations. Removal
reclaims them. With no other payload this draft admits at most 85 reserved value
fields. This deliberately counts capacity instead of current editable lengths,
so both accepted and rejected admission agree while a parent edit awaits Worker
delivery. The native flag fits the existing boxed editing state; JS size maps
are copied with transactions and omitted from inspection snapshots. Synchronized
field records are validated before value/counter changes.

Final scoped evidence: **114 varta-view tests**, **44 filtered runtime/view
checks**, **119 oracle-enabled Node tests**, five registry checks, eleven
embedding documentation checks and the no-std common-core check pass. Strict
all-target Clippy for common/view/project/runtime, formatting and whitespace
checks pass. The intrinsic-library fingerprint is current. Tests cover exact
capacity, every source slot, hidden/detached state, inter-batch accumulation,
replacement/deletion, type transitions and cache poisoning after rejected
transactions. A real Wasm failure retains the previous view and reaches neither
guest catch nor subsequent logging.

Real Chrome matches **68 selection-work/retained-text batches** from a fresh
native oracle, including admission, final-inventory failure and DOM rollback.
The existing 1839 radio observations, eight disabled-select batches, six
checkable activation and eleven select identity/event scenarios pass. The oracle
is byte-identical before/after moving the private native capacity flag, so those
browser comparisons still cover the final native observations. Chrome and
WebKit each pass five mockups at three sizes and **47 regression checks**,
including retained-payload refusal and successful reclamation.

Fresh CLI/companion and projection builds pass. The UARP application passes
**107 tests in 28 groups and two expected native views**. Selected live chat
probes pass in Chrome 156.0.8078.80 and Playwright WebKit 26.5, including trusted
thinking/approval/option/input/Enter, the native ordered tree, selected
accessibility and 195 declarations across 14 properties. An optional engine
selector now supports these focused runs; the default still attempts all three.
Firefox and the complete matrix, live-service acceptance, other native targets,
process RSS/cascade/other-host-buffer measurements and release readiness remain
open. No new full-workspace pass is claimed.

Local evidence: `target/view-text-component-final.log`,
`target/view-text-runtime-final.log`, `target/view-text-oracle-node-final.log`,
`target/view-text-chrome.log`, `target/view-text-clippy-final.log`,
`target/view-text-pilot-native.log`, `target/view-text-live-chrome.log`,
`target/view-text-live-webkit.log`, `target/view-text-pilot-chrome.log` and
`target/view-text-pilot-webkit.log`. These are ignored development results.


## Stylesheet expansion and indexed cascade — scoped verification 2026-11-02

Working tree after `a71d281`, including the radio/navigation/retained-text batches:
shared parsed-work accounting checks source bytes, syntax nodes and copied media
conditions before expansion. Per-transaction and installed-inventory ceilings
reject atomically; replacement/removal reclaim installed capacity. Native root
and class indices avoid scanning unrelated sheets, borrow matched declarations
and resolve custom maps lazily. Display-only traversal skips hidden subtrees.
Keyword `var()` admission and empty-declaration placement now agree in the sampled
native/JS grammar. Memoized JS grammar paths remove the reproduced exponential
invalid-tail case without rejecting its valid repeated-variable counterpart.

Scoped checks pass: **119 native view tests**, **45 filtered runtime/view checks**,
**127 oracle-enabled Node tests**, 18 common view tests, five registry checks,
eleven embedding documentation checks and the no-std common check. Strict
all-target Clippy passes for common/view/project/runtime; project Clippy also
passes after the bundle repair. Real Chrome matches **88 work/payload batches**,
1839 radio observations, eight disabled-select batches, six checkable activation
and eleven select identity/event scenarios. Chrome and WebKit each pass 15
mockup projections and **51 regressions**.

Fresh CLI/companion builds and the normal UARP check pass: **107 tests in 28
groups and two expected native views**. The first compiled WebKit run exposed
missing checker/error bindings in manual browser bundle assembly. The bindings
were repaired, binaries rebuilt, and the final live Chrome/WebKit runs both pass
trusted interactions, ordered tree, selected accessibility and 195 declarations
across 14 properties. A separate source-free generated-artifact regression passes
initial CSS and a trusted event whose excessive stylesheet fails through the
Worker before guest catch/logging, retaining its prior DOM and title.

Evidence: `target/style-work-component.log`, `target/style-work-runtime.log`,
`target/style-work-oracle-node-final.log`, `target/style-work-chrome-final.log`,
`target/style-work-pilot-chrome.log`, `target/style-work-pilot-webkit.log`,
`target/style-work-pilot-native.log`, `target/style-work-live-chrome-final.log`,
`target/style-work-live-webkit-final.log` and
`target/style-work-generated-bundle.log`. The earlier
`target/style-work-live-webkit.log` records the repaired failure, not a pass.
These scoped results do not establish full cascade/output/accessibility budgets,
Firefox, other native platforms, sustained resource measurements or release
readiness. The commit-boundary full gate is recorded separately.


## View resource package — full gate 2026-11-02

The radio/navigation/retained-text/stylesheet package after `a71d281` passed
**3066 tests, 0 failures, 16 ignored**. All eight full-gate commands passed with
an unchanged before/after source fingerprint. Report:
`target/verification/20261102-215501-s16w2dmu/report.json` (local, ignored).
The conformance baseline remains 123 passing and four known unsupported cases.
Only documentation result records were edited after this gate; engine and build
inputs remained unchanged. Scoped browser, Worker, generated-bundle and UARP
results are recorded above. Full CSS/resource, browser/platform and release
acceptance remain open.

The additional packaging-boundary CLI smoke repeat also passed **350 tests**;
evidence: `target/view-resource-commit-cli-repeat.log`. No engine/build inputs
changed between the full gate, this repeat and the commit.


## Component depth and iterative native projection — scoped verification 2026-11-02

Working tree after `6830dab`: the draft registry now declares a maximum component
depth of **512 edges**, measured from the implicit view root or each detached
component root. Each insertion checks the whole moved subtree in its existing
metered walk, including text leaves, and reports the insertion on rejection. The
copied transaction, its title and pending effects roll back; later transactions
can reclaim/reparent normally. Local field values and counters do not affect the
rule. Later removal in the same batch does not admit excessive transient depth.

A real native test reproduced stack exhaustion even at the admitted boundary,
so the fix includes iterative node/accessibility projection, descendant name
computation and subtree text collection. The top-level snapshot inserts built
JSON values directly instead of recursively reserializing them. The final child
regression passes on a normal **2 MiB** thread stack with generic flattening,
nested accessibility roles, descendant names, serialization, clone and drop;
no stack allowance was raised.

Scoped evidence: the **119 existing native view checks** pass on the final engine
code. An intermediate new boundary assertion walked one accessibility level too
far; after correcting that test, all **four depth tests** pass. The original
`target/tree-depth-component.log` therefore records an overall test failure,
not a full-suite pass; `target/tree-depth-boundary-final.log` records the corrected
four-test pass. The earlier `target/tree-depth-component-first.log` captures the
repaired stack abort. **46 filtered runtime/view checks**, including compiled
fatal failure before catch/logging and retention of the prior view, pass.

A fresh native oracle agrees with **131 Node checks** and **102 real Chrome
work/payload/depth batches**, including operation positions and DOM rollback.
The existing 1839 radio observations, eight disabled-select batches, six checkable
activation and eleven select identity/event scenarios pass. Chrome and WebKit
each pass 15 mockup projections and **55 regressions**. Five registry checks,
eleven embedding documentation checks, no-std, formatting/whitespace, current
intrinsic fingerprint and strict all-target Clippy for common/view/project/runtime
pass. Fresh CLI/companion builds pass. UARP passes **107 tests in 28 groups and
two expected native views**. The source-free generated browser probe passes both
the new trusted-event depth failure and its existing stylesheet failure, keeping
the prior DOM/title and reaching neither catch nor later logging.

Local logs: `target/tree-depth-runtime.log`, `target/tree-depth-oracle-node.log`,
`target/tree-depth-chrome.log`, `target/tree-depth-pilot-chrome.log`,
`target/tree-depth-pilot-webkit.log`, `target/tree-depth-clippy.log`,
`target/tree-depth-generated-bundle.log`, `target/tree-depth-css-bundle.log` and
`target/tree-depth-pilot-native.log`. These ignored scoped results do not replace
the last full gate for `6830dab`. Full cascade/derived-output/accessibility work
budgets, other host buffers, Firefox/current full matrix, native platforms,
metrics and release acceptance remain open. The existing experimental native
layout depth ceiling is unchanged.


## Potential cascade reads — scoped verification 2026-11-02

Working tree after `6830dab` and the component-depth package: the shared draft
registry now declares **8,388,608 units** for potential cascade inventory reads.
Every live node (including detached components) charges its node/class/inline
source cost and deduplicated root/first-class candidate rule construction costs.
Media, remaining selector classes, pseudo-state, local field edits and winning
declarations do not discount candidates. Raw variable candidates additionally
charge ancestor own-cost prefixes when any installed custom property exists.
Native rule costs are cached with parsed indices; private browser rule records
and anchor sets are incrementally updated, copied per transaction and discarded
on failure. The estimator walks component prefixes iteratively.

Explicit focus/select/scroll reads and final inventory share one allowance.
Equal focus commands still count. A style-reading command rejects excessive
intermediate state before computation, even if the guest planned to remove the
sheet later in the same batch. Final excess has no operation index; replacement,
deletion and index reclamation remain atomic. This is a potential matching and
inherited-map discovery contract, not a complete frame CPU/output/RSS budget.

Final scoped checks pass: **129 native view tests**, **47 filtered runtime/view
checks**, **138 oracle-enabled Node checks**, five registry checks and eleven
embedding documentation checks. Exact allowance/one-byte overflow, compound and
duplicate anchors, rejected replacement/index rollback, detached nodes, inactive
media, unmatched selector tails, inheritance/reparenting, repeated equal focus,
command/final failure positions and smaller retries are covered. Common no-std,
strict all-target Clippy for common/view/project/runtime, formatting/whitespace
and the current intrinsic fingerprint pass. The fresh oracle also compares
native rule costs with JS parser metadata.

Real Chrome agrees on **127 work/payload/depth/cascade batches**; 1839 radio
observations, eight disabled-select batches, six activation and eleven select
identity/event scenarios still pass. Chrome and WebKit each pass 15 mockup
projections and **59 regressions**, including command/final cascade refusal,
DOM/model rollback and a smaller retry with correct computed color.

Fresh CLI/companion/projection builds pass. UARP passes **107 tests in 28 groups
and two expected native views**. Live compiled chat passes in Chrome 156.0.8078.80
and Playwright WebKit 26.5, including trusted interactions, the ordered native
tree, selected accessibility and 195 declarations across 14 style properties.
Source-free generated browser artifacts pass the new trusted-event cascade
failure before catch/logging and retain prior DOM/title; depth and stylesheet
failure modes also pass with the new model. The native Wasm regression likewise
rejects a transient excessive command read before a later attempted repair.

Evidence: `target/cascade-component.log`, `target/cascade-runtime.log`,
`target/cascade-oracle-node.log`, `target/cascade-chrome.log`,
`target/cascade-pilot-chrome.log`, `target/cascade-pilot-webkit.log`,
`target/cascade-clippy.log`, `target/cascade-generated-bundle.log`,
`target/cascade-depth-bundle.log`, `target/cascade-css-bundle.log`,
`target/cascade-pilot-native.log`, `target/cascade-live-chrome.log` and
`target/cascade-live-webkit.log` (local, ignored). The initial focused native
five-test log had an unused test assignment; it was removed before the final
six-test/all-component pass and strict Clippy. No production allowance was raised
to obtain passing results.

The last full workspace gate still belongs to `6830dab`. Complete derived-value
expansion/output, repeated accessibility evaluation, browser-private invalidation,
other host buffers/lifetime, Firefox/current full matrix, native targets, sustained
resource measurements and release acceptance remain open.


## Computed custom-property scopes — scoped verification 2026-11-03

The native renderer now resolves custom declarations before inheritance, freezes
aliases, masks invalid local bindings and tracks active dependency cycles with an
iterative work stack. Only selected fallback branches participate. An initial
eager graph based on the 2022 specification snapshot disagreed with both tested
browsers on an unused self-fallback; it was replaced using the current draft's
substitution contexts and independent platform-only CSS controls. The arena shares
fragments and checks UTF-16 size before materialization. Whole property grammar
and numerical notes defer for valid variable references, then native snapshot,
geometry and paint validate selected values. Empty/nested/function fallbacks,
quoted text, token boundaries and case-insensitive variable functions are covered.

Scoped checks pass: **143 native view tests**, **48 runtime/view checks**,
**18 common view checks**, **142 oracle-enabled Node checks**, five registry and
eleven embedding-documentation checks, common no-std and strict all-target Clippy
for common/view/project/runtime. The native suite includes nine resolver unit
checks and six variable integration checks. Chrome passes **20 variable fixtures
and 220 computed-style observations**.

WebKit does **not** pass that comparison: the `cycle-escape` fixture differs in
color and its derived border color. Platform-only controls also expose a renamed
outside-cycle consumer difference. This remains an explicit browser compatibility
gap; the failing probe is retained. No complete Chrome/WebKit variable parity,
Firefox matrix, generated variable-specific browser acceptance or release
readiness is claimed.

Evidence: `target/variables-complete-native.log`,
`target/variables-complete-runtime.log`, `target/variables-complete-common.log`,
`target/variables-complete-node.log`, `target/variables-complete-chrome.log`,
`target/variables-complete-webkit.log`, `target/variables-complete-registry.log`,
`target/variables-complete-docs.log` and `target/variables-complete-clippy.log`.
These ignored scoped results precede the commit gate. Shared expansion/output
budgets, repeated accessibility evaluation, broader CSS defaults/inheritance and
lexical parity, other host buffers, native targets and release acceptance remain
open.


## Commit verification — 2026-11-03

The full gate for the depth, potential-cascade and computed-variable batch passes
**8/8 steps**, including workspace all-target strict Clippy/build, no-std checks,
**3093 passing tests** and 16 ignored tests. The conformance inventory reports
123 passes, four known unsupported cases, zero regressions and zero infrastructure
failures; its baseline matches. Sources remained unchanged during verification.
Local report: `target/verification/20261103-004541-0we5hp3_/report.json`.
The subsequent edit only records these results in documentation.

The current real Chrome DOM boundary probe also passes with the fresh variable
oracle (`target/commit-20261103-browser.log`). This full gate does not erase the
separately failing WebKit variable comparison recorded above, establish a full
browser/platform matrix, or close the remaining development plan.


## Source-free CSS-variable acceptance — 2026-11-03

After `bfbeaf8`, a new generated-artifact probe closes the pending variable-specific
loader/Worker acceptance criterion. It builds a real v38 program with the current
CLI, deletes the source/configuration and serves only the generated directory.
The production CSP and closed shadow-root mode remain intact; a host-test handle
permits computed-style inspection. Requests outside the owned origin and page
errors fail the probe.

Chrome **156.0.8078.80** and Playwright WebKit **26.5** each pass initial style
checks and two trusted mouse turns. Coverage includes computed inherited alias
freezing, stylesheet replacement and removal, empty component substitution and
auto width, chosen numeric fallback validation, an ordinary dependency cycle,
an unused self-fallback and quoted font text in the private font namespace.
Removing the sheet then applying an inline variable reference selects the missing
alias fallback instead of retaining stale custom-property state.

The first Chrome run exposed an incorrect fixture width expectation: the content
box is 186px after subtracting margins, padding and borders from the 200px parent.
The expectation was corrected; no engine code, allowance or browser policy was
changed. The final checks also wait for both the committed title and the turn's
log marker to avoid observing an intermediate Worker message.

Evidence: `target/variables-bundle-chrome-final.log` and
`target/variables-bundle-webkit-final.log` (local, ignored). The earlier failing
Chrome fixture log remains `target/variables-bundle-chrome.log`. These are focused
acceptance results for new verification tooling; production code is unchanged
from the full gate recorded above. The independent complex outside-cycle WebKit
comparison still fails and remains an explicit compatibility gap. Firefox and the
full matrix, shared computed-output limits, broader CSS defaults/inheritance,
accessibility work, other host buffers and platform/release acceptance remain open.


## Empty CSS-reference fan-out — scoped verification 2026-11-03

Preparation for shared browser variable evaluation uncovered an independent native
resource gap. The arena retained edges to valid empty values. Repeatedly doubling
those references caused exponential materialization despite producing zero UTF-16
units, so the output-size ceiling could never stop it. The native append path now
omits zero-length fragments while retaining the valid empty binding. Dependency
resolution and cycle detection still execute; empty is not converted to missing.

A legal stylesheet with 2048 forward fan-out definitions reproduced the failure
before the fix: the isolated child consumed one CPU core until its 45-second test
watchdog killed and reaped it. After the fix, the focused regression passed in
0.12 seconds of harness execution, covering style-reading focus, snapshot,
layout, paint and stylesheet replacement. The initial post-fix integration run
had already completed substitution but failed its geometry assertion because the
fixture used a button without the explicit control metrics required by native
layout. A focusable div replaced that button; no production allowance or control
layout restriction changed. This is a regression result, not a sustained resource
benchmark or a complete renderer work budget.

Final focused verification passes **145 native view tests**, including seven
variable integration tests. The ten resolver unit checks were also rerun after
adding the empty-fragment/real-cycle case; quoted empty strings, component-token
boundaries and inherited CSS-wide keywords remain covered. **48 runtime/view
tests** pass, with the compiled-Wasm fixture extended to 32 doubled empty levels
across two turns. **142 oracle-enabled Node checks** pass. Strict all-target
Clippy passes for view/runtime, with view checked again after the final unit-test
addition. Formatting and whitespace checks pass.

The fresh oracle contains **21 variable fixtures**; Chrome passes all **231
computed-style observations**. WebKit agrees on the new empty-reference fixture
but its complete comparison still fails on the same two color observations in
the previously recorded outside-cycle case. No new browser-cycle compatibility
claim is made. Fresh CLI/companion builds pass. UARP passes **107 tests in 28
groups and two expected native views**. Chrome 156.0.8078.80 and Playwright WebKit
26.5 each pass the source-free generated loader/Worker probe, now including the
32-level empty fan-out margin check and the two trusted update/removal turns.

Evidence (local, ignored): `target/variables-empty-fanout-before.log`,
`target/variables-empty-integration.log` (the intermediate fixture failure),
`target/variables-empty-fanout-geometry.log`, `target/variables-empty-native.log`,
`target/variables-empty-unit-final.log`, `target/variables-empty-runtime.log`,
`target/variables-empty-node.log`, `target/variables-empty-chrome.log`,
`target/variables-empty-webkit.log`, `target/variables-empty-clippy.log`,
`target/variables-empty-view-clippy-final.log`,
`target/variables-empty-bundle-chrome.log`,
`target/variables-empty-bundle-webkit.log` and `target/variables-empty-pilot.log`.
The last full workspace gate remains the one recorded for `bfbeaf8`; these newer
engine changes have scoped evidence. Shared browser variable evaluation,
nonempty aggregate output/expansion budgets, accessibility work, other host
buffers, full browser/native platform acceptance and release criteria remain open.


## Browser computed custom-property scopes — scoped verification 2026-11-03

The official browser renderer now uses an engine-owned port of native computed
custom-property scopes instead of delegating cycle resolution to the CSS engine.
`view-variables.mjs` compiles quoted/component-preserving templates and executes
dependency resolution with explicit work stacks and indexed fragments. Scoped
checkpoints retain inherited aliases and restore siblings without copying ancestor
maps. UTF-16 length summaries check the shared per-value ceiling before expansion;
empty fragments remain valid without retaining exponential traversal edges.

`view-custom-properties.mjs` extracts custom declarations from unadopted prepared
CSSOM sheets, removes them before adoption and maintains candidate indices for
the current sheets. Browser-native selectors/media select winning definitions;
the scope algorithm determines computed custom values. Non-ASCII private literal
names are outside the guest grammar; original and private font bindings reference
those inherited literals. Missing standard references are masked from ambient page
variables. CSSOM pending shorthands require reading authored `cssText`, because
their longhand getters serialize as empty before substitution. Root rewriting now
skips quoted values instead of rewriting literal `:root` text.

Local pseudo-state changes coalesce into animation-frame refreshes, media queries
have owned change listeners and commits/focus effects refresh synchronously.
Replacement/removal and close/freeze release listeners; rejected preparations do
not install them. Instrumented live tests retain stable media-listener counts on
replacement/rejection and observe zero after removal/close. Frozen DOM retains its
last computed bindings. The pure Worker/model authority and snapshots remain
separate from renderer-private caches and DOM declarations.

Final focused evidence passes **149 oracle-enabled Node checks**, including six
new scope checks (dependency depth 5000, bounded empty fan-out, UTF-16 boundaries,
cycles, scope restoration and token/keyword semantics) and a quoted-root rewrite
check. **35 project browser checks**, strict project all-target Clippy, formatting
and whitespace checks pass. Fresh CLI/companion builds pass.

Chrome **156.0.8078.80** and Playwright WebKit **26.5** each match **23 native
fixtures and 253 computed-style observations**, including oversized ASCII/UTF-16
custom values. Each also passes **34 live observations** covering hover/active,
focus-visible via View focus, checked/disabled state, viewport/color-scheme media,
reparenting and detached-component scope changes, rejected-sheet rollback,
removal, ambient-variable isolation (including a shorthand), quoted packaged-font
names, implicit-root selector separation and media-listener lifecycle. Platform-only controls still expose WebKit's
raw outside-cycle difference; the official host now normalizes both tested engines
to the native algorithm. The old failing host comparison is not silently skipped.

Each selected engine passes **15 UARP official-host mockup projections and 59
regressions**, the source-free compiled variable bundle with the complex
outside-cycle consumer, and live compiled chat (ordered native tree, selected
accessibility and 195 declarations across 14 properties). The broader Chrome
boundary probe passes 127 work/payload/depth/cascade batches, 1839 radio checks,
six activation and eleven select identity/event scenarios. Fresh generated CSS,
depth and cascade failure probes retain prior DOM/title and terminate before
catch/logging. The named-asset mockup server test passes after adding the two new
public dependencies to its allowlist.

Intermediate failures are retained: the first scope test attempted non-LIFO
checkpoint restoration and was corrected to restore the grandchild first. A live
fixture tried to assert a styled border on a native-appearance checkbox; the same
cycle assertion now uses a div, with the original computed custom value already
confirmed correct. WebKit's macOS Tab policy skipped that checkbox, so focus-visible
coverage now uses a text input and the View focus command. A further live check reproduced class rules matching the implicit root and
therefore inheriting ambient page color, despite the native matcher excluding
those rules. A zero-specificity private selector guard now excludes the root from
class rules; both engines retain the original root color and no class-defined
custom binding. The initial mockup
loads timed out because their server omitted the new public modules; its explicit
allowlist was updated, and both engines then passed. No source, fuel, memory,
admission allowance or production layout restriction was raised for these passes.

Evidence (local, ignored): `target/variables-owned-unit-final.log`,
`target/variables-owned-node-canonical.log`,
`target/variables-owned-project-canonical.log`,
`target/variables-owned-clippy-canonical.log`,
`target/variables-owned-chrome-canonical.log`,
`target/variables-owned-webkit-canonical.log`, `target/variables-owned-dom-canonical.log`,
`target/variables-owned-mockup-server.log`,
`target/variables-owned-mockups-chrome-canonical.log`,
`target/variables-owned-mockups-webkit-canonical.log`,
`target/variables-owned-bundle-chrome-canonical.log`,
`target/variables-owned-bundle-webkit-canonical.log`, `target/variables-owned-chat-chrome-canonical.log`,
`target/variables-owned-chat-webkit-canonical.log`, `target/variables-owned-css-failure-canonical.log`,
`target/variables-owned-depth-failure-canonical.log` and
`target/variables-owned-cascade-failure-canonical.log`. Earlier `variables-owned-*` failure
logs retain the diagnostic steps above. The last full workspace gate remains
`bfbeaf8`; the subsequent empty-reference and browser-scope packages have focused
evidence only. Complete late-value typed-subset/default/inheritance/lexical parity,
aggregate output/work/RSS guarantees, accessibility work, other host lifetimes,
Firefox/full browser matrix, native platform acceptance, metrics and release
criteria remain open.

## Potential expanded component values — scoped verification 2026-11-03

A shared 8 MiB potential expanded-component byte allowance now runs after cascade
admission at each style-reading command and final publication. Native and browser
implementations cache numeric value programs and walk every component with
iterative dependency graphs and inherited scope checkpoints. The estimate keeps
inactive candidate and fallback alternatives, charges fresh custom content and
per-property maxima, and avoids copying pure aliases. Excess rejects before
external effects, with the command index or no index for final validation.

Focused evidence: four common estimator tests; six native integration checks;
six Node expansion checks, including 1458 comparisons of potential bounds with
actual candidate-world resolution; 151 complete native view tests; 49 runtime/view
tests; 156 oracle-enabled Node checks; strict all-target Clippy for the four
affected crates; no-default-features common/intrinsics checks; five registry and
11 embedding-documentation tests. The Chrome DOM probe passes 144 admission
batches, 1839 radio checks, six activation scenarios, eleven select scenarios and
eight disabled-select batches. Chrome/WebKit each retain 23 native variable
fixtures, 253 computed-style and 34 live observations, plus 15 mockup projections
and 59 regressions. UARP passes 107 tests in 28 groups and two view snapshots.
The fresh source-free expanded-value failure probe retains previous DOM/title and
terminates before guest catch/logging.

An intermediate regression exposed an underestimate for ordinary stylesheet
literals exceeding the custom-value ceiling: the ordinary no-reference fast path
retains those literals. The estimator now records full UTF-16 source length in
that context, preserving the accepted language instead of imposing a new
per-value restriction. Native, Node and native/Chrome oracle cases retain this
regression. The prior failing Node log remains in
`target/expansion-literal-before.log`.

Evidence: `target/expansion-common-unit-final.log`,
`target/expansion-native-complete.log`, `target/expansion-runtime.log`,
`target/expansion-node-complete.log`, `target/expansion-dom-complete.log`,
`target/expansion-clippy-complete.log`, `target/expansion-no-std.log`,
`target/expansion-registry.log`, `target/expansion-docs.log`,
`target/expansion-vars-chrome.log`, `target/expansion-vars-webkit.log`,
`target/expansion-mockup-chrome.log`, `target/expansion-mockup-webkit.log`,
`target/expansion-pilot.log` and `target/expansion-bundle.log`.
These are scoped results; the full commit gate is recorded separately under
`target/verification/`. Serialized output/history/backend/RSS limits, late-value
subset parity and complete browser/native platform and release acceptance remain
open.

## Commit boundary 01acc09 — full verification 2026-11-03

The empty-reference, browser computed custom-property and potential expansion
packages were committed as `01acc09` after the full eight-command gate passed:
**3106 tests passed, 16 ignored**, source fingerprint unchanged. Conformance
remains 123 passed and four known unsupported out of 127, with a matching baseline
and no regressions. The fresh source-free positive variable bundle also passed in
Chrome 156.0.8078.80 and Playwright WebKit 26.5. `.claude/` was excluded and no push
was performed. Evidence: `target/verification/20261103-042426-afqeylv0/report.json`,
`target/commit-css-bundle-chrome.log` and `target/commit-css-bundle-webkit.log`.
This is the last full gate before the accessible-name work below.

## Potential accessible-name work — scoped verification 2026-11-03

A shared final-inventory allowance now checks potential name evaluation before
publication. Iterative descendant summaries distinguish relation-disabled and
relation-enabled traversal, include repeated `aria-labelledby` tokens and native
host labels, and reserve future text-field/select capacity. All components are
included. Hidden state, blankness, current local edits and author precedence do
not discount eligible branches. Root content is eligible only for roles that name
from content, using shared role tables emitted into the browser registry. Excess
rejects the copied transaction with no operation index before external effects;
transient excess can be repaired before final validation.

Native snapshots now index attached ids, labels in document order and own/ancestor
visibility once per snapshot. Name-from-content traversal retains an ordered
fragment range until it finishes, eliminating repeated text concatenation at
every ancestor. Indices do not survive a snapshot. Existing hidden-ancestor,
cycle, placeholder and root/descendant rules are preserved.

Scoped evidence passes **158 native view tests**, including a **144-world**
comparison of computed element-name sizes with the numeric bound, and six new
integration checks. The final six-test naming suite was rerun after adding
all-content-role rejection and one-content-role acceptance to the maximum-depth
fixture. **50 runtime/view tests** and **162 oracle-enabled Node tests** pass.
The Worker deferred-input check admits/refuses identically before and after a
full-size local Unicode field edit is delivered; a rejected batch is never sent.
Strict all-target Clippy passes for the four affected crates, with a final
view-only check after test/oracle extensions. Registry tests **5**, embedding
documentation **11**, no-default-features common/intrinsics, formatting and
whitespace checks pass. CLI/companion binaries were freshly built.

The native oracle adds five naming cases with **25 batches**: exact boundary,
rollback/reclamation, detached/hidden candidates, embedded-field capacity, native
host-label references and author-only/content-role transitions. Chrome passes
**169 resource batches**, 1839 radio observations, six activation scenarios,
eleven select identity/event scenarios and eight disabled-select batches.
Chrome 156.0.8078.80 and Playwright WebKit 26.5 each pass **15 official-host mockup
projections and 63 regressions**, including naming refusal, DOM/model/title
rollback, node identity and a smaller retry. Both pass the positive source-free
variable bundle and live compiled chat (ordered native tree, selected
accessibility and 195 declarations across 14 properties). A fresh source-free
naming-failure application terminates in the Worker before guest catch/logging,
retaining the previous DOM/title. Normal UARP passes **107 tests in 28 groups and
two view snapshots**.

Two estimator refinements preserve admitted behavior without increasing limits.
An initial inventory counted static source bytes again and conflicted with the
existing retained-text boundary. The final metric charges the fixed inventory
walk and repeated element-name reads, leaving one-time source/raw-text publication
to the retained-source allowance. A new depth-512 fixture with a full-size text
leaf reproduced overcounted content reads for author-only `list` roles. Root
content now follows the common name-from-content table; references/descendants
still retain their content rules. Turning all those roles into `cell` rejects,
while turning only one does not. Initial naming fixtures also had an unsupported
explicit role and text under the wrong referenced branch; their correction kept
the checked role whitelist and relation semantics intact. Diagnostic logs remain
in `target/naming-native-fixture-before.log`, `target/naming-native-all.log`,
`target/naming-node-static-before.log` and `target/naming-author-only-before.log`.

Final evidence: `target/naming-native-complete.log`,
`target/naming-native-focused-complete.log`, `target/naming-runtime-complete.log`,
`target/naming-node-oracle-complete.log`, `target/naming-worker-complete.log`,
`target/naming-dom-complete.log`, `target/naming-clippy-complete.log`,
`target/naming-view-clippy-final.log`, `target/naming-cli-complete.log`,
`target/naming-registry-complete.log`, `target/naming-docs-complete.log`,
`target/naming-no-std-complete.log`, `target/naming-format.log`,
`target/naming-mockup-chrome.log`, `target/naming-mockup-webkit.log`,
`target/naming-bundle.log`, `target/naming-positive-chrome.log`,
`target/naming-positive-webkit.log`, `target/naming-chat-chrome.log`,
`target/naming-chat-webkit.log` and `target/naming-pilot.log`.
These are scoped results after `01acc09`, not a new full-workspace pass. No source,
fuel, deadline, memory or existing admission allowance was increased. Complete
serialized output/frame/backend quotas, browser accessibility-engine CPU and
platform accessibility APIs, RSS, late-value CSS parity and complete browser/native
platform and release acceptance remain open.

## Bounded view diagnostic export and effects — scoped verification 2026-11-03

Native view/application APIs now provide compact and pretty diagnostic JSON under
a separate draft **16 MiB encoded-byte allowance**. The CLI's scripted-view path
uses the bounded encoder. UTF-8, keys, escaping, punctuation and indentation count
before output-fragment allocation/copy; geometric growth caps requested capacity
without per-character reallocations. Export returns a complete document or a
typed limit/allocation/encoding error, with no exposed partial buffer. Export
refusal leaves application state and events intact. CLI refusal retains guest logs
and requested budget diagnostics, prints no partial snapshot/completion marker,
and returns the export error. Structured inspection and caller-owned copies/custom
formatting remain separate from this serialized-buffer allowance.

The simulated platform's successful-open diagnostics now retain the newest
**64 complete URLs within 1 MiB of raw UTF-8**, evicting oldest entries before
retaining a new URL. `openedDropped` is added only after a drop and saturates.
Permission/platform refusals do not enter the successful history. Retention never
prevents a later authorized action. Prompt counts also saturate. Existing ordinary
snapshot fields/values remain unchanged when no records have been dropped.

Final scoped evidence passes **166 native view tests**, including four encoder
boundary/capacity checks, a counter/oversized-entry check and three native export/
effect integrations. **51 runtime/view tests**, **7 CLI view scenarios** and
**162 oracle-enabled Node tests** pass. The CLI scenarios cover ordinary scripted
views, virtual timers, determinism, explicit network waiting, capability refusals,
native packaging/browser-view build and the new export refusal/recovery path.
Strict all-target Clippy passes for common/view/runtime/project/CLI; fresh CLI and
companion builds, registry **5**, embedding documentation **11**, no-default-features
common/intrinsics, formatting and whitespace checks pass. The normal UARP CLI
path passes **107 tests in 28 groups and two view snapshots**. No new real-browser
style/control matrix is inferred from the Node run; browser applier logic is
unchanged by this native diagnostic increment.

The deep/wide fixture retains 500 span levels and 3000 text leaves. Pretty export
exceeds the allowance; compact export succeeds. A compiled TSX instance builds in
110 bounded timer turns under normal guest budgets, remains running after export
refusal and handles a later clear click, after which pretty export succeeds. The
CLI exercises the same source with an explicit timer grant. Earlier fixture
failures are retained: one monolithic native batch exhausted the existing select
work allowance, so construction now uses bounded batches; the compiled fixture
initially omitted its timer grant and used an input identifier without the required
`#` prefix. Corrections preserved the intended scene, current admission rules and
all production source/fuel/deadline/memory allowances. Logs:
`target/snapshot-export-batch-before.log`,
`target/snapshot-runtime-permission-before.log` and
`target/snapshot-runtime-target-before.log`.

Frame-retention source inspection confirms the desktop worker's existing
`in_flight`/presentation acknowledgement gate: no next raster is built until the
outstanding frame is acknowledged, alongside the GUI's current frame. There is no
archived-frame vector. Existing raster checks cap dimensions at 4096×4096 and
16,777,216 pixels. This is source evidence for count/dimension invariants, not a
new GUI slow-consumer or cross-platform acceptance run, and not a total quota for
geometry, glyph/font caches, surface buffers or RSS.

Evidence (local, ignored): `target/snapshot-encoder-final.log`,
`target/snapshot-export-final.log`, `target/snapshot-runtime-complete.log`,
`target/snapshot-cli-complete.log`, `target/snapshot-native-complete.log`,
`target/snapshot-runtime-view.log`, `target/snapshot-cli-view.log`,
`target/snapshot-node.log`, `target/snapshot-clippy.log`,
`target/snapshot-cli-build.log`, `target/snapshot-registry.log`,
`target/snapshot-docs.log`, `target/snapshot-no-std.log`,
`target/snapshot-format.log` and `target/snapshot-pilot.log`.
The last full workspace gate remains `01acc09` (3106 passed, 16 ignored); these
newer engine/tooling changes have scoped evidence. Aggregate structured-graph/
backend/RSS accounting, repeated backend work, native input/GUI acceptance,
late-value CSS parity, full browser/native platform matrices, measurements and
release criteria remain open.

## Native GUI input backlog — scoped verification 2026-11-03

The desktop preview now routes all key/character/IME and focus-loss cleanup
producers through one bounded `PendingKeys` controller. The 64-record GUI cap,
eight-message worker channel and 4096-byte ordinary key/text guards are preserved
and centralized. A separate 1 MiB aggregate owned-text allowance bounds retained
payloads. Admission checks prospective count/bytes before copying/compacting or
mutating the backlog. Only a consecutive preedit for the same node and guest
commit binding can replace its predecessor; replacement refunds its old capacity.
A refused append/replacement leaves the prior FIFO and preedit intact.

Incoming strings release spare allocation capacity on admission through boxed
string conversion. Retained string capacity therefore equals its UTF-8 length;
short valid text does not cause a GUI failure merely because a platform string
had excessive spare capacity. Ordinary input and IME commits are never coalesced
or truncated. Pop, clear and stale-binding retention release accounting. Overflow
uses the preview's existing explicit queue-full error/exit behavior.

**11 desktop tests pass**: six private-controller quota/order/lifetime checks,
three checks using the actual preview enqueue/flush methods with bounded channels
and synthetic CPU frames but no OS window, and the two existing IME/focus tests.
Coverage includes exact aggregate capacity and one-byte excess; count capacity
with valid replacement; replacement refusal/refund; binding boundaries; clear/
retain; complete maximum-size Unicode commits; valid text with spare platform
allocation; no send before the first frame; at most one key before a new frame;
and stale IME discard followed by successful reuse of capacity. Desktop all-target
strict Clippy and a fresh `varta-preview` build pass, as do formatting/whitespace
checks. Initial Clippy feedback concerned collapsible event-handler branches;
the branches were simplified without suppressing warnings. The final tests were
rerun after normalization replaced a more restrictive initial spare-capacity
refusal, preserving valid input rather than narrowing that behavior.

Pinned-source inspection confirms Parley 0.11.1 `shape/mod.rs` uses three
16-entry LRUs for shaping data, instances and plans. Its `LayoutContext::begin`
clears resolution, style, inline-box, analysis and bidi scratch before reuse;
ranged construction prunes font sources. `TextMeasurer` supplies fixed explicit
fonts under existing count/byte/name/alias bounds with system-font discovery
disabled. No production shaping code, cache reset or allowance was changed.
These are entry/lifecycle observations, not a full cache-byte, glyph-work, GUI
slow-consumer, OS IME, cross-platform or RSS measurement.

Evidence (local, ignored): `target/input-queue-check.log`,
`target/input-queue-normalized-tests.log`,
`target/input-queue-normalized-clippy.log`,
`target/input-queue-normalized-build.log` and `target/input-queue-format.log`.
The last full gate remains `01acc09`; no unchanged engine/browser/CLI suites were
repeated for this desktop-only increment. Other backend buffers/caches, native
rendering/platform acceptance, late-value CSS parity and release criteria remain
open.


## Consolidated commit gate — 2026-11-03 (`ab045dc`)

The accessible-name admission/indexing, bounded native JSON/export diagnostics
and native GUI input-backlog packages were committed together after one full
`python3 scripts/verify.py full` run. All eight commands passed: formatting, core
checks, no-default-features checks, companion build, strict workspace/all-target
Clippy, workspace/all-target build, workspace tests and conformance. **3133 tests
passed, 16 were ignored**. Conformance retained **123 passed / four known
unsupported / 127 total**, matched the baseline and reported no regressions,
unexpected support or infrastructure failures. The source fingerprint remained
unchanged. The separate browser naming/session selection passed **19 tests**.
No push was performed; `.claude/` was excluded from the commit.

Evidence (local, ignored):
`target/verification/20261103-064728-vqi3ny2f/report.json`,
`target/commit-verification.log` and `target/commit-browser-check.log`.
This gate covers `ab045dc`; it does not cover subsequent renderer changes.

## Native field layout reuse — scoped verification 2026-11-03

Native nonempty fields now pass the layout already used for caret, selection,
composition underline and hit positions directly to glyph painting. Previously,
painting cloned the complete display into a temporary `TextShape` and built a
second Parley layout while the first was live. The shared glyph painter now
accepts an existing layout; paragraph painting still shapes its own text once.
Ordinary field values are borrowed through `Cow`; only composition replacement
or password masking needs an owned display. Placeholders are shaped from their
borrowed attribute string without a temporary text copy. No persistent layout
cache, shaper/paint allowance increase, or source truncation was introduced.

**166 native view tests pass**, including password/plain display comparisons,
UTF-8 editing/hits, combining preedit, selection/undo, multiline caret/scroll,
select popup/control rendering, text shaping and existing work-limit boundaries.
Strict all-target view Clippy, a freshly built `varta-preview`, formatting and
whitespace checks pass. No unrelated engine/Node/CLI or full workspace repeat was
performed for this private native-painting change. These checks verify the
existing observable rendering/editing contract; the reduction from two
simultaneously retained field layouts to one is source evidence, not a measured
RSS/cache-byte/performance result or a new OS/window/platform matrix.

Evidence (local, ignored): `target/render-reuse-view.log`,
`target/render-reuse-clippy.log` and `target/render-reuse-preview.log`.
The preceding complete gate is recorded separately at `ab045dc`.

The public shaper still limits input to 65536 UTF-8 bytes. A valid 32768-unit
UTF-16 field or a 32768-grapheme password mask can occupy 98304 UTF-8 bytes;
IME replacement is independently checked against the combined value bound. At
this checkpoint the existing maximum-password test still recorded a rendering
refusal under the byte limit. That mismatch is addressed by the subsequent
Unicode-field package below; complete backend acceptance is broader than its
maximum-size fixtures. Other
backend-buffer/cache/RSS, native/browser platform, late-value CSS and release
acceptance also remain open.


## Maximum Unicode native field display — scoped verification 2026-11-03

Private native shaping options now carry the text source. General/public
measurement retains its 65536-byte UTF-8 allowance. Fields recheck the existing
32768-UTF-16-unit semantic bound and its derived 98304-byte UTF-8 reservation
before layout construction. This supports the already-admitted Unicode value,
mask or placeholder without widening general text admission or leaving mutable
budget state in the reused shaper. Content-sized textarea options retain this
classification through intrinsic and width-constrained remeasurement. The
existing glyph, outline, aggregate layout/paint, guest, image and font limits
are unchanged. A whole field remains one contextual shaping input.

The first focused run (without a dependency correction) passed four prior field
tests but failed all three initial maximum-field regressions: Parley 0.11.1
stores run-local cluster byte offsets as `u16`, so long UTF-8 runs wrapped those
offsets and caused invalid UTF-8 slicing in line-hit extraction. Merely accepting
more bytes in the view shaper did not fix this. The pinned crate is now supplied
through `[patch.crates-io]` from `vendor/parley`, excluded from workspace members.
Only its private offset field and one construction cast change to `u32`; the
rest of its source and manifest are identical to the release. Both upstream
licenses, patch diff, original package/data hashes and removal criteria are
included. The lockfile changes source selection only, without version updates.
A check of the cached original release archive against the preceding lockfile
SHA-256 and all **45 copied files** confirms this provenance and exact patch.

**181 native tests pass: 170 view and 11 desktop**. The focused field/text
selection passed **11 tests**; all **eight field tests** were repeated after
Clippy replaced a test's forward `.last()` scan with `.next_back()`. Coverage
includes maximum BMP/astral/combining values and password masks; first/last source
positions, end caret and subsequent grapheme deletion; one-unit excess rollback;
full IME selection replacement, unchanged value before commit and later combined-
value overflow cancellation; Unicode placeholders and content-sized textarea
wrap/measure/paint. Large mixed-direction suffix pixels, caret geometry and
shifted source hits match a short reference. A subsequent public call on that
same shaper still refuses the large text under the general byte limit.

Strict all-target view/desktop Clippy passes, as do a fresh `varta-preview`
build, formatting and whitespace checks. The local dependency emits its unchanged
upstream ICU `to_icu4c_value` deprecation warning; it was not suppressed or changed
as part of this two-line patch. No browser/Node/CLI/full-workspace repeat or
real-window/OS/platform result is inferred from these native checks. The last
complete workspace gate remains `ab045dc`; this package is newer scoped work.

Evidence (local, ignored): `target/field-unicode-focused.log` (initial failure),
`target/field-parley-lock.log`, `target/field-parley-source-diff.log`,
`target/field-parley-release-check.log`, `target/field-unicode-patched.log`,
`target/field-unicode-native.log`, `target/field-unicode-clippy.log` (initial test
lint), `target/field-unicode-clippy-final.log`,
`target/field-unicode-fields-final.log`, `target/field-unicode-preview.log` and
`target/field-unicode-format.log`.

The previous note suggesting a larger IME overlay was corrected after inspecting
the existing combined prefix/replacement/suffix UTF-16 guard; this package adds
no larger transient allowance. Font-specific glyph/outline expansion, complete
bidi navigation, aggregate graph/cache/backend/RSS bounds, real-window IME and
native/browser matrices, late-value CSS and release acceptance remain open.


## Large native glyph clusters — scoped verification 2026-11-03

Complex-font probes exposed three additional Parley 0.11.1 assumptions: the
`u8` glyph count collided with the inline marker (the first regression observed
one glyph instead of 255), a contributing-character `u8` panicked on a long
grapheme, and source inspection found font coverage zipped into only 32 scratch
slots. The private glyph count is now `u32` with a separate `u32::MAX` inline
marker; contributing/coverage counts use `usize`. Coverage visits the full
cluster. A second pure nominal-mapping pass updates IDs only for a better font,
preserving losing-candidate state without a dynamically enlarged scratch buffer.
Pairwise normalization's independently bounded two-character form is unchanged.

Two original synthetic OpenType fonts, regenerated byte-for-byte with fontTools
4.60.2, provide zero-advance combining glyphs, known three-glyph `ccmp` expansion
and a partial first font missing the last character of a long cluster. Two
focused shaping checks verify actual counts 254/255/256/257 and 901, and fallback
coverage after 31/32/64/253/256/1024 prefix marks. A focused field test accepts
65536 painted glyphs, refuses 65539 and then renders a smaller value. Every render
attempt preserves the guest value. Existing glyph, outline, input/font and
aggregate work budgets are unchanged; no text or glyphs are silently truncated.

**184 native tests pass (173 view, 11 desktop)**. Strict all-target view/desktop
Clippy, a fresh preview binary, formatting/whitespace checks and deterministic
font regeneration pass. The same unchanged upstream ICU deprecation warning is
visible in the vendored dependency. Archive SHA-256 and file comparison confirm
only four changed upstream source files; the rest of the copied source, manifest
and licenses match the release. `VARTA-PATCH.md` and the current full diff record
the expanded patch. No dependency versions changed. No unrelated browser/Node/
CLI/full-workspace or real-window/platform coverage is claimed; the last full
workspace gate remains `ab045dc`.

Evidence (local, ignored): `target/cluster-glyph-before.log`,
`target/cluster-budget-before.log` and `target/cluster-budget-bypass-before.log`
(counter panic; no release-mode budget bypass was claimed),
`target/cluster-glyph-fixed.log`, `target/cluster-text-final.log`,
`target/cluster-budget-fixed.log`, `target/cluster-fixture-reproducibility.log`,
`target/cluster-vendor-diff.log`, `target/cluster-native.log`,
`target/cluster-clippy.log`, `target/cluster-preview.log` and
`target/cluster-format.log`.

Pre-paint allocation/CPU and retained shaping buffers after a refused frame still
need separate accounting/measurements. These count/mapping fixes do not establish
a complete cache/backend/RSS quota, full bidi navigation, OS IME/native/browser
platform matrices, late-value CSS parity or release acceptance.


## Bounded native shaping requests — scoped verification 2026-11-03

Native shaping now checks explicit completion before copying glyphs into layout
or calculating metrics. The pinned HarfRust 0.12.0 local patch adds optional
intermediate-array/charged-operation allowances, result status/statistics and
bounded fallible capacity growth. Existing spare capacity is reused. Parley's
request controller reserves Unicode scalars rather than UTF-8 byte slots,
accounts completed glyphs/charged operations across runs, clears incomplete
layout contents and releases refused or oversized reusable glyph arrays.
Context cloning preserves configured resource policy while dropping scratch.
Varta refuses before rendering any partial result.

Draft request settings: 65536 completed glyphs across a layout (the existing
paint allowance, now checked before layout copying), 262144 slots per
intermediate glyph-info/position array, an initial one-million charged-operation
allowance and 65536 retained slots per array. Intermediate expansion and final
output are distinct bounds. Source/font/image/guest allowances are unchanged;
normal maximum ASCII and Unicode/password/IME/textarea fixtures remain accepted.

**188 native tests pass (177 view, 11 desktop)**. The focused lib/field/text
selection passed **42 tests**; **three policy checks** were repeated after
preserving limits in context clone. Final strict all-target view/desktop Clippy,
fresh preview, workspace and modified-vendor formatting, whitespace and all
three deterministic synthetic-font regenerations pass. No unchanged browser/
Node/CLI/full-workspace or real-window/platform repeat is inferred. The last
complete workspace gate remains `ab045dc`.

Actual dependency probes verify a 64-slot array boundary and overflow, aggregate
129/128-glyph script-run boundary, operation exhaustion/recovery and cache release.
A 16× expansion refusal reaches **10485760 array-capacity bytes**, retains no
glyph-array slots and permits a later small request. Valid 65536-glyph output
uses **21846 charged operations** and **3495360 capacity bytes**, with oversized
scratch dropped; a small retry uses **800 capacity bytes**, retaining 20 slots
per array. Measurements caught unnecessary geometric reservation within already
available capacity during development; it was corrected before final checks.

The new HarfRust vendor contains **80** release files and changes only two source
files. Parley's **45** copied files change only six source files including the
previous cluster fixes. Archive hashes, untouched manifests/licenses and all
source differences agree with the documented patches. Dependency versions are
unchanged. Both upstream license sets/provenance and full current diffs are kept.
Unmodified upstream warnings (HarfRust's unused legacy function and Parley's ICU
deprecation) remain visible, without suppression.

Evidence (local, ignored): `target/shaping-budget-lock.log`,
`target/shaping-budget-focused-all.log`, `target/shaping-budget-policy-final.log`,
`target/shaping-budget-native.log`, `target/shaping-budget-clippy-final.log`,
`target/shaping-budget-preview-final.log`, `target/shaping-budget-format.log`,
`target/shaping-budget-vendor-final.log` and
`target/shaping-budget-fixture-reproducibility.log`.

Array capacities exclude allocator metadata, overlap during reallocation and
other font/analysis/plan/layout/cache buffers; they are not total RSS. Operation
allowances are exhaustion thresholds: upstream batch debits can report more than
the configured allowance on a failed request (44 charged units versus an
allowance of 43 in the multi-run probe). Partial output is rejected, but strict
pre-charge accounting remains open. The meter also omits some parser/Unicode/
plan work and is per layout, not a complete frame CPU/deadline boundary. Complete
backend/cache/RSS, real-window/native/browser matrices, late-value CSS and release
acceptance remain open.


## Strict native shaping pre-charge — scoped verification 2026-11-03

HarfRust's existing operation debits now route through one checked helper.
Conversion/affordability are checked before subtraction; refusal is sticky,
leaves unused allowance unchanged and executes no unfunded corresponding batch.
Buffer mask/cluster writes return before their charged work on refusal. Forward
shifts reserve work before capacity growth. Contextual lookup recursion charges
before cursor movement. Kern/kerx/morx state drivers charge transitions before
flags/actions, never force advancement on exhaustion, and use the same helper
for current/marked insertion counts. Exact zero remaining is accepted when no
further charged work is requested. All configured resource allowances remain
unchanged. This fixes the preceding 44-unit/43-allowance overspending behavior.

The multi-script probe now completes with exactly 44 units and refuses at 43
without spending beyond 43. Five original AAT fixtures use real state tables:
current insertion yields four glyphs with six units; marked+current insertion
yields seven with eight; legacy kern and extended kerx yield one with three
units each. Every lower budget refuses without overspending, clears reusable
glyph arrays, and the exact budget reproduces completed metrics. A nonadvancing
morx loop refuses exactly at budgets 1/2/8/64; a normal space request then works.
All five font files regenerate identically with fixed timestamps and original
outlines, without copied platform font data.

Focused OpenType/resource (**6**) and AAT (**2**) tests pass. **190 native tests
pass (179 view, 11 desktop)**, with strict all-target view/desktop Clippy, a fresh
preview, workspace and modified-vendor formatting/whitespace checks. Source
inspection finds one direct counter subtraction inside the checked helper, no
remaining counter-driven forced advancement, and no direct debits elsewhere.
Archive/source/manifest/license comparison matches the current six-file
HarfRust and six-file Parley patches; dependency versions are unchanged.
Unmodified upstream dead-code/deprecation warnings remain visible. The last
full workspace gate remains `ab045dc`; no unrelated browser/Node/CLI or full
platform/window matrix is inferred from this scoped native package.

Evidence (local, ignored): `target/shaping-charge-focused.log`,
`target/shaping-charge-aat.log`, `target/shaping-charge-aat-complete.log`,
`target/shaping-charge-fixtures.log`, `target/shaping-charge-vendor.log`,
`target/shaping-charge-native.log`, `target/shaping-charge-clippy.log`,
`target/shaping-charge-preview.log` and `target/shaping-charge-format.log`.

The exact bound concerns the existing charged meter, including newly charged
state transitions. Nominal coverage, parser/analysis/plan work, some range
extension/outline loops and frame-wide repeated work still need separate
accounting; this is not a complete CPU/deadline guarantee. Complete cache/
backend/RSS, native/browser platform matrices, late-value CSS and release
acceptance remain open.


## Shared native view shaping scope — scoped verification 2026-11-03

Native rasterization, geometry queries and layout-bearing commits now establish
one outer charged-shaping scope per text measurer. Intrinsic measurement, Taffy
remeasurement, painting, scaled frames and multiple layout commands share its
16777216-operation allowance. Nested calls and handled refusals do not replenish
it. Each build additionally respects the existing per-layout limits through a
temporary operation ceiling. Charged failed work is deducted before returning.
An owned guard clears request state on ordinary/error/unwind exit, with
pointer-sized atomics preserving the existing thread traits. The diagnostic
`last_view_shaping_operations()` reports the last outer request using the
measurer; standalone measurement has its own profile and leaves that diagnostic
unchanged. Budget-state cleanup does not establish general font-cache unwind
safety.

Four focused checks pass through actual view APIs and AAT/Taffy shaping:
geometry costs 16 charged units, a frame 24 and two layout commands 32. Exact
allowances succeed, insufficient ones refuse across geometry/paint and scaled/
box paths, snapshots stay unchanged, and deferred clipboard writes occur only
once after a funded complete commit. Nested/caught failures cannot reset quota;
independent retries work. A deliberate caught panic after completed shaping
verifies guard cleanup. Initial fixture compilation used a nonexistent batch
URL helper; it was corrected to the supported deferred clipboard operation.
Pointer-width type mismatches during the atomic adjustment were corrected before
final checks.

**194 native tests pass (183 view, 11 desktop)**. Strict all-target view/desktop
Clippy, a fresh preview, workspace and modified-vendor formatting, whitespace and
archive/source/manifest/license checks pass. Dependency versions remain unchanged;
only the recorded six Parley and six HarfRust source files differ from their
checksum-pinned releases. Existing upstream warnings remain unsuppressed.
Evidence (local, ignored): `target/frame-shaping-focused-pointer-atomic.log`,
`target/frame-shaping-native.log`, `target/frame-shaping-clippy.log`,
`target/frame-shaping-preview.log`, `target/frame-shaping-format.log` and
`target/frame-shaping-vendor.log`.

The last full gate at this scoped checkpoint remains `ab045dc`. This counter
adapter does not claim browser/Node/ABI changes, total CPU/RSS, full font-cache
accounting, real-window/platform or release acceptance. Unmetered parser/plan/
Unicode/coverage/outline work and known-failed measurement retries remain separate
work. Development was paused after these checks; only result documentation and
commit-boundary verification were requested next.

## Native text/shaping checkpoint — full verification 2026-11-03

The consolidated commit gate passes on macOS arm64: **3150 tests passed,
0 failed, 16 ignored**, with all eight verification commands successful and an
unchanged source fingerprint throughout. This includes the native text/shaping
and shared view-request accounting increments recorded above. The compatibility
inventory remains **123 passed and 4 known unsupported out of 127**; the baseline
matches, with no regressions or infrastructure failures. Existing upstream
HarfRust/Parley warnings remain visible.

Evidence (local, ignored):
`target/verification/20261103-092401-wtk_ap8t/report.json` and
`target/native-shaping-commit-verification.log`. Only result documentation was
updated after this successful gate; code and build inputs stayed unchanged.
Development remains paused. This commit checkpoint does not close the remaining
unmetered shaping work, total memory/CPU accounting, browser/pilot or native
platform matrices, full L0 styling/accessibility, distribution and release
acceptance.

## Native layout measurement failure — scoped verification 2026-11-03

Taffy remeasurement previously continued creating text builders after the first
measurement failure, although the complete geometry was already destined for
rejection. A new regression with eight actual content-sized textarea nodes and
the original marked-insertion AAT font observed **16 builds instead of 9** before
the fix. A test-only counter observes real ranged-builder construction; no mock
shaper or timing threshold is involved.

The measurement callback now retains its first failure and returns intrinsic
fallback sizes without subsequent text preparation. Taffy finishes its remaining
traversal; its fallback geometry is discarded before returning an error. The
expanded tests exercise geometry, box queries, ordinary and scaled rasterization,
and a layout-command transaction. They require exactly eight successful intrinsic
builds plus one refused width-constrained build, the original text-limit
diagnostic, no geometry/image publication, unchanged snapshots, and an independent
funded retry with identical pixels. A failed two-command batch stops at its first
layout command, retains title/commit count/model and makes no deferred clipboard
write; its funded retry writes once. All existing production limits are unchanged.

**6 focused shaping-scope tests** and **196 native tests (185 view, 11 desktop)**
pass with no failures or ignored native tests. Strict all-target view/desktop
Clippy, a fresh preview build, workspace formatting and whitespace checks pass.
Existing upstream dependency warnings remain visible. The focused log includes
the earlier intentional caught unwind probe; it passes. No vendor sources,
dependency versions, public API or ABI changed.

Evidence (local, ignored): `target/layout-failfast-before.log` (reproduced
failure), `target/layout-failfast-focused.log`, `target/layout-failfast-native.log`,
`target/layout-failfast-clippy.log`, `target/layout-failfast-preview.log` and
`target/layout-failfast-format.log`. The last full gate remains the `1111baa`
checkpoint; these are scoped results for newer work. No new real-window,
browser/Node, cross-platform, total CPU/RSS or release acceptance is inferred.
Unmetered shaping work, full L0 styling/accessibility and the platform/release
queue remain open. The plan's compatibility table also now acknowledges the
already implemented native v38 HTTP adapters, official browser bundles and
experimental desktop preview instead of listing those as wholly future work.

## Native Unicode text nodes — scoped verification 2026-11-03

Building the unmodified console view entry succeeds. The generic native
initial-frame probe then reproduced `Layout(Unsupported(44))`, whose text is
the send-button `↑`. The native text-node alphabet restriction was narrower than
the explicit-font shaper already used by fields. It now admits Unicode while
retaining general UTF-8/aggregate/glyph/shaping limits, the existing whitespace
subset and control refusal. Wire NUL rejection remains earlier and atomic.

All three new paragraph tests failed on the old alphabet restriction. Final
tests pass for symbols, accented Latin, supplementary input, distinct Hebrew/
Arabic visual order with an LTR prefix, combining-mark ink, CJK wrapping,
normal/nowrap/pre-wrap/nonbreaking whitespace, unchanged source, control rejection
and the 64 KiB UTF-8 shaping boundary with recovery. Initial negative fixture
expectations were corrected to distinguish earlier wire NUL rejection from
native layout refusal. The new original font has fixed timestamps and regenerates
identically; its bar/square glyph aliases expose ordering without third-party
font data. Synthetic glyphs do not establish complete script typography.

The native oracle emits 33 block/flex/grid cases. Real Chrome passes **81
checks**: native/container rectangle parity, exact source text and applicable
canvas source-to-alias comparisons. Native tests independently compare their
source/alias pixels within the native backend. This is not cross-rasterizer pixel
identity or a Firefox/WebKit/full-browser matrix.

The first native suite invocation passed **78 cases** before an older headless
test required emoji rejection. That test now checks a missing glyph's explicit
font `.notdef` advance and preserves its control-character negative. With
renderer code and fonts unchanged, the corrected continuation passes **121
cases**, including all 72 headless tests: **199 native cases total (188 view,
11 desktop)** across all 14 view integration suites plus library/desktop tests.
After the test-only `as_chunks` lint adjustment, all three focused paragraph
tests pass again. Strict all-target view/desktop Clippy, fresh preview and
headless-example builds, formatting and whitespace checks pass. Upstream warnings
remain visible; no dependency/profile/ABI changes were made.

The generic manifest-driven `headless_frame` example successfully renders an
actual Unicode Wasm artifact into a 1024×768 premultiplied RGBA frame with its
exact snapshot text. Missing arguments, a non-UTF-8 family and an oversized font
refuse cleanly; the oversized-font path produces no output. The example uses
public runtime/raster APIs with normal production budgets and explicit font
aliases, and does not step later timer/view turns. An initial probe-only use of
a type selector was corrected to an admitted class selector; this was not an
engine stylesheet change.

The console artifact SHA-256 is
`ac764bd1184ebc4c5b80e7929fdfca6efa13345746687657af392a24a3043164`.
Its before/after initial snapshots are identical; the same artifact now reaches
`Layout(Unsupported(2))` for the thread's `max-width: 48rem`. It still does not
render a complete console frame. Relative-length/reference-font parity is the
next native package; full styling/inline layout, natural typography, real-window
macOS/Pi/other-target sessions, accessibility, CPU/RSS and release acceptance remain
open. The last full workspace gate remains the `1111baa` checkpoint.

Evidence (local, ignored): `target/native-console-build.log`,
`target/native-console-initial.log`/`.json`,
`target/native-console-unicode.log`/`.json`,
`target/native-console-unicode-summary.log`, `target/unicode-paragraph-before.log`,
`target/unicode-paragraph-focused.log`, `target/unicode-paragraph-oracle.log`,
`target/native-paragraph-oracle.json`, `target/unicode-paragraph-chrome.log`,
`target/unicode-paragraph-native.log`,
`target/unicode-paragraph-native-continuation.log`,
`target/unicode-paragraph-native-summary.log`, `target/unicode-paragraph-clippy.log`,
`target/unicode-paragraph-preview.log`, `target/unicode-paragraph-format.log`,
`target/unicode-paragraph-fixture.log`, `target/unicode-paragraph-artifact.log`
and `target/unicode-paragraph-probe-checks.log`.

## Native rem reference lengths — scoped verification 2026-11-03

The current L0 grammar admits rem, but native length, border, grid and font
parsers previously required pixels/percentages. Three new default-reference
regressions all failed with unsupported native layout. Real Chrome independently
confirmed the reference distinction: with document root font 20px and a shadow
view root at 40px, `2rem` measures 40px, not 80px. Native hosts now explicitly
supply that document-root metric through `Environment::root_font_size` (initial
default retained), independently of view/default/media fonts. The checked
`ViewLayout` builder applies it before Wasm instantiation/main. Existing Rust
environment literals were updated to retain defaults. Wire/schema/profile,
permissions, guest ABI and dependency versions are unchanged.

A shared native converter preserves coefficient/resolved numeric bounds and
property sign rules across box dimensions/edges, padding/margins, flex basis,
gaps, nested grid tracks, font sizes, absolute line heights, border and outline
widths and signed outline offsets. Root metrics are retained when geometry and
scaled painting construct viewport contexts. Style strings are not rewritten in
the model. Invalid host values reject before environment publication; zero is
valid geometry while text minima remain separate. A failed layout transaction
preserves its model and defers no clipboard effect; funded/valid retry succeeds.
No production shaping/paint/guest budget was raised.

**6 focused length tests** pass, including dynamic/zero/invalid references,
rem/pixel geometry and native pixel parity, exact resolved-limit refusal,
variable/inherited values and separate media font semantics. The inherited-font
fixture was strengthened from 50% to 25%, so absolute rem line height differs
from an inherited numeric multiplier; all six tests pass again. **3 runtime
native-layout tests** pass, including actual Wasm initial-turn scroll geometry,
resize preservation and later root/DPI changes, plus prior theme/scroll cases.
The native view/desktop run passes **205 tests (194 view, 11 desktop), 0 ignored**.

The original-font native oracle and independent real Chrome shadow-tree probe
pass **168 checks across 18 fixtures**, with document references 16/20/24px and
view roots twice those sizes. They cover flex edges, nested grid, positioning,
48rem max-width, inherited text and media rem's initial-font reference. A probe
macro-expression compilation error was corrected before this oracle ran. This
does not prove all fractional border rounding, typography, real-window or target
acceptance. Strict all-target view/desktop/runtime Clippy, fresh preview and
headless-example builds, formatting and whitespace checks pass. Existing upstream
warnings remain visible. The last full gate remains `1111baa`.

The console artifact remains SHA-256
`ac764bd1184ebc4c5b80e7929fdfca6efa13345746687657af392a24a3043164`.
Initial snapshots before/after rem support are identical. Native rendering moves
from node 2's `max-width: 48rem` to `Layout(Unsupported(11))` at
`span.author-name`. The complete console frame/session still does not pass.
Proper semantic/inline formatting, remaining styling, accessibility, real
macOS/Pi/other-target sessions, CPU/RSS and release acceptance remain open.

Evidence (local, ignored): `target/rem-before.log`,
`target/rem-browser-reference.log`, `target/rem-focused.log`,
`target/rem-runtime.log`, `target/native-lengths-oracle.json`,
`target/rem-oracle.log`, `target/rem-chrome.log`, `target/rem-native.log`,
`target/rem-clippy.log`, `target/rem-preview.log`, `target/rem-format.log`,
`target/native-console-rem.log`/`.json` and `target/rem-console-summary.log`.

### Native text batch — full verification 2026-11-03

The consolidated batch includes fail-fast Taffy measurement after its first text
error, Unicode paragraphs, explicit document-root rem lengths and shared inline
formatting contexts. Inline source ranges retain style and shaping context,
semantic defaults, ancestor line struts and empty-node positions. Font-box
fragments drive backgrounds/hits and scroll clipping; their geometry unions do
not make wrapped gaps clickable. The final shaped layout also supplies glyph
painting. Original ligature/mark/Arabic fixtures and real Chrome confirm first-
source-node cluster ownership, including zero-width continuation boxes.

Inline collection/path, span-style, projection and fragment work share a bounded
native request counter. Existing production limits are unchanged. Real failed
layout-command tests preserve the model/commit count and defer clipboard writes;
nested calls cannot refill the counter and independent retry succeeds. Unsupported
inline edges, decoration and mixed block/inline contexts remain explicit errors.

Scoped evidence: **11 inline tests**, one work-scope/atomicity regression, and
**2724 Chrome comparisons across 13 fixtures**. The browser probe uses standards
mode, identical original fonts, element rectangles, interior targets and selected
screenshot pixels. It excludes points near pixel-footprint boundaries; native
regressions separately test point hits/gaps, scrolling and DPI. Semantic font
faces, unsupported styles and snapshot preservation are also checked. Both new
font generators reproduce their fixture bytes in an isolated ignored directory.
Fresh preview/headless-example builds and script syntax checks pass.

The final full gate passes **3174 tests, 0 failed, 16 ignored** with all eight
commands successful and the source fingerprint unchanged. It includes **217
native tests (206 view, 11 desktop)**, **2007 runtime unit tests**, public API,
CLI/packaged execution, dependency boundaries, strict workspace Clippy and all-
target builds. Conformance remains **123 passed and 4 known unsupported / 127**,
with the baseline matched and no regression. Evidence: `target/verification/20261103-122544-p42wl8c3/report.json`
(local, ignored), plus its per-command logs. Only result/status documentation
changed after this gate; code and build inputs did not change.

The unchanged console artifact remains SHA-256
`ac764bd1184ebc4c5b80e7929fdfca6efa13345746687657af392a24a3043164`.
Initial snapshots before/after are identical; native rendering now passes the
author span and reaches `Layout(Unsupported(16))` at the thinking button. The
complete console frame/session still fails. Remaining button/text alignment and
styling, full inline typography, OS accessibility, macOS/Pi/other-target sessions,
performance/resource measurements and release acceptance remain open.

Additional local evidence: `target/inline-focused.log`,
`target/native-inline-oracle.json`, `target/inline-chrome.log`,
`target/inline-preview.log`, `target/native-console-inline.log`/`.json`,
`target/inline-console-summary.log`, `target/native-text-full-gate.log` and
`target/inline-font-regeneration/`. Historical scoped records above describe
intermediate states; this full gate covers their final code together.

### Native accessibility UI-2 consolidated checkpoint — 2026-11-04

The package following renderer commit `be85c981` implements the lazy native
observer, bounded descriptions and actions, exact field source maps and painted
geometry, guarded assistive scrolling and coalesced live announcements. The
headless JSON, guest ABI and production budgets retain their existing contracts.
After a failed boundary run rejected the desktop consumer test dependency, the
rule was narrowed to allow only that AccessKit test dependency in the desktop
host. Both boundary cases pass. The clean final full gate passes **3281 tests,
0 failures, 16 ignored**, all eight commands and unchanged source inputs.
Report: `target/verification/20261104-011619-8_yzsx6c/report.json`; source SHA-256
`2ab8cfa1489dc188cae26237c3f9ea5cf2d9b9332e48977199448f47176e62df`. Documentation-only status updates follow the gate.

Separate GPU/Showcase and headless evidence remain scoped as recorded in the
plan. UI-tool app selection/inventory timed out, so current native/VoiceOver
acceptance is open. A subsequently reproduced upstream range-overflow and
missing native visible-range route are tracked for the next implementation
package; preparatory ignored staging files are outside this checkpoint.

### macOS exact range and reveal package — 2026-11-04

The package after `5a52125` replaces unchecked NSRange end arithmetic with
checked addition and exact UTF-16 boundary validation. Standard native visible
range getter/setter and owner reveal route through current source/selection
maps, preserving focus/value/selection and stale-frame protections. Whole
vertically visible lines include horizontally clipped text. ScrollView receives
the public AppKit scroll-area role. The vendor diff and release provenance are
updated without adding unsafe blocks or changing guest/profile contracts.

Final full gate: **3292 passed, 0 failures, 16 ignored**, all eight commands
with unchanged inputs. Report: `target/verification/20261104-015702-6g3mx2fb/report.json`;
source SHA-256 `6294ded534da348dc9dc43516e10f0ecd1159e367184dc4462cc79c313823653`.
Separate GPU scoped verification passes 66 desktop cases and strict all-target
Clippy. Native/VoiceOver, page/item/absolute platform routes and the remaining
whole-plan requirements stay open. Documentation-only annotations follow the gate.

### Host-owned native clipboard defaults — 2026-11-04

View/Application now split trusted copy/cut/paste into a bounded task and an
opaque source/selection-bound completion between guest turns. Prevention,
password/readonly rules, stale/replay/foreign responses, failed Cut, normal
history/maxlength and no-fresh-gesture completion are covered. Desktop owns the
text-only arboard worker, physical shortcuts and FIFO across deferred I/O; the
engine remains free of clipboard SDKs and OS I/O. Guest ABI/capabilities, frozen
script/JSON shapes and production budgets retain their existing meanings.

Final full gate: **3323 passed, 0 failures, 16 ignored**, all eight commands with unchanged inputs. Report: `target/verification/20261104-032530-xtwjknm6/report.json`; source SHA-256 `011b1246d98baaba320d4a7e85bf0b676c75779b3e03099e3cf279d2659d7ced`.
Separate scoped checks pass 15 core cases, six real Application/Wasm cases and
76 desktop GPU cases, strict affected all-target Clippy, both dependency boundary
cases and the 12-input Showcase script. The fresh CPU Debug bundle builds; Cua
app selection times out after 60 s, leaving actual launch/system clipboard and
IME/VoiceOver/other-platform acceptance open. Source comments/documentation-only
annotations follow verification. No full-plan completion is inferred.

### V38 package specifiers (wave 1) — 2026-11-04

`package_specifiers()` (v38) lets a project import a typed npm package by
name. The project loader resolves `<config dir>/node_modules/<name>` or
`@scope/<name>` only, through the pinned no-follow root: a bounded
`package.json`, the exact `exports` key with only its `types` condition (the
top-level `types` for `.`), the `.d.ts` inside the package and its adjacent
`.d.ts.map`, whose single source must be a shipped `.ts`/`.tsx` file inside
the package. Untyped, declarations-only, symlinked, `*`-pattern and escaping
packages are module errors at the import; v1–v37 name the admitting profile.
No ABI, capability or budget changed; the compiler is unchanged.

Scoped verification only: `verify.py local varta-project` (60 unit, 2
boundary, 8 package tests), `verify.py local varta-compiler`, `cargo test -p
varta --test modules` (29), `cargo test -p varta-common`, strict Clippy of
the affected crates and formatting pass. A scratch project outside the
repository with a copy of the pinned `uarp-sdk` 0.13.2 checks under v38
when importing `UarpClient` from `"uarp-sdk"` (66 modules, no declaration
or JavaScript file) and fails under v37 with the named diagnostic. The full
gate is left to the wave integration.

Review fixes: a mapped source's extension is classified ASCII-case-insensitively
and must be lowercase (on case-insensitive APFS `./index.D.ts` had loaded the
declaration as a module); an empty `exports` object exports nothing instead of
falling back to the top-level `types`; declaration source maps are cached by
path, so a map shared by several exports counts once against the metadata
budget. Scoped verification: `verify.py local varta-project` (60 unit, 2
boundary, 9 package tests), `verify.py local varta-compiler` (164 unit),
`cargo test -p varta --test modules` (29), strict Clippy of `varta-project`
and formatting pass.

### Native shell core (`varta-shell`) — wave 1, scoped 2026-11-04

`varta-desktop` gains the production `varta-shell` binary beside
`varta-preview`. The window, worker, input and render code moved from the
preview's `main.rs` into the library (`src/shell/window/`) unchanged in
behaviour; the preview remains an unprivileged bare-module host. The shell
reads a packaged artifact with `varta-artifact` exactly as the companion does,
takes the manifest as the request, narrows it with an optional `--policy` host
ceiling, denies optional grants (no prompt yet) and pins KV roots to a private
host-owned directory per artifact hash
([contract](architecture.md#native-shell-varta-shell)). Fonts stay explicit
arguments. The boundary test now forbids compiler, SWC, project, CLI and `clap`
dependencies of `varta-desktop`, production and dev, direct and transitive
(the transitive dev-dependency check came with the review fixes below).

Scoped checks: `verify.py local varta-desktop` passes (96 tests: 82 library,
3 binary, 11 shell-core), as do the GPU-feature build and tests, strict
desktop Clippy with and without `gpu`, the two dependency-boundary cases, the
launcher tests and formatting. A manual run on macOS opened the window and
presented the first frame of a packaged view example and of the probe (KV
round-trip in its per-hash directory); the agent environment could not capture
the screen. No full gate was run for this package.

Review fixes (same wave). The checked-in probe Wasm and manifest are gone:
the tests build the workspace's `varta` binary with Cargo (offline, locked)
and run `varta build` on `tests/fixtures/shell-probe` as an external process.
`varta-shell` checks the renderer and reads the font before preparing the
artifact, so a bad `--font` creates no storage; empty font and module files
are refused again, as the preview's old reader did. Library diagnostics (GPU
adapter and loss, accessibility observer warnings, frame timing) go through
the binary's console and label instead of `eprintln!`. `--renderer gpu` is
refused without the `gpu` feature and is an error when the GPU does not
initialize; `auto` keeps the CPU fallback. KV directories are gated per root
by the ceiling's `kv.read` patterns, a ceiling mapping of a KV root is tested
to win, and the contract states that the directories precede the runtime's
code validation (which runs once, at start).

### V38 SDK paths (`sdk_paths`) — wave 2, scoped 2026-11-04

`sdk_paths()` (v38) closes the subset gaps that uarp-sdk 0.13.2 reaches on its
multipart, query and error paths, or refuses them by name
([contract](architecture.md#sdk-paths-sdk_paths)). Admitted: `error.cause`
reads of native errors and Error-instance unions (pilot finding F3),
`new Error(message, options)` and the native families as instances of the
family's Error root (created in `link` before any body is lowered),
`xs[i].m<T>()` on element receivers (pilot finding F1), `instanceof Map`/
`Set`/`Uint8Array` of closed unions, the blob prelude's `File`, and
`for...of` over `FormData`. Record and dictionary spreads of a view whose
sources have other fields or another field order now fail in `resolve`
(they copied the view's fields in the view's order); the SDK's same-shape
`{ ...spec, headers, query }` stays admitted. Still refused: a `Uint8Array`
held by `unknown` (owner decision), the record view of a class instance held
by `unknown` (so the SDK's `appendPart` throws `TypeError` for `Uint8Array`,
`ArrayBuffer` and `{ data: Uint8Array }` parts), `A[] | B[]` of records,
`Blob.stream()` and `formData()`. No ABI, value layout, capability or budget
changed.

Scoped verification only: `verify.py local varta-compiler` (164 unit tests),
`verify.py local varta-runtime --filter sdk_path` (11 tests; multipart and
query wire bytes equal the checked-in Node 24.20.0 output), `cargo test -p
varta-common`, `cargo test -p varta-runtime --lib` (2041 passed; the one
failure, `a_network_wait_times_out_without_running_later_inputs`, is a
wall-clock bound under parallel builds and passes alone), strict Clippy of
the three crates, formatting, and the conformance baseline (`cargo run -p
varta-conformance --locked -- --json`, baseline matches). `varta check` of
every `examples/uarp-console` configuration passes with the pinned SDK. The
full gate is left to the wave integration.

Review fixes (2026-11-05): the element-receiver arm of `peek_type` is gated
on `sdk_paths()` and limited to keys other than string literals (a `switch`
over `xs['length']` trapped at `unreachable`); `FormData` entries follow
WHATWG "create an entry" (every `Blob` entry is a `File`, `FormDataEntryValue`
is `string | File`) without dispatching through an overridable method, and
program overrides of engine-internal prelude members are refused; `"cause"
in` an Error instance is refused (Node is false for `new Error('m', {})`);
own-key enumerations of views of wider objects (`Object.keys`/`values`/
`entries`, `Object.assign` sources, `for...in`) are refused like spreads;
the v38 error-constructor refusal names `(message?, options?)`. Each fix has
a regression against Node 24.20.0 output or its named diagnostic.

### Browser KV imports (wave 2) — 2026-11-04

The official browser host admits the nine v38 `kv_*` imports and implements
the embedding contract's host without KV storage
([mapping](embedding.md#browser-effect-mapping-official-host)): every
`kv_open` reads its path with the native checks, the broker audits a denied
`kv.read` operation, the denial is logged and the open rejects with
`NotAllowedError`, whatever the grants and the ceiling. Nothing is stored in
the browser. The Worker module is `kv-store.mjs`; the browser registry gains a
`kv` section (operation names, capabilities, the failure code, descriptor
separator and limit, error name and denial template) and the per-operation `kvBookkeeping`
flag for `kv_entry`, `kv_text` and `kv_close`, which the native runtime does not
charge to the host-call budget. The planned IndexedDB backend was not built:
the KV imports are synchronous host calls and a commit is reported only once
durable, which needs an owner decision on a synchronous bridge
(cross-origin-isolated `SharedArrayBuffer` and `Atomics.wait`, or JSPI) and a
browser form of the value staging, V8 codec and checked decoding
([open items](development-plan.md#batch-v38-in-development)). The docs state
that `environmentInputs` are a per-run snapshot, not a credential store.

Scoped verification: `cargo test -p varta-project --lib browser::` (45),
`cargo test -p varta-common --lib browser::` (7), the documentation drift
tests (`--test documentation`, `--test embedding_documentation`), registry
regeneration without a diff, `node --test
crates/varta-common/browser/fuel.test.mjs
crates/varta-project/src/browser/*.test.mjs` (188), strict Clippy of
`varta-common` and `varta-project` and formatting. `node
scripts/test-browser-kv.mjs target/debug/varta` passes in local Chrome: a
program without a grant and one granted under an empty host ceiling run with
native logs and exact fuel, host calls, microtasks, output bytes and turns,
and a granted, mapped program, which opens and commits natively, runs in the
browser exactly as the native program without a grant. The new `--kv` option
of `scripts/test-browser-matrix.mjs` ran the same cases (and `--effects`) in
Firefox 153, WebKit 26.5 and Chrome 156 through Playwright 1.62.1, the
version installed locally; Playwright 1.64.1 was not available. No full gate
was run for this package.

Review fixes (2026-11-05). The browser build and dev server refuse a manifest
that grants `kv.read` or `kv.write` before any output (`browser KV storage is
not implemented; the manifest grants …`, generated as
`browser::kv_storage_refusal` and the registry's `storageRefusal`); before,
such a manifest built and every open failed at run time naming a granted
capability. KV programs without a grant still build. The granted cases of
`scripts/test-browser-kv.mjs` became refusal assertions for both entry
points. The equivalence claim is narrowed to compiler-produced programs (a
hand-crafted `kv_stage`, `kv_read` or `kv_commit` without an open database is
fatal at once in the browser); the linked-intrinsics text now names the KV
imports as the ones that read and build whole value layouts; the matrix
report records the version of the Playwright package actually loaded. The
owner chose the backend: OPFS sync access handles in the Worker and the
shared Rust value layer compiled to wasm32
([agreed design](development-plan.md#browser-kv-storage-agreed-design-wave-3)),
a separate wave-3 package. Scoped verification: `cargo test -p varta-project
--lib browser::` (46), `cargo test -p varta-common --lib browser::` (7), `cargo
test -p varta --test smoke browser_build` (14), registry regeneration
idempotent, the 188 Node tests, the documentation drift tests, formatting and
strict Clippy of the three crates; the KV cases pass in local Chrome and,
through Playwright 1.62.1, in WebKit 26.5 and Chrome 156 (Firefox did not
launch in this environment).

### Shell fonts and guest effects (`varta-shell`) — wave 2, scoped 2026-11-04

`varta-artifact` gained a versioned resource section (`Resources`, section
version 1): canonically ordered font faces with per-face SHA-256, bound to the
SHA-256 of the portable Wasm, each face admitted only when its own `name` and
`OS/2` tables state the declared family, weight and style. Bare artifacts carry
it as `<out>.resources` (a stale sidecar is removed on rebuild); containers
use format 2 with the section and its length before the unchanged footer, and
write format 1 bytes without resources. `Artifact::read` and `embedded` validate
and drop the section, so the runtime companion refuses tampered sections too.
`varta-project` packages the configuration's new top-level `fonts` list through
the pinned no-follow loader (`artifact.rs`; one field in `config.rs`, two calls
in `lib.rs`). `varta-shell` loads every face through `FontSet` and
`TextMeasurer::new_with_fonts` before creating storage and refuses `--font` for
an artifact that ships fonts. `varta-view` gained the `PlatformEffects` sink
(`platform_effects.rs`, two calls in `events.rs`, a field and builder in
`lib.rs`) and `varta-runtime` passes `ApplicationOptions::platform_effects` to
hosted views; the shell's `NativeEffects` writes the system clipboard and runs
`/usr/bin/open -u` (macOS) or `/usr/bin/xdg-open` with a bounded wait.
`scripts/test-native-inline.mjs` accepts several faces with weight and style
(`fonts`), keeping the single-face input; the existing native inline corpus
still passes with it. The new `native_font_probe` passes it at 1, 1.25 and 2
(852 checks each, mean RGB delta below 0.35, both for the self-packaged
fixture faces and for the view probe built by `varta build`). Scoped
verification: `verify.py local varta-desktop` and `local varta-artifact`,
the launcher tests, the boundary tests, the view library tests, the runtime's
view-filtered library tests, the project library tests, the CLI smoke tests
filtered to `build`, formatting and strict Clippy (desktop also with `gpu`).
A manual run presented the view probe's four lines from artifact fonts; no
trusted clicks could be performed in the agent environment. No full gate was
run for this package.

Review fixes (same wave): `varta build --target browser` and the browser
development server now carry the resource section's faces (built once through
the same loader) with weight, style and generic families; the loader registers
them as `FontFace`s before the renderer exists, the applier maps declared
generic families and names the first family on the view root, and a
configuration with both `fonts` and `browser.fonts` is refused. The Chrome
oracle registers packaged faces through `view-fonts.mjs`'s namespace and adds
`generic` and `default` cases (mutations dropping the alias or reordering the
default family fail it), and `browser_bundle_probe`'s `view-font-packaged`
bundle passes `scripts/test-browser-fonts.mjs`. Launcher protocol 3 adds
`containerFormats`; packaging a section refuses a companion without format 2
before writing output and validates the sidecar section through the probe.
The shell's browser launcher is detached and succeeds on spawn. The font
limits moved to `varta_common::view::fonts`, a repeated generic family in one
face is `ResourceError::DuplicateGeneric`, and a leftover debug print in a
runtime view test was removed.

Second review fixes (same wave): the applier's base sheet, from
`createFontNamespace(...).rules`, maps the user-agent `monospace` of `pre`
and `code` to the declaring packaged family, makes form controls inherit
their family and sets `font-synthesis: none`; the shell loads a resource
section through `TextMeasurer::new_with_packaged_fonts`, which sizes a mapped
`monospace` as that family (Chrome drops its 13/16 fixed-font ratio once the
family is rewritten). The registry exports `UA_MONOSPACE_TAGS` and
`CONTROL_FONT_TAGS`, which a view test ties to the native user-agent rows.
`test-browser-fonts.mjs` adds `code`/`pre`/guest-set `monospace` at absolute,
medium and inherited sizes, control families and synthesis (a mutation without
the rules fails at `ua-code`, 48.17 against 72), and `native_font_probe` adds
five such cases that pass `test-native-inline.mjs` at 1, 1.25 and 2 (2356
checks each); without the packaged measurer `ua-medium` fails. The shell keeps
at most `MAX_RUNNING_LAUNCHERS` launchers instead of a reaper thread per
opening, the prototype `varta-web-host` refuses artifacts with a resource
section, and the sfnt record offsets and the renderer's family/alias limits
are named. The manual trusted-gesture run is still open.
