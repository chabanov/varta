import { registry } from "../../../varta-common/browser/registry.mjs";

// The browser host's bridge to the view style engine (`view-engine.wasm`,
// the wasm32 build of the Rust `varta-style` crate the native renderer
// runs). Every stylesheet, class and style value is admitted, and every
// computed value produced, by that module; this file only moves strings and
// numbers across the boundary. See crates/varta-style/src/session.rs.

/** Status codes of `varta_style::session::status`. */
export const ENGINE_STATUS = Object.freeze({
  ok: 0, invalid: 1, unrenderable: 2, fontFamilyKeyword: 3,
  sheetId: 4, sheetInvalid: 5, sheetWork: 6, sheetBytes: 7,
  cascadeWork: 8, expandedValues: 9, misuse: 10,
});
/** Recorded media inputs (`varta_style::session::inputs`). */
export const ENGINE_INPUTS = Object.freeze({
  dark: 1, reducedMotion: 1 << 1, forcedColors: 1 << 2, contrastShift: 3,
  noHover: 1 << 5, noAnyHover: 1 << 6, pointerShift: 7, anyPointerShift: 9,
  packagedMonospace: 1 << 11,
});
/** Element state bits (`varta_style::ElementState`): the model records the
 * dialog states, the DOM bridge the platform's field states. */
export const ENGINE_STATES = Object.freeze({ open: 1, modal: 1 << 1, badInput: 1 << 2, userValidity: 1 << 3 });
/** The tag argument of `create` for a text node. */
const TEXT_NODE = 0xffffffff;
/** The family kinds the engine asks `fontMetrics` to measure
 * (`varta-style-wasm` `fonts`): a family by name, a generic family, or the
 * browser's default font. */
export const FONT_KINDS = Object.freeze(["named", "generic", "default"]);
/** The generic families of `face`'s `generics` argument, in bit order. */
const GENERIC_FAMILIES = ["serif", "sans-serif", "monospace", "system-ui"];
/** The measures `fontMetrics` returns, in the import's order. */
const FONT_MEASURES = 5;
const encoder = new TextEncoder(), decoder = new TextDecoder("utf-8", { fatal: true });
const propertyCodes = new Map(registry.view.css.map((row) => [row.name, row.code]));
const propertyNames = new Map(registry.view.css.map((row) => [row.code, row.name]));

/** One style session over an instance of the compiled engine module.
 * `fontMetrics(kind, family, weight, italic, size)` measures a font the
 * browser renders with its own (a `FONT_KINDS` kind): `[ex, cap, ch, ic,
 * line-height normal]` in CSS pixels, or `undefined` where it cannot. The
 * engine decides which font is the first available one; without the
 * callback (a Worker), the units take the CSS Values 4 fallbacks. */
