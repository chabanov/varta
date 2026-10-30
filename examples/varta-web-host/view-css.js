// CSS checks for the L0 view subset, driven by the generated tables of
// embedding.md ("View protocol draft v0"): a matcher for the per-property value
// grammars (CSS value definition syntax: juxtaposition, `||`, `|`, `[ ]`, `?`, `+`,
// `#`, `{m,n}` and a trailing `where` clause) and a parser for the stylesheet
// grammar. Runs in Node and in browsers.

export class CssError extends Error {}

// ----------------------------------------------------------------- value tokens

const NUMBER = /^-?(\d+(\.\d+)?|\.\d+)/;
const IDENT = /^-?[A-Za-z_][A-Za-z0-9_-]*/;

/** Tokenizes one component-value list; functions carry their argument tokens. */
export function tokenizeValue(text) {
  let at = 0;
  function list(closing) {
    const out = [];
    for (;;) {
      while (/\s/.test(text[at] ?? "")) at += 1;
      if (at >= text.length) {
        if (closing) throw new CssError("unclosed function");
        return out;
      }
      const ch = text[at];
      if (ch === ")") {
        if (!closing) throw new CssError("unexpected )");
        at += 1;
        return out;
      }
      if (ch === "," || ch === "/") {
        out.push({ kind: ch === "," ? "comma" : "slash" });
        at += 1;
        continue;
      }
      if (ch === '"') {
        const end = text.indexOf(ch, at + 1);
        if (end < 0) throw new CssError("unterminated string");
        out.push({ kind: "string", value: text.slice(at + 1, end) });
        at = end + 1;
        continue;
      }
      if (ch === "#") {
        const match = /^#[0-9A-Fa-f]+/.exec(text.slice(at));
        if (!match) throw new CssError("bad hash");
        out.push({ kind: "hash", value: match[0].slice(1) });
        at += match[0].length;
        continue;
      }
      const number = NUMBER.exec(text.slice(at));
      if (number && !/^-[A-Za-z_]/.test(text.slice(at))) {
        at += number[0].length;
        const value = Number(number[0]);
        if (text[at] === "%") {
          at += 1;
          out.push({ kind: "percentage", value });
        } else {
          const unit = /^[A-Za-z]+/.exec(text.slice(at));
          if (unit) {
            at += unit[0].length;
            out.push({ kind: "dimension", value, unit: unit[0].toLowerCase(), raw: number[0] });
          } else out.push({ kind: "number", value, raw: number[0] });
        }
        continue;
      }
      const ident = IDENT.exec(text.slice(at)) ?? (/^--[A-Za-z0-9_-]+/.exec(text.slice(at)));
      if (ident) {
        at += ident[0].length;
        if (text[at] === "(") {
          at += 1;
          out.push({ kind: "function", name: ident[0].toLowerCase(), args: list(true) });
        } else out.push({ kind: "ident", value: ident[0] });
        continue;
      }
      throw new CssError(`unexpected character ${JSON.stringify(ch)}`);
    }
  }
  return list(false);
}

// -------------------------------------------------------------- grammar parser

