# Experimental desktop preview

## Host-owned copy, cut and paste

The preview implements Command+C/X/V on macOS and Control+C/X/V elsewhere,
plus Control+Insert and Shift+Insert/Delete outside macOS. Physical shortcuts
retain the original logical key for guest keydown handlers. Static prevention,
current field/source/selection, readonly, disabled and composition checks precede
clipboard work; password default copy/cut is refused. Cut waits for a successful
write. Paste retains ordinary Undo and UTF-16 maxlength behavior.

Text-only `arboard` 3.7.0 (MIT OR Apache-2.0) supplies the OS bridge. Its context
is created lazily on a dedicated I/O worker; the UI/application loops continue
while following keys remain FIFO. One opaque operation binds the completion to
the original source and selection. Failed or stale results cannot delete or
overwrite a field. Input/clipboard channels and retained field buffers remain
bounded. SDK/OS temporary allocation is outside retained-response accounting.
Tests use injected providers and do not touch the user's clipboard. Actual
system clipboard, platform and IME acceptance remain separate checks.

## Native accessibility foundation

The preview installs AccessKit 0.25.2 through accesskit_winit 0.34.2 before
showing its window. These exact Apache-2.0 dependencies are desktop-only; the
headless view and runtime have no platform accessibility dependency. Existing
locked package versions are retained. The adapters supply the native platform
integration without adding unsafe code to Varta.

When an assistive client activates accessibility, the worker prepares a flat
semantic tree using the headless view's existing role and accessible-name rules.
It includes actionable roleless containers, masked password values, control
states and stable opaque identities. The established headless JSON shape stays
unchanged. The GUI publishes the tree only after presenting its matching CPU or
GPU frame, with visible element-fragment unions in physical pixels. Offscreen,
text-only and select-popup option geometry is not complete.

Supported actions are Focus, Click, SetValue with a string and
ReplaceSelectedText and SetTextSelection. Callbacks enqueue at most eight requests, with at most
1 MiB of retained text and the existing per-field UTF-16 limits; one coalesced
data-free GUI wake represents pending requests. They execute between guest turns
through ordinary view input and permission checks. Source identity and displayed
revision/viewport/theme/scale guards are checked before worker effects. A queued
semantic request can rebind across a repaint only if its native ID, opaque source
identity and current action remain valid; focus-then-edit bursts keep FIFO order.
Hidden, replaced and unsupported targets are discarded; readonly fields allow
selection and reject edits. Native IDs are never
reassigned to replacement source nodes, including after deactivation.
Deactivation clears bindings, cached native data and queued requests. Inactive
rendering checks one atomic flag and does not construct semantic snapshots.

Typed snapshots now include `aria-describedby` descriptions. The observer API
returns `Result<AccessibilityTree, AccessibilityError>`: one 8 MiB potential-work
allowance preflights native names (including actionable generic containers) and
descriptions before allocating them. Description traversal reuses the admitted
name algorithm and masks embedded passwords; cycles do not recurse through
`describedby`. Existing guest admission and headless JSON stay unchanged.
On observer refusal, the app keeps rendering and accepting ordinary input; the
native tree exposes a static-text explanation and clears old guest actions.
Valid subsequent state restores native content with fresh IDs. Caller-owned
copies and total backend memory remain the embedder's responsibility.
[Current description/observer checks and native limitations](../../examples/native-showcase/accessibility-descriptions-2026-11-03.json).

Text fields now project logical text runs, Unicode word starts, hard lines and
directional selections. Runs contain at most 255 scalar/CRLF units, with linked
chunks for long lines and an observer-only cap of 4096 auxiliary nodes. Password
units are rendered grapheme masks; headless JSON keeps its previous mask count. Exact
source-byte maps remain host-owned; password runs expose masks only. Content
revisions and nonrecycled run IDs reject ranges from earlier edits, Undo/Redo or
input-type changes. Selection and replacement use the ordinary editor path.

