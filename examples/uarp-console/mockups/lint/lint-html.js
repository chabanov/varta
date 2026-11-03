// Checks a mockup's guest tree against the L0 view subset (embedding.md "View
// protocol draft v0"): elements, attribute placement and value grammars, classes,
// inline styles and strings. The guest tree is the content of `#l0-root`; the page
// around it is the embedder and is not checked.
//
// Mockups are static, so a few states that only operations or commands can
// produce at run time are written as attributes; they are reported as "state"
// notes, never as errors: `open` on a dialog (show_modal), `value` on
// input/textarea (set_value), `checked` on an input (set_checked).
import {view,createValueMatcher,checkStyleDeclaration,wellFormed,createViewModel,encodeViewBatch,linkPolicy,OPEN_URL_PATTERNS,STATE_ATTRIBUTES} from "../official-host.js";
import {mockupBatches} from "../conformance/mockup-batch.js";

const CLASS_NAME = /^[A-Za-z_][A-Za-z0-9_-]*$/;

/**
 * @param {Document} doc parsed mockup
 * @returns {{errors: string[], states: string[], elements: number, classes: Set<string>}}
 */
export function lintTree(doc, {openUrlPatterns=OPEN_URL_PATTERNS,stylesheet="",assets=new Set()} = {}) {
  const root = doc.getElementById("l0-root");
  const errors = [];
  const states = [];
  const classes = new Set();
  if (!root) return { errors: ["no #l0-root element"], states, elements: 0, classes };
  const checkStyle = createValueMatcher(Object.fromEntries(view.css.map(row=>[row.name,row.grammar])),Object.fromEntries(view.css.map(row=>[row.name,row.note??""])));
  const tags = new Set(view.tags.map(row=>row.name)),attributes=new Map(view.attributes.map(row=>[row.name,row]));
  const linkRequested=linkPolicy(openUrlPatterns);
  const assetRequested=name=>assets.has(name);
  const probe=(tag,operation)=>{
    const model=createViewModel({linkRequested,assetRequested});
    model.commit(encodeViewBatch([{name:"create_element",node:view.wire.root+1,tag},{...operation,node:view.wire.root+1}]));
  };
  let elements = 0;
  const where = (el) => `<${el.localName}${el.id ? `#${el.id}` : ""}${el.className ? `.${String(el.className).split(" ")[0]}` : ""}>`;

  const walker = doc.createTreeWalker(root, 1 | 4);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (node.nodeType === 3) {
      if (!wellFormed(node.data)) errors.push(`text in ${where(node.parentElement)}: lone surrogate or NUL`);
      continue;
    }
    const el = node;
    elements += 1;
    const tag = el.localName;
    if (!tags.has(tag)) {
      errors.push(`${where(el)}: element outside the subset`);
      continue;
    }
    for (const { name, value } of [...el.attributes]) {
      if (!wellFormed(value)) errors.push(`${where(el)} ${name}: lone surrogate or NUL`);
      if (name === "class") {
        for (const c of value.split(/\s+/).filter(Boolean)) {
          if (!CLASS_NAME.test(c)) errors.push(`${where(el)}: class ${c} is not an ident`);
          classes.add(c);
        }
        continue;
      }
      if (name === "style") {
        for (const declaration of value.split(";").map((d) => d.trim()).filter(Boolean)) {
          const colon = declaration.indexOf(":");
          const property = declaration.slice(0, colon).trim();
          const css = declaration.slice(colon + 1).trim();
          if (property.startsWith("--")) errors.push(`${where(el)}: custom properties exist only in stylesheets`);
          else {
            try {
              checkStyleDeclaration(view, property, css, checkStyle);
              probe(tag,{name:"set_style",property,text:css});
            } catch (error) {
              errors.push(`${where(el)} style: ${error.message}`);
            }
          }
        }
        continue;
      }
      if (STATE_ATTRIBUTES[tag]?.includes(name)) {
        states.push(`${where(el)} ${name}: run-time state (${name === "open" ? "show_modal" : name === "checked" ? "set_checked" : "set_value"})`);
        continue;
      }
      const rule = attributes.get(name);
      if (!rule) {
        errors.push(`${where(el)}: attribute ${name} outside the subset`);
        continue;
      }
      try{probe(tag,{name:"set_attribute",attribute:name,text:value});}
      catch(error){errors.push(`${where(el)} ${name}: ${error.message}`);}
    }
    if (tag === "textarea" && el.textContent !== "") states.push(`${where(el)} text: run-time state (set_value)`);
    if (tag === "a" && !el.hasAttribute("href")) errors.push(`${where(el)}: a without href is not focusable; use button`);
  }
  for (const id of [...root.querySelectorAll("[for]")].map((el) => el.getAttribute("for"))) {
    if (!root.querySelector(`#${CSS.escape(id)}`)) errors.push(`label for=${id} has no target`);
  }
  if (![...root.querySelectorAll('*')].some(el=>!el.disabled&&(el.hasAttribute('tabindex')||el.tabIndex>=0))) states.push("no focusable element");
  if(!errors.length){
    try{
      const model=createViewModel({linkRequested,assetRequested}),batches=mockupBatches(doc,stylesheet);
      model.commit(batches.tree);model.commit(batches.states);
    }catch(error){errors.push(`view transaction: ${error.message}`);}
  }
  return { errors, states, elements, classes };
}
