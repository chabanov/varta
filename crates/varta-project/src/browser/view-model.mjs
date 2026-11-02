import { registry } from "../../../varta-common/browser/registry.mjs";
import { decodeViewBatch, ViewProtocolError } from "./view-codec.mjs";
import { checkStylesheet, checkStyleDeclaration, createValueMatcher } from "./view-css.mjs";

import {selectOwner,optionAttribute,normalizeSelect,syncSelect,syncSelectFor,setSelectValue,syncSelectChoice,optionValue,selectionTextBytes,selectOperationWork,selectInventoryWork} from './view-select.mjs';

const V = registry.view, W = V.wire;
const named = (rows) => new Map(rows.map((row) => [row.name, row]));
const tags = named(V.tags), attributes = named(V.attributes), commands = named(V.commands);
const element = (tag) => ({ kind: "element", tag, parent: W.noNode, children: [], attributes: new Map(), style: new Map(), class: "", listeners: [], value: "", checked: false, seq: 0, open: false });
const textNode = (text) => ({ kind: "text", text, textBytes:selectionTextBytes(text), parent: W.noNode, children: [], listeners: [] });
const ident = (text) => text.length <= V.limits.identChars && /^[A-Za-z][A-Za-z0-9_-]*$/.test(text);
const inputType = (node) => node.attributes.get("type") ?? "text";
const checkable = (node) => node.tag === "input" && ["checkbox", "radio"].includes(inputType(node));
function radioGroup(nodes, id) {
  const node = nodes.get(id), name = node?.attributes?.get("name");
  if (!node || !checkable(node) || inputType(node) !== "radio") return [];
  if (!name) return [id];
  let owner = id;
  for (let at = id; at !== W.noNode; at = nodes.get(at).parent) {
    owner = at;
    if (nodes.get(at).tag === "form") break;
  }
  const group = [], stack = [owner];
  while (stack.length) {
    const current = stack.pop(), candidate = nodes.get(current);
    if (current !== owner && candidate.tag === "form") continue;
    if (candidate.kind === "element" && checkable(candidate) && inputType(candidate) === "radio" && candidate.attributes.get("name") === name) group.push(current);
    stack.push(...candidate.children.toReversed());
  }
  return group;
}
function uncheckRadio(nodes, id) {
  for (const other of radioGroup(nodes, id)) if (other !== id) nodes.get(other).checked = false;
}
const focusable = (node) => ["button", "input", "select", "textarea", "a"].includes(node.tag) || node.attributes.has("tabindex");
const admitted = (rule, node) => rule === "all" || rule === "focusable" && focusable(node) || Array.isArray(rule) && rule.includes(node.tag);