function tokenizeGrammar(text) {
  const tokens = [];
  const re = /\s*(\|\||\||\[|\]|\{\d+(?:,\d*)?\}|\+|#|\?|\*|<[a-z-]+>|[A-Za-z][A-Za-z0-9-]*\(|\)|,|\/|[A-Za-z0-9-]+)/y;
  let match;
  while (re.lastIndex < text.length && (match = re.exec(text))) tokens.push(match[1]);
  if (re.lastIndex < text.trimEnd().length) throw new Error(`cannot read grammar near ${text.slice(re.lastIndex)}`);
  return tokens;
}

/** Parses a value definition into an AST (`|` < `||` < juxtaposition). */
function parseGrammar(text) {
  const tokens = tokenizeGrammar(text);
  let i = 0;
  const peek = () => tokens[i];
  function alternation() {
    const options = [anyOrder()];
    while (peek() === "|") {
      i += 1;
      options.push(anyOrder());
    }
    return options.length === 1 ? options[0] : { type: "alt", options };
  }
  function anyOrder() {
    const items = [sequence()];
    while (peek() === "||") {
      i += 1;
      items.push(sequence());
    }
    return items.length === 1 ? items[0] : { type: "any", items };
  }
  function sequence() {
    const items = [];
    while (i < tokens.length && !["|", "||", "]", ")"].includes(peek())) items.push(term());
    return items.length === 1 ? items[0] : { type: "seq", items };
  }
  function term() {
    let node = atom();
    for (;;) {
      const t = peek();
      if (t === "+") node = { type: "repeat", node, min: 1, max: Infinity };
      else if (t === "*") node = { type: "repeat", node, min: 0, max: Infinity };
      else if (t === "?") node = { type: "repeat", node, min: 0, max: 1 };
      else if (t === "#") node = { type: "list", node };
      else if (/^\{/.test(t ?? "")) {
        const [min, max] = t.slice(1, -1).split(",");
        node = { type: "repeat", node, min: Number(min), max: max === undefined ? Number(min) : max === "" ? Infinity : Number(max) };
      } else return node;
      i += 1;
    }
  }
  function atom() {
    const t = tokens[i++];
    if (t === "[") {
      const inner = alternation();
      if (tokens[i++] !== "]") throw new Error("expected ]");
      return inner;
    }
    if (t.endsWith("(")) {
      const args = alternation();
      if (tokens[i++] !== ")") throw new Error("expected )");
      return { type: "function", name: t.slice(0, -1).toLowerCase(), args };
    }
    if (t === ",") return { type: "comma" };
    if (t === "/") return { type: "slash" };
    if (t.startsWith("<")) return { type: "ref", name: t };
    return { type: "keyword", value: t };
  }
  const ast = alternation();
  if (i !== tokens.length) throw new Error(`trailing grammar tokens: ${tokens.slice(i).join(" ")}`);
  return ast;
}

// ---------------------------------------------------------------------- matcher

const isZero = (t) => t.kind === "number" && t.value === 0;
const BUILTIN = {
  "<number>": (t) => t.kind === "number",
  "<integer>": (t) => t.kind === "number" && /^-?\d+$/.test(t.raw),
  "<length>": (t) => (t.kind === "dimension" && (t.unit === "px" || t.unit === "rem")) || isZero(t),
  "<percentage>": (t) => t.kind === "percentage",
  "<time>": (t) => t.kind === "dimension" && (t.unit === "ms" || t.unit === "s"),
  "<string>": (t) => t.kind === "string" && !t.value.includes("\\"),
  "<ident>": (t) => t.kind === "ident" && /^[A-Za-z_][A-Za-z0-9_-]*$/.test(t.value) && !/^(inherit|initial|unset|revert)$/i.test(t.value),
};
// Composite value types of the generated value-type table that are grammars themselves.
const COMPOSITE_TYPES = { "<line>": "auto | <integer> | span <integer>" };
BUILTIN["<length-percentage>"] = (t) => BUILTIN["<length>"](t) || BUILTIN["<percentage>"](t);

const COLOR_KEYWORDS = new Set(["transparent", "currentcolor"]);
const HASH_LENGTHS = new Set([3, 6, 8]);
const COLOR_FUNCTIONS = {
  rgb: parseGrammar("<integer> , <integer> , <integer>"),
  rgba: parseGrammar("<integer> , <integer> , <integer> , <number>"),
};
const TRACK_FUNCTIONS = {
  minmax: parseGrammar("<track> , <track>"),
  repeat: parseGrammar("<integer> , <track>+"),
};

/**
 * @param {Record<string, string>} grammars property -> value definition
 * @param {Record<string, string>} notes property -> restriction note ("nonnegative", "0 to 1", …)
 */
export function createValueMatcher(grammars, notes = {}) {
  const cache = new Map();
  const composites = Object.fromEntries(Object.entries(COMPOSITE_TYPES).map(([name, definition]) => [name, parseGrammar(definition)]));
  const astFor = (property) => {
    if (!cache.has(property)) {
      const raw = grammars[property];
      if (raw === undefined) throw new CssError(`unknown property ${property}`);
      cache.set(property, { ast: parseGrammar(raw), locals: composites, note: notes[property] ?? "" });
    }
    return cache.get(property);
  };

  function single(name, t, locals) {
    if (t.kind === "function" && t.name === "var") return isVar(t);
    if (locals[name]) return null;
    if (BUILTIN[name]) return BUILTIN[name](t);
    if (name === "<color>") {
      if (t.kind === "hash") return HASH_LENGTHS.has(t.value.length);
      if (t.kind === "ident") return COLOR_KEYWORDS.has(t.value.toLowerCase());
      if (t.kind === "function" && COLOR_FUNCTIONS[t.name]) return full(COLOR_FUNCTIONS[t.name], t.args, locals);
      return false;
    }
    if (name === "<track>") {
      if (BUILTIN["<length-percentage>"](t) || (t.kind === "dimension" && t.unit === "fr") || (t.kind === "ident" && t.value === "auto")) return true;
      if (t.kind === "function" && TRACK_FUNCTIONS[t.name]) return full(TRACK_FUNCTIONS[t.name], t.args, locals);
      return false;
    }
    throw new CssError(`unknown value type ${name}`);
  }

  function isVar(t) {
    const [name, ...rest] = t.args;
    if (!name || name.kind !== "ident" || !name.value.startsWith("--")) return false;
    return rest.length === 0 || (rest[0].kind === "comma" && rest.length > 1);
  }

  function full(ast, tokens, locals) {
    for (const end of match(ast, tokens, 0, locals)) if (end === tokens.length) return true;
    return false;
  }

  // Generator of every end index the node can reach from `i` (backtracking).
  function* match(node, tokens, i, locals) {
    const t = tokens[i];
    switch (node.type) {
      case "keyword":
        if (t && t.kind === "ident" && t.value.toLowerCase() === node.value.toLowerCase()) yield i + 1;
        else if (t && t.kind === "number" && t.raw === node.value) yield i + 1;
        return;
      case "comma":
        if (t?.kind === "comma") yield i + 1;
        return;
      case "slash":
        if (t?.kind === "slash") yield i + 1;
        return;
      case "ref":
        if (locals[node.name]) {
          yield* match(locals[node.name], tokens, i, locals);
          return;
        }
        if (t && single(node.name, t, locals)) yield i + 1;
        return;
      case "function":
        if (t?.kind === "function" && t.name === node.name && full(node.args, t.args, locals)) yield i + 1;
        return;
      case "seq":
        yield* sequence(node.items, 0, tokens, i, locals);
        return;
      case "alt":
        for (const option of node.options) yield* match(option, tokens, i, locals);
        return;
      case "any":
        yield* anyOrder(node.items, new Set(), tokens, i, locals, false);
        return;
      case "repeat":
        yield* repeat(node, tokens, i, locals, 0);
        return;
      case "list":
        for (const end of match(node.node, tokens, i, locals)) {
          yield end;
          if (tokens[end]?.kind === "comma") yield* match(node, tokens, end + 1, locals);
        }
        return;
    }
  }
  function* sequence(items, k, tokens, i, locals) {
    if (k === items.length) {
      yield i;
      return;
    }
    for (const end of match(items[k], tokens, i, locals)) yield* sequence(items, k + 1, tokens, end, locals);
  }
  function* anyOrder(items, used, tokens, i, locals, matchedOne) {
    if (matchedOne) yield i;
    for (let k = 0; k < items.length; k += 1) {
      if (used.has(k)) continue;
      for (const end of match(items[k], tokens, i, locals)) {
        if (end === i) continue;
        yield* anyOrder(items, new Set([...used, k]), tokens, end, locals, true);
      }
    }
  }
  function* repeat(node, tokens, i, locals, count) {
    if (count >= node.min) yield i;
    if (count >= node.max) return;
    for (const end of match(node.node, tokens, i, locals)) {
      if (end === i) continue;
      yield* repeat(node, tokens, end, locals, count + 1);
    }
  }

  function numbers(tokens) {
    return tokens.flatMap((t) => (t.kind === "function" ? numbers(t.args) : ["number", "dimension", "percentage"].includes(t.kind) ? [t.value] : []));
  }

  /** Throws CssError unless `value` matches the property's grammar. */
  return function checkValue(property, value) {
    const { ast, locals, note } = astFor(property);
    const tokens = tokenizeValue(value);
    if (tokens.length === 0) throw new CssError(`${property}: empty value`);
    if (!full(ast, tokens, locals)) throw new CssError(`${property}: ${JSON.stringify(value)} does not match ${grammars[property]}`);
    if (/nonnegative/.test(note) && numbers(tokens).some((n) => n < 0)) throw new CssError(`${property}: negative value`);
    if (/0 to 1/.test(note) && numbers(tokens).some((n) => n < 0 || n > 1)) throw new CssError(`${property}: value outside 0 to 1`);
  };
}

// ------------------------------------------------------------------ stylesheets

const CLASS_SELECTOR = /^(?::root|(?:\.[A-Za-z_][A-Za-z0-9_-]*)+(?::[a-z-]+)*)$/;
const CONDITION = /^\(\s*(prefers-color-scheme\s*:\s*(light|dark)|(min-width|max-width)\s*:\s*(\S+))\s*\)$/;

/**
 * Checks stylesheet text against the published grammar; returns the rules found
 * ({selectors, declarations}) for reporting.
 */
export function checkStylesheet(text, view) {
  const checkValue = createValueMatcher(view.cssGrammar, view.cssNotes);
  const forbidden = view.stylesheetTokens["Forbidden anywhere"];
  const pseudo = new Set(view.stylesheetTokens["Pseudo-classes"]);
  for (const token of forbidden) if (text.includes(token)) throw new CssError(`forbidden token ${token}`);
  const errors = [];
  const rules = [];
  let at = 0;
  const skip = () => {
    while (/\s/.test(text[at] ?? "")) at += 1;
  };
  function readRule(where) {
    const open = text.indexOf("{", at);
    const close = text.indexOf("}", open);
    if (open < 0 || close < 0) throw new CssError(`${where}: unterminated rule`);
    const selectors = text.slice(at, open).split(",").map((s) => s.trim());
    for (const selector of selectors) {
      if (!CLASS_SELECTOR.test(selector)) errors.push(`${where}: selector ${JSON.stringify(selector)} is outside the subset`);
      for (const p of selector.match(/:[a-z-]+/g) ?? []) if (p !== ":root" && !pseudo.has(p)) errors.push(`${where}: pseudo-class ${p} is outside the subset`);
    }
    const body = text.slice(open + 1, close);
    at = close + 1;
    const declarations = body.split(";").map((d) => d.trim()).filter(Boolean);
    for (const declaration of declarations) {
      const colon = declaration.indexOf(":");
      const property = declaration.slice(0, colon).trim();
      const value = declaration.slice(colon + 1).trim();
      if (colon < 0) errors.push(`${where}: bad declaration ${JSON.stringify(declaration)}`);
      else if (property.startsWith("--")) {
        if (!/^--[A-Za-z_][A-Za-z0-9_-]*$/.test(property)) errors.push(`${where}: bad custom property ${property}`);
        try {
          tokenizeValue(value);
        } catch (error) {
          errors.push(`${where}: ${property}: ${error.message}`);
        }
      } else {
        try {
          checkValue(property, value);
        } catch (error) {
          errors.push(`${where}: ${error.message}`);
        }
      }
    }
    rules.push({ selectors, declarations: declarations.length });
  }
  for (;;) {
    skip();
    if (at >= text.length) break;
    if (text.startsWith("@media", at)) {
      const open = text.indexOf("{", at);
      const conditions = text.slice(at + "@media".length, open).trim().split(/\s+and\s+/);
      for (const condition of conditions) {
        const match = CONDITION.exec(condition);
        if (!match) errors.push(`@media condition ${JSON.stringify(condition)} is outside the subset`);
        else if (match[4] && !BUILTIN["<length>"](tokenizeValue(match[4])[0] ?? {})) errors.push(`@media length ${match[4]} is outside the subset`);
      }
      at = open + 1;
      for (;;) {
        skip();
        if (text[at] === "}") {
          at += 1;
          break;
        }
        readRule("@media");
      }
    } else readRule("rule");
  }
  return { rules, errors };
}

/** Checks one `set_style` value: forbidden tokens, no custom properties, grammar. */
export function checkStyleDeclaration(view, property, value, checkValue = createValueMatcher(view.cssGrammar, view.cssNotes)) {
  const forbidden = [...view.stylesheetTokens["Forbidden anywhere"], ...view.stylesheetTokens["Forbidden in `set_style` values"]];
  const token = forbidden.find((t) => value.includes(t));
  if (token) throw new CssError(`${property}: forbidden token ${token}`);
  checkValue(property, value);
}

/** Rewrites `:root` selectors to the renderer's root-container selector. */
export function rewriteRootSelector(text, selector) {
  return text.replace(/:root(?![A-Za-z0-9_-])/g, selector);
}
