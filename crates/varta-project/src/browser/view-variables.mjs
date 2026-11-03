import {registry} from '../../../varta-common/browser/registry.mjs';

// This is the native computed-scope algorithm, including active substitution
// contexts. Arena references and explicit work stacks never recurse with aliases.
const word = c => c !== undefined && /[A-Za-z0-9_%.#-]/.test(c);
const inherited = new Set(['inherit', 'unset', 'revert', 'revert-layer']);
const token = text => !text.trim() ? 'blank' : text.trim().toLowerCase() === 'initial' ? 'initial' : inherited.has(text.trim().toLowerCase()) ? 'inherited' : 'other';
const blank = () => ({pieces: [], units: 0, first: undefined, last: undefined, token: 'blank'});
function append(to, from) {
  if (!from.units) return to; // Empty is valid; retaining its edges can explode.
  const separator = word(to.last) && word(from.first);
  const units = to.units + from.units + Number(separator);
  if (units > registry.view.limits.valueBytes / 2) return null;
  if (separator) to.pieces.push(' ');
  to.pieces.push(...from.pieces); to.units = units;
  if (to.token === 'blank') to.token = from.token;
  else if (from.token !== 'blank') to.token = 'other';
  to.first ??= from.first; to.last = from.last ?? to.last;
  return to;
}
const literal = text => ({pieces: [text], units: text.length, first: text[0], last: text.at(-1), token: token(text)});

/** Compile already admitted component text, preserving quotes and token edges. */
export function compileVariableValue(text, depth = 0) {
  if (depth > registry.view.limits.cssTokens) return null;
  const parts = [], references = new Set();
  let at = 0, start = 0;
  while (at < text.length) {
    if (text[at] === '"') {
      const end = text.indexOf('"', at + 1); if (end < 0) return null;
      at = end + 1; continue;
    }
    if (!word(text[at])) { at++; continue; }
    const begin = at++; while (word(text[at])) at++;
    if (text.slice(begin, at).toLowerCase() !== 'var' || text[at] !== '(') continue;
    const open = at++; let nesting = 1, comma;
    while (nesting) {
      const c = text[at]; if (c === undefined) return null;
      if (c === '"') { const end = text.indexOf('"', at + 1); if (end < 0) return null; at = end + 1; continue; }
      if (c === '(') nesting++;
      else if (c === ')') nesting--;
      else if (c === ',' && nesting === 1 && comma === undefined) comma = at;
      if (nesting) at++;
    }
    const name = text.slice(open + 1, comma ?? at).trim();
    if (!/^--[A-Za-z_][A-Za-z0-9_-]*$/.test(name)) return null;
    const fallback = comma === undefined ? undefined : compileVariableValue(text.slice(comma + 1, at).trim(), depth + 1);
    if (fallback === null) return null;
    if (start < begin) parts.push(text.slice(start, begin));
    parts.push({name, fallback}); references.add(name);
    for (const name of fallback?.references ?? []) references.add(name);
    at++; start = at;
  }
  if (start < text.length) parts.push(text.slice(start));
  return {parts, references};
}

export function createVariableScope() {
  const values = new Map(), nodes = [];
  const reference = id => id === null || id === undefined ? null : {...nodes[id], pieces: nodes[id].units ? [id] : []};
  const materializeValue = value => {
    if (!value) return null;
    const output = [], pending = value.pieces.toReversed();
    while (pending.length) {
      const part = pending.pop();
      if (typeof part === 'string') output.push(part);
      else for (let i = nodes[part].pieces.length - 1; i >= 0; i--) pending.push(nodes[part].pieces[i]);
    }
    return output.join('').trim();
  };
  const evaluate = template => {
    if (!template) return null;
    let value = blank();
    for (const part of template.parts) {
      const next = typeof part === 'string' ? literal(part) : values.get(part.name) != null ? reference(values.get(part.name)) : evaluate(part.fallback);
      if (!next || !(value = append(value, next))) return null;
    }
    return value;
  };
  return {
    lookup: name => values.get(name) ?? null,
    materialize: id => materializeValue(reference(id)),
    substitute: text => materializeValue(evaluate(compileVariableValue(text))),
    extend(raw) {
      const names = [...raw.keys()].sort(), indices = new Map(names.map((name, i) => [name, i]));
      const templates = names.map(name => raw.get(name));
      const checkpoint = {nodes: nodes.length, previous: names.map(name => [name, values.get(name)]), bindings: new Map()};
      const resolved = names.map(() => null), done = new Uint8Array(names.length), cyclic = new Uint8Array(names.length);
      const positions = new Int32Array(names.length).fill(-1), active = [];
      for (let root = 0; root < names.length; root++) {
        if (done[root]) continue;
        const work = [{kind: 'definition', i: root}], results = [];
        while (work.length) {
          const task = work.pop(), i = task.i;
          if (['template', 'part', 'variable'].includes(task.kind) && cyclic[active.at(-1)]) { results.push(null); continue; }
          switch (task.kind) {
            case 'definition':
              if (!templates[i]) { done[i] = 1; break; }
              positions[i] = active.length; active.push(i);
              work.push({kind: 'finishDefinition', i}, {kind: 'template', template: templates[i]}); break;
            case 'finishDefinition': {
              const value = results.pop();
              if (!cyclic[i] && value) {
                if (value.token === 'inherited') resolved[i] = values.get(names[i]) ?? null;
                else if (value.token !== 'initial') {
                  if (value.pieces.length === 1 && typeof value.pieces[0] === 'number') resolved[i] = value.pieces[0];
                  else { resolved[i] = nodes.length; nodes.push(value); }
                }
              }
              active.pop(); positions[i] = -1; done[i] = 1; break;
            }
            case 'template':
              work.push({kind: 'finishTemplate', start: results.length});
              for (let j = task.template.parts.length - 1; j >= 0; j--) work.push({kind: 'part', part: task.template.parts[j]});
              break;
            case 'finishTemplate': {
              let joined = blank();
              for (const part of results.splice(task.start)) joined = joined && part ? append(joined, part) : null;
              results.push(joined); break;
            }
            case 'part': {
              const part = task.part;
              if (typeof part === 'string') { results.push(part.length <= registry.view.limits.valueBytes / 2 ? literal(part) : null); break; }
              const index = indices.get(part.name), inherited = index === undefined ? values.get(part.name) : null;
              work.push({kind: 'variable', index, inherited, fallback: part.fallback});
              if (index !== undefined && !done[index]) {
                if (positions[index] >= 0) {
                  if (!cyclic[index]) for (let j = positions[index]; j < active.length; j++) cyclic[active[j]] = 1;
                } else work.push({kind: 'definition', i: index});
              }
              break;
            }
            case 'variable': {
              const id = task.index === undefined ? task.inherited : resolved[task.index];
              if (id != null) results.push(reference(id));
              else if (task.fallback) work.push({kind: 'template', template: task.fallback});
              else results.push(null);
              break;
            }
          }
        }
      }
      names.forEach((name, i) => { values.set(name, resolved[i]); checkpoint.bindings.set(name, resolved[i]); });
      checkpoint.end = nodes.length;
      return checkpoint;
    },
    restore(checkpoint) {
      for (const [name, value] of checkpoint.previous) {
        if (value === undefined) values.delete(name); else values.set(name, value);
      }
      nodes.length = checkpoint.nodes;
    },
  };
}
