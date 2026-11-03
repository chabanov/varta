# Experimental desktop preview

## UI diagnostics and GPU prototype (working tree)

The default build remains the software preview. `VARTA_PREVIEW_TIMING=1`
prints a host-entry-to-first-presentation sample plus host-only JSON input
diagnostics to stderr without input values: queue entry to
worker acceptance, accepted input/application-turn work (`guest_ms`), frame
preparation, window wait and host presentation return. This excludes OS event
delivery before queue entry, queued-key waiting before dispatch, GPU completion
and physical screen scanout. The `guest_ms` phase includes host input handling and application-step work; it
is not an isolated Wasm CPU timer. Multiple accepted inputs in a frame share
the first worker-accepted stamp and an explicit input count.
Pointer-state samples remain coalesced outside the activation FIFO; a generation
and condition variable wake the worker on pointer/input/frame acknowledgement
without losing notifications that arrive just before it waits.

An optional GPU prototype is implemented in the working tree:

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
explicit rectangular control boxes; platform-native control themes are not supplied.

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
Full IME support, accessibility, broader platform integration, packaging, target
acceptance and a production shell remain open. The preview is not a `--single` launcher
and does not accept a package manifest or embedded artifact.

The text-aware renderer also admits inline-only paragraphs with adjacent text,
nested semantic inline tags, shared shaping, inherited styles and fragment hits.
Flex/grid children are blockified by their parent. Fonts remain explicit; mixed
block/inline content, nonzero inline edges/decoration and full typography are
outside this experimental subset. Element geometry unions do not make the gaps
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
