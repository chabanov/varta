// Checks a mockup's guest tree against the L0 view subset (embedding.md "View
// protocol draft v0"): elements, attribute placement and value grammars, classes,
// inline styles and strings. The guest tree is the content of `#l0-root`; the page
// around it is the embedder and is not checked.
//
// Mockups are static, so a few states that only operations or commands can
// produce at run time are written as attributes; they are reported as "state"
// notes, never as errors: `open` on a dialog (show_modal), `value` on
// input/textarea (set_value), `checked` on an input (set_checked).
import { createValueMatcher, checkStyleDeclaration, CssError } from "../../../varta-web-host/view-css.js";
import { wellFormed, attributeValueOk, attributeAllowedOn } from "../../../varta-web-host/view-rules.js";

const CLASS_NAME = /^[A-Za-z_][A-Za-z0-9_-]*$/;
const STATE_ATTRIBUTES = { dialog: ["open"], input: ["value", "checked"], textarea: [] };
const FOCUSABLE = new Set(["a", "button", "input", "select", "textarea"]);

/**
 * @param {Document} doc parsed mockup
 * @returns {{errors: string[], states: string[], elements: number, classes: Set<string>}}
 */
export function lintTree(doc, view, { openUrlPatterns = [] } = {}) {
  const root = doc.getElementById("l0-root");
  const errors = [];
  const states = [];
  const classes = new Set();
  if (!root) return { errors: ["no #l0-root element"], states, elements: 0, classes };
  const checkStyle = createValueMatcher(view.cssGrammar, view.cssNotes);
  const tags = new Set(Object.keys(view.tags));
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
          else if (/textarea only/.test(view.cssNotes[property] ?? "") && tag !== "textarea") errors.push(`${where(el)}: ${property} applies only to textarea`);
          else {
            try {
              checkStyleDeclaration(view, property, css, checkStyle);
            } catch (error) {
              if (error instanceof CssError) errors.push(`${where(el)} style: ${error.message}`);
              else throw error;
            }
          }
        }
        continue;
      }
      if (STATE_ATTRIBUTES[tag]?.includes(name)) {
        states.push(`${where(el)} ${name}: run-time state (${name === "open" ? "show_modal" : name === "checked" ? "set_checked" : "set_value"})`);
        continue;
      }
      const rule = view.attributeRules[name];
      if (!rule) {
        errors.push(`${where(el)}: attribute ${name} outside the subset`);
        continue;
      }
      if (!attributeAllowedOn(rule, tag)) errors.push(`${where(el)}: ${name} is not allowed on <${tag}>`);
      else if (!attributeValueOk(rule, tag, value, { openUrlPatterns })) errors.push(`${where(el)}: ${name}=${JSON.stringify(value)} violates "${rule.value}"`);
    }
    if (tag === "textarea" && el.textContent !== "") states.push(`${where(el)} text: run-time state (set_value)`);
    if (tag === "a" && !el.hasAttribute("href")) errors.push(`${where(el)}: a without href is not focusable; use button`);
  }
  for (const id of [...root.querySelectorAll("[for]")].map((el) => el.getAttribute("for"))) {
    if (!root.querySelector(`#${CSS.escape(id)}`)) errors.push(`label for=${id} has no target`);
  }
  if (!root.querySelector("[tabindex], " + [...FOCUSABLE].join(","))) states.push("no focusable element");
  return { errors, states, elements, classes };
}
