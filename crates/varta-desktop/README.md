# Experimental desktop preview

`varta-preview` opens a native CPU-rendered window for a precompiled v38 view
module. Its Wasmtime application runs on a worker thread; the main thread owns
the window and presents bounded frames. Pass a TrueType/OpenType file and its
family name for the currently supported explicit-font text subset:

```sh
cargo run -p varta-desktop --bin varta-preview -- app.wasm font.ttf 'Font Family'
```

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

The font arguments can be omitted for a view without text. This preview uses no
package capability grants and has no permission prompts. It accepts only the
current experimental `varta-view` CPU painting subset. Unsupported content
fails explicitly; explicitly styled buttons and text inputs are included, including
disabled controls. Disabled platform-default styling is not supplied.
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
