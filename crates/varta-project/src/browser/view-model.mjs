import { registry } from "../../../varta-common/browser/registry.mjs";
import { decodeViewBatch, ViewProtocolError } from "./view-codec.mjs";
import { potentialExpandedValues } from './view-expansion.mjs';
import { potentialNamingWork } from './view-naming.mjs';
import { checkStylesheet, checkStyleDeclaration, createValueMatcher, StylesheetWorkError } from "./view-css.mjs";

import {selectOwner,optionAttribute,normalizeSelect,syncSelect,syncSelectFor,setSelectValue,syncSelectChoice,optionValue,selectionTextBytes,selectOperationWork,selectInventoryWork} from './view-select.mjs';

const V = registry.view, W = V.wire;
const named = (rows) => new Map(rows.map((row) => [row.name, row]));
const tags = named(V.tags), attributes = named(V.attributes), commands = named(V.commands);
const element = (tag) => ({ kind: "element", tag, parent: W.noNode, children: [], attributes: new Map(), style: new Map(), attributeBytes: new Map(), styleBytes: new Map(), class: "", classBytes: 0, valueReserved: false, listeners: [], value: "", checked: false, seq: 0, open: false });
const textNode = (text) => ({ kind: "text", text, textBytes:selectionTextBytes(text), parent: W.noNode, children: [], listeners: [] });
const ident = (text) => text.length <= V.limits.identChars && /^[A-Za-z][A-Za-z0-9_-]*$/.test(text);
const inputType = (node) => node.attributes.get("type") ?? "text";
const checkable = (node) => node.tag === "input" && ["checkbox", "radio"].includes(inputType(node));
const inputValue=value=>typeof value==='string'&&value.length*2<=V.limits.valueBytes&&!value.includes('\0')&&value.isWellFormed();
function radioGroup(nodes, id, charge = () => {}) {
  const node = nodes.get(id), name = node?.attributes?.get("name");
  if (!node || !checkable(node) || inputType(node) !== "radio") return [];
  if (!name) { charge(V.limits.radioNodeWork); return [id]; }
  let owner = id;
  for (let at = id; at !== W.noNode; at = nodes.get(at).parent) {
    charge(V.limits.radioNodeWork);
    owner = at;
    if (nodes.get(at).tag === "form") break;
  }
  const group = [], stack = [owner];
  while (stack.length) {
    charge(V.limits.radioNodeWork);
    const current = stack.pop(), candidate = nodes.get(current);
    if (current !== owner && candidate.tag === "form") continue;
    if (candidate.kind === "element" && checkable(candidate) && inputType(candidate) === "radio") {
      charge(candidate.attributes.has("name") ? candidate.radioNameBytes : 0);
      if (candidate.attributes.get("name") === name) group.push(current);
    }
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

const resource = (text) => Boolean(text) && text.split("/").every((segment) => segment !== "." && segment !== ".." && /^[A-Za-z0-9._-]+$/.test(segment));
// The resource names of a valid HTML `srcset` (the Rust registry's
// `ImageCandidates` grammar), or null: each candidate a resource with an
// optional positive density or width descriptor, never mixed or repeated.
export function imageCandidateNames(text) {
  const names = [], seen = new Set();
  let kind = null;
  for (const part of text.split(",")) {
    const words = part.split(/[\t\n\f\r ]+/).filter(Boolean);
    if (words.length < 1 || words.length > 2 || !resource(words[0])) return null;
    const descriptor = words[1] ?? "1x";
    const width = /^[1-9][0-9]*w$/.test(descriptor), density = /^[0-9]+(?:\.[0-9]+)?x$/.test(descriptor) && Number(descriptor.slice(0, -1)) > 0;
    if (!width && !density) return null;
    const value = width ? descriptor : `${Number(descriptor.slice(0, -1))}x`;
    if ((kind !== null && kind !== width) || seen.has(value)) return null;
    kind = width; seen.add(value); names.push(words[0]);
  }
  return names;
}
const sizeLength = (text) => /^[0-9]+(?:\.[0-9]+)?(?:px|rem|vw)$/.test(text);
// A valid HTML `sizes` list (the Rust registry's `SourceSizes` grammar).
function sourceSizes(text) {
  const parts = text.split(",").map((part) => part.trim());
  return parts.every((part, index) => {
    const match = /^\((min-width|max-width)\s*:\s*([^)]*)\)\s*(.*)$/.exec(part);
    if (!match) return index === parts.length - 1 && sizeLength(part);
    return index < parts.length - 1 && sizeLength(match[2].trim()) && sizeLength(match[3].trim());
  });
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
    case "resource": return resource(text);
    case "imageCandidates": return imageCandidateNames(text) !== null;
    case "sourceSizes": return sourceSizes(text);
    default: return false;
  }
}

