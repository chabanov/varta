// Browser applier for the L0 view protocol draft v0, rounds 1–5 (embedding.md
// "View protocol draft v0 (unstable)" and its round sections), written only from
// the published tables and rules. It is the trust boundary between guest patches
// and the DOM: no application logic, no HTML parsing, whole-batch validation
// before any change, fail closed.
//
// The guest tree lives in a closed shadow root with `:host { all: initial }`; the
// applier's base stylesheet sits below every guest sheet.
import { createValueMatcher, checkStylesheet, checkStyleDeclaration, rewriteRootSelector, CssError } from "./view-css.js";
import { wellFormed, toWellFormed, attributeValueOk, attributeAllowedOn, commandAllowedOn, isExternalUrl } from "./view-rules.js";

export class ViewProtocolError extends Error {}

const LITTLE_ENDIAN = true;
const U16_BYTES = 2;
const U32_BYTES = 4;
const F64_BYTES = 8;
const OPERATION_PADDING_BYTES = 3;
const CODE_PADDING_BYTES = 2;
const CLASS_NAME = /^[A-Za-z_][A-Za-z0-9_-]*$/;
const FIELD_TAGS = new Set(["input", "textarea", "select"]);
const CHECKABLE_TYPES = new Set(["checkbox", "radio"]);
const TEXTAREA_ONLY = /textarea only/;
const NEW_WINDOW = ["_blank", "noopener,noreferrer"];
// DOM event per protocol event; non-bubbling ones are captured at the root.
const DOM_EVENT = {
  click: { dom: "click", capture: false },
  input: { dom: "input", capture: false },
  change: { dom: "change", capture: false },
  submit: { dom: "submit", capture: false },
  keydown: { dom: "keydown", capture: false },
  focusin: { dom: "focusin", capture: false },
  focusout: { dom: "focusout", capture: false },
  scroll: { dom: "scroll", capture: true },
  cancel: { dom: "cancel", capture: true },
  close: { dom: "close", capture: true },
};
const COMPOSING_KEY_CODE = 229;

const pair = (text) => text.split(" / ").map(Number);
const bit = (layout, key) => Number(/bit value (\d+)/.exec(layout[key])[1]);
const invert = (table) => new Map(Object.entries(table).map(([name, code]) => [code, name]));

function eventOffsets(layout) {
  const offsets = {};
  for (const [key, value] of Object.entries(layout)) {
    const match = /^Event field `(\w+)`$/.exec(key);
    if (match) offsets[match[1]] = Number(/offset (\d+)/.exec(value)[1]);
  }
  return offsets;
}

function protocol(view) {
  const layout = view.layout;
  const [batchBytes, operations] = pair(layout["Placeholder: batch bytes / operations"]);
  const [liveNodes, listeners] = pair(layout["Placeholder: live nodes / listeners"]);
  const [valueBytes, stylesheetBytes] = pair(layout["Placeholder: value bytes / stylesheet bytes / pending events"]);
  const rootSelector = view.defaults["Browser selector substituted for `:root`"][0];
  return {
    limits: { batchBytes, operations, liveNodes, listeners, valueBytes, stylesheetBytes, pendingCopies: Number(layout["Placeholder: pending `copy_text` requests"]) },
    version: Number(layout["Draft version"]),
    rootId: Number(layout["Root node id"]),
    noNode: Number(layout['"No node" id']),
    batchHeaderBytes: Number(layout["Batch header bytes"]),
    operationHeaderBytes: Number(layout["Operation header bytes"]),
    stringAlignment: Number(layout["String alignment bytes"]),
    eventHeaderBytes: Number(layout["Event header bytes"]),
    anyKey: Number(/key code (\d+)/.exec(layout["Key filter: any key"])[1]),
    modifierMask: Number(/^(\d+)/.exec(layout["Key filter: modifier mask"])[1]),
    option: {
      preventDefault: bit(layout, "`listen` option `preventDefault`"),
      stopPropagation: bit(layout, "`listen` option `stopPropagation`"),
      notComposing: bit(layout, "`listen` option `notComposing`"),
    },
    flag: Object.fromEntries(["shiftKey", "ctrlKey", "altKey", "metaKey", "isComposing", "checked"].map((n) => [n, bit(layout, `Event flag \`${n}\``)])),
    field: eventOffsets(layout),
    rootAttribute: /^\[([a-z0-9-]+)\]$/.exec(rootSelector)[1],
    rootSelector,
  };
}

