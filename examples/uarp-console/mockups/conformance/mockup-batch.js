// Encodes a static mockup's guest tree (`#l0-root`) as L0 batches, so the same
// screen can be rendered by the applier and compared with the mockup. States that
// only operations or commands produce at run time (an open dialog, a field value,
// a checked box) become those operations in a second batch.
import { batchEncoder } from "../../../varta-web-host/view-encoder.js";

const STATE_ATTRIBUTES = { dialog: ["open"], input: ["value", "checked"], textarea: [] };
const STYLESHEET_ID = 1;
const ROOT_ID = 1;

/**
 * @param {object} view contract.view
 * @param {Document} doc parsed mockup
 * @param {string} stylesheet guest stylesheet text
 * @returns {{tree: Uint8Array, states: Uint8Array}}
 */
export function mockupBatches(view, doc, stylesheet) {
  const tree = batchEncoder(view).op("stylesheet", { sheet: STYLESHEET_ID, text: stylesheet });
  const states = batchEncoder(view);
  let nextId = ROOT_ID + 1;

  function emit(parentId, domNode) {
    if (domNode.nodeType === 3) {
      const id = nextId++;
      tree.op("create_text", { node: id, text: domNode.data }).op("insert", { parent: parentId, node: id, before: 0 });
      return;
    }
    if (domNode.nodeType !== 1) return;
    const tag = domNode.localName;
    const id = nextId++;
    tree.op("create_element", { node: id, tag });
    for (const { name, value } of [...domNode.attributes]) {
      if (name === "class") tree.op("set_class", { node: id, text: value });
      else if (name === "style") {
        for (const declaration of value.split(";").map((d) => d.trim()).filter(Boolean)) {
          const colon = declaration.indexOf(":");
          tree.op("set_style", { node: id, property: declaration.slice(0, colon).trim(), text: declaration.slice(colon + 1).trim() });
        }
      } else if (STATE_ATTRIBUTES[tag]?.includes(name)) {
        if (name === "open") states.op("command", { node: id, command: "show_modal", argument: 0 });
        else if (name === "value") states.op("set_value", { node: id, text: value, base_seq: 0 });
        else if (name === "checked") states.op("set_checked", { node: id, checked: 1, base_seq: 0 });
      } else tree.op("set_attribute", { node: id, attribute: name, text: value });
    }
    if (tag === "textarea") {
      if (domNode.textContent !== "") states.op("set_value", { node: id, text: domNode.textContent, base_seq: 0 });
    } else for (const child of domNode.childNodes) emit(id, child);
    tree.op("insert", { parent: parentId, node: id, before: 0 });
  }

  for (const child of doc.getElementById("l0-root").childNodes) emit(ROOT_ID, child);
  return { tree: tree.encode(), states: states.encode() };
}