Matching frames now supply visual line and direction splits, clipped character
positions/widths and caret affinity. Exact source-range keys preserve native IDs
only while their character positions retain the same meaning after wrapping.
The auxiliary-run ceiling includes visual spans; refusal still leaves the guest
running. Native geometry/caret checks are separate from platform acceptance.
[Current geometry checks, artifact hashes and native timeout](../../examples/native-showcase/accessibility-text-geometry-2026-11-03.json).

Assistive scrolling and announcements, screen-reader
behavior and Linux/Windows/Pi acceptance remain open. Pure projection/queue/core
tests do not prove platform acceptance.

Local macOS observations now cover the system accessibility API in CPU Debug
and real Metal GPU Release: names/controls, buttons, Ukrainian SetValue and
concrete option selection. Debug also covers radio/checkbox, Undo and resize
with preserved state and working subsequent input. The Release async example
completes through its accessible button. This is API-level evidence, not a
VoiceOver audio test. [Pinned inputs and scope](../../examples/native-showcase/accessibility-checkpoint-2026-11-03.json).

## UI diagnostics and GPU prototype

The default build remains the software preview. `VARTA_PREVIEW_TIMING=1`
prints a host-entry-to-first-presentation sample plus host-only JSON input
diagnostics to stderr without input values: queue entry to
worker acceptance, accepted input/application-turn work (`guest_ms`), frame
preparation, window wait and host presentation return. This excludes OS event
delivery before queue entry, queued-key waiting before dispatch, GPU completion
and physical screen scanout. The `guest_ms` phase includes host input handling and application-step work; it
is not an isolated Wasm CPU timer. Multiple accepted inputs in a frame share
the first worker-accepted stamp and an explicit input count.
Deferred clipboard samples preserve initial worker acceptance; `deferred_ms`
records elapsed host deferral through completion instead of attributing that
wait to queue time. `guest_ms` describes work in the returned frame and is not
the total CPU cost of an operation spanning earlier frames. These overlapping
spans are not an additive decomposition of `total_ms`.
Pointer-state samples remain coalesced outside the activation FIFO; a generation
and condition variable wake the worker on pointer/input/frame acknowledgement
without losing notifications that arrive just before it waits.

An optional GPU prototype was committed in `be85c981`:

```sh
cargo build -p varta-desktop --features gpu --bin varta-preview --locked
VARTA_PREVIEW_RENDERER=gpu VARTA_PREVIEW_TIMING=1 target/debug/varta-preview app.wasm font.ttf Arial
```

`gpu` or `auto` requests GPU rendering in a GPU-enabled build; the default is CPU.
The host logs the selected adapter or a CPU fallback. wgpu 30.0.0 and Vello GPU
0.3.0 are pinned optional dependencies (MIT OR Apache-2.0); pollster 0.4.0
(MIT OR Apache-2.0) drives host device initialization. They are desktop-only,
not runtime/compiler dependencies. Their native implementations contain unsafe
code; Varta's own renderer/host code continues to forbid unsafe code.

Vector preparation shares CSS/layout, font shaping and input metadata with the
CPU renderer, without producing a CPU pixel buffer. The backend consumes bounded
rectangles and glyph paths, transformed and clipped in physical pixels. It
presents directly to the window without per-frame readback. Recording/scene
overflow retains software fallback; device/surface failures switch to CPU. An
occluded window waits for visibility rather than continuously redrawing. At most
one GPU submission remains outstanding; completion is polled without blocking the
window thread and a two-second timeout triggers CPU recovery. A macOS window uses
an explicit Display P3 surface and converts sRGB paint primaries before emission;
this avoids oversaturated colors on untagged wide-gamut Metal layers.

