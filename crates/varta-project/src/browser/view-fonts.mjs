import { tokenizeValue } from './view-css.mjs';

const generic = new Set(['serif', 'sans-serif', 'monospace', 'system-ui']);
const wide = new Set(['inherit', 'initial', 'unset', 'revert', 'revert-layer']);

/** Private font names and parallel custom properties prevent cross-session or
 * ambient named-font lookup. Original variables remain unchanged for other CSS. */
export function createFontNamespace(prefix, families = []) {
  if (!/^[A-Za-z0-9_]+$/.test(prefix)) throw new TypeError('invalid font namespace');
  const names = new Map(families.map((name,index)=>[name.toLowerCase(),`varta_font_${prefix}_${index}`]));
  const family = (name, quoted = true) => {
    const key = name.toLowerCase();
    if (!quoted && (generic.has(key) || wide.has(key))) return name;
    return names.get(key) ?? `varta_font_${prefix}_missing`;
  };
  // No per-turn cache: changing font/custom-property names forever must not
  // accumulate state outside the bounded live DOM and stylesheet inventory.
  const variable = (name) => `--varta-font-${prefix}-${Array.from(name,(unit)=>unit.charCodeAt(0).toString(16).padStart(4,'0')).join('')}`;
  const tokens = (values) => values.map((token) => {
    switch (token.kind) {
      case 'string': return JSON.stringify(family(token.value));
      case 'ident': return family(token.value, false);
      case 'function': {
        if (token.name === 'var') {
          const [name, ...fallback] = token.args;
          if (name?.kind !== 'ident' || !name.value.startsWith('--')) throw new TypeError('invalid font variable');
          return `var(${variable(name.value)} ${tokens(fallback)})`;
        }
        return `${token.name}(${tokens(token.args)})`;
      }
      case 'number': return token.raw;
      case 'dimension': return token.raw + token.unit;
      case 'percentage': return token.raw + '%';
      case 'hash': return '#' + token.value;
      case 'comma': return ',';
      case 'slash': return '/';
      default: throw new TypeError('invalid font token');
    }
  }).join(' ');
  const value = (text) => tokens(tokenizeValue(text));
  const sheet = (rules) => {
    for (const rule of rules) {
      if (rule.style) {
        const declarations = [...rule.style].map((property) => [property, rule.style.getPropertyValue(property)]);
        for (const [property, text] of declarations) {
          if (property === 'font-family') rule.style.setProperty(property, value(text));
          else if (property.startsWith('--')) rule.style.setProperty(variable(property), value(text));
        }
      }
      if (rule.cssRules) sheet(rule.cssRules);
    }
  };
  return { family, variable, value, sheet };
}
