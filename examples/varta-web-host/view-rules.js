// Value rules of the L0 view subset shared by the browser applier and the mockup
// linter, driven by the generated tables of embedding.md: attribute grammars,
// strings and URL grants.

const IDENT = /^[A-Za-z][A-Za-z0-9_-]{0,63}$/;
const COUNT = /^(0|[1-9][0-9]*)$/;
const POSITIVE = /^[1-9][0-9]*$/;
const NONNEGATIVE_NUMBER = /^(\d+(\.\d+)?|\.\d+)$/;
const RESOURCE_PATH = /^[A-Za-z0-9._-]+(\/[A-Za-z0-9._-]+)*$/;
const ROUTE_CHAR = /^([A-Za-z0-9\-._~!$&'()*+,;=:@/?#]|%[0-9A-Fa-f]{2})$/;
const EXTERNAL = /^https?:\/\//;

/** True when a string has no lone surrogate and no U+0000 (draft v0 "Strings"). */
export function wellFormed(text) {
  for (let i = 0; i < text.length; i += 1) {
    const unit = text.charCodeAt(i);
    if (unit === 0) return false;
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(i + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
      i += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return false;
  }
  return true;
}

/** Replaces lone surrogates with U+FFFD (strings the host writes into event records). */
export function toWellFormed(text) {
  let out = "";
  for (let i = 0; i < text.length; i += 1) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff && text.charCodeAt(i + 1) >= 0xdc00 && text.charCodeAt(i + 1) <= 0xdfff) {
      out += text[i] + text[i + 1];
      i += 1;
    } else out += unit >= 0xd800 && unit <= 0xdfff ? "�" : text[i];
  }
  return out;
}

/** Internal route: `#fragment` or `/path[?query][#fragment]`, never `//`. */
export function internalRoute(value) {
  if (/[\s\\]/.test(value) || /[\u0000-\u001f\u007f]/.test(value)) return false;
  if (value.startsWith("#")) return true;
  if (!value.startsWith("/") || value.startsWith("//")) return false;
  return [...value.matchAll(/%[0-9A-Fa-f]{2}|./gs)].every((m) => ROUTE_CHAR.test(m[0]));
}

export const isExternalUrl = (value) => EXTERNAL.test(value);

/**
 * URL grant match (embedding.md "Pattern matching"): scheme, normalized host and
 * effective port must match exactly; `*` matches any run after the authority;
 * `?` keeps its query-separator meaning there.
 */
export function urlMatches(pattern, value) {
  let url;
  let base;
  try {
    url = new URL(value);
    base = new URL(pattern.replace(/[*].*$/, ""));
  } catch {
    return false;
  }
  if (url.username || url.password || url.hash) return false;
  if (url.protocol !== base.protocol || url.hostname !== base.hostname || url.port !== base.port) return false;
  const rest = (s) => s.slice(s.indexOf("/", s.indexOf("//") + 2) < 0 ? s.length : s.indexOf("/", s.indexOf("//") + 2));
  const patternRest = rest(pattern) || "/";
  const valueRest = `${url.pathname}${url.search}`;
  const regex = new RegExp(`^${patternRest.split("*").map((part) => part.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*")}$`);
  return regex.test(valueRest);
}

/**
 * Checks an attribute value against its generated grammar for one element.
 * `openUrlPatterns` are the package request's `ui.open_url` patterns.
 */
export function attributeValueOk(rule, tag, value, { openUrlPatterns = [] } = {}) {
  const perElement = new RegExp(`(?:^|; )${tag}: ([^;]+)`).exec(rule.value);
  const grammar = perElement ? perElement[1] : rule.value;
  if (grammar.startsWith("boolean")) return value === "";
  if (grammar.startsWith("ident")) return IDENT.test(value);
  if (grammar.startsWith("count")) return COUNT.test(value);
  if (grammar.startsWith("positive integer")) return POSITIVE.test(value);
  if (grammar.startsWith("internal route")) {
    if (internalRoute(value)) return true;
    return /ui\.open_url/.test(grammar) && isExternalUrl(value) && openUrlPatterns.some((p) => urlMatches(p, value));
  }
  if (grammar.startsWith("bundled resource path")) return RESOURCE_PATH.test(value) && !value.split("/").includes("..");
  if (grammar.startsWith("finite nonnegative number")) return NONNEGATIVE_NUMBER.test(value);
  if (grammar.startsWith("space-separated ident list")) return value.split(" ").every((v) => IDENT.test(v));
  if (grammar === "text") return true;
  const choices = [...grammar.matchAll(/`([^`]+)`/g)].map((m) => m[1]);
  if (choices.length) return choices.includes(value);
  throw new Error(`unknown attribute grammar ${grammar}`);
}

/** Whether an element may carry an attribute per the generated table. */
export function attributeAllowedOn(rule, tag) {
  return rule.elements === "all" || rule.elements.includes(tag);
}

const FOCUSABLE = new Set(["a", "button", "input", "select", "textarea"]);

/** Whether an element may receive a command per the generated table. */
export function commandAllowedOn(rule, tag, hasTabindex) {
  if (rule.elements === "all") return true;
  if (rule.elements === "focusable") return FOCUSABLE.has(tag) || hasTabindex;
  return rule.elements.includes(tag);
}