The prototype preserves the existing 4096-per-axis/16,777,216-pixel bounds, limits
each GPU buffer to 64 MiB and disables intermediate layer textures for the admitted
path-only UI. These are per-resource limits, not total GPU-memory accounting.
An additional 8 MiB estimated cache reuses structurally equal neutral path
segments; full scene admission is charged even on cache hits. Per-resource caps
do not establish a total device-memory quota. Broader target acceptance and
painting remain work in progress. This backend supplies no guest GPU API or Three.js compatibility.

The browser diagnostic equivalent uses `?ui-timing=1` on a generated application
page, or a trusted `mount({onFrameTiming})` callback. It reports queue/Worker
round-trip/DOM-apply phases and two animation-frame callbacks. The endpoint is a
paint opportunity, not actual screen presentation, and is not identical to the
native presentation-return endpoint. Samples are bounded, dropped counts are
explicit and pending callbacks are cancelled on shutdown. The diagnostics do not
change guest fuel, event ordering, capabilities or gesture admission.

For host recovery diagnostics only,
`VARTA_PREVIEW_GPU_TEST_LOSS_AFTER=N` destroys the device after N submitted frames.
The observed Metal recovery preserves Wasm state and subsequent input.
`varta-preview --renderer-info` reports compiled backend availability without
opening a window.

The offscreen `gpu_probe` example compares real GPU output and input metadata
against the software renderer at 1×/2×. It preserves RGBA outputs and reports
readback timings; readback is diagnostic and is never used for window presentation:

```sh
cargo run -p varta-desktop --features gpu --example gpu_probe --locked -- app.wasm font.ttf Arial target/gpu-frame
```

The `gpu_clip_probe` diagnostic uses host-owned box/control fixtures, including
rounded/nested clips, translucent fills, borders, oversized asymmetric inner
arcs, fractional edges, offsets, textarea, password,
select, button, checkbox and radio. Its text cases use the deterministic test
font; it needs no Wasm/font arguments and does not open a window:

```sh
mkdir -p target/clip-probe
cargo run -p varta-desktop --features gpu --example gpu_clip_probe --locked -- target/clip-probe/frame
```

Twenty scenes at 1×/2× each compare a cold frame and a visible interior color
change with CPU pixels and identical input metadata. Partial dirty regions must
be nonzero and smaller than the frame; images are saved as `.rgba`. The mean RGB
threshold stays the original probe's 2/255; maximum differences are reported,
including AA edge differences. CPU preparation timing covers only these small
host fixtures, not guest execution, event queuing or physical presentation. GPU
buffer/dimension caps and disabled intermediate layer textures stay unchanged.
At that committed checkpoint, fifteen scenes exercise actual GPU drawing (60 observations); the two
opacity and three mixed-border scenes use explicit CPU fallback (20 observations), compared with retained
and fresh stateless pixels. Their rows say `gpu_rendered:false`. Repeated group
preparation must have zero dirty pixels, while a local mutation retains damage
reuse. `cpu_prepare_ms`/`dirty_pixels` describe the CPU reference renderer;
`prepare_ms`/`prepare_dirty_pixels` and `unchanged_prepare_ms` describe fallback
preparation separately. No speed ratio or direct GPU group support is inferred.
The committed painting checkpoint's macOS Debug/Release logs are
`target/paint-next-stage/native-painting-precommit-debug.log` and
`native-painting-precommit-release.log`; `native-painting-precommit-checkpoint.json`
records source/binary/image hashes and samples in that directory. Earlier opacity
and geometry checkpoints remain separate historical inputs. Mixed-field preparation
spans7.084–66.547ms Debug and0.353–7.616ms Release in these single-sample scenes;
complex Debug corners required further optimization at that checkpoint. These are offscreen observations,
not general UI smoothness, direct GPU group/color-field composition or actual
application/window acceptance.