/** The applier's own base stylesheet: the draft's defaults, below every guest sheet. */
function baseStylesheet(view) {
  const d = view.defaults;
  const [backdropLight, backdropDark] = d["`dialog` backdrop colour"];
  const [backgroundLight, backgroundDark] = d["`dialog` background colour"];
  const [textLight, textDark] = d["`dialog` text colour"];
  // The view root (node 1) fills the host element, which the embedder sizes (round 7).
  const rootStyle = d["View root (node 1) base style"][0];
  return `:host { all: initial; display: block; }
${d["Browser selector substituted for `:root`"][0]} { ${rootStyle}; }
textarea { resize: none; }
dialog { border: none; padding: 0; background-color: ${backgroundLight}; color: ${textLight}; }
dialog::backdrop { background-color: ${backdropLight}; }
@media (prefers-color-scheme: dark) {
  dialog { background-color: ${backgroundDark}; color: ${textDark}; }
  dialog::backdrop { background-color: ${backdropDark}; }
}`;
}

/**
 * @param {object} view contract.view
 * @param {{container: Element, assetBase: string,
 *   openUrlPatterns?: string[], clipboardRequested?: boolean,
 *   broker?: (capability: string, resource: string, perform: () => Promise<number>, notAllowed: number) => Promise<number>,
 *   onEvent: (record: Uint8Array) => void, onTitle?: (title: string) => void,
 *   clipboard?: {writeText: (text: string) => Promise<void>},
 *   openWindow?: (url: string, target: string, features: string) => unknown}} options
 */
