import { registry } from "../../../varta-common/browser/registry.mjs";
import { createFontNamespace } from './view-fonts.mjs';
import { decodeViewBatch, ViewProtocolError } from "./view-codec.mjs";
import { createViewModel, imageCandidateNames } from "./view-model.mjs";
import { readMediaInputs, MEDIA_INPUT_QUERIES, ENGINE_INPUTS, ENGINE_STATES } from "./view-engine.mjs";

/** The pseudo-elements the renderers draw, which the bridge styles with one
 * generated rule per element (CSS Pseudo-Elements 4); the engine decides
 * their values. */
const RENDERED_PSEUDO_ELEMENTS = new Set(["before", "after", "placeholder", "backdrop"]);
/** The attribute that names an element in its generated pseudo-element
 * rules: the bridge's own, never a guest attribute (the view protocol has
 * no attribute of this name). */
const PSEUDO_ATTRIBUTE = "varta-pseudo";

const V = registry.view, W = V.wire, F = V.filters;
const field = Object.fromEntries(W.eventFields.map((entry) => [entry.name, entry]));
const eventKinds = new Map(V.events.map((entry) => [entry.name, entry]));
const keyNames = new Map(V.keys.map((entry) => [entry.code, entry.domKey]));
const rootAttribute = /^\[([a-z0-9-]+)\]$/.exec(V.defaults.rootSelector)[1];
const composingKeyCode = 229;
/** The units whose references `fontMetrics` measures, in the engine's
 * order (`ex`, `cap`, `ch`, `ic`, and `lh` under `line-height: normal`). */