The subsequent scoped CPU follow-up is recorded in
`target/paint-next-stage/border-speed-profiled/checkpoint.json`. Three alternating
before/current pairs per profile preserve all 80 images byte-for-byte each time.
Mixed-scene preparation medians span 2.199–12.866 ms Debug and 0.365–2.798 ms
Release; the cold2× opposing-corner scene moves from66.357 to12.866 ms Debug and
7.517 to2.798 ms Release. These are host-owned offscreen timings; the CPU fallback
contract and application/platform acceptance limits above remain. Final scoped
checks cover 83 View library +265 integration cases, strict View all-target
Clippy and both optional GPU example builds/probes, not a new full-workspace pass.

The subsequent inline-background follow-up adds default sliced radii across
wrapped/bidi fragments, per-fragment radius resolution and independent glyph/hit
geometry. That checkpoint probe has21 scenes:64 actual GPU observations and20 explicit
CPU fallbacks per build. Rounded inline backgrounds use the existing vector path;
groups and mixed-color fields remain software fallbacks. Both Debug/Release probes
pass, with all80 earlier images per profile byte-identical. Scoped evidence is in
`target/paint-next-stage/inline-next/final/checkpoint.json`:85 View library +272
integration cases, strict affected all-target GPU Clippy and4344 Chrome checks.
That earlier checkpoint left inline opacity/context ordering, inline edges/
borders/outlines and real application/platform acceptance as separate work.
Firefox launch/profile failures and the sampled WebKit corner-hit difference
remain explicit; no full cross-target parity is inferred from these offscreen
checks.

The subsequent paint-rounding/inline-opacity implementation has scoped evidence
at `target/paint-next-stage/pixel-snapping/checkpoint.json`:86 View library +287
integration +78 Desktop GPU-feature tests pass, with strict affected all-target
Clippy. Both Debug and Release probes now have22 scenes, each with64 actual GPU
observations and24 explicit CPU fallbacks. Own inline opacity uses CPU isolation;
its stateless comparison includes the same explicit font. The original33-scene
Chrome probe passes at1× and20 decoration scenes pass at2×. At1.25×,19/20 pass;
the remaining ink/curve scene exceeds the unchanged2/255 mean bound. Pixel
rounding preserves logical input/text geometry and subpixel glyph positions.
That historical checkpoint independently reproduced normal block/inline phase
ordering as a pixel/target gap. The following phase scheduler fixes it with scoped
86 library +291 integration +78 Desktop tests and strict affected GPU Clippy.
All24 new block/flex/grid browser cases pass at1×/1.25×/2×; the expanded59-case
corpus retains dense overlapping-text raster failures and the125% curved-text
failure. See `target/paint-next-stage/paint-phases/`. No full-workspace, app-window,
system input or platform acceptance is claimed for these scoped increments.

The following vertical-paragraph-origin increment has456 passing scoped tests
(86 library,292 integration,78 Desktop) and strict affected GPU Clippy.
It preserves horizontal/logical fractional geometry while fixing the125% curved
text scene under the same2/255 mean bound. The59-case browser corpus now completes
58/59 at each scale; dense overlapping text still fails. Current evidence lives
in `target/paint-next-stage/text-origin/`.

The macOS preview now installs CoreText glyph coverage from the same explicit
font bytes selected by the shared shaper. Existing CPU/GPU rectangle commands,
clips, opacity and cache ceilings remain in use; unsupported collection members,
variable/color fonts and resource refusal preserve portable outlines. The native
59-case Chrome corpus passes10105 checks at1×/1.25×/2× under the unchanged bound.
Use `native_text_probe` for that host path and `native_inline_probe` for the
portable outline reference, whose smoothing difference stays explicit. New
`system-text`/`system-field` GPU scenes require actual native coverage. Evidence:
`target/paint-next-stage/glyph-raster/`. This does not establish application-window
or cross-platform typography acceptance.

The consolidated painting/glyph package passes3434 workspace tests with16 ignored
and all8 full-gate checks on unchanged source; strict GPU Clippy also passes.
Current Debug/Release Metal probes each have24 scenes:72 actual GPU observations
and24 explicit CPU fallbacks, max mean0.1472. New native scenes use coverage for
12 dense-text glyphs and13 field glyphs and require no outline substitution.
Report: `target/verification/20261104-131013-v4eh1yvu/report.json`.


