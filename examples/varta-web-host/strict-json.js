// Strict JSON reader for manifests, catalogs, language metadata and tool arguments.
// Unlike JSON.parse it rejects duplicate keys (after escape decoding) and keeps the
// raw text of numbers, because tool arguments accept only plain i32 tokens.

export class JsonNumber {
  /** @param {string} raw */
  constructor(raw) {
    this.raw = raw;
  }
}

export class JsonError extends Error {}

/** @param {string} text */
export function parseStrictJson(text) {
  let at = 0;
  const fail = (what) => {
    throw new JsonError(`${what} at offset ${at}`);
  };
  const ws = () => {
    while (at < text.length && " \t\n\r".includes(text[at])) at += 1;
  };
  const expect = (literal) => {
    if (text.startsWith(literal, at)) at += literal.length;
    else fail(`expected ${literal}`);
  };

  function string() {
    expect('"');
    let out = "";
    for (;;) {
      if (at >= text.length) fail("unterminated string");
      const ch = text[at];
      const unit = text.charCodeAt(at);
      if (ch === '"') {
        at += 1;
        return out;
      }
      if (unit < 0x20) fail("control character in string");
      if (ch !== "\\") {
        out += ch;
        at += 1;
        continue;
      }
      const esc = text[at + 1];
      at += 2;
      const simple = { '"': '"', "\\": "\\", "/": "/", b: "\b", f: "\f", n: "\n", r: "\r", t: "\t" };
      if (esc in simple) out += simple[esc];
      else if (esc === "u") {
        const hex = text.slice(at, at + 4);
        if (!/^[0-9a-fA-F]{4}$/.test(hex)) fail("bad unicode escape");
        out += String.fromCharCode(parseInt(hex, 16));
        at += 4;
      } else fail("bad escape");
    }
  }

  function number() {
    const match = /^-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?/.exec(text.slice(at));
    if (!match) fail("bad number");
    at += match[0].length;
    return new JsonNumber(match[0]);
  }

  function value() {
    ws();
    const ch = text[at];
    if (ch === "{") {
      at += 1;
      const object = Object.create(null);
      ws();
      if (text[at] === "}") {
        at += 1;
        return object;
      }
      for (;;) {
        ws();
        const key = string();
        if (Object.prototype.hasOwnProperty.call(object, key)) fail(`duplicate key ${JSON.stringify(key)}`);
        ws();
        expect(":");
        object[key] = value();
        ws();
        if (text[at] === ",") at += 1;
        else if (text[at] === "}") {
          at += 1;
          return object;
        } else fail("expected , or }");
      }
    }
    if (ch === "[") {
      at += 1;
      const array = [];
      ws();
      if (text[at] === "]") {
        at += 1;
        return array;
      }
      for (;;) {
        array.push(value());
        ws();
        if (text[at] === ",") at += 1;
        else if (text[at] === "]") {
          at += 1;
          return array;
        } else fail("expected , or ]");
      }
    }
    if (ch === '"') return string();
    if (text.startsWith("true", at)) return (at += 4), true;
    if (text.startsWith("false", at)) return (at += 5), false;
    if (text.startsWith("null", at)) return (at += 4), null;
    return number();
  }

  const result = value();
  ws();
  if (at !== text.length) fail("trailing data");
  return result;
}

/** True when a decoded JSON string contains a lone surrogate code unit. */
export function hasLoneSurrogate(text) {
  for (let i = 0; i < text.length; i += 1) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(i + 1);
      if (next >= 0xdc00 && next <= 0xdfff) i += 1;
      else return true;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return true;
  }
  return false;
}

/** Checks that `object` has exactly the `required` keys plus any of `optional`. */
export function exactKeys(object, required, optional = [], where = "object") {
  if (object === null || typeof object !== "object" || Array.isArray(object)) {
    throw new JsonError(`${where} must be an object`);
  }
  for (const key of required) {
    if (!(key in object)) throw new JsonError(`${where} is missing ${key}`);
  }
  for (const key of Object.keys(object)) {
    if (!required.includes(key) && !optional.includes(key)) throw new JsonError(`${where} has unknown field ${key}`);
  }
}
