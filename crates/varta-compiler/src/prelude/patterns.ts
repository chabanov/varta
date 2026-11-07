// Engine prelude of checked-f64-utf16-v38 (`type_level`, K9 A): the run-time
// test of a pattern template literal type (`on${string}`, `${number}px`,
// `Uppercase<string>`) in a checked assertion (`x as T`, `JSON.parse(text) as
// T`), in typed guest code with no host import and no capability. The
// compiler loads it only when a module spells a template literal type with a
// placeholder or an intrinsic string type, and calls `patternMatches` from
// `lower::unknown` (see docs/architecture.md, Type level).
//
// The test is the compile-time one (`type_level::patterns::text_matches`),
// TypeScript's inference from a string literal to a template literal type:
// a placeholder followed by text takes the text up to that text's first
// occurrence, one followed by another placeholder one code unit, and the last
// one the rest; every taken text must be a value of its placeholder.

// Placeholder kinds, in the order of `lower::type_level::patterns::Hole`.
const STRING_HOLE = 0;
const NUMBER_HOLE = 1;
const BIGINT_HOLE = 2;
const UPPERCASE_HOLE = 3;
const LOWERCASE_HOLE = 4;
const CAPITALIZE_HOLE = 5;
const UNCAPITALIZE_HOLE = 6;

// TypeScript's `isValidNumberString(s, false)`: not empty, and the
// ECMAScript `ToNumber` of the text is finite.
function numberText(text: string): boolean {
  return text !== '' && Number.isFinite(Number(text));
}

function digitsOf(text: string, start: number, radix: number): boolean {
  if (start >= text.length) {
    return false;
  }
  for (let index = start; index < text.length; index++) {
    const unit = varta.text.unit(text, index);
    let digit = 99;
    if (unit >= 0x30 && unit <= 0x39) {
      digit = unit - 0x30;
    } else if (unit >= 0x61 && unit <= 0x66) {
      digit = unit - 0x61 + 10;
    } else if (unit >= 0x41 && unit <= 0x46) {
      digit = unit - 0x41 + 10;
    }
    if (digit >= radix) {
      return false;
    }
  }
  return true;
}

// TypeScript's `isValidBigIntString(s, false)`: an optional `-` and the
// digits of a bigint literal without its `n` (decimal without a leading
// zero, or `0x`/`0o`/`0b` digits; no separators or whitespace).
function bigintText(text: string): boolean {
  const start = text.startsWith('-') ? 1 : 0;
  const body = text.slice(start);
  const prefix = body.slice(0, 2);
  if (prefix === '0x' || prefix === '0X') {
    return digitsOf(body, 2, 16);
  }
  if (prefix === '0o' || prefix === '0O') {
    return digitsOf(body, 2, 8);
  }
  if (prefix === '0b' || prefix === '0B') {
    return digitsOf(body, 2, 2);
  }
  return body === '0' || (digitsOf(body, 0, 10) && !body.startsWith('0'));
}

// Whether `text` is a value of the placeholder `hole`.
function holeAccepts(hole: number, text: string): boolean {
  if (hole === STRING_HOLE) {
    return true;
  }
  if (hole === NUMBER_HOLE) {
    return numberText(text);
  }
  if (hole === BIGINT_HOLE) {
    return bigintText(text);
  }
  if (hole === UPPERCASE_HOLE) {
    return text.toUpperCase() === text;
  }
  if (hole === LOWERCASE_HOLE) {
    return text.toLowerCase() === text;
  }
  const first = text.slice(0, 1);
  if (hole === CAPITALIZE_HOLE) {
    return first.toUpperCase() === first;
  }
  if (hole === UNCAPITALIZE_HOLE) {
    return first.toLowerCase() === first;
  }
  return false;
}

// Whether `text` is a value of the pattern whose literal texts are `texts`
// (one more than its placeholders `holes`, empty where two placeholders
// meet or at an end).
export function patternMatches(text: string, texts: string[], holes: number[]): boolean {
  const first = texts[0];
  const last = texts[texts.length - 1];
  if (!text.startsWith(first)) {
    return false;
  }
  if (text.length < first.length + last.length || !text.endsWith(last)) {
    return false;
  }
  const end = text.length - last.length;
  let position = first.length;
  for (let index = 0; index < holes.length; index++) {
    const final = index + 1 === holes.length;
    const following = texts[index + 1];
    let pieceEnd = end;
    if (!final) {
      if (following.length === 0) {
        if (position >= end) {
          return false;
        }
        pieceEnd = position + 1;
      } else {
        const found = text.indexOf(following, position);
        if (found < 0 || found + following.length > end) {
          return false;
        }
        pieceEnd = found;
      }
    }
    if (!holeAccepts(holes[index], text.slice(position, pieceEnd))) {
      return false;
    }
    position = final ? pieceEnd : pieceEnd + following.length;
  }
  return true;
}