## Software preview usage

`varta-preview` opens a native CPU-rendered window for a precompiled v38 view
module. Its Wasmtime application runs on a worker thread; the main thread owns
the window and presents bounded frames. Pass a TrueType/OpenType file and its
family name for the currently supported explicit-font text subset:

```sh
cargo run -p varta-desktop --bin varta-preview -- app.wasm font.ttf 'Font Family'
```

For a diagnostic initial frame without a window, the `headless_frame` example
uses the same public runtime/raster APIs. It runs `main` with the supplied
validated manifest's grants and normal production budgets. Additional arguments
map CSS aliases to the explicit font:

```sh
cargo run -p varta-desktop --example headless_frame --locked -- app.wasm app.manifest.json font.ttf 'Font Family' target/frame system-ui sans-serif
```

The output is a bounded `target/frame.json` snapshot and, on successful painting,
a 1024×768 `target/frame.rgba` image (premultiplied RGBA8, row-major). The output
directory must exist. A rendering failure retains the snapshot for locating its
node and returns a nonzero status. The example does not drive later timer/view
turns or prove an interactive session. Font and input files are size-bounded;
font aliases do not enable system-font discovery. The unprivileged window preview
above still does not load a manifest or grant capabilities.

To exercise guest-initiated scrolling without a font, build and open the
included fixture from the workspace root:

```sh
cargo run -p varta -- build --config crates/varta-desktop/examples/guest-scroll/varta.config.json
cargo run -p varta-desktop --bin varta-preview -- crates/varta-desktop/examples/guest-scroll/dist.wasm
```

The initial frame shows a green block below a short red strip and the guest
logs `guest-scroll 100`. This fixture was observed on the development macOS
machine; the window title changes to `Varta scroll — 100` after the guest scroll
handler runs. Cross-platform acceptance remains open.

The multiline editing fixture uses the deterministic test font:

```sh
cargo run -p varta -- build --config crates/varta-desktop/examples/text-editor/varta.config.json
cargo run -p varta-desktop --bin varta-preview -- crates/varta-desktop/examples/text-editor/dist.wasm crates/varta-view/tests/fixtures/advance-400.ttf 'Varta Fixture 400'
```

Press Tab to focus. Home, Shift+End and Backspace remove the last row's text;
Ctrl/Command+Z restores it. The guest logs the changed value as escaped JSON.
Up twice from the initial caret position crosses the short middle row while
preserving the desired column. These keyboard scenarios were observed on macOS;
pointer, IME and other-platform textarea acceptance remain open.

Password and search fields have a separate fixture using a font whose mask is a
small square and whose plaintext glyphs are tall rectangles:

```sh
cargo run -p varta -- build --config crates/varta-desktop/examples/field-types/varta.config.json
cargo run -p varta-desktop --bin varta-preview -- crates/varta-desktop/examples/field-types/dist.wasm crates/varta-view/tests/fixtures/password.ttf 'Varta Password Fixture'
```

Tab focuses the password field, then the search field. The first field paints one
mask per grapheme; selection, replacement, IME preedit and Undo preserve original
guest text. The fixture logs edit notifications without the field values. The
search field uses plain text editing; a search-clear button is not supplied.
On the development macOS Retina display, keyboard selection removed two password
graphemes without revealing text, Undo restored the five masks, and deletion/Undo
in the search field restored its four plaintext glyphs. Native CJK and other-platform
password acceptance remain open.

The checkbox/radio fixture needs no font:

```sh
cargo run -p varta -- build --config crates/varta-desktop/examples/checkables/varta.config.json
cargo run -p varta-desktop --bin varta-preview -- crates/varta-desktop/examples/checkables/dist.wasm
```