function route(text) {
  const part = (value, extra) => {
    for (let i = 0; i < value.length; i++) {
      if (value[i] === "%") { if (!/^[0-9a-f]{2}$/i.test(value.slice(i + 1, i + 3))) return false; i += 2; }
      else if (!/[A-Za-z0-9\-._~!$&'()*+,;=:@/]/.test(value[i]) && !extra.includes(value[i])) return false;
    }
    return true;
  };
  if (text.startsWith("#")) return part(text.slice(1), "?");
  if (!text.startsWith("/") || text.startsWith("//")) return false;
  const hash = text.indexOf("#"), before = hash < 0 ? text : text.slice(0, hash), fragment = hash < 0 ? "" : text.slice(hash + 1);
  const query = before.indexOf("?");
  return part(query < 0 ? before : before.slice(0, query), "") && part(query < 0 ? "" : before.slice(query + 1), "?") && part(fragment, "?");
}

function attributeValue(rule, tag, text, linkRequested) {
  switch (rule.kind) {
    case "ident": return ident(text);
    case "identList": return Boolean(text) && text.split(" ").every(ident);
    case "text": return true;
    case "boolean": return text === "";
    case "count": return /^(0|[1-9][0-9]*)$/.test(text);
    case "positiveCount": return /^[1-9][0-9]*$/.test(text);
    case "number": return /^[0-9]+(?:\.[0-9]+)?$/.test(text);
    case "keywords": return rule.values.includes(text);
    case "elementType": return (tag === "input" ? V.inputTypes : tag === "button" ? V.buttonTypes : []).includes(text);
    case "href": return /^https?:\/\//.test(text) ? linkRequested(text) : route(text);
    case "resource": return Boolean(text) && text.split("/").every((segment) => segment !== "." && segment !== ".." && /^[A-Za-z0-9._-]+$/.test(segment));
    default: return false;
  }
}

/** Pure transaction validator shared by the Worker and DOM adapter. */
export function createViewModel({ linkRequested = () => false, assetRequested = () => true } = {}) {
  const checkValue = createValueMatcher(Object.fromEntries(V.css.map((row) => [row.name, row.grammar])), Object.fromEntries(V.css.map((row) => [row.name, row.note ?? ""])));
  let nodes = new Map([[W.root, element("div")]]), sheets = new Map(), pending = new Set(), title = "", focus = W.noNode;
  let closed = false;
  const validateChoice = (record, choice) => {
    const MAX_NODE_ID=0xffff_ffff;
    if(choice!==undefined&&(!Number.isSafeInteger(choice)||choice<0||choice>MAX_NODE_ID||!['input','change'].includes(record.kind)))throw new ViewProtocolError('invalid select option envelope');
  };
  const commit = (bytes) => {
    if (closed) throw new ViewProtocolError("closed view");
    const operations = decodeViewBatch(bytes);
    // Copy every mutable container. A failed last operation cannot affect a
    // previously committed tree, listener, field, stylesheet or command request.
    const next = new Map([...nodes].map(([id, node]) => [id, { ...node, children: [...node.children], listeners: [...node.listeners], ...(node.kind === "element" ? { attributes: new Map(node.attributes), style: new Map(node.style) } : {}) }]));
    const nextSheets = new Map(sheets), copies = new Set(pending), effects = [];
    const handlers = new Set([...nodes.values()].flatMap((node) => node.listeners.map((listener) => listener.handler)));
    let index, nextTitle = title, nextFocus = focus;
    const fail = (rule) => { throw new ViewProtocolError(rule, index); };
    let selectionWork=0;
    let lastSelectValue;
    const skippedValues=new Set();
    const chargeSelection=(work)=>{selectionWork+=work;if(selectionWork>V.limits.selectWork)fail('select work budget');};
    const live = (id) => { const node = next.get(id); if (!node) fail("unknown node"); return node; };
    const asElement = (id) => { const node = live(id); if (node.kind !== "element") fail("not an element"); return node; };
    const detach = (id) => { const node = live(id); if (node.parent !== W.noNode) { const siblings = live(node.parent).children; siblings.splice(siblings.indexOf(id), 1); node.parent = W.noNode; } };
    const attached = (id) => { for (let at = id; at !== W.noNode; at = live(at).parent) if (at === W.root) return true; return false; };
    for (index = 0; index < operations.length; index++) {
      const op = operations[index];
      const selectValue=op.name==='set_value'&&next.get(op.node)?.tag==='select';
      // Admission cannot depend on an edit still awaiting Worker delivery.
      // Fold idempotent consecutive writes in both the model and DOM replay.
      if(selectValue&&lastSelectValue?.node===op.node&&lastSelectValue.base_seq===op.base_seq&&lastSelectValue.text===op.text){skippedValues.add(index);continue;}
      lastSelectValue=undefined;
      chargeSelection(selectOperationWork(next,op));
      if(selectValue)lastSelectValue=op;
      switch (op.name) {
        case "create_element": case "create_text":
          if (!op.node || next.has(op.node)) fail("duplicate node");
          if (next.size >= V.limits.nodes) fail("node budget");
          next.set(op.node, op.name === "create_element" ? element(op.tag) : textNode(op.text)); break;
        case "set_text": { const node = live(op.node); if (node.kind !== "text") fail("not a text"); node.text = op.text; node.textBytes=selectionTextBytes(op.text); syncSelectFor(next,op.node); break; }
        case "set_attribute": {
          const node = asElement(op.node), rule = attributes.get(op.attribute);
          if (!admitted(rule.tags, node)) fail("attribute not allowed");
          if (!attributeValue(rule.grammar, node.tag, op.text, linkRequested)) fail("attribute value");
          if (op.attribute === "src" && !assetRequested(op.text)) fail("unpackaged view asset");
          const changed=node.attributes.get(op.attribute)!==op.text;
          node.attributes.set(op.attribute, op.text);
          if(changed)optionAttribute(next,op.node,op.attribute,true);
          if (["name", "type"].includes(op.attribute) && node.checked && checkable(node) && inputType(node) === "radio") uncheckRadio(next, op.node);
          break;
        }
        case "remove_attribute": { const node = asElement(op.node); if (!admitted(attributes.get(op.attribute).tags, node)) fail("attribute not allowed"); if(node.attributes.delete(op.attribute))optionAttribute(next,op.node,op.attribute,false); break; }
        case "set_class": { const node = asElement(op.node); if (op.text.split(/[\t\n\f\r ]+/).filter(Boolean).some((value) => !/^[A-Za-z_][A-Za-z0-9_-]*$/.test(value))) fail("class value"); node.class = op.text; break; }
        case "set_style": {
          const node = asElement(op.node); if (["field-sizing", "resize"].includes(op.property) && node.tag !== "textarea") fail("property not allowed");
          try { checkStyleDeclaration(V, op.property, op.text, checkValue); } catch { fail("style value"); } node.style.set(op.property, op.text); break;
        }
        case "remove_style": asElement(op.node).style.delete(op.property); break;
        case "insert": {
          if (op.node === W.root) fail("root not movable");
          const parent = asElement(op.parent), node = live(op.node), rule = tags.get(parent.tag);
          if (node.kind === "text" ? !rule.textChild : !rule.children.includes(node.tag)) fail("child not allowed");
          for (let at = op.parent; at !== W.noNode; at = live(at).parent) if (at === op.node) fail("cycle");
          if (op.before !== W.noNode && (op.before === op.node || live(op.before).parent !== op.parent)) fail("before is not a sibling");
          const oldSelect=selectOwner(next,op.node);
          detach(op.node); parent.children.splice(op.before === W.noNode ? parent.children.length : parent.children.indexOf(op.before), 0, op.node); node.parent = op.parent;
          if(oldSelect!==undefined){if(node.tag==='option')normalizeSelect(next,oldSelect);else syncSelect(next,oldSelect);}
          const owner=selectOwner(next,op.node);
          if(owner!==undefined){if(node.tag==='option')normalizeSelect(next,owner,node.optionSelected?op.node:undefined);else syncSelect(next,owner);}
          const stack = [op.node];
          while (stack.length) {
            const id = stack.pop(), child = live(id);
            if (child.checked && checkable(child) && inputType(child) === "radio") uncheckRadio(next, id);
            stack.push(...child.children.toReversed());
          }
          break;
        }
        case "remove": {
          if (op.node === W.root) fail("root not movable");
          const owner=selectOwner(next,op.node),option=live(op.node).tag==='option';
          detach(op.node); const stack = [op.node];
          while (stack.length) { const id = stack.pop(), node = live(id); for (const listener of node.listeners) handlers.delete(listener.handler); for (const child of node.children) stack.push(child); next.delete(id); if (nextFocus === id) nextFocus = W.noNode; }
          if(owner!==undefined){if(option)normalizeSelect(next,owner);else syncSelect(next,owner);}break;
        }
        case "listen": {
          const node = asElement(op.node), F = V.filters, mask = Object.values(F.options).reduce((a, b) => a | b, 0);
          if (op.event === "command_result") fail("event not listenable");
          if (op.options & ~mask || (op.required_modifiers | op.forbidden_modifiers) & ~F.modifierMask) fail("filter bits");
          if (op.event !== "keydown" && (op.key !== F.anyKey || op.required_modifiers || op.forbidden_modifiers || op.options & F.options.notComposing)) fail("key filter on other event");
          if (!op.handler || handlers.has(op.handler)) fail("duplicate handler"); if (handlers.size >= V.limits.listeners) fail("listener budget");
          const same = (listener) => listener.event === op.event && listener.key === op.key && listener.required_modifiers === op.required_modifiers && listener.forbidden_modifiers === op.forbidden_modifiers && Boolean(listener.options & F.options.notComposing) === Boolean(op.options & F.options.notComposing);
          if (node.listeners.some(same)) fail("duplicate filter"); node.listeners.push({ ...op }); handlers.add(op.handler); break;
        }
        case "unlisten": { const node = asElement(op.node), at = node.listeners.findIndex((listener) => listener.event === op.event && listener.handler === op.handler); if (at < 0) fail("not listening"); node.listeners.splice(at, 1); handlers.delete(op.handler); break; }
        case "set_value": { const node = asElement(op.node); if (!(node.tag === "input" && !checkable(node) || ["textarea", "select"].includes(node.tag))) fail("not a value field"); if (node.seq === op.base_seq) {if(node.tag==='select'){if(node.value!==op.text)setSelectValue(next,op.node,op.text);}else node.value = op.text;} break; }
        case "set_checked": { if (op.checked > 1) fail("checked value"); const node = asElement(op.node); if (!checkable(node)) fail("not a checked field"); if (node.seq === op.base_seq) { node.checked = Boolean(op.checked); if (node.checked && inputType(node) === "radio") uncheckRadio(next, op.node); } break; }
        case "command": {
          const node = asElement(op.node), rule = commands.get(op.command); if (!admitted(rule.tags, node)) fail("command not allowed");
          if (rule.argument ? !(Number.isFinite(op.argument) && op.argument >= 0) : !Object.is(op.argument, 0)) fail("command argument");
          if (op.command === "show_modal") { if (!attached(op.node)) fail("detached modal"); node.open = true; }
          if (op.command === "close") node.open = false;
          if (op.command === "focus" && !node.attributes.has("disabled") && attached(op.node)) nextFocus = op.node;
          if (op.command === "blur" && nextFocus === op.node) nextFocus = W.noNode;
          effects.push({ ...op }); break;
        }
        case "stylesheet": {
          if (!op.sheet) fail("stylesheet id");
          if (op.text) { let checked; try { checked = checkStylesheet(op.text, V); } catch { fail("stylesheet value"); } if (checked.errors.length) fail("stylesheet value"); nextSheets.set(op.sheet, op.text); } else nextSheets.delete(op.sheet);
          let size = 0; for (const text of nextSheets.values()) size += text.length * 2; if (size > V.limits.stylesheetBytes) fail("stylesheet budget"); break;
        }
        case "set_title": nextTitle = op.text; break;
        case "copy_text": if (!op.request || copies.has(op.request)) fail("duplicate copy request"); if (copies.size >= V.limits.commands) fail("copy budget"); copies.add(op.request); effects.push({ ...op }); break;
        default: fail("unimplemented operation");
      }
    }
    index = undefined; const ids = new Set();
    chargeSelection(selectInventoryWork(next));
    for (const node of next.values()) if (node.kind === "element" && node.attributes.has("id")) { const id = node.attributes.get("id"); if (ids.has(id)) fail("duplicate id attribute"); ids.add(id); }
    for(const [id,node] of next)if((node.tag==='select'&&node.value.length*2>V.limits.valueBytes)||(node.tag==='option'&&optionValue(next,id).length*2>V.limits.valueBytes))fail('select value budget');
    nodes = next; sheets = nextSheets; pending = copies; title = nextTitle; focus = nextFocus;
    return { operations, effects, skippedValues };
  };
  return { commit,
    // Clone inspection so an adapter cannot mutate the validation authority.
    snapshot() { return { nodes: [...nodes].map(([id, node]) => ({ id, ...node, children: [...node.children], listeners: node.listeners.map((listener) => ({ ...listener })), ...(node.kind === "element" ? { attributes: Object.fromEntries(node.attributes), style: Object.fromEntries(node.style) } : {}) })), sheets: Object.fromEntries(sheets), title, focus, pending: [...pending] }; },
    close() { closed = true; nodes.clear(); sheets.clear(); pending.clear(); title = ""; focus = W.noNode; },
    complete(request) { if (!pending.delete(request)) throw new ViewProtocolError("unknown command result"); },
    validateChoice,
    syncEvent(record, selectedOption) {
      validateChoice(record, selectedOption);
      if (record.kind === "command_result" && record.key === "copy_text") { if (!pending.delete(record.seq)) throw new ViewProtocolError("unknown command result"); }
      const node = nodes.get(record.target);
      if(node){if(record.kind==="focusin")focus=record.target;if(record.kind==="focusout"&&focus===record.target)focus=W.noNode;if(record.kind==="close"&&node.tag==="dialog")node.open=false;}
      if (!node || node.kind !== "element" || !["input", "textarea", "select"].includes(node.tag) || !["input", "change"].includes(record.kind)) return;
      node.seq = record.seq;
      if (checkable(node)) node.checked = Boolean(record.flags & V.filters.flags.checked);
      else if(node.tag==='select'){
        syncSelectChoice(nodes,record.target,record.value,selectedOption);
      }else node.value = record.value;
      if (node.checked && inputType(node) === "radio") uncheckRadio(nodes, record.target);
    },
    input(id, value, checked, advance = true, selectedOption) {
      const node = nodes.get(id);
      if (!node || node.kind !== "element" || !["input", "textarea", "select"].includes(node.tag)) throw new ViewProtocolError("input target");
      if (typeof value !== "string" || value.length * 2 > V.limits.valueBytes || value.includes("\0") || !value.isWellFormed()) throw new ViewProtocolError("input value");
      validateChoice({kind:'input'},selectedOption);
      if(selectedOption!==undefined&&node.tag!=='select')throw new ViewProtocolError('input select target');
      if (advance) node.seq = (node.seq + 1) >>> 0;
      if (checkable(node)) node.checked = Boolean(checked);
      else if(node.tag==='select'){
        syncSelectChoice(nodes,id,value,selectedOption);
      }else node.value = value;
      if (node.checked && inputType(node) === "radio") uncheckRadio(nodes, id);
      return node.seq;
    },
  };
}