export function createApplier(view, options) {
  const P = protocol(view);
  const opcodes = new Map(view.opcodes.map((o) => [o.opcode, o]));
  const tags = invert(view.tags);
  const attributes = invert(view.attributes);
  const cssProperties = invert(view.cssProperties);
  const events = invert(view.events);
  const commands = invert(view.commands);
  const keys = new Map(Object.values(view.keys).map((k) => [k.code, k.domKey]));
  const results = Object.fromEntries(Object.entries(view.commandResults).map(([name, r]) => [name, r.status]));
  const checkValue = createValueMatcher(view.cssGrammar, view.cssNotes);
  const openUrlPatterns = options.openUrlPatterns ?? [];
  const broker = options.broker ?? (async (_capability, _resource, _perform, notAllowed) => notAllowed);
  const clipboard = options.clipboard ?? globalThis.navigator?.clipboard;
  const openWindow = options.openWindow ?? ((...args) => globalThis.window.open(...args));
  const doc = options.container.ownerDocument;
  const optionMask = P.option.preventDefault | P.option.stopPropagation | P.option.notComposing;

  const shadow = options.container.attachShadow({ mode: "closed" });
  const rootElement = doc.createElement("div");
  rootElement.setAttribute(P.rootAttribute, "");
  shadow.append(rootElement);
  const base = new CSSStyleSheet();
  base.replaceSync(baseStylesheet(view));
  const sheets = new Map();
  const adopt = () => {
    shadow.adoptedStyleSheets = [base, ...[...sheets.entries()].sort(([a], [b]) => a - b).map(([, s]) => s.sheet)];
  };
  adopt();

  // Committed model: id -> {kind, tag, parent, children[], listeners[], type, tabindex}.
  let model = new Map([[P.rootId, { kind: "element", tag: "div", parent: P.noNode, children: [], listeners: [] }]]);
  const domNodes = new Map([[P.rootId, rootElement]]);
  const idOf = new WeakMap([[rootElement, P.rootId]]);
  const fieldSeq = new Map();
  const pendingCopies = new Set();
  let stylesheetBytes = 0;

  // ------------------------------------------------------------------ decoding

  function decode(bytes) {
    if (bytes.length > P.limits.batchBytes) throw new ViewProtocolError("batch too large");
    const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let at = 0;
    const need = (n) => {
      if (at + n > bytes.length) throw new ViewProtocolError("truncated batch");
    };
    const u16 = () => (need(U16_BYTES), (at += U16_BYTES), dv.getUint16(at - U16_BYTES, LITTLE_ENDIAN));
    const u32 = () => (need(U32_BYTES), (at += U32_BYTES), dv.getUint32(at - U32_BYTES, LITTLE_ENDIAN));
    const f64 = () => (need(F64_BYTES), (at += F64_BYTES), dv.getFloat64(at - F64_BYTES, LITTLE_ENDIAN));
    const zeros = (n) => {
      need(n);
      for (let i = 0; i < n; i += 1) if (bytes[at + i] !== 0) throw new ViewProtocolError("nonzero padding");
      at += n;
    };
    const string = () => {
      const length = u32();
      if (length % U16_BYTES) throw new ViewProtocolError("odd UTF-16 string length");
      if (length > Math.max(P.limits.valueBytes, P.limits.stylesheetBytes)) throw new ViewProtocolError("string too large");
      need(length);
      let text = "";
      for (let i = 0; i < length; i += U16_BYTES) text += String.fromCharCode(dv.getUint16(at + i, LITTLE_ENDIAN));
      at += length;
      zeros((P.stringAlignment - (length % P.stringAlignment)) % P.stringAlignment);
      if (!wellFormed(text)) throw new ViewProtocolError("string with a lone surrogate or U+0000");
      return { text, bytes: length };
    };
    if (bytes.length < P.batchHeaderBytes) throw new ViewProtocolError("truncated batch header");
    if (u16() !== P.version) throw new ViewProtocolError("unsupported view protocol version");
    if (u16() !== 0) throw new ViewProtocolError("batch flags must be 0");
    const count = u32();
    if (count > P.limits.operations) throw new ViewProtocolError("too many operations");
    const ops = [];
    for (let n = 0; n < count; n += 1) {
      need(P.operationHeaderBytes);
      const spec = opcodes.get(bytes[at]);
      if (!spec) throw new ViewProtocolError(`unknown opcode ${bytes[at]}`);
      at += 1;
      zeros(OPERATION_PADDING_BYTES);
      const op = { name: spec.name };
      for (const operand of spec.operands) {
        if (operand.kind === "id" || operand.kind === "u32") op[operand.name] = u32();
        else if (operand.kind === "f64") op[operand.name] = f64();
        else if (operand.kind === "string") op[operand.name] = string();
        else {
          op[operand.name] = u16();
          zeros(CODE_PADDING_BYTES);
        }
      }
      ops.push(op);
    }
    if (at !== bytes.length) throw new ViewProtocolError("trailing bytes after the last operation");
    return ops;
  }

  // ---------------------------------------------------------------- validation

  function named(table, code, what) {
    const name = table.get(code);
    if (!name) throw new ViewProtocolError(`unknown ${what} code ${code}`);
    return name;
  }

  const sameFilter = (a, b) => a.event === b.event && a.key === b.key && a.required === b.required && a.forbidden === b.forbidden && Boolean(a.options & P.option.notComposing) === Boolean(b.options & P.option.notComposing);

  /** Validates every operation against a copy of the model; returns the new state. */
  function validate(ops) {
    const next = new Map();
    for (const [id, node] of model) next.set(id, { ...node, children: [...node.children], listeners: [...node.listeners] });
    const handlers = new Set([...next.values()].flatMap((n) => n.listeners.map((l) => l.handler)));
    const live = (id) => {
      const node = next.get(id);
      if (id === P.noNode || !node) throw new ViewProtocolError(`unknown or dead node id ${id}`);
      return node;
    };
    const element = (id) => {
      const node = live(id);
      if (node.kind !== "element") throw new ViewProtocolError(`node ${id} is not an element`);
      return node;
    };
    const fresh = (id) => {
      if (id === P.noNode || next.has(id)) throw new ViewProtocolError(`node id ${id} is zero or already live`);
      if (next.size >= P.limits.liveNodes) throw new ViewProtocolError("too many live nodes");
    };
    const valueSize = (s) => {
      if (s.bytes > P.limits.valueBytes) throw new ViewProtocolError("value too large");
    };
    const detach = (id) => {
      const node = next.get(id);
      if (node.parent !== P.noNode) {
        const siblings = next.get(node.parent).children;
        siblings.splice(siblings.indexOf(id), 1);
        node.parent = P.noNode;
      }
    };
    let sheetBytes = stylesheetBytes;
    const sheetSizes = new Map([...sheets].map(([id, s]) => [id, s.bytes]));
    const copies = new Set(pendingCopies);

    for (const op of ops) {
      switch (op.name) {
        case "create_element":
          fresh(op.node);
          next.set(op.node, { kind: "element", tag: named(tags, op.tag, "tag"), parent: P.noNode, children: [], listeners: [] });
          break;
        case "create_text":
          fresh(op.node);
          valueSize(op.text);
          next.set(op.node, { kind: "text", tag: null, parent: P.noNode, children: [], listeners: [] });
          break;
        case "set_text":
          if (live(op.node).kind !== "text") throw new ViewProtocolError("set_text applies only to text nodes");
          valueSize(op.text);
          break;
        case "set_attribute": {
          const name = named(attributes, op.attribute, "attribute");
          const node = element(op.node);
          const rule = view.attributeRules[name];
          if (!attributeAllowedOn(rule, node.tag)) throw new ViewProtocolError(`${name} is not allowed on <${node.tag}>`);
          valueSize(op.text);
          if (!attributeValueOk(rule, node.tag, op.text.text, { openUrlPatterns })) throw new ViewProtocolError(`${name}=${JSON.stringify(op.text.text)} violates "${rule.value}"`);
          if (name === "type") node.type = op.text.text;
          if (name === "tabindex") node.tabindex = true;
          break;
        }
        case "remove_attribute": {
          const name = named(attributes, op.attribute, "attribute");
          const node = element(op.node);
          if (!attributeAllowedOn(view.attributeRules[name], node.tag)) throw new ViewProtocolError(`${name} is not allowed on <${node.tag}>`);
          if (name === "type") delete node.type;
          if (name === "tabindex") delete node.tabindex;
          break;
        }
        case "set_class":
          element(op.node);
          valueSize(op.text);
          if (op.text.text.split(" ").filter(Boolean).some((c) => !CLASS_NAME.test(c))) throw new ViewProtocolError("class names must be idents");
          break;
        case "set_style": {
          const node = element(op.node);
          const property = named(cssProperties, op.property, "CSS property");
          if (TEXTAREA_ONLY.test(view.cssNotes[property] ?? "") && node.tag !== "textarea") throw new ViewProtocolError(`${property} applies only to textarea`);
          valueSize(op.text);
          try {
            checkStyleDeclaration(view, property, op.text.text, checkValue);
          } catch (error) {
            if (error instanceof CssError) throw new ViewProtocolError(error.message);
            throw error;
          }
          break;
        }
        case "remove_style":
          element(op.node);
          named(cssProperties, op.property, "CSS property");
          break;
        case "insert": {
          element(op.parent);
          live(op.node);
          if (op.node === P.rootId) throw new ViewProtocolError("the root cannot be inserted");
          for (let at = op.parent; at !== P.noNode; at = next.get(at).parent) {
            if (at === op.node) throw new ViewProtocolError("insert would create a cycle");
          }
          if (op.before !== P.noNode && (op.before === op.node || live(op.before).parent !== op.parent)) throw new ViewProtocolError("before is not a child of parent");
          detach(op.node);
          const siblings = next.get(op.parent).children;
          siblings.splice(op.before === P.noNode ? siblings.length : siblings.indexOf(op.before), 0, op.node);
          next.get(op.node).parent = op.parent;
          break;
        }
        case "remove": {
          live(op.node);
          if (op.node === P.rootId) throw new ViewProtocolError("the root cannot be removed");
          detach(op.node);
          const release = (id) => {
            const node = next.get(id);
            node.listeners.forEach((l) => handlers.delete(l.handler));
            node.children.forEach(release);
            next.delete(id);
          };
          release(op.node);
          break;
        }
        case "listen": {
          const node = element(op.node);
          const event = named(events, op.event, "event");
          if (!(event in DOM_EVENT)) throw new ViewProtocolError(`${event} cannot be listened to`);
          if (op.handler === 0 || handlers.has(op.handler)) throw new ViewProtocolError("handler ids are nonzero and unique among live listeners");
          if (op.options & ~optionMask) throw new ViewProtocolError("unknown listen option bits");
          if ((op.required_modifiers | op.forbidden_modifiers) & ~P.modifierMask) throw new ViewProtocolError("unknown modifier bits");
          if (op.key !== P.anyKey) named(keys, op.key, "key");
          if (event !== "keydown" && (op.key !== P.anyKey || op.required_modifiers || op.forbidden_modifiers || op.options & P.option.notComposing)) {
            throw new ViewProtocolError("key filters apply only to keydown");
          }
          const listener = { event, handler: op.handler, options: op.options, key: op.key, required: op.required_modifiers, forbidden: op.forbidden_modifiers };
          if (node.listeners.some((l) => sameFilter(l, listener))) throw new ViewProtocolError(`node ${op.node} already has an identical ${event} filter`);
          if (handlers.size >= P.limits.listeners) throw new ViewProtocolError("too many listeners");
          node.listeners.push(listener);
          handlers.add(op.handler);
          break;
        }
        case "unlisten": {
          const node = element(op.node);
          const event = named(events, op.event, "event");
          const index = node.listeners.findIndex((l) => l.event === event && l.handler === op.handler);
          if (index < 0) throw new ViewProtocolError(`handler ${op.handler} is not listening to ${event} on node ${op.node}`);
          node.listeners.splice(index, 1);
          handlers.delete(op.handler);
          break;
        }
        case "set_value":
          if (!FIELD_TAGS.has(element(op.node).tag)) throw new ViewProtocolError("set_value needs input, textarea or select");
          valueSize(op.text);
          break;
        case "set_checked": {
          const node = element(op.node);
          if (node.tag !== "input" || !CHECKABLE_TYPES.has(node.type)) throw new ViewProtocolError("set_checked needs a checkbox or radio input");
          if (op.checked > 1) throw new ViewProtocolError("checked must be 0 or 1");
          break;
        }
        case "command": {
          const name = named(commands, op.command, "command");
          const node = element(op.node);
          if (!commandAllowedOn(view.commandRules[name], node.tag, node.tabindex)) throw new ViewProtocolError(`${name} is not allowed on <${node.tag}>`);
          if (name === "scroll_to" ? !(Number.isFinite(op.argument) && op.argument >= 0) : op.argument !== 0) throw new ViewProtocolError(`bad ${name} argument`);
          op.commandName = name;
          break;
        }
        case "stylesheet": {
          if (op.sheet === 0) throw new ViewProtocolError("sheet ids are nonzero");
          if (op.text.text !== "") {
            let checked;
            try {
              checked = checkStylesheet(op.text.text, view);
            } catch (error) {
              if (error instanceof CssError) throw new ViewProtocolError(error.message);
              throw error;
            }
            if (checked.errors.length) throw new ViewProtocolError(`invalid stylesheet: ${checked.errors[0]}`);
          }
          sheetBytes += op.text.bytes - (sheetSizes.get(op.sheet) ?? 0);
          sheetSizes.set(op.sheet, op.text.bytes);
          if (sheetBytes > P.limits.stylesheetBytes) throw new ViewProtocolError("stylesheet budget exceeded");
          break;
        }
        case "set_title":
          valueSize(op.text);
          break;
        case "copy_text":
          if (op.request === 0 || copies.has(op.request)) throw new ViewProtocolError("copy_text request ids are nonzero and unique among pending requests");
          if (copies.size >= P.limits.pendingCopies) throw new ViewProtocolError("too many pending copy_text requests");
          valueSize(op.text);
          copies.add(op.request);
          break;
        default:
          throw new ViewProtocolError(`unhandled operation ${op.name}`);
      }
    }
    return { next, sheetBytes };
  }

  // ------------------------------------------------------------------ applying

  function applyOne(op, gesture) {
    const at = (id) => domNodes.get(id);
    switch (op.name) {
      case "create_element": {
        const node = doc.createElement(tags.get(op.tag));
        domNodes.set(op.node, node);
        idOf.set(node, op.node);
        if (FIELD_TAGS.has(node.localName)) fieldSeq.set(op.node, 0);
        return;
      }
      case "create_text": {
        const node = doc.createTextNode(op.text.text);
        domNodes.set(op.node, node);
        idOf.set(node, op.node);
        return;
      }
      case "set_text":
        at(op.node).data = op.text.text;
        return;
      case "set_attribute":
        at(op.node).setAttribute(attributes.get(op.attribute), op.text.text);
        return;
      case "remove_attribute":
        at(op.node).removeAttribute(attributes.get(op.attribute));
        return;
      case "set_class":
        at(op.node).setAttribute("class", op.text.text);
        return;
      case "set_style":
        at(op.node).style.setProperty(cssProperties.get(op.property), op.text.text);
        return;
      case "remove_style":
        at(op.node).style.removeProperty(cssProperties.get(op.property));
        return;
      case "insert":
        at(op.parent).insertBefore(at(op.node), op.before === P.noNode ? null : at(op.before));
        return;
      case "remove": {
        const release = (node) => {
          for (const child of node.childNodes) release(child);
          const id = idOf.get(node);
          domNodes.delete(id);
          fieldSeq.delete(id);
        };
        const node = at(op.node);
        node.remove();
        release(node);
        return;
      }
      case "listen":
      case "unlisten":
        return; // listeners live in the model; one delegated DOM listener per kind
      case "set_value": {
        if (op.base_seq !== fieldSeq.get(op.node)) return; // stale: newer local edits exist
        const node = at(op.node);
        if (node.value !== op.text.text) node.value = op.text.text;
        return;
      }
      case "set_checked":
        if (op.base_seq === fieldSeq.get(op.node)) at(op.node).checked = op.checked === 1;
        return;
      case "command":
        runCommand(at(op.node), op.commandName, op.argument);
        return;
      case "stylesheet":
        if (op.text.text === "") sheets.delete(op.sheet);
        else {
          const sheet = sheets.get(op.sheet)?.sheet ?? new CSSStyleSheet();
          sheet.replaceSync(rewriteRootSelector(op.text.text, P.rootSelector));
          sheets.set(op.sheet, { sheet, bytes: op.text.bytes });
        }
        adopt();
        return;
      case "set_title":
        options.onTitle?.(op.text.text);
        return;
      case "copy_text":
        pendingCopies.add(op.request);
        copyText(op.request, op.text.text, gesture);
        return;
    }
  }

  function runCommand(node, name, argument) {
    if (name === "focus") node.focus();
    else if (name === "blur") node.blur();
    else if (name === "select") node.select();
    else if (name === "scroll_into_view") node.scrollIntoView({ block: "nearest" });
    else if (name === "scroll_to") node.scrollTop = argument;
    else if (name === "show_modal") {
      if (!node.open) node.showModal();
    } else if (name === "close") node.close();
  }

  // The broker owns the decision (request ∩ ceiling, stored decision, prompt) and
  // runs `perform` itself: at once when already granted, or inside the click
  // handler of its own "Allow" button, which is a fresh trusted gesture (round 6).
  // It resolves to the status of `perform`, or `not_allowed`.
  const decide = (capability, resource, perform) => broker(capability, resource, perform, results.not_allowed);

  // Every copy request gets exactly one command_result (embedding.md round 5).
  async function copyText(request, text, gesture) {
    let status;
    if (!gesture) status = results.invalid_context;
    else if (!options.clipboardRequested) status = results.not_allowed;
    else {
      status = await decide("clipboard.write", "", async () => {
        try {
          await clipboard.writeText(text);
          return results.ok;
        } catch {
          return results.refused_by_platform;
        }
      });
    }
    pendingCopies.delete(request);
    options.onEvent(encodeEvent("command_result", 0, 0, { seq: request, status, key: "copy_text" }));
  }

  // Opening happens only inside the handling of a trusted activation of the link;
  // in a browser only a throwing window.open is a platform refusal (round 6).
  async function openExternal(anchorId, url) {
    const status = await decide("ui.open_url", url, async () => {
      try {
        openWindow(url, ...NEW_WINDOW);
        return results.ok;
      } catch {
        return results.refused_by_platform;
      }
    });
    options.onEvent(encodeEvent("command_result", 0, anchorId, { status, key: "open_url" }));
  }

  /**
   * Validates the whole batch, then applies it. `gesture` tells whether the turn
   * that produced the batch handled a trusted click or keydown record. Any error is
   * fatal for the instance.
   */
  function apply(bytes, { gesture = false } = {}) {
    const ops = decode(bytes);
    const { next, sheetBytes } = validate(ops);
    model = next;
    stylesheetBytes = sheetBytes;
    for (const op of ops) applyOne(op, gesture);
  }

  // -------------------------------------------------------------------- events

  function encodeEvent(kind, handler, target, details) {
    const strings = [toWellFormed(details.key ?? ""), toWellFormed(details.value ?? "")];
    const pad = (n) => (P.stringAlignment - (n % P.stringAlignment)) % P.stringAlignment;
    const bytes = new Uint8Array(P.eventHeaderBytes + strings.reduce((n, s) => n + U32_BYTES + s.length * U16_BYTES + pad(s.length * U16_BYTES), 0));
    const dv = new DataView(bytes.buffer);
    const f = P.field;
    dv.setUint16(f.version, P.version, LITTLE_ENDIAN);
    dv.setUint16(f.kind, view.events[kind], LITTLE_ENDIAN);
    dv.setUint32(f.handler, handler, LITTLE_ENDIAN);
    dv.setUint32(f.target, target, LITTLE_ENDIAN);
    dv.setUint32(f.seq, details.seq ?? 0, LITTLE_ENDIAN);
    dv.setUint32(f.flags, details.flags ?? 0, LITTLE_ENDIAN);
    dv.setUint32(f.status, details.status ?? 0, LITTLE_ENDIAN);
    dv.setFloat64(f.scroll_top, details.scrollTop ?? 0, LITTLE_ENDIAN);
    dv.setFloat64(f.scroll_height, details.scrollHeight ?? 0, LITTLE_ENDIAN);
    dv.setFloat64(f.client_height, details.clientHeight ?? 0, LITTLE_ENDIAN);
    let at = P.eventHeaderBytes;
    for (const s of strings) {
      dv.setUint32(at, s.length * U16_BYTES, LITTLE_ENDIAN);
      at += U32_BYTES;
      for (let i = 0; i < s.length; i += 1, at += U16_BYTES) dv.setUint16(at, s.charCodeAt(i), LITTLE_ENDIAN);
      at += pad(s.length * U16_BYTES);
    }
    return bytes;
  }

  function nearestId(node) {
    for (let at = node; at && at !== shadow; at = at.parentNode) {
      const id = idOf.get(at);
      if (id !== undefined) return id;
    }
    return undefined;
  }

  function modifiers(event) {
    return (event.shiftKey ? P.flag.shiftKey : 0) | (event.ctrlKey ? P.flag.ctrlKey : 0) | (event.altKey ? P.flag.altKey : 0) | (event.metaKey ? P.flag.metaKey : 0);
  }

  function filterMatches(listener, event) {
    if (listener.event !== "keydown") return true;
    if (listener.key !== P.anyKey && keys.get(listener.key) !== event.key) return false;
    const held = modifiers(event);
    if ((held & listener.required) !== listener.required || held & listener.forbidden) return false;
    const composing = event.isComposing || event.keyCode === COMPOSING_KEY_CODE;
    return !(listener.options & P.option.notComposing && composing);
  }

  function details(kind, event, targetNode, targetId) {
    const flags = modifiers(event) | (event.isComposing ? P.flag.isComposing : 0) | (targetNode?.checked ? P.flag.checked : 0);
    const out = { flags };
    if (kind === "keydown") out.key = event.key;
    if ((kind === "input" || kind === "change") && FIELD_TAGS.has(targetNode?.localName)) {
      out.value = targetNode.value;
      out.seq = fieldSeq.get(targetId);
    }
    if (kind === "scroll") {
      out.scrollTop = targetNode.scrollTop;
      out.scrollHeight = targetNode.scrollHeight;
      out.clientHeight = targetNode.clientHeight;
    }
    return out;
  }

  function dispatch(kind, event) {
    const targetNode = event.composedPath()[0];
    const targetId = nearestId(targetNode);
    if (targetId === undefined) return;
    // Only `input` advances a field's edit counter (generated event table).
    if (view.eventRules[kind]?.incrementsSeq && fieldSeq.has(targetId)) fieldSeq.set(targetId, fieldSeq.get(targetId) + 1);
    if (kind === "submit") event.preventDefault();
    const anchor = kind === "click" ? targetNode.closest?.("a") : null;
    if (anchor) event.preventDefault(); // an `a` never navigates
    const bubbles = !DOM_EVENT[kind].capture;
    let stopped = false;
    for (let id = targetId; !stopped && id !== undefined && id !== P.noNode; id = bubbles ? model.get(id)?.parent : undefined) {
      for (const listener of model.get(id)?.listeners ?? []) {
        if (listener.event !== kind || !filterMatches(listener, event)) continue;
        if (listener.options & P.option.preventDefault) event.preventDefault();
        options.onEvent(encodeEvent(kind, listener.handler, targetId, details(kind, event, targetNode, targetId)));
        if (listener.options & P.option.stopPropagation) {
          stopped = true;
          break;
        }
      }
      if (!bubbles) break;
    }
    if (anchor && event.isTrusted && isExternalUrl(anchor.getAttribute("href") ?? "")) openExternal(idOf.get(anchor), anchor.getAttribute("href"));
  }

  for (const [kind, { dom, capture }] of Object.entries(DOM_EVENT)) {
    shadow.addEventListener(dom, (event) => dispatch(kind, event), { capture });
  }

  return {
    apply,
    /** For conformance tests: the live node count and the field counters. */
    inspect: () => ({ liveNodes: model.size, fieldSeq: Object.fromEntries(fieldSeq), pendingCopies: pendingCopies.size }),
    /** For conformance tests only: the guest subtree (the shadow root is closed). */
    debugRoot: () => rootElement,
    /** For conformance tests only: simulate the trusted activation of an external link. */
    debugActivateLink: (id) => openExternal(id, domNodes.get(id).getAttribute("href")),
  };
}