// Private immutable rule records and copied anchor sets make rejected sheet
// updates unable to poison later admission. No media or local input state is read.
function cascadeIndex(source) {
  return { roots:new Set(source?.roots), classes:new Map(source?[...source.classes].map(([key,rules])=>[key,new Set(rules)]):[]), custom:source?.custom??0 };
}
function updateCascade(index,rules,adding) {
  for(const rule of rules){
    if(rule.custom)index.custom+=adding?1:-1;
    const anchors=new Set(rule.selectors.map(s=>s===':root'?null:s.match(/^\.[A-Za-z_][A-Za-z0-9_-]*/)[0].slice(1)));
    for(const anchor of anchors){
      let set=anchor===null?index.roots:index.classes.get(anchor);
      if(adding){if(!set){set=new Set();index.classes.set(anchor,set);}set.add(rule);}
      else if(set){set.delete(rule);if(anchor!==null&&!set.size)index.classes.delete(anchor);}
    }
  }
}
function potentialCascadeRead(nodes,index,charge) {
  const weight=V.limits.stylesheetNodeWork,pending=[];
  for(const [id,node] of nodes)if(node.parent===W.noNode)pending.push([id,0]);
  while(pending.length){
    const [id,ancestors]=pending.pop(),node=nodes.get(id);charge(weight);
    let own=weight,variables=false;
    if(node.kind==='element'){
      charge(node.classBytes);own+=node.classBytes;
      for(const [property,value] of node.style){const work=weight+node.styleBytes.get(property);charge(work);own+=work;variables ||= value.toLowerCase().includes('var(');}
      let candidates=index.roots;
      if(id!==W.root){
        candidates=new Set();
        for(const className of new Set(node.class.split(/[\t\n\f\r ]+/).filter(Boolean)))
          for(const rule of index.classes.get(className)??[])candidates.add(rule);
      }
      for(const rule of candidates){charge(rule.work);own+=rule.work;variables ||= rule.variables;}
    }
    const scope=ancestors+own;if(variables&&index.custom)charge(scope);
    for(const child of node.children)pending.push([child,scope]);
  }
}

