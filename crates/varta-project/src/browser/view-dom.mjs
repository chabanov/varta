import { registry } from "../../../varta-common/browser/registry.mjs";
import { createFontNamespace } from './view-fonts.mjs';
import { decodeViewBatch, ViewProtocolError } from "./view-codec.mjs";
import { createViewModel } from "./view-model.mjs";
import { rewriteRootSelector } from "./view-css.mjs";

const V = registry.view, W = V.wire, F = V.filters;
const field = Object.fromEntries(W.eventFields.map((entry) => [entry.name, entry]));
const eventKinds = new Map(V.events.map((entry) => [entry.name, entry]));
const keyNames = new Map(V.keys.map((entry) => [entry.code, entry.domKey]));
const rootAttribute = /^\[([a-z0-9-]+)\]$/.exec(V.defaults.rootSelector)[1];
const composingKeyCode = 229;
const mountedContainers=new WeakMap();
const fieldTag = (node) => ["input", "textarea", "select"].includes(node?.localName);

// Keep this guard after freezing: an inert view must never turn raw anchors
// or form submission into ambient navigation. Hrefs below are inert as well,
// preventing speculative cross-origin DNS before a broker decision.
function preventViewNavigation(event) {
  if(event.type==="submit" || event.composedPath()[0]?.closest?.("a"))event.preventDefault();
}

/** Encode only host-owned event data. The receiver still validates its queue. */
export function encodeViewEvent(kind, values = {}) {
  const code = eventKinds.get(kind)?.code; if (code === undefined) throw new ViewProtocolError("unknown event");
  const strings = W.eventStrings.map((name) => String(values[name] ?? "").toWellFormed().replaceAll("\0", "\ufffd"));
  if (strings.some((value) => value.length * 2 > V.limits.valueBytes)) throw new ViewProtocolError("event string size");
  const length = W.eventHeaderBytes + strings.reduce((size, value) => size + 4 + Math.ceil(value.length * 2 / W.alignment) * W.alignment, 0);
  const bytes = new Uint8Array(length), view = new DataView(bytes.buffer);
  const header = { ...values, version: W.version, kind: code };
  for (const [name, entry] of Object.entries(field)) {
    const value = header[name] ?? 0;
    if (entry.type === "f64") view.setFloat64(entry.offset, value, true);
    else if (entry.type === "u16") view.setUint16(entry.offset, value, true);
    else view.setUint32(entry.offset, value, true);
  }
  let at = W.eventHeaderBytes;
  for (const text of strings) { const size = text.length * 2; view.setUint32(at, size, true); at += 4; for (let i = 0; i < text.length; i++) view.setUint16(at + i * 2, text.charCodeAt(i), true); at += Math.ceil(size / W.alignment) * W.alignment; }
  return bytes;
}