Space toggles the checkbox. Tab visits a radio group once; arrow keys move its
selection. The bottom radio belongs to a separate form and stays selected when
the upper group changes. Checked marks use the supplied foreground color inside
explicit control content boxes, clipped by an authored inset corner curve when
present; platform-native control themes are not supplied.

The single-select fixture includes its own deterministic font:

```sh
cargo run -p varta -- build --config crates/varta-desktop/examples/select/varta.config.json
cargo run -p varta-desktop --bin varta-preview -- crates/varta-desktop/examples/select/dist.wasm crates/varta-desktop/examples/select/font.ttf 'Varta Fixture 400'
```

Tab focuses the control. Space/Enter opens its popup; arrows preview a choice,
Enter accepts and Escape cancels. The popup supports pointer selection and wheel
scrolling. Character input cycles matching labels. Two options deliberately share
a value: choosing between them still emits an edit, and a guest echo preserves
the selected option. Pointer hover shades an enabled row without changing the
selection or the keyboard preview. Timed multi-character search and platform
control themes remain open.

On the development macOS Retina display, Tab/Space opened the popup, Down
previewed a different option without changing the closed label, and Enter selected
the duplicate value with one input/change pair. Escape cancelled a later preview;
wheel scrolling and a pointer choice selected the expected row. An outside click
dismissed the popup, and character input cycled to another matching label. Guest
echoes kept the concrete option. Native API/CLI tests separately cover a logical
character without committed text; text fields still wait for committed text.
Other-platform select acceptance, full input-method behavior and accessibility
remain open.

The font arguments can be omitted for a view without text. This preview uses no
package capability grants and has no permission prompts. It accepts only the
current experimental `varta-view` CPU painting subset. Unsupported content
fails explicitly; explicitly styled buttons and text/search/password inputs are included, including
disabled controls. Disabled platform-default styling is not supplied.
Explicitly styled checkbox/radio inputs also paint their checked marks without
fonts, with pointer/Space activation and scoped radio keyboard navigation.
Explicitly styled textareas also paint newlines and wrapped text;
pointer selection resolves both coordinates and Enter inserts a newline. Wheel
input scrolls the field and chains remaining movement to ancestors; a new edit
brings the caret back into view. Up/Down and Home/End navigate rendered rows;
Shift extends selection. With an explicit width, `field-sizing: content` grows and
shrinks automatic height within explicit minimum/maximum heights. Use `resize: none`;
resize handles, additional modifier shortcuts and complete bidi navigation remain open.
Explicit solid rectangular outlines, including `:focus-visible` styles, are painted
with their width, color and offset; platform-default focus styling remains open.
Text inputs accept direct keyboard text, arrow/Home/End movement, Shift selection,
Space, Backspace and Delete, with a non-blinking caret and a fixed-gray placeholder.
A character keydown can be prevented by the guest, and Ctrl/Command+A selects all.
Ctrl/Command+Z undoes an edit; Shift+Ctrl/Command+Z or Ctrl/Command+Y redoes it.
History is bounded across the view and starts afresh when the guest changes the
field value. Consecutive keystrokes are separate history entries.
A click places the caret;
a drag within the window applies its selection on release. Live drag highlighting,
paste and bidirectional visual navigation are not implemented.
The preview displays IME preedit without changing the field value, then applies
the committed text as an input event. Composition is cancelled on window focus
loss, field focus changes and guest view commits. Guest composition events and
real CJK input-method acceptance remain open. Late IME text received while the
window is unfocused is ignored; malformed composition cancels the preedit.
Left-click activation is supported for the admitted
rectangular paint subset; events from stale frames are discarded. Vertical
wheel/trackpad scrolling follows the nearest scrollable ancestor and chains
remaining movement outward. Tab, Shift+Tab and the supported navigation and
activation keys are delivered to the focused view element. Window dimensions
also drive guest scroll commands from the initial turn onward. Positioned paths
and horizontal scroll-into-view are not yet supported. The window's device scale
controls physical-resolution painting while layout uses CSS pixels. DPI changes
update the viewport and invalidate stale frames and input; cross-display
acceptance remains open.
The system light/dark theme drives `prefers-color-scheme` from the first turn and
on theme changes. Direct OS theme-switch acceptance remains open.
Full IME support, complete accessibility, broader platform integration, packaging, target
acceptance and a production shell remain open. The preview is not a `--single` launcher
and does not accept a package manifest or embedded artifact.