export function createViewEngine(module, { fontMetrics } = {}) {
  if (!(module instanceof WebAssembly.Module)) throw new TypeError("view engine module required");
  let exports;
  const font_metrics = (kind, family, length, weight, italic, size, out) => {
    const name = decoder.decode(new Uint8Array(exports.memory.buffer, family, length));
    const measured = fontMetrics?.(FONT_KINDS[kind], name, weight, italic === 1, size);
    if (!Array.isArray(measured) || measured.length !== FONT_MEASURES || !measured.every(Number.isFinite)) return 0;
    new Float64Array(exports.memory.buffer, out, FONT_MEASURES).set(measured);
    return 1;
  };
  exports = new WebAssembly.Instance(module, { varta_view: { font_metrics } }).exports;
  const input = (text) => {
    const bytes = encoder.encode(text);
    const at = exports.view_engine_input(bytes.length);
    new Uint8Array(exports.memory.buffer, at, bytes.length).set(bytes);
  };
  const code = (property) => {
    const value = propertyCodes.get(property);
    if (value === undefined) throw new TypeError(`unknown CSS property ${property}`);
    return value;
  };
  return {
    begin: () => exports.view_engine_begin(),
    commit: () => exports.view_engine_commit(),
    abort: () => exports.view_engine_abort(),
    /** `tag` is an element's tag code, or `null` for a text node. */
    create: (id, tag) => exports.view_engine_create(id, tag ?? TEXT_NODE),
    insert: (parent, child, before) => exports.view_engine_insert(parent, child, before),
    remove: (id) => exports.view_engine_remove(id),
    setClass(id, text) { input(text); return exports.view_engine_class(id); },
    setStyle(id, property, text) { input(text); return exports.view_engine_style(id, code(property)); },
    removeStyle: (id, property) => exports.view_engine_unstyle(id, code(property)),
    setFlags: (id, disabled, checked) => exports.view_engine_flags(id, disabled ? 1 : 0, checked ? 1 : 0),
    /** An attribute the model admitted, by name. */
    setAttribute(id, name, value) {
      const key = encoder.encode(name), text = encoder.encode(value), at = exports.view_engine_input(key.length + text.length);
      const buffer = new Uint8Array(exports.memory.buffer, at, key.length + text.length);
      buffer.set(key); buffer.set(text, key.length);
      return exports.view_engine_attribute(id, key.length);
    },
    removeAttribute(id, name) { input(name); return exports.view_engine_unattribute(id); },
    /** A form control's value or a text node's data. */
    setValue(id, value) { input(value); return exports.view_engine_value(id); },
    /** The `ENGINE_STATES` bits of `mask`. */
    setStates: (id, bits, mask) => exports.view_engine_states(id, bits, mask),
    stylesheet(id, text) { input(text); return exports.view_engine_sheet(id); },
    /** A packaged face: its family, font file bytes and the generic
     * families it stands for. */
    face(family, bytes, generics = []) {
      const indices = generics.map((generic) => GENERIC_FAMILIES.indexOf(generic.toLowerCase()));
      if (indices.includes(-1)) throw new TypeError("unknown generic family");
      const name = encoder.encode(family), at = exports.view_engine_input(name.length + bytes.length);
      const buffer = new Uint8Array(exports.memory.buffer, at, name.length + bytes.length);
      buffer.set(name); buffer.set(bytes, name.length);
      return exports.view_engine_face(name.length, indices.reduce((bits, index) => bits | (1 << index), 0));
    },
    /** The family of text without `font-family`; `undefined` is the
     * browser's default font. */
    defaultFamily(family) { input(family ?? ""); return exports.view_engine_default_family(); },
    charge: () => exports.view_engine_charge(),
    environment: (width, height, rootFontSize, mediumFontSize, flags) => exports.view_engine_environment(width, height, rootFontSize, mediumFontSize, flags),
    state: (focusVisible, hovered, active) => exports.view_engine_state(focusVisible, hovered, active),
    focus: (focused) => exports.view_engine_focus(focused),
    /** The committed computed values of element `id`: property name to CSS text. */
    computed(id) {
      const length = exports.view_engine_computed(id);
      const bytes = new Uint8Array(exports.memory.buffer, exports.view_engine_output(), length).slice();
      const view = new DataView(bytes.buffer), values = new Map();
      for (let at = 0; at < bytes.length;) {
        const property = view.getUint16(at, true), size = view.getUint32(at + 2, true);
        values.set(propertyNames.get(property), decoder.decode(bytes.subarray(at + 6, at + 6 + size)));
        at += 6 + size;
      }
      return values;
    },
  };
}

/** The recorded media inputs of a window: viewport, root font size and the
 * preference features as the engine's bit set (Media Queries 4/5). */
export function readMediaInputs(window) {
  const matches = (query) => window.matchMedia(query).matches;
  const I = ENGINE_INPUTS;
  const pointer = (name) => matches(`(${name}: coarse)`) ? 1 : matches(`(${name}: none)`) ? 2 : 0;
  const contrast = matches("(prefers-contrast: more)") ? 1 : matches("(prefers-contrast: less)") ? 2 : matches("(prefers-contrast: custom)") ? 3 : 0;
  const flags = (matches("(prefers-color-scheme: dark)") ? I.dark : 0)
    | (matches("(prefers-reduced-motion: reduce)") ? I.reducedMotion : 0)
    | (matches("(forced-colors: active)") ? I.forcedColors : 0)
    | (contrast << I.contrastShift)
    | (matches("(hover: none)") ? I.noHover : 0)
    | (matches("(any-hover: none)") ? I.noAnyHover : 0)
    | (pointer("pointer") << I.pointerShift)
    | (pointer("any-pointer") << I.anyPointerShift);
  const root = Number.parseFloat(window.getComputedStyle(window.document.documentElement).fontSize);
  return { width: window.innerWidth, height: window.innerHeight, rootFontSize: Number.isFinite(root) ? root : 16, flags };
}

/** The media queries whose changes change the recorded inputs. */
export const MEDIA_INPUT_QUERIES = Object.freeze([
  "(prefers-color-scheme: dark)", "(prefers-reduced-motion: reduce)", "(forced-colors: active)",
  "(prefers-contrast: more)", "(prefers-contrast: less)", "(prefers-contrast: custom)",
  "(hover: none)", "(any-hover: none)", "(pointer: coarse)", "(pointer: none)",
  "(any-pointer: coarse)", "(any-pointer: none)",
]);
