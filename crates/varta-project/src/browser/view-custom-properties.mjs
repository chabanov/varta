import {registry} from '../../../varta-common/browser/registry.mjs';
import {CssError} from './view-css.mjs';
import {compileVariableValue, createVariableScope} from './view-variables.mjs';

// Non-ASCII names cannot be declared or referenced by the guest's ASCII subset.
// A private literal is installed once at its defining element; aliases refer to
// that inherited literal, avoiding a copy of expanded text for every alias.
const valueName = id => `--varta-λ-value-${id}`;
const fontName = id => `--varta-λ-font-${id}`;
const stateEvents = ['pointerover', 'pointerout', 'pointerdown', 'pointerup', 'pointercancel', 'keydown', 'keyup', 'focusin', 'focusout', 'input', 'change'];
function references(text) {
  if (!text.toLowerCase().includes('var(')) return [];
  const compiled = compileVariableValue(text);
  if (!compiled) throw new CssError('invalid variable syntax');
  return compiled.references;
}

/** Renderer-owned custom-property cascade; ordinary layout remains platform CSS. */
export function createCustomPropertyStyles({root, shadow, nodes, ids, fonts, onError}) {
  const window = root.ownerDocument.defaultView;
  const sheets = new Map(), roots = new Set(), classes = new Map();
  let inline = new Map(), installed = new WeakMap(), frame, closed = false, records = 0, dynamic = 0, hadData = false;
  const schedule = () => {
    if (closed || frame !== undefined) return;
    frame = window.requestAnimationFrame(() => { frame = undefined; try { refresh(); } catch (error) { onError(error); } });
  };
  const stateChanged = () => { if (dynamic) schedule(); };
  for (const kind of stateEvents) shadow.addEventListener(kind, stateChanged, true);
  window.addEventListener('blur', stateChanged); window.addEventListener('focus', stateChanged);

  function prepare(sheet) {
    const prepared = {rules: [], queries: new Set()}; let order = 0;
    const read = (list, media) => {
      for (const rule of list) {
        if (rule.style) {
          const custom = new Map(), refs = new Set();
          // Pending variable shorthands serialize their longhands as empty.
          // cssText retains the authored shorthand and its reference names.
          // Semicolons inside values are outside the admitted stylesheet subset.
          for (const declaration of rule.style.cssText.split(';')) {
            const colon = declaration.indexOf(':'); if (colon < 0) continue;
            const name = declaration.slice(0, colon).trim(), text = declaration.slice(colon + 1).trim();
            if (name.startsWith('--')) {
              custom.set(name, compileVariableValue(text));
              rule.style.removeProperty(name);
            } else for (const name of references(text)) refs.add(name);
          }
          const selectors = rule.selectorText.split(',').map(text => {
            text = text.trim(); const isRoot = text === registry.view.defaults.rootSelector;
            return {text, root: isRoot, anchor: isRoot ? null : /^\.([A-Za-z_][A-Za-z0-9_-]*)/.exec(text)?.[1],
              specificity: isRoot ? 1 : (text.match(/\.[A-Za-z_][A-Za-z0-9_-]*|:[a-z-]+/g) ?? []).length};
          });
          if (custom.size || refs.size) prepared.rules.push({selectors, custom, refs, media, order,
            dynamic: selectors.some(selector => !selector.root && selector.text.includes(':'))});
          // The protocol's implicit root matches only :root selectors, even if
          // it has guest classes. A zero-specificity guard keeps platform CSS
          // consistent with the native matcher and its candidate-work inventory.
          rule.selectorText = selectors.map(selector => selector.root ? selector.text : `:where(:not(${registry.view.defaults.rootSelector}))${selector.text}`).join(', ');
          order++;
        } else if (rule.cssRules) {
          const query = window.matchMedia(rule.conditionText); prepared.queries.add(query);
          read(rule.cssRules, [...media, query]);
        }
      }
    };
    read(sheet.cssRules, []);
    fonts.sheet(sheet.cssRules);
    return prepared;
  }
  function index(prepared, adding) {
    for (const record of prepared.rules) {
      records += adding ? 1 : -1;
      if (record.dynamic) dynamic += adding ? 1 : -1;
      for (const anchor of new Set(record.selectors.map(selector => selector.anchor))) {
        if (anchor === undefined) continue;
        let set = anchor === null ? roots : classes.get(anchor);
        if (adding) { if (!set) { set = new Set(); classes.set(anchor, set); } set.add(record); }
        else { set?.delete(record); if (anchor !== null && !set?.size) classes.delete(anchor); }
      }
    }
    for (const query of prepared.queries) {
      if (adding) query.addEventListener('change', schedule);
      else query.removeEventListener('change', schedule);
    }
  }
  function replace(id, prepared) {
    const previous = sheets.get(id); if (previous) { index(previous, false); sheets.delete(id); }
    if (prepared) {
      for (const record of prepared.rules) record.sheet = id;
      sheets.set(id, prepared); index(prepared, true);
    }
  }
  function matched(node) {
    const candidates = new Set(node === root ? roots : []);
    if (node !== root) for (const name of node.classList) for (const rule of classes.get(name) ?? []) candidates.add(rule);
    const result = [];
    for (const record of candidates) {
      if (!record.media.every(query => query.matches)) continue;
      let specificity = 0;
      for (const selector of record.selectors) if (selector.root === (node === root) && node.matches(selector.text)) specificity = Math.max(specificity, selector.specificity);
      if (specificity) result.push([specificity, record]);
    }
    result.sort(([a, x], [b, y]) => a - b || x.sheet - y.sheet || x.order - y.order);
    return result.map(([, record]) => record);
  }
  function refresh() {
    if (closed || !records && !inline.size && !hadData) return;
    for (const component of nodes.values()) {
      if (component.nodeType !== 1 || ids.has(component.parentNode)) continue;
      const scope = createVariableScope(), work = [{node: component}];
      while (work.length) {
        const task = work.pop();
        if (task.leave) { scope.restore(task.leave); continue; }
        const node = task.node, definitions = new Map(), refs = new Set(inline.get(ids.get(node)));
        for (const rule of matched(node)) {
          for (const [name, value] of rule.custom) definitions.set(name, value);
          for (const name of rule.refs) refs.add(name);
        }
        const checkpoint = scope.extend(definitions), next = new Map();
        for (const name of refs) if (scope.lookup(name) === null) {
          // Missing guest variables cannot pick up ambient document bindings.
          next.set(name, 'initial'); next.set(fonts.variable(name), 'initial');
        }
        for (let id = checkpoint.nodes; id < checkpoint.end; id++) {
          const text = scope.materialize(id);
          next.set(valueName(id), text || ' ');
          let font;
          try { font = fonts.value(text); }
          catch (error) { if (!(error instanceof CssError)) throw error; font = 'initial'; }
          next.set(fontName(id), font || ' ');
        }
        for (const [name, id] of checkpoint.bindings) {
          next.set(name, id === null ? 'initial' : `var(${valueName(id)})`);
          next.set(fonts.variable(name), id === null ? 'initial' : `var(${fontName(id)})`);
        }
        const previous = installed.get(node);
        for (const name of previous?.keys() ?? []) if (!next.has(name)) node.style.removeProperty(name);
        for (const [name, value] of next) if (previous?.get(name) !== value) node.style.setProperty(name, value);
        installed.set(node, next);
        work.push({leave: checkpoint});
        for (let i = node.children.length - 1; i >= 0; i--) if (ids.has(node.children[i])) work.push({node: node.children[i]});
      }
    }
    hadData = Boolean(records || inline.size);
  }
  return {
    prepare, replace, refresh,
    commit(current) {
      inline = new Map();
      for (const [id, node] of current) {
        const refs = new Set();
        for (const text of Object.values(node.style ?? {})) for (const name of references(text)) refs.add(name);
        if (refs.size) inline.set(id, refs);
      }
      refresh();
    },
    close() {
      if (closed) return; closed = true;
      if (frame !== undefined) window.cancelAnimationFrame(frame);
      for (const kind of stateEvents) shadow.removeEventListener(kind, stateChanged, true);
      window.removeEventListener('blur', stateChanged); window.removeEventListener('focus', stateChanged);
      for (const prepared of sheets.values()) index(prepared, false);
      sheets.clear(); classes.clear(); roots.clear(); inline.clear(); installed = new WeakMap();
    },
  };
}
