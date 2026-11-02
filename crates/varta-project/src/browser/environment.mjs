// Explicit host-provided values only. Never consult an ambient process/browser
// environment. Authority checks are separate and must precede every lookup.
import {registry} from '../../../varta-common/browser/registry.mjs';
const MAX_ENTRIES = 256;
const MAX_NAME_BYTES = registry.environment.maxNameBytes;
const MAX_BYTES = registry.environment.maxResultBytes;
const encoder = new TextEncoder();
function unicode(text) {
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const low = text.charCodeAt(++i);
      if (!(low >= 0xdc00 && low <= 0xdfff)) return false;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return false;
  }
  return true;
}
/** Own and bound all inputs before guest execution. No permissions are granted. */
export function snapshotEnvironment(entries = []) {
  if (!Array.isArray(entries) || entries.length > MAX_ENTRIES) throw new TypeError('invalid environment inputs');
  const values = new Map(); let bytes = 0;
  for (const pair of entries) {
    if (!Array.isArray(pair) || pair.length !== 2) throw new TypeError('invalid environment input');
    const [name, value] = pair;
    if (typeof name !== 'string' || typeof value !== 'string' || !unicode(name) || !unicode(value)
      || !name || name.includes('=') || name.includes('\0') || value.includes('\0')
      || encoder.encode(name).length > MAX_NAME_BYTES || value.length * 2 > MAX_BYTES || values.has(name)) {
      throw new TypeError('invalid environment input');
    }
    bytes += encoder.encode(name).length + encoder.encode(value).length;
    if (bytes > MAX_BYTES) throw new TypeError('environment inputs exceed limit');
    values.set(name, value);
  }
  return Object.freeze({has: name => values.has(name), get: name => values.get(name), entries: () => [...values].map(([name,value]) => [name,value])});
}

function patterns(value) {
  if (!Array.isArray(value) || value.length > registry.environment.maxPatterns) throw new TypeError('invalid environment scope');
  return Object.freeze(Array.from(value, pattern => {
    if (typeof pattern !== 'string' || !unicode(pattern) || pattern.includes('\0') || encoder.encode(pattern).length > registry.environment.maxPatternBytes) throw new TypeError('invalid environment scope');
    return pattern;
  }));
}
function matches(pattern, name) {
  if (encoder.encode(pattern).length * encoder.encode(name).length > registry.environment.maxMatchWork) return false;
  const p = Array.from(pattern), t = Array.from(name); let row = new Array(t.length + 1).fill(false); row[0] = true;
  for (let i = 0; i < p.length; i++) {
    const next = new Array(row.length).fill(false);
    if (p[i] === '*') {
      next[0] = row[0];
      for (let j = 0; j < t.length; j++) next[j+1] = row[j+1] || next[j];
      if (p[i+1] === '*') i++;
    } else for (let j = 0; j < t.length; j++) next[j+1] = row[j] && (p[i] === '?' || p[i] === t[j]);
    row = next;
  }
  return row[t.length];
}
/** Scope intersection only; the permission broker must independently grant id. */
export function createEnvironmentPolicy(requested, ceiling) {
  const request = patterns(requested), limit = ceiling === undefined ? undefined : patterns(ceiling);
  const valid = name => typeof name === 'string' && unicode(name) && name.length > 0 && !name.includes('=') && !name.includes('\0') && encoder.encode(name).length <= MAX_NAME_BYTES;
  return Object.freeze({
    allows(name) { return valid(name) && request.some(p => matches(p,name)) && (limit === undefined || limit.some(p => matches(p,name))); },
    exactNames() {
      const exact = p => !p.includes('*') && !p.includes('?');
      const allowed = limit === undefined ? undefined : new Set(limit.filter(exact));
      return [...new Set(request.filter(p => exact(p) && valid(p) && (allowed === undefined || allowed.has(p))))].sort((a,b) => {
        const x = Array.from(a, c => c.codePointAt(0)), y = Array.from(b, c => c.codePointAt(0));
        for (let i = 0; i < Math.min(x.length,y.length); i++) if (x[i] !== y[i]) return x[i]-y[i];
        return x.length-y.length;
      });
    },
  });
}

/** Authorize and audit before inspecting any value. This does not implement the Wasm ABI. */
export function createEnvironmentAccess(snapshot, policy, broker, id) {
  if (!['env.get','env.has'].includes(id)) throw new TypeError('invalid environment capability');
  return Object.freeze({
    read(name) {
      if (!broker.operation(id, name, policy.allows(name))) return {status:'denied'};
      if (!snapshot.has(name)) return id === 'env.has' ? {status:'ok',value:false} : {status:'missing'};
      return id === 'env.has' ? {status:'ok', value:true} : {status:'ok', value:snapshot.get(name)};
    },
  });
}