The text-aware renderer also admits inline-only paragraphs with adjacent text,
nested semantic inline tags, shared shaping, inherited styles and fragment hits.
Flex/grid children are blockified by their parent. Fonts remain explicit; mixed
block/inline content, distinct inline border side colors and full typography are
outside this experimental subset. Solid underlines propagate through inline
text, retain decorating-box color/thickness and skip intersecting unhinted ink.
Solid outlines surround joined fragment contours, including rounded corners,
and paint after descendants with the original clips and opacity.
Vertical inline padding expands the background/hit region without changing
line height; percentage padding uses the containing block's content width.
Top/bottom margins do not move inline text, and horizontal auto margins are zero.
Horizontal padding, solid borders and signed px/rem margins become shaping
edges: they move
later text and wrap with their words without adding a break opportunity, and
the box's first/last fragment receives the opening/closing border and padding.
A border ring is sliced across fragments like a rounded background. Horizontal
percentages, distinct border side colors and edges in right-to-left text are
refused.
Element geometry unions do not make the gaps
between wrapped fragments clickable. See the native layout contract for scope.

## Local frame timing

The preview applies `:hover` to the pointed element and its ancestors, and
`:active` while the primary mouse button is down. Leaving the window or losing
focus clears pointer state. The last pointer sample is coalesced separately from
the activation/key FIFO, so motion cannot fill that queue. Native cursors honor
the checked `default`, `pointer`, `text` and `not-allowed` values, including inline
fragments. Color/cursor feedback preserves the frame's input revision; pointer
styles that change input geometry invalidate stale hit coordinates. Pointer state
does not dispatch a guest event, activate a control, edit a field or grant a gesture.

`frame_timing` separates a trusted click's guest turns from native CPU frame
construction for an unprivileged compiled view. Pass an existing button ID
without `#`. It reuses the explicit font and application for five samples each
at 800×600 CSS pixels, scales 1 and 2, under normal production budgets:

```sh
cargo run -p varta-desktop --example frame_timing --locked -- app.wasm font.ttf 'Font Family' button-id
```

The numbers exclude window presentation, event-queue delay and application
startup; they are local diagnostic observations, not a cross-platform benchmark
or throughput/readiness claim. Debug and release must be measured separately.
Use `--retained` to exercise the preview's persistent renderer and `--verify`
to compare every resulting pixel and input metadata against stateless rendering
outside the measured interval. `--type=текст` targets an editable ID;
`--scroll=40` alternates a scroll container between zero and that CSS offset.
`--samples=N` selects 1–512 samples per scale. An additional `--release` before
the cargo argument separator selects the release build. Run timings without a
concurrent compilation or test load. The first frame includes cold caches;
report it separately from subsequent frames.
`packing_ms` separately measures the same damaged-region RGBA-to-RGB conversion
and owned RGB copy used by the worker. Add it to `frame_ms` for complete worker
frame preparation; window presentation and event-queue latency remain excluded.
The dev profile keeps the view/runtime/compiler at opt-level 0, graphics
dependencies at opt-level 3, and the small desktop host at opt-level 3 with debug
assertions and symbols. Graphics SIMD is enabled in both profiles.
`--pointer` cycles hover, primary press, release and leave on the target without
activating it. It is mutually exclusive with `--type` and `--scroll`; it uses the
same retained/reference pixel and metadata comparison with `--verify`.
