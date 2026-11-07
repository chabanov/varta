/** A `font-family` value outside what the namespace maps. */
export class FontValueError extends Error {}

const monospaceFamily = 'monospace';
const generic = new Set(['serif', 'sans-serif', monospaceFamily, 'system-ui']);
// Values reaching the namespace are the view style engine's computed
// `font-family` lists (strings and identifiers, comma-separated), with
// CSS-wide keywords already resolved by it (`inherit` is applied as is).
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
  const family = (name, quoted = true) => {
    const key = name.toLowerCase();
    if (!quoted && generics.has(key)) return generics.get(key);
    if (!quoted && generic.has(key)) return name;
    return names.get(key) ?? `varta_font_${prefix}_missing`;
  };
  /** A computed `font-family` value with every family mapped into the
   * namespace (CSS Syntax 3 strings and identifiers, escapes decoded). */
  const value = (text) => familyList(text).map(([name, quoted]) => quoted ? JSON.stringify(family(name)) : family(name, false)).join(', ');
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
  return { family, value, rules, defaultFamily: fallback, defaultName: defaultFamily };
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

/** The families of a computed `font-family` list: `[name, quoted]` pairs.
 * The text is the engine's (its strings use hexadecimal escapes); anything
 * else is refused rather than guessed. */
function familyList(text) {
  const out = []; let at = 0;
  const escape = () => {
    const hex = /^[0-9A-Fa-f]{1,6}[ \t\n]?/.exec(text.slice(at + 1));
    if (hex) { at += 1 + hex[0].length; return String.fromCodePoint(Number.parseInt(hex[0], 16) || 0xfffd); }
    at += 2; return text[at - 1] ?? '';
  };
  while (at < text.length) {
    while (/[ \t\n]/.test(text[at] ?? '')) at++;
    let name = '', quoted = false;
    if (text[at] === '"' || text[at] === "'") {
      const quote = text[at++]; quoted = true;
      while (at < text.length && text[at] !== quote) name += text[at] === '\\' ? escape() : text[at++];
      if (text[at++] !== quote) throw new FontValueError('unterminated font family string');
    } else {
      while (at < text.length && !/[, \t\n]/.test(text[at])) name += text[at] === '\\' ? escape() : text[at++];
    }
    if (!name && !quoted) throw new FontValueError('empty font family');
    out.push([name, quoted]);
    while (/[ \t\n]/.test(text[at] ?? '')) at++;
    if (at < text.length && text[at++] !== ',') throw new FontValueError('font families must be comma-separated');
  }
  return out;
}
