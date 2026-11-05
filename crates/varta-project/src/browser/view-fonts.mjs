import { tokenizeValue, CssError } from './view-css.mjs';

const monospaceFamily = 'monospace';
const generic = new Set(['serif', 'sans-serif', monospaceFamily, 'system-ui']);
// CSS-wide keywords: the shared L0 validator refuses them in `font-family`
// (the browser would roll back to its own font, which native hosts cannot
// supply); custom property values keep them unchanged.
const wide = new Set(['inherit', 'initial', 'unset', 'revert', 'revert-layer']);
const faceStyles = new Set(['normal', 'italic']);
// The CSS font-weight number range a FontFace descriptor admits.
const minFaceWeight = 1, maxFaceWeight = 1000;

/** Private font names and parallel custom properties prevent cross-session or
 * ambient named-font lookup. Original variables remain unchanged for other CSS.
 * `aliases` maps a generic family to the package family it stands for (the
 * resource section's generic families), and `defaultFamily` is the family of
 * text without a font-family; both resolve exactly as in the native shell. */
export function createFontNamespace(prefix, families = [], { aliases = new Map(), defaultFamily } = {}) {
  if (!/^[A-Za-z0-9_]+$/.test(prefix)) throw new TypeError('invalid font namespace');
  const names = new Map(families.map((name,index)=>[name.toLowerCase(),`varta_font_${prefix}_${index}`]));
  const generics = new Map();
  for (const [name, target] of aliases) {
    const key = name.toLowerCase();
    if (!generic.has(key) || generics.has(key) || !names.has(target.toLowerCase())) throw new TypeError('invalid generic font alias');
    generics.set(key, names.get(target.toLowerCase()));
  }
  if (defaultFamily !== undefined && !names.has(defaultFamily.toLowerCase())) throw new TypeError('invalid default font family');
  const family = (name, quoted = true, strict = true) => {
    const key = name.toLowerCase();
    if (!quoted && generics.has(key)) return generics.get(key);
    if (!quoted && wide.has(key)) {
      if (strict) throw new CssError(`font-family: CSS-wide keyword ${name}`);
      return name;
    }
    if (!quoted && generic.has(key)) return name;
    return names.get(key) ?? `varta_font_${prefix}_missing`;
  };
  // No per-turn cache: changing font/custom-property names forever must not
  // accumulate state outside the bounded live DOM and stylesheet inventory.
  const variable = (name) => `--varta-font-${prefix}-${Array.from(name,(unit)=>unit.charCodeAt(0).toString(16).padStart(4,'0')).join('')}`;
  const tokens = (values, strict) => values.map((token) => {
    switch (token.kind) {
      case 'string': return JSON.stringify(family(token.value));
      case 'ident': return family(token.value, false, strict);
      case 'function': {
        if (token.name.toLowerCase() === 'var') {
          const [name, ...fallback] = token.args;
          if (name?.kind !== 'ident' || !name.value.startsWith('--')) throw new TypeError('invalid font variable');
          return `var(${variable(name.value)} ${tokens(fallback, strict)})`;
        }
        return `${token.name}(${tokens(token.args, strict)})`;
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
  /** A `font-family` value (and a variable's font value): CSS-wide keywords
   * throw `CssError`. */
  const value = (text) => tokens(tokenizeValue(text), true);
  const sheet = (rules) => {
    for (const rule of rules) {
      if (rule.style) {
        const declarations = [...rule.style].map((property) => [property, rule.style.getPropertyValue(property)]);
        for (const [property, text] of declarations) {
          if (property === 'font-family') rule.style.setProperty(property, value(text));
          else if (property.startsWith('--')) rule.style.setProperty(variable(property), tokens(tokenizeValue(text), false));
        }
      }
      if (rule.cssRules) sheet(rule.cssRules);
    }
  };
  const fallback = defaultFamily === undefined ? undefined : family(defaultFamily);
  /** Base-sheet font rules for the registry's view `defaults`, so every way
   * text reaches a family resolves as in the native shell. Only packaged
   * faces (a default family) add rules: the view root names the default
   * family and turns synthesized bold and italic off (the native renderer
   * never synthesizes); the user-agent monospace elements take the family
   * that declares `monospace` (Chrome's UA sheet would give them its own
   * generic font), and form controls inherit their family instead of
   * Chrome's control font. A mapped generic is an ordinary family name in
   * Chrome, so its fixed-font size ratio no longer applies, which the
   * native packaged measurer models too. Every rule is zero-specificity or
   * the root's own and comes first, so any guest rule or inline style wins
   * as it wins over the user-agent sheet. */
  const rules = ({ rootSelector, monospaceTags, controlFontTags }) => {
    if (fallback === undefined) return '';
    const mono = generics.get(monospaceFamily);
    return `${rootSelector} { font-family: ${fallback}; font-synthesis: none; }`
      + (mono === undefined ? '' : ` :where(${monospaceTags.join(', ')}) { font-family: ${mono}; }`)
      + ` :where(${controlFontTags.join(', ')}) { font-family: inherit; font-synthesis: inherit; }`;
  };
  return { family, variable, value, sheet, rules, defaultFamily: fallback };
}

/** Namespace arguments for font descriptors in the resource section's
 * canonical order: unique families, every face's generic families, and the
 * first family as the default once any face declares weight and style
 * (packaged `fonts`); `browser.fonts` faces keep the browser default. */
export function packagedFamilies(faces) {
  const families = [], aliases = new Map();
  for (const face of faces) {
    if (!families.some((known) => known.toLowerCase() === face.key.toLowerCase())) families.push(face.key);
    for (const name of face.generics ?? []) if (aliases.get(name) !== face.key) {
      if (aliases.has(name)) throw new TypeError('invalid generic font alias');
      aliases.set(name, face.key);
    }
  }
  const packaged = faces.some((face) => face.weight !== undefined);
  return [families, { aliases, defaultFamily: packaged ? families[0] : undefined }];
}

/** FontFace descriptors of one packaged face: its declared weight and style. */
export function faceDescriptors(face) {
  if (face.weight === undefined) return {};
  if (!Number.isInteger(face.weight) || face.weight < minFaceWeight || face.weight > maxFaceWeight || !faceStyles.has(face.style)) throw new TypeError('invalid font face descriptor');
  return { weight: String(face.weight), style: face.style };
}