const FONT_UNITS = ["ex", "cap", "ch", "ic", "lh"];
const mountedContainers=new WeakMap();
const fieldTag = (node) => ["input", "textarea", "select"].includes(node?.localName);
const checkableField = (node) => node?.localName === "input" && ["checkbox", "radio"].includes(node.type);

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
export function createViewDom({ container, viewEngine, onEvent = () => {}, onError = error => {queueMicrotask(()=>{throw error;});}, onTitle = () => {}, onVisualInput, assets = new Map(), fonts = createFontNamespace(crypto.randomUUID().replaceAll('-', '')), fontFaces = [], linkRequested = () => false, copyText = async () => V.statuses.not_allowed, openUrl = () => V.statuses.not_allowed, interactive = true } = {}) {
  if(onVisualInput!==undefined&&typeof onVisualInput!=='function')throw new TypeError('invalid visual timing callback');
  if (!container?.isConnected) throw new TypeError("view container must be attached");
  if(mountedContainers.has(container))throw new TypeError("view container already hosts an active session");
  const document = container.ownerDocument, window = document.defaultView, Sheet = window.CSSStyleSheet;
  // The style engine decides which font is an element's first available
  // one and reads packaged faces itself; a font the browser renders with
  // its own (a generic family, the default font) is measured here with
  // the browser's own units, on a hidden probe in the view's shadow tree.
  const probe = document.createElement("div"); probe.style.display = "none";
  const fontMetrics = (kind, family, weight, italic, size) => {
    const style = probe.style;
    style.fontFamily = kind === "default" ? "initial" : kind === "generic" ? family : fonts.value(JSON.stringify(family));
    style.fontWeight = String(weight); style.fontStyle = italic ? "italic" : "normal"; style.fontSize = `${size}px`; style.lineHeight = "normal";
    return FONT_UNITS.map((unit) => { style.wordSpacing = `1${unit}`; return probe.computedStyleMap().get("word-spacing").value; });
  };
  const model = createViewModel({ linkRequested, viewEngine, fontMetrics });
  const root = document.createElement("div"); root.setAttribute(rootAttribute, "");
  const shell=document.createElement("div");shell.style.height="100%";container.replaceChildren(shell);
  const shadow = shell.attachShadow({ mode: "closed" }); shadow.append(root, probe);
  mountedContainers.set(container,true);
  shadow.addEventListener("click",preventViewNavigation,{capture:true});shadow.addEventListener("submit",preventViewNavigation,{capture:true});
  const base = new Sheet();
  const D = V.defaults;
  base.replaceSync(`:host { all: initial; display: block; } ${D.rootSelector} { ${D.rootStyle}; } ${fonts.rules(D)} button, input, textarea, select { box-sizing: border-box; margin: 0; } textarea { resize: none; } dialog { border: none; padding: 0; background-color: ${D.dialogBackground[0]}; color: ${D.dialogText[0]}; } dialog::backdrop { background-color: ${D.dialogBackdrop[0]}; } @media (prefers-color-scheme: dark) { dialog { background-color: ${D.dialogBackground[1]}; color: ${D.dialogText[1]}; } dialog::backdrop { background-color: ${D.dialogBackdrop[1]}; } }`);
  const nodes = new Map([[W.root, root]]), ids = new WeakMap([[root, W.root]]), ownedAssets = new Map(assets);
  let applying = false, closed = false;
  // Pseudo-elements take no inline style: each styled one gets a rule of
  // its own, keyed by the element's id, whose declarations are set
  // through the CSSOM from the engine's computed values.
  const pseudoSheet = new Sheet(), pseudoRules = new Map();
  const animationSheet = new Sheet(); let animationCss = "";
  shadow.adoptedStyleSheets = [base, pseudoSheet, animationSheet];
  // Every element's style is the view style engine's computed value, set
  // inline; guest stylesheets never reach the platform cascade. The engine's
  // inputs are recorded here: the viewport, the media preferences and the
  // pointer and focus states the platform reports.
  const applied = new WeakMap(), engine = model.engine, fieldStates = ENGINE_STATES.badInput | ENGINE_STATES.userValidity;
  if (engine) {
    for (const face of fontFaces) if (engine.face(face.family, face.bytes, face.generics) !== 0) throw new TypeError("invalid packaged font face");
    engine.defaultFamily(fonts.defaultName);
  }
  const idOf = (element) => { for (let at = element; at; at = at.parentNode) if (ids.has(at)) return ids.get(at); return W.noNode; };
  const deepest = (selector) => { const list = shadow.querySelectorAll(selector); return list.length ? idOf(list[list.length - 1]) : W.noNode; };
  function restyle() {
    if (closed) return;
    const inputs = readMediaInputs(window);
    // The shadow host is `all: initial`, so its size is the platform's
    // `medium`: the base of computed font sizes. A generic `monospace` that
    // names a packaged face keeps no fixed-font size.
    const medium = Number.parseFloat(window.getComputedStyle(container).fontSize);
    const packagedMonospace = fonts.family("monospace", false) !== "monospace" ? ENGINE_INPUTS.packagedMonospace : 0;
    engine.environment(inputs.width, inputs.height, inputs.rootFontSize, Number.isFinite(medium) ? medium : inputs.rootFontSize, inputs.flags | packagedMonospace);
    const focused = shadow.activeElement?.matches?.(":focus-visible") ? idOf(shadow.activeElement) : W.noNode;
    engine.state(focused, deepest(":hover"), deepest(":active"));
    engine.focus(shadow.activeElement ? idOf(shadow.activeElement) : W.noNode);
    // The fields' states the platform keeps: a number field's bad input and
    // the user validity of `:user-valid`/`:user-invalid` (HTML §4.10.18.4.1).
    for (const [id, element] of nodes) if (["input", "textarea", "select"].includes(element.localName))
      engine.setStates(id, (element.validity.badInput ? ENGINE_STATES.badInput : 0) | (element.matches(":user-valid, :user-invalid") ? ENGINE_STATES.userValidity : 0), fieldStates);
    const keyframes = engine.animationCss();
    if (animationCss !== keyframes) { animationSheet.replaceSync(keyframes); animationCss = keyframes; }
    for (const [id, element] of nodes) {
      if (element.nodeType !== 1) continue;
      const values = engine.computed(id), previous = applied.get(element) ?? new Map();
      for (const property of previous.keys()) if (!values.has(property)) element.style.removeProperty(property);
      for (const [property, value] of values) {
        if (previous.get(property) === value) continue;
        element.style.setProperty(property, property === "font-family" && value !== "inherit" ? fonts.value(value) : value);
      }
      applied.set(element, values);
    }
    restylePseudoElements();
  }
  /** Mirror the engine's computed values of every selected, rendered
   * pseudo-element into its rule, removing the rules of those that have
   * none any more. */
  function restylePseudoElements() {
    const kinds = engine.pseudoElements().filter((kind) => RENDERED_PSEUDO_ELEMENTS.has(kind)), live = new Set();
    for (const [id, element] of nodes) {
      if (element.nodeType !== 1) continue;
      for (const kind of kinds) {
        const values = engine.computedPseudo(id, kind);
        if (values.size === 0) continue;
        const key = `${id}::${kind}`;
        live.add(key);
        let entry = pseudoRules.get(key);
        if (entry === undefined || entry.element !== element) {
          if (entry !== undefined) removePseudoRule(entry.rule);
          element.setAttribute(PSEUDO_ATTRIBUTE, String(id));
          const index = pseudoSheet.insertRule(`[${PSEUDO_ATTRIBUTE}="${id}"]::${kind} {}`, pseudoSheet.cssRules.length);
          entry = { element, rule: pseudoSheet.cssRules[index], values: new Map() };
          pseudoRules.set(key, entry);
        }
        for (const property of entry.values.keys()) if (!values.has(property)) entry.rule.style.removeProperty(property);
        for (const [property, value] of values) {
          if (entry.values.get(property) === value) continue;
          entry.rule.style.setProperty(property, property === "font-family" && value !== "inherit" ? fonts.value(value) : value);
        }
        entry.values = values;
      }
    }
    for (const [key, entry] of pseudoRules) if (!live.has(key)) { removePseudoRule(entry.rule); pseudoRules.delete(key); }
  }
  function removePseudoRule(rule) {
    const rules = pseudoSheet.cssRules;
    for (let index = 0; index < rules.length; index++) if (rules[index] === rule) { pseudoSheet.deleteRule(index); return; }
  }
  let frame;
  const schedule = () => { if (!closed && frame === undefined) frame = window.requestAnimationFrame(() => { frame = undefined; try { restyle(); } catch (error) { onError(error); } }); };
  const stateEvents = ["pointerover", "pointerout", "pointerdown", "pointerup", "pointercancel", "focusin", "focusout", "keydown", "keyup", "input", "change", "submit"];
  for (const kind of stateEvents) shadow.addEventListener(kind, schedule, true);
  const queries = MEDIA_INPUT_QUERIES.map((query) => window.matchMedia(query));
  for (const query of queries) query.addEventListener("change", schedule);
  window.addEventListener("resize", schedule);
  const unobserve = () => { for (const kind of stateEvents) shadow.removeEventListener(kind, schedule, true); for (const query of queries) query.removeEventListener("change", schedule); window.removeEventListener("resize", schedule); if (frame !== undefined) window.cancelAnimationFrame(frame); };
  const snapshots = () => new Map(model.snapshot().nodes.map((node) => [node.id, node]));
  const publish = (kind, values, trusted, selectedOption) => onEvent({ ...(selectedOption===undefined?{}:{selectedOption}), bytes: encodeViewEvent(kind, values), gesture: Boolean(trusted && eventKinds.get(kind).gesture), coalescible: eventKinds.get(kind).coalescible, handler: values.handler });

  function dispatch(kind, event) {
    if (closed || applying || !interactive) return;
    let target = event.composedPath()[0];
    while (target && !ids.has(target)) target = target.parentNode;
    const id = ids.get(target); if (id === undefined) return;
    const all = snapshots();
    const anchor = kind === "click" ? target.closest?.("a") : null;
    if (kind === "submit" || anchor) event.preventDefault();
    let seq = all.get(id)?.seq ?? 0;
    const selectedOption=target.localName==='select'&&['input','change'].includes(kind)?ids.get(target.selectedOptions[0])??0:undefined;
    if ((kind === "input" || kind === "change") && fieldTag(target)) seq = model.input(id, target.value, target.checked, kind === "input",selectedOption);
    const flags = (event.shiftKey ? F.flags.shiftKey : 0) | (event.ctrlKey ? F.flags.ctrlKey : 0) | (event.altKey ? F.flags.altKey : 0) | (event.metaKey ? F.flags.metaKey : 0) | (event.isComposing ? F.flags.isComposing : 0) | (target.checked ? F.flags.checked : 0);
    const held = flags & F.modifierMask;
    let stopped = false;
    for (let at = id; at !== W.noNode && !stopped; at = kind === "scroll" || kind === "close" || kind === "cancel" ? W.noNode : all.get(at)?.parent ?? W.noNode) {
      for (const listener of all.get(at)?.listeners ?? []) {
        if (listener.event !== kind || listener.key !== F.anyKey && keyNames.get(listener.key) !== event.key || (held & listener.required_modifiers) !== listener.required_modifiers || held & listener.forbidden_modifiers || listener.options & F.options.notComposing && (event.isComposing || event.keyCode === composingKeyCode)) continue;
        if (listener.options & F.options.preventDefault) event.preventDefault();
        publish(kind, { handler: listener.handler, target: id, flags, seq, key: kind === "keydown" ? event.key : "", value: (kind === "input" || kind === "change") && fieldTag(target) && !checkableField(target) ? target.value : "", scroll_top: kind === "scroll" ? target.scrollTop : 0, scroll_height: kind === "scroll" ? target.scrollHeight : 0, client_height: kind === "scroll" ? target.clientHeight : 0 }, event.isTrusted, selectedOption);
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
  if(onVisualInput!==undefined){
    for(const kind of ['pointerover','pointerout','pointerdown','pointerup','scroll','input','keydown','focusin','focusout']){
      const listener=event=>{if(!closed&&!applying&&interactive&&event.isTrusted)onVisualInput(kind);};
      shadow.addEventListener(kind,listener,{capture:true,passive:true});handlers.push([kind,listener,true]);
    }
  }
  for (const kind of V.events.map((entry) => entry.name).filter((name) => name !== "command_result")) {
    const capture = ["scroll", "close", "cancel"].includes(kind), listener = (event) => dispatch(kind, event);
    shadow.addEventListener(kind, listener, { capture }); handlers.push([kind, listener, capture]);
  }
  function apply(bytes, { gesture = false } = {}) {
    if (closed) throw new ViewProtocolError("closed view");
    // Asset authority is a trusted, already verified blob map. A guest resource
    // name must never become an ambient URL fetch, even on an unattached node.
    const decoded = decodeViewBatch(bytes), resolved = new Map();
    const owned = (name, i) => { const url = ownedAssets.get(name); if (typeof url !== "string" || !url.startsWith("blob:")) throw new ViewProtocolError("unpackaged view asset", i); return url; };
    for (let i = 0; i < decoded.length; i++) if (decoded[i].name === "set_attribute" && decoded[i].attribute === "src") resolved.set(i, owned(decoded[i].text, i));
    else if (decoded[i].name === "set_attribute" && decoded[i].attribute === "srcset") {
      // Each candidate keeps its descriptor; only its resource name becomes
      // the owned Blob URL (a name never becomes an ambient fetch).
      const names = imageCandidateNames(decoded[i].text); if (names === null) throw new ViewProtocolError("attribute value", i);
      resolved.set(i, decoded[i].text.split(",").map((part, index) => part.trim().replace(names[index], owned(names[index], i))).join(", "));
    }
    const { operations, effects, skippedValues } = model.commit(bytes), current = snapshots();
    applying = true;
    try {
      for (let i = 0; i < operations.length; i++) {
        if(skippedValues.has(i))continue;
        const op = operations[i], node = nodes.get(op.node);
        switch (op.name) {
          case "create_element": { const element = document.createElement(op.tag); nodes.set(op.node, element); ids.set(element, op.node); break; }
          case "create_text": { const text = document.createTextNode(op.text); nodes.set(op.node, text); ids.set(text, op.node); break; }
          case "set_text": node.data = op.text; break;
          case "set_attribute": node.setAttribute(op.attribute, op.attribute === "src" || op.attribute === "srcset" ? resolved.get(i) : op.attribute === "href" ? "#" : op.text); break;
          case "remove_attribute": node.removeAttribute(op.attribute); break;
          case "set_data": node.setAttribute(op.data_name, op.text); break;
          case "remove_data": node.removeAttribute(op.data_name); break;
          case "set_class": node.className = op.text; break;
          case "insert": nodes.get(op.parent).insertBefore(node, op.before === W.noNode ? null : nodes.get(op.before)); break;
          case "remove": { const stack = [node]; while (stack.length) { const removed = stack.pop(); for (const child of removed.childNodes) stack.push(child); nodes.delete(ids.get(removed)); ids.delete(removed); } node.remove(); break; }
          case "set_value": if (current.get(op.node)?.seq === op.base_seq && node.value!==op.text) node.value = op.text; break;
          case "set_checked": if (current.get(op.node)?.seq === op.base_seq) node.checked = Boolean(op.checked); break;
          case "set_title": onTitle(op.text); break;
          // Listener and command state is already committed in the pure model.
        }
      }
      // Browsers differ when all options are disabled, when a detached select
      // is reinserted, and when attribute writes meet internal dirty flags.
      // The shared model owns selectedness. Reconcile once after mutation,
      // retaining duplicate identity and avoiding writes on equal/no-op commits.
      for(const [id,node] of current)if(node.tag==='select'){
        const selected=node.children.findIndex(option=>current.get(option)?.optionSelected);
        const element=nodes.get(id);
        if(element.selectedIndex!==selected)element.selectedIndex=selected;
      }
      restyle();
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
    if(effects.some(op=>['focus','blur','show_modal','close'].includes(op.command)))restyle();
    return 0;
  }
  return { apply, snapshot: () => model.snapshot(), complete: (id) => model.complete(id),
    freeze() { if(closed)return;closed=true;root.inert=true;unobserve();mountedContainers.delete(container);for(const [kind,listener,capture] of handlers)shadow.removeEventListener(kind,listener,{capture}); },
    close() { if (closed) return; closed = true;unobserve();mountedContainers.delete(container); for (const [kind, listener, capture] of handlers) shadow.removeEventListener(kind, listener, { capture }); root.replaceChildren(); nodes.clear(); ownedAssets.clear(); model.close(); pseudoRules.clear(); shadow.adoptedStyleSheets = []; },
    // Explicit inspection seam for host tests. The guest receives no DOM handle.
    inspectNode(id) { return nodes.get(id); },
  };
}