/** Pure transaction validator shared by the Worker and DOM adapter. */
export function createViewModel({ linkRequested = () => false, assetRequested = () => true } = {}) {
  const checkValue = createValueMatcher(Object.fromEntries(V.css.map((row) => [row.name, row.grammar])), Object.fromEntries(V.css.map((row) => [row.name, row.note ?? ""])));
  let nodes = new Map([[W.root, element("div")]]), sheets = new Map(), sheetBytes = new Map(), sheetWork = new Map(), stylesheetBytes=0, installedStylesheetWork=0, pending = new Set(), title = "", titleBytes = 0, focus = W.noNode;
  let cascade=cascadeIndex(), sheetRules=new Map();
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
    const next = new Map([...nodes].map(([id, node]) => [id, { ...node, children: [...node.children], listeners: [...node.listeners], ...(node.kind === "element" ? { attributes: new Map(node.attributes), style: new Map(node.style), attributeBytes: new Map(node.attributeBytes), styleBytes: new Map(node.styleBytes) } : {}) }]));
    const nextCascade=cascadeIndex(cascade),nextRules=new Map(sheetRules);
    const nextSheets = new Map(sheets), nextSheetBytes = new Map(sheetBytes), nextSheetWork=new Map(sheetWork), copies = new Set(pending), effects = [];
    const handlers = new Set([...nodes.values()].flatMap((node) => node.listeners.map((listener) => listener.handler)));
    let index, nextTitle = title, nextTitleBytes = titleBytes, nextFocus = focus;
    let parsedStylesheetWork=0,nextStylesheetBytes=stylesheetBytes,nextInstalledStylesheetWork=installedStylesheetWork;
    const fail = (rule) => { throw new ViewProtocolError(rule, index); };
    let cascadeWork=0,expandedValueBytes=0;
    const chargeCascade=()=>{
      potentialCascadeRead(next,nextCascade,work=>{cascadeWork+=work;if(cascadeWork>V.limits.cascadeWork)fail('cascade work budget');});
      potentialExpandedValues(next,nextCascade,W.root,bytes=>{expandedValueBytes+=bytes;if(expandedValueBytes>V.limits.expandedValueBytes)fail('expanded value budget');});
    };
    let selectionWork=0;
    let lastSelectValue;
    const skippedValues=new Set();
    const chargeSelection=(work)=>{selectionWork+=work;if(selectionWork>V.limits.selectWork)fail('select work budget');};
    let radioWork=0;
    const chargeRadio=work=>{radioWork+=work;if(radioWork>V.limits.radioWork)fail('radio work budget');};
    const reconcileRadio=(id,apply)=>{
      const group=radioGroup(next,id,chargeRadio);
      if(apply)for(const peer of group)if(peer!==id)next.get(peer).checked=false;
    };
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
          if (op.attribute === "srcset" && !imageCandidateNames(op.text).every(assetRequested)) fail("unpackaged view asset");
          const changed=node.attributes.get(op.attribute)!==op.text;
          node.attributes.set(op.attribute, op.text);
          node.attributeBytes.set(op.attribute,selectionTextBytes(op.text));
          if(op.attribute === "name")node.radioNameBytes=node.attributeBytes.get(op.attribute);
          if(changed)optionAttribute(next,op.node,op.attribute,true);
          if (["name", "type"].includes(op.attribute) && checkable(node) && inputType(node) === "radio") reconcileRadio(op.node,node.checked);
          break;
        }
        case "remove_attribute": { const node = asElement(op.node); if (!admitted(attributes.get(op.attribute).tags, node)) fail("attribute not allowed"); if(node.attributes.delete(op.attribute)){node.attributeBytes.delete(op.attribute);optionAttribute(next,op.node,op.attribute,false);} break; }
        case "set_class": { const node = asElement(op.node); if (op.text.split(/[\t\n\f\r ]+/).filter(Boolean).some((value) => !/^[A-Za-z_][A-Za-z0-9_-]*$/.test(value))) fail("class value"); node.class = op.text; node.classBytes=selectionTextBytes(op.text); break; }
        case "set_style": {
          const node = asElement(op.node); if (["field-sizing", "resize"].includes(op.property) && node.tag !== "textarea") fail("property not allowed");
          try { checkStyleDeclaration(V, op.property, op.text, checkValue); } catch { fail("style value"); } node.style.set(op.property, op.text); node.styleBytes.set(op.property,selectionTextBytes(op.text)); break;
        }
        case "remove_style": {const node=asElement(op.node);node.style.delete(op.property);node.styleBytes.delete(op.property);break;}
        case "insert": {
          if (op.node === W.root) fail("root not movable");
          const parent = asElement(op.parent), node = live(op.node), rule = tags.get(parent.tag);
          if (node.kind === "text" ? !rule.textChild : !rule.children.includes(node.tag)) fail("child not allowed");
          let depth=0;
          for (let at = op.parent; at !== W.noNode; at = live(at).parent) { if (at === op.node) fail("cycle"); depth++; }
          if (op.before !== W.noNode && (op.before === op.node || live(op.before).parent !== op.parent)) fail("before is not a sibling");
          const oldSelect=selectOwner(next,op.node);
          detach(op.node); parent.children.splice(op.before === W.noNode ? parent.children.length : parent.children.indexOf(op.before), 0, op.node); node.parent = op.parent;
          const stack = [[op.node,depth]];
          while (stack.length) {
            const [id,depth] = stack.pop(), child = live(id);
            if(depth>V.limits.treeDepth)fail('tree depth budget');
            chargeRadio(V.limits.radioNodeWork);
            if (checkable(child) && inputType(child) === "radio") reconcileRadio(id,child.checked);
            for(let i=child.children.length-1;i>=0;i--)stack.push([child.children[i],depth+1]);
          }
          if(oldSelect!==undefined){if(node.tag==='option')normalizeSelect(next,oldSelect);else syncSelect(next,oldSelect);}
          const owner=selectOwner(next,op.node);
          if(owner!==undefined){if(node.tag==='option')normalizeSelect(next,owner,node.optionSelected?op.node:undefined);else syncSelect(next,owner);}
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
        case "set_value": { const node = asElement(op.node); if (!(node.tag === "input" && !checkable(node) || ["textarea", "select"].includes(node.tag))) fail("not a value field"); node.valueReserved=true; if (node.seq === op.base_seq) {if(node.tag==='select'){if(node.value!==op.text)setSelectValue(next,op.node,op.text);}else node.value = op.text;} break; }
        case "set_checked": {
          if (op.checked > 1) fail("checked value"); const node = asElement(op.node); if (!checkable(node)) fail("not a checked field");
          const accepted=node.seq===op.base_seq;
          if(accepted)node.checked=Boolean(op.checked);
          // Meter stale writes too, independently of deferred parent edits.
          if(op.checked&&inputType(node)==="radio")reconcileRadio(op.node,accepted);
          break;
        }
        case "command": {
          const node = asElement(op.node), rule = commands.get(op.command); if (!admitted(rule.tags, node)) fail("command not allowed");
          if (rule.argument ? !(Number.isFinite(op.argument) && op.argument >= 0) : !Object.is(op.argument, 0)) fail("command argument");
          if(['focus','select','scroll_into_view','scroll_to'].includes(op.command))chargeCascade();
          if (op.command === "show_modal") { if (!attached(op.node)) fail("detached modal"); node.open = true; }
          if (op.command === "close") node.open = false;
          if (op.command === "focus" && !node.attributes.has("disabled") && attached(op.node)) nextFocus = op.node;
          if (op.command === "blur" && nextFocus === op.node) nextFocus = W.noNode;
          effects.push({ ...op }); break;
        }
        case "stylesheet": {
          if(!op.sheet)fail('stylesheet id');
          let work=0,rules=[];
          if(op.text){
            let checked;try{checked=checkStylesheet(op.text,V,{workLimit:V.limits.stylesheetWork-parsedStylesheetWork});}
            catch(error){fail(error instanceof StylesheetWorkError?'stylesheet work budget':'stylesheet value');}
            if(checked.errors.length)fail('stylesheet value');work=checked.work;rules=checked.rules;parsedStylesheetWork+=work;
          }
          nextStylesheetBytes=nextStylesheetBytes-(nextSheets.get(op.sheet)?.length??0)*2+op.text.length*2;
          nextInstalledStylesheetWork=nextInstalledStylesheetWork-(nextSheetWork.get(op.sheet)??0)+work;
          if(nextStylesheetBytes>V.limits.stylesheetBytes)fail('stylesheet budget');
          if(nextInstalledStylesheetWork>V.limits.stylesheetWork)fail('stylesheet work budget');
          updateCascade(nextCascade,nextRules.get(op.sheet)??[],false);
          if(op.text){nextRules.set(op.sheet,rules);updateCascade(nextCascade,rules,true);nextSheets.set(op.sheet,op.text);nextSheetBytes.set(op.sheet,selectionTextBytes(op.text));nextSheetWork.set(op.sheet,work);}
          else{nextRules.delete(op.sheet);nextSheets.delete(op.sheet);nextSheetBytes.delete(op.sheet);nextSheetWork.delete(op.sheet);}
          break;
        }
        case "set_title": nextTitle = op.text; nextTitleBytes=selectionTextBytes(op.text); break;
        case "copy_text": if (!op.request || copies.has(op.request)) fail("duplicate copy request"); if (copies.size >= V.limits.commands) fail("copy budget"); copies.add(op.request); effects.push({ ...op }); break;
        default: fail("unimplemented operation");
      }
    }
    index = undefined; const ids = new Set();
    chargeCascade();
    chargeSelection(selectInventoryWork(next));
    let retained=nextTitleBytes;
    const retain=bytes=>{retained+=bytes;if(retained>V.limits.retainedTextBytes)fail('retained text budget');};
    retain(0);
    for(const size of nextSheetBytes.values())retain(size);
    for(const node of next.values()){
      if(node.kind==='text')retain(node.textBytes);
      else{
        node.valueReserved ||= node.tag==='select'||node.tag==='textarea'||node.tag==='input'&&!checkable(node);
        retain(node.classBytes);
        for(const size of node.attributeBytes.values())retain(size);
        for(const size of node.styleBytes.values())retain(size);
        if(node.valueReserved)retain(V.limits.fieldValueReservationBytes);
      }
    }
    for (const node of next.values()) if (node.kind === "element" && node.attributes.has("id")) { const id = node.attributes.get("id"); if (ids.has(id)) fail("duplicate id attribute"); ids.add(id); }
    for(const [id,node] of next)if((node.tag==='select'&&node.value.length*2>V.limits.valueBytes)||(node.tag==='option'&&optionValue(next,id).length*2>V.limits.valueBytes))fail('select value budget');
    if(potentialNamingWork(next)>V.limits.namingWork)fail('accessible name work budget');
    cascade=nextCascade;sheetRules=nextRules;
    nodes = next; sheets = nextSheets; sheetBytes=nextSheetBytes; sheetWork=nextSheetWork; stylesheetBytes=nextStylesheetBytes; installedStylesheetWork=nextInstalledStylesheetWork; pending = copies; title = nextTitle; titleBytes=nextTitleBytes; focus = nextFocus;
    return { operations, effects, skippedValues };
  };
  return { commit,
    // Clone inspection so an adapter cannot mutate the validation authority.
    snapshot() { return { nodes: [...nodes].map(([id, node]) => {
      const {attributeBytes,styleBytes,...state}=node;
      return { id, ...state, children: [...node.children], listeners: node.listeners.map((listener) => ({ ...listener })), ...(node.kind === "element" ? { attributes: Object.fromEntries(node.attributes), style: Object.fromEntries(node.style) } : {}) };
    }), sheets: Object.fromEntries(sheets), title, focus, pending: [...pending] }; },
    close() { closed = true; nodes.clear(); sheets.clear(); sheetBytes.clear(); sheetWork.clear();sheetRules.clear();cascade=cascadeIndex(); stylesheetBytes=0; installedStylesheetWork=0; pending.clear(); title = ""; titleBytes=0; focus = W.noNode; },
    complete(request) { if (!pending.delete(request)) throw new ViewProtocolError("unknown command result"); },
    validateChoice,
    syncEvent(record, selectedOption) {
      validateChoice(record, selectedOption);
      if (record.kind === "command_result" && record.key === "copy_text") { if (!pending.delete(record.seq)) throw new ViewProtocolError("unknown command result"); }
      const node = nodes.get(record.target);
      if(node){if(record.kind==="focusin")focus=record.target;if(record.kind==="focusout"&&focus===record.target)focus=W.noNode;if(record.kind==="close"&&node.tag==="dialog")node.open=false;}
      if (!node || node.kind !== "element" || !["input", "textarea", "select"].includes(node.tag) || !["input", "change"].includes(record.kind)) return;
      if(!inputValue(record.value))throw new ViewProtocolError("input value");
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
      if (!inputValue(value)) throw new ViewProtocolError("input value");
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