/** Decodes an event record (for tests and for a host that forwards it to the guest). */
export function decodeEventRecord(view, bytes) {
  const layout = view.layout;
  const field = eventOffsets(layout);
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const alignment = Number(layout["String alignment bytes"]);
  let at = Number(layout["Event header bytes"]);
  const string = () => {
    const length = dv.getUint32(at, LITTLE_ENDIAN);
    at += U32_BYTES;
    let text = "";
    for (let i = 0; i < length; i += U16_BYTES) text += String.fromCharCode(dv.getUint16(at + i, LITTLE_ENDIAN));
    at += length + ((alignment - (length % alignment)) % alignment);
    return text;
  };
  return {
    version: dv.getUint16(field.version, LITTLE_ENDIAN),
    kind: invert(view.events).get(dv.getUint16(field.kind, LITTLE_ENDIAN)),
    handler: dv.getUint32(field.handler, LITTLE_ENDIAN),
    target: dv.getUint32(field.target, LITTLE_ENDIAN),
    seq: dv.getUint32(field.seq, LITTLE_ENDIAN),
    flags: dv.getUint32(field.flags, LITTLE_ENDIAN),
    status: dv.getUint32(field.status, LITTLE_ENDIAN),
    scrollTop: dv.getFloat64(field.scroll_top, LITTLE_ENDIAN),
    key: string(),
    value: string(),
  };
}