/** A DOM boundary with no HTML parsing, ambient navigation or guest JS. */
export function createViewDom({ container, onEvent = () => {}, onError = error => {queueMicrotask(()=>{throw error;});}, onTitle = () => {}, assets = new Map(), fonts = createFontNamespace(crypto.randomUUID().replaceAll('-', '')), linkRequested = () => false, copyText = async () => V.statuses.not_allowed, openUrl = () => V.statuses.not_allowed, interactive = true } = {}) {
  if (!container?.isConnected) throw new TypeError("view container must be attached");
  if(mountedContainers.has(container))throw new TypeError("view container already hosts an active session");
  const model = createViewModel({ linkRequested }), document = container.ownerDocument, Sheet = document.defaultView.CSSStyleSheet;
  const root = document.createElement("div"); root.setAttribute(rootAttribute, "");
  const shell=document.createElement("div");shell.style.height="100%";container.replaceChildren(shell);
  const shadow = shell.attachShadow({ mode: "closed" }); shadow.append(root);
  mountedContainers.set(container,true);
  shadow.addEventListener("click",preventViewNavigation,{capture:true});shadow.addEventListener("submit",preventViewNavigation,{capture:true});
  const base = new Sheet();
  const D = V.defaults;
  base.replaceSync(`:host { all: initial; display: block; } ${D.rootSelector} { ${D.rootStyle}; } textarea { resize: none; } dialog { border: none; padding: 0; background-color: ${D.dialogBackground[0]}; color: ${D.dialogText[0]}; } dialog::backdrop { background-color: ${D.dialogBackdrop[0]}; } @media (prefers-color-scheme: dark) { dialog { background-color: ${D.dialogBackground[1]}; color: ${D.dialogText[1]}; } dialog::backdrop { background-color: ${D.dialogBackdrop[1]}; } }`);
  const sheets = new Map(), nodes = new Map([[W.root, root]]), ids = new WeakMap([[root, W.root]]), ownedAssets = new Map(assets);
  let applying = false, closed = false;
  const adopt = () => { shadow.adoptedStyleSheets = [base, ...[...sheets].sort(([a], [b]) => a - b).map(([, sheet]) => sheet)]; }; adopt();
  const snapshots = () => new Map(model.snapshot().nodes.map((node) => [node.id, node]));
  const publish = (kind, values, trusted) => onEvent({ bytes: encodeViewEvent(kind, values), gesture: Boolean(trusted && eventKinds.get(kind).gesture), coalescible: eventKinds.get(kind).coalescible, handler: values.handler });

  function dispatch(kind, event) {
    if (closed || applying || !interactive) return;
    let target = event.composedPath()[0];
    while (target && !ids.has(target)) target = target.parentNode;
    const id = ids.get(target); if (id === undefined) return;
    const all = snapshots();
    const anchor = kind === "click" ? target.closest?.("a") : null;
    if (kind === "submit" || anchor) event.preventDefault();
    let seq = all.get(id)?.seq ?? 0;
    if ((kind === "input" || kind === "change") && fieldTag(target)) seq = model.input(id, target.value, target.checked, kind === "input");
    const flags = (event.shiftKey ? F.flags.shiftKey : 0) | (event.ctrlKey ? F.flags.ctrlKey : 0) | (event.altKey ? F.flags.altKey : 0) | (event.metaKey ? F.flags.metaKey : 0) | (event.isComposing ? F.flags.isComposing : 0) | (target.checked ? F.flags.checked : 0);
    const held = flags & F.modifierMask;
    let stopped = false;
    for (let at = id; at !== W.noNode && !stopped; at = kind === "scroll" || kind === "close" || kind === "cancel" ? W.noNode : all.get(at)?.parent ?? W.noNode) {
      for (const listener of all.get(at)?.listeners ?? []) {
        if (listener.event !== kind || listener.key !== F.anyKey && keyNames.get(listener.key) !== event.key || (held & listener.required_modifiers) !== listener.required_modifiers || held & listener.forbidden_modifiers || listener.options & F.options.notComposing && (event.isComposing || event.keyCode === composingKeyCode)) continue;
        if (listener.options & F.options.preventDefault) event.preventDefault();
        publish(kind, { handler: listener.handler, target: id, flags, seq, key: kind === "keydown" ? event.key : "", value: (kind === "input" || kind === "change") && fieldTag(target) ? target.value : "", scroll_top: kind === "scroll" ? target.scrollTop : 0, scroll_height: kind === "scroll" ? target.scrollHeight : 0, client_height: kind === "scroll" ? target.clientHeight : 0 }, event.isTrusted);
        if (listener.options & F.options.stopPropagation) { stopped = true; break; }
      }
    }
    const href=anchor?all.get(ids.get(anchor))?.attributes?.href:undefined;
    if(anchor && event.isTrusted && /^https?:\/\//.test(href ?? "")) {
      const linkId=ids.get(anchor),complete=status=>{
        if(closed)return;if(!Object.values(V.statuses).includes(status))throw new ViewProtocolError('invalid link result');
        publish("command_result",{target:linkId,status,key:"open_url"},false);
      };
      try{
        const result=openUrl(href);
        if(result&&typeof result.then==='function')Promise.resolve(result).then(complete).catch(onError);else complete(result);
      }catch(error){onError(error);}
    }
  }
  const handlers = [];
  for (const kind of V.events.map((entry) => entry.name).filter((name) => name !== "command_result")) {
    const capture = ["scroll", "close", "cancel"].includes(kind), listener = (event) => dispatch(kind, event);
    shadow.addEventListener(kind, listener, { capture }); handlers.push([kind, listener, capture]);
  }
  function apply(bytes, { gesture = false } = {}) {
    if (closed) throw new ViewProtocolError("closed view");
    // Asset authority is a trusted, already verified blob map. A guest resource
    // name must never become an ambient URL fetch, even on an unattached node.
    const decoded = decodeViewBatch(bytes), resolved = new Map();
    for (let i = 0; i < decoded.length; i++) if (decoded[i].name === "set_attribute" && decoded[i].attribute === "src") {
      const url = ownedAssets.get(decoded[i].text); if (typeof url !== "string" || !url.startsWith("blob:")) throw new ViewProtocolError("unpackaged view asset", i); resolved.set(i, url);
    }
    // Parsing sheets before commit catches platform allocation/parse failures
    // without advancing either validation state or visible DOM.
    const preparedSheets = new Map(), preparedFonts = new Map();
    for (let i = 0; i < decoded.length; i++) {
      if (decoded[i].name === "stylesheet" && decoded[i].text) { const sheet = new Sheet(); sheet.replaceSync(rewriteRootSelector(decoded[i].text, D.rootSelector)); fonts.sheet(sheet.cssRules); preparedSheets.set(i, sheet); }
      if (decoded[i].name === "set_style" && decoded[i].property === "font-family") preparedFonts.set(i, fonts.value(decoded[i].text));
    }
    const { operations, effects } = model.commit(bytes), current = snapshots();
    applying = true;
    try {
      for (let i = 0; i < operations.length; i++) {
        const op = operations[i], node = nodes.get(op.node);
        switch (op.name) {
          case "create_element": { const element = document.createElement(op.tag); nodes.set(op.node, element); ids.set(element, op.node); break; }
          case "create_text": { const text = document.createTextNode(op.text); nodes.set(op.node, text); ids.set(text, op.node); break; }
          case "set_text": node.data = op.text; break;
          case "set_attribute": node.setAttribute(op.attribute, op.attribute === "src" ? resolved.get(i) : op.attribute === "href" ? "#" : op.text); break;
          case "remove_attribute": node.removeAttribute(op.attribute); break;
          case "set_class": node.className = op.text; break;
          case "set_style": node.style.setProperty(op.property, preparedFonts.get(i) ?? op.text); break;
          case "remove_style": node.style.removeProperty(op.property); break;
          case "insert": nodes.get(op.parent).insertBefore(node, op.before === W.noNode ? null : nodes.get(op.before)); break;
          case "remove": { const stack = [node]; while (stack.length) { const removed = stack.pop(); for (const child of removed.childNodes) stack.push(child); nodes.delete(ids.get(removed)); ids.delete(removed); } node.remove(); break; }
          case "set_value": if (current.get(op.node)?.seq === op.base_seq) node.value = op.text; break;
          case "set_checked": if (current.get(op.node)?.seq === op.base_seq) node.checked = Boolean(op.checked); break;
          case "stylesheet": if (op.text) sheets.set(op.sheet, preparedSheets.get(i)); else sheets.delete(op.sheet); adopt(); break;
          case "set_title": onTitle(op.text); break;
          // Listener and command state is already committed in the pure model.
        }
      }
    } finally { applying = false; }
    for (const op of effects) {
      const node = nodes.get(op.node);
      if (op.name === "copy_text") {
        Promise.resolve(gesture ? copyText(op.text) : V.statuses.invalid_context).then((status)=>{if(!closed)publish("command_result",{seq:op.request,status,key:"copy_text"},false);}); continue;
      }
      if (!node) continue;
      switch (op.command) {
        case "focus": node.focus(); break;
        case "blur": node.blur(); break;
        case "select": node.select(); break;
        case "scroll_into_view": node.scrollIntoView(); break;
        case "scroll_to": node.scrollTop = op.argument; break;
        case "show_modal": node.showModal(); break;
        case "close": node.close(); break;
      }
    }
    return 0;
  }
  return { apply, snapshot: () => model.snapshot(), complete: (id) => model.complete(id),
    freeze() { if(closed)return;closed=true;root.inert=true;mountedContainers.delete(container);for(const [kind,listener,capture] of handlers)shadow.removeEventListener(kind,listener,{capture}); },
    close() { if (closed) return; closed = true;mountedContainers.delete(container); for (const [kind, listener, capture] of handlers) shadow.removeEventListener(kind, listener, { capture }); root.replaceChildren(); nodes.clear(); sheets.clear(); ownedAssets.clear(); model.close(); shadow.adoptedStyleSheets = []; },
    // Explicit inspection seam for host tests. The guest receives no DOM handle.
    inspectNode(id) { return nodes.get(id); },
  };
}
