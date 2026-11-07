// Engine prelude of checked-f64-utf16-v38 (`full_regexp`, K9 D3): `RegExp`,
// its match arrays, match indices and the regex string methods, in typed
// guest code with no capability. It replaces the v37 prelude
// (`prelude/regexp.ts`) under v38 and is loaded the same way; see
// docs/architecture.md (Full RegExp).
//
// Literals and constant `new RegExp` calls are compiled ahead of time into
// backtracking programs (format 2) and constructed through `RegExp.compiled`;
// a pattern only known at run time is compiled by `varta.regexp.compile`, the
// linked `regexp_compile` (the same pattern compiler, as guest code metered as
// fuel). `varta.regexp.exec` is the linked `regexp_exec` (guest code metered
// as fuel, no host call), `slot` reads one 32-bit word of a result (a capture
// position) and `unit` one code unit.

const NOT_GLOBAL_MATCH_ALL = 'String.prototype.matchAll called with a non-global RegExp argument';
const NOT_GLOBAL_REPLACE_ALL = 'replaceAll must be called with a global RegExp';
const DOLLAR = 36;
const AMPERSAND = 38;
const BACKTICK = 96;
const QUOTE = 39;
const LESS = 60;
const DIGIT_0 = 48;
const DIGIT_9 = 57;
const DECIMAL = 10;
const HIGH_FIRST = 0xd800;
const HIGH_LAST = 0xdbff;
const LOW_FIRST = 0xdc00;
const LOW_LAST = 0xdfff;
// Flag bits passed by the compiler, in `dgimsuvy` order of the getters.
const GLOBAL = 1;
const IGNORE_CASE = 2;
const MULTILINE = 4;
const DOT_ALL = 8;
const UNICODE = 16;
const STICKY = 32;
const HAS_INDICES = 64;
const UNICODE_SETS = 128;
// The construction record of `varta.regexp.compile`
// (`varta_common::regexp::construction`): words, then texts from TEXT_UNIT.
const RECORD_STATUS = 0;
const RECORD_OK = 0;
const RECORD_MESSAGE_UNIT = 2;
const RECORD_BITS = 1;
const RECORD_COUNT = 2;
const RECORD_PROGRAM_UNITS = 3;
const RECORD_LEAN_UNITS = 4;
const RECORD_NAMES_UNITS = 5;
const RECORD_SOURCE_UNITS = 6;
const RECORD_FLAGS_UNITS = 7;
const RECORD_TEXT_UNIT = 16;
// RegExp.escape (ES2025 §22.2.5.1).
const HEX_DIGITS = '0123456789abcdef';
const HEX = 16;
const BYTE_LAST = 0xff;
const SUPPLEMENTARY = 0x10000;
const SURROGATE_BITS = 1024;
// SyntaxCharacter and `/`, escaped with a backslash.
const ESCAPE_SYNTAX = '^$\\.*+?()[]{}|/';
// ControlEscape: TAB, LF, VT, FF and CR as `\t`, `\n`, `\v`, `\f`, `\r`.
const ESCAPE_CONTROLS = '\t\n\v\f\r';
const ESCAPE_LETTERS = 'tnvfr';
// otherPunctuators, escaped as hexadecimal.
const ESCAPE_PUNCTUATORS = ',-=<>#&!%:;@~\'`"';
const ZWNBSP = 0xfeff;
// WhiteSpace beyond the controls (USP, the Zs characters) and the line
// terminators LS and PS.
const TAB = 0x09;
const LF = 0x0a;
const VT = 0x0b;
const FF = 0x0c;
const CR = 0x0d;
const SPACE = 0x20;
const NBSP = 0xa0;
const OGHAM_SPACE = 0x1680;
const EN_QUAD = 0x2000;
const HAIR_SPACE = 0x200a;
const LINE_SEPARATOR = 0x2028;
const PARAGRAPH_SEPARATOR = 0x2029;
const NARROW_NBSP = 0x202f;
const MATH_SPACE = 0x205f;
const IDEOGRAPHIC_SPACE = 0x3000;
const ASCII_END = 0x80;
const DIGIT_FIRST = 0x30;
const DIGIT_LAST = 0x39;
const UPPER_FIRST = 0x41;
const UPPER_LAST = 0x5a;
const LOWER_FIRST = 0x61;
const LOWER_LAST = 0x7a;
const BYTE_DIGITS = 2;
const UNIT_DIGITS = 4;

// `digits` lowercase hexadecimal digits of `value`.
function hex(value: number, digits: number): string {
  let text = '';
  let rest = value;
  for (let index = 0; index < digits; index++) {
    const digit = rest % HEX;
    text = HEX_DIGITS.slice(digit, digit + 1) + text;
    rest = (rest - digit) / HEX;
  }
  return text;
}

// WhiteSpace or LineTerminator (§12.2, §12.3).
function space(code: number): boolean {
  return (
    code === TAB ||
    code === LF ||
    code === VT ||
    code === FF ||
    code === CR ||
    code === SPACE ||
    code === NBSP ||
    code === OGHAM_SPACE ||
    (code >= EN_QUAD && code <= HAIR_SPACE) ||
    code === LINE_SEPARATOR ||
    code === PARAGRAPH_SEPARATOR ||
    code === NARROW_NBSP ||
    code === MATH_SPACE ||
    code === IDEOGRAPHIC_SPACE ||
    code === ZWNBSP
  );
}

// EncodeForRegExpEscape (§22.2.5.1.1) of the code point `code`, spelled `text`.
function escapeCodePoint(code: number, text: string): string {
  if (code < ASCII_END && ESCAPE_SYNTAX.indexOf(text) >= 0) {
    return '\\' + text;
  }
  const control = code < ASCII_END ? ESCAPE_CONTROLS.indexOf(text) : -1;
  if (control >= 0) {
    return '\\' + ESCAPE_LETTERS.slice(control, control + 1);
  }
  if (
    (code < ASCII_END && ESCAPE_PUNCTUATORS.indexOf(text) >= 0) ||
    space(code) ||
    (code >= HIGH_FIRST && code <= LOW_LAST)
  ) {
    // Every such code point is in the BMP: one code unit.
    return code <= BYTE_LAST ? '\\x' + hex(code, BYTE_DIGITS) : '\\u' + hex(code, UNIT_DIGITS);
  }
  return text;
}

// AdvanceStringIndex.
function advance(input: string, index: number, unicode: boolean): number {
  if (!unicode || index + 1 >= input.length) {
    return index + 1;
  }
  const high = varta.regexp.unit(input, index);
  if (high < HIGH_FIRST || high > HIGH_LAST) {
    return index + 1;
  }
  const low = varta.regexp.unit(input, index + 1);
  return low >= LOW_FIRST && low <= LOW_LAST ? index + 2 : index + 1;
}

// Capture `group` of a matcher result, or undefined when it did not take part.
function capture(caps: string, input: string, group: number): string | undefined {
  const start = varta.regexp.slot(caps, 2 * group);
  return start < 0 ? undefined : input.slice(start, varta.regexp.slot(caps, 2 * group + 1));
}

// The group index named `name` in the compiler's comma-separated list of
// names for groups 1.., or -1; of several groups sharing the name, the one
// that took part in `caps`.
function groupIndex(names: string, count: number, name: string, caps: string): number {
  let start = 0;
  let found = -1;
  for (let group = 1; group < count; group++) {
    let end = names.indexOf(',', start);
    if (end < 0) {
      end = names.length;
    }
    if (end > start && names.slice(start, end) === name) {
      if (varta.regexp.slot(caps, 2 * group) >= 0) {
        return group;
      }
      if (found < 0) {
        found = group;
      }
    }
    start = end + 1;
  }
  return found;
}

function captures(caps: string, input: string, count: number): (string | undefined)[] {
  const values: (string | undefined)[] = [];
  for (let group = 0; group < count; group++) {
    values.push(capture(caps, input, group));
  }
  return values;
}

// The groups object: every named group in index order, undefined when the
// group did not take part.
function groupsOf(
  values: (string | undefined)[],
  names: string,
  count: number,
): Record<string, string | undefined> | undefined {
  if (names.length === 0) {
    return undefined;
  }
  const groups: Record<string, string | undefined> = {};
  let start = 0;
  for (let group = 1; group < count; group++) {
    let end = names.indexOf(',', start);
    if (end < 0) {
      end = names.length;
    }
    if (end > start) {
      // A name shared by groups in different alternatives takes the one
      // that took part; its key keeps the first group's position.
      const name = names.slice(start, end);
      const value = values[group];
      if (value !== undefined || groups[name] === undefined) {
        groups[name] = value;
      }
    }
    start = end + 1;
  }
  return groups;
}

// The capture positions of a match with the `d` flag (MakeMatchIndicesIndexPairArray).
function indicesOf(caps: string, names: string, count: number): RegExpIndicesArray {
  const values: ([number, number] | undefined)[] = [];
  for (let group = 0; group < count; group++) {
    const start = varta.regexp.slot(caps, 2 * group);
    values.push(start < 0 ? undefined : [start, varta.regexp.slot(caps, 2 * group + 1)]);
  }
  let groups: Record<string, [number, number] | undefined> | undefined = undefined;
  if (names.length > 0) {
    const named: Record<string, [number, number] | undefined> = {};
    let start = 0;
    for (let group = 1; group < count; group++) {
      let end = names.indexOf(',', start);
      if (end < 0) {
        end = names.length;
      }
      if (end > start) {
        const name = names.slice(start, end);
        const value = values[group];
        if (value !== undefined || named[name] === undefined) {
          named[name] = value;
        }
      }
      start = end + 1;
    }
    groups = named;
  }
  return new RegExpIndicesArray(values, groups);
}

// GetSubstitution over a matcher result.
function substitute(
  replacement: string,
  caps: string,
  input: string,
  count: number,
  names: string,
): string {
  const position = varta.regexp.slot(caps, 0);
  const end = varta.regexp.slot(caps, 1);
  let result = '';
  let from = 0;
  for (;;) {
    const dollar = replacement.indexOf('$', from);
    if (dollar < 0) {
      break;
    }
    result = result + replacement.slice(from, dollar);
    const next = varta.regexp.unit(replacement, dollar + 1);
    from = dollar + 2;
    if (next === DOLLAR) {
      result = result + '$';
    } else if (next === AMPERSAND) {
      result = result + input.slice(position, end);
    } else if (next === BACKTICK) {
      result = result + input.slice(0, position);
    } else if (next === QUOTE) {
      result = result + input.slice(end);
    } else if (next >= DIGIT_0 && next <= DIGIT_9) {
      const first = next - DIGIT_0;
      const second = varta.regexp.unit(replacement, dollar + 2);
      const both = first * DECIMAL + second - DIGIT_0;
      if (second >= DIGIT_0 && second <= DIGIT_9 && both >= 1 && both < count) {
        result = result + (capture(caps, input, both) ?? '');
        from = dollar + 3;
      } else if (first >= 1 && first < count) {
        result = result + (capture(caps, input, first) ?? '');
      } else {
        result = result + '$';
        from = dollar + 1;
      }
    } else if (next === LESS && names.length > 0) {
      const close = replacement.indexOf('>', dollar + 2);
      if (close < 0) {
        result = result + '$';
        from = dollar + 1;
      } else {
        const group = groupIndex(names, count, replacement.slice(dollar + 2, close), caps);
        if (group > 0) {
          result = result + (capture(caps, input, group) ?? '');
        }
        from = close + 1;
      }
    } else {
      result = result + '$';
      from = dollar + 1;
    }
  }
  return result + replacement.slice(from);
}

// GetSubstitution for a string pattern (no captures).
function substituteText(replacement: string, input: string, position: number, matched: string): string {
  if (replacement.indexOf('$') < 0) {
    return replacement;
  }
  let result = '';
  let from = 0;
  for (;;) {
    const dollar = replacement.indexOf('$', from);
    if (dollar < 0) {
      break;
    }
    result = result + replacement.slice(from, dollar);
    const next = varta.regexp.unit(replacement, dollar + 1);
    from = dollar + 2;
    if (next === DOLLAR) {
      result = result + '$';
    } else if (next === AMPERSAND) {
      result = result + matched;
    } else if (next === BACKTICK) {
      result = result + input.slice(0, position);
    } else if (next === QUOTE) {
      result = result + input.slice(position + matched.length);
    } else {
      result = result + '$';
      from = dollar + 1;
    }
  }
  return result + replacement.slice(from);
}

// ToUint32 of a split limit.
function limitOf(limit: number): number {
  return limit >>> 0;
}

// Match arrays are arrays (ES2025 §22.2.7.2): the compiler reads and writes
// their elements, `length` and every Array method on `values`, the elements
// array, so `index`, `input`, `groups` and `indices` are their own data
// properties. Element 0 of a match always took part.

// The match indices of a `d` match: every element is a `[start, end]` pair
// (`undefined` for a group that did not take part).
export class RegExpIndicesArray {
  groups: Record<string, [number, number] | undefined> | undefined;
  private readonly values: ([number, number] | undefined)[];

  constructor(
    values: ([number, number] | undefined)[],
    groups: Record<string, [number, number] | undefined> | undefined,
  ) {
    this.groups = groups;
    this.values = values;
  }
}

export class RegExpExecArray {
  index: number;
  input: string;
  groups: Record<string, string | undefined> | undefined;
  indices: RegExpIndicesArray | undefined;
  private readonly values: (string | undefined)[];

  constructor(
    index: number,
    input: string,
    values: (string | undefined)[],
    groups: Record<string, string | undefined> | undefined,
    indices: RegExpIndicesArray | undefined,
  ) {
    this.index = index;
    this.input = input;
    this.groups = groups;
    this.indices = indices;
    this.values = values;
  }
}

export class RegExpMatchArray {
  index: number | undefined;
  input: string | undefined;
  groups: Record<string, string | undefined> | undefined;
  indices: RegExpIndicesArray | undefined;
  private readonly values: (string | undefined)[];

  constructor(
    index: number | undefined,
    input: string | undefined,
    values: (string | undefined)[],
    groups: Record<string, string | undefined> | undefined,
    indices: RegExpIndicesArray | undefined,
  ) {
    this.index = index;
    this.input = input;
    this.groups = groups;
    this.indices = indices;
    this.values = values;
  }
}

export class RegExp {
  lastIndex: number;
  readonly source: string;
  readonly flags: string;
  readonly global: boolean;
  readonly ignoreCase: boolean;
  readonly multiline: boolean;
  readonly dotAll: boolean;
  readonly unicode: boolean;
  readonly sticky: boolean;
  readonly hasIndices: boolean;
  readonly unicodeSets: boolean;
  // The compiled matcher program, the names of groups 1.. (comma-separated,
  // empty for unnamed groups; empty without named groups) and the number of
  // captures including the whole match.
  private readonly program: string;
  // The same pattern without group captures: the match bounds only.
  private readonly lean: string;
  private readonly names: string;
  private readonly count: number;
  private readonly bits: number;
  // [[OriginalSource]]: the pattern as written (`source` escapes it).
  private readonly pattern: string;

  // Construction is compiler-only: `RegExp.compiled` is the target of every
  // literal and constant `new RegExp`, `RegExp.create` and `RegExp.call` of
  // every other `new RegExp(…)` and `RegExp(…)`.
  constructor(
    source: string,
    flags: string,
    bits: number,
    program: string,
    lean: string,
    names: string,
    count: number,
    pattern: string,
  ) {
    this.lastIndex = 0;
    this.source = source;
    this.flags = flags;
    this.global = (bits & GLOBAL) !== 0;
    this.ignoreCase = (bits & IGNORE_CASE) !== 0;
    this.multiline = (bits & MULTILINE) !== 0;
    this.dotAll = (bits & DOT_ALL) !== 0;
    this.unicode = (bits & UNICODE) !== 0;
    this.sticky = (bits & STICKY) !== 0;
    this.hasIndices = (bits & HAS_INDICES) !== 0;
    this.unicodeSets = (bits & UNICODE_SETS) !== 0;
    this.program = program;
    this.lean = lean;
    this.names = names;
    this.count = count;
    this.bits = bits;
    this.pattern = pattern;
  }

  private static compiled(
    source: string,
    flags: string,
    bits: number,
    program: string,
    lean: string,
    names: string,
    count: number,
    pattern: string,
  ): RegExp {
    return new RegExp(source, flags, bits, program, lean, names, count, pattern);
  }

  // RegExpInitialize at run time (§22.2.3.3): the linked pattern compiler,
  // or V8's SyntaxError.
  private static initialize(pattern: string, flags: string): RegExp {
    const record = varta.regexp.compile(pattern, flags);
    if (varta.regexp.slot(record, RECORD_STATUS) !== RECORD_OK) {
      throw new SyntaxError(record.slice(RECORD_MESSAGE_UNIT));
    }
    let at = RECORD_TEXT_UNIT;
    const parts: string[] = [];
    for (const word of [
      RECORD_PROGRAM_UNITS,
      RECORD_LEAN_UNITS,
      RECORD_NAMES_UNITS,
      RECORD_SOURCE_UNITS,
      RECORD_FLAGS_UNITS,
    ]) {
      const end = at + varta.regexp.slot(record, word);
      parts.push(record.slice(at, end));
      at = end;
    }
    return new RegExp(
      parts[3]!,
      parts[4]!,
      varta.regexp.slot(record, RECORD_BITS),
      parts[0]!,
      parts[1]!,
      parts[2]!,
      varta.regexp.slot(record, RECORD_COUNT),
      pattern,
    );
  }

  // A new RegExp with the same pattern and flags (`lastIndex` 0).
  private copy(): RegExp {
    return new RegExp(
      this.source,
      this.flags,
      this.bits,
      this.program,
      this.lean,
      this.names,
      this.count,
      this.pattern,
    );
  }

  // `new RegExp(pattern, flags)` (§22.2.4.1) with arguments only known at
  // run time: a RegExp pattern keeps its [[OriginalSource]] and, without
  // flags, its [[OriginalFlags]].
  private static create(pattern: string | RegExp, flags: string | undefined): RegExp {
    if (pattern instanceof RegExp) {
      return flags === undefined ? pattern.copy() : RegExp.initialize(pattern.pattern, flags);
    }
    return RegExp.initialize(pattern, flags ?? '');
  }

  // `RegExp(pattern, flags)` called as a function: a RegExp pattern without
  // flags is returned itself (its constructor is `RegExp`).
  private static call(pattern: string | RegExp, flags: string | undefined): RegExp {
    if (pattern instanceof RegExp && flags === undefined) {
      return pattern;
    }
    return RegExp.create(pattern, flags);
  }

  // RegExp.escape (ES2025 §22.2.5.1): `text` with every character that
  // could be pattern syntax escaped; a leading digit or ASCII letter as
  // `\x` hexadecimal.
  static escape(text: string): string {
    let escaped = '';
    let index = 0;
    while (index < text.length) {
      const unit = varta.regexp.unit(text, index);
      let code = unit;
      let width = 1;
      if (unit >= HIGH_FIRST && unit <= HIGH_LAST && index + 1 < text.length) {
        const low = varta.regexp.unit(text, index + 1);
        if (low >= LOW_FIRST && low <= LOW_LAST) {
          code = (unit - HIGH_FIRST) * SURROGATE_BITS + (low - LOW_FIRST) + SUPPLEMENTARY;
          width = 2;
        }
      }
      const character = text.slice(index, index + width);
      if (
        index === 0 &&
        ((code >= DIGIT_FIRST && code <= DIGIT_LAST) ||
          (code >= UPPER_FIRST && code <= UPPER_LAST) ||
          (code >= LOWER_FIRST && code <= LOWER_LAST))
      ) {
        escaped = '\\x' + hex(code, BYTE_DIGITS);
      } else {
        escaped = escaped + escapeCodePoint(code, character);
      }
      index = index + width;
    }
    return escaped;
  }

  // RegExpBuiltinExec up to the match: the matcher result (empty without a
  // match), with `lastIndex` read and written as the global and sticky
  // flags require. A `bounds` run uses the lean program: slots 0 and 1 only.
  private run(input: string, bounds: boolean): string {
    const advancing = this.global || this.sticky;
    let start = 0;
    if (advancing) {
      start = this.lastIndex > 0 ? Math.floor(this.lastIndex) : 0;
      if (start > input.length) {
        this.lastIndex = 0;
        return '';
      }
    }
    const caps = varta.regexp.exec(bounds ? this.lean : this.program, input, start, this.sticky);
    if (advancing) {
      this.lastIndex = caps.length === 0 ? 0 : varta.regexp.slot(caps, 1);
    }
    return caps;
  }

  exec(input: string): RegExpExecArray | null {
    const caps = this.run(input, false);
    if (caps.length === 0) {
      return null;
    }
    const values = captures(caps, input, this.count);
    return new RegExpExecArray(
      varta.regexp.slot(caps, 0),
      input,
      values,
      groupsOf(values, this.names, this.count),
      this.indicesFor(caps),
    );
  }

  // fullUnicode of AdvanceStringIndex: the `u` or `v` flag.
  private full(): boolean {
    return this.unicode || this.unicodeSets;
  }

  // The match indices of a match, with the `d` flag.
  private indicesFor(caps: string): RegExpIndicesArray | undefined {
    return this.hasIndices ? indicesOf(caps, this.names, this.count) : undefined;
  }

  test(input: string): boolean {
    return this.run(input, true).length !== 0;
  }

  // String.prototype.search: from 0 with `lastIndex` unchanged.
  private static search(input: string, regexp: RegExp): number {
    const caps = varta.regexp.exec(regexp.lean, input, 0, regexp.sticky);
    return caps.length === 0 ? -1 : varta.regexp.slot(caps, 0);
  }

  // String.prototype.match.
  private static match(input: string, regexp: RegExp): RegExpMatchArray | null {
    if (!regexp.global) {
      const caps = regexp.run(input, false);
      if (caps.length === 0) {
        return null;
      }
      const values = captures(caps, input, regexp.count);
      return new RegExpMatchArray(
        varta.regexp.slot(caps, 0),
        input,
        values,
        groupsOf(values, regexp.names, regexp.count),
        regexp.indicesFor(caps),
      );
    }
    regexp.lastIndex = 0;
    const found: (string | undefined)[] = [];
    for (;;) {
      const caps = regexp.run(input, true);
      if (caps.length === 0) {
        break;
      }
      const start = varta.regexp.slot(caps, 0);
      const end = varta.regexp.slot(caps, 1);
      found.push(input.slice(start, end));
      if (end === start) {
        regexp.lastIndex = advance(input, regexp.lastIndex, regexp.full());
      }
    }
    return found.length === 0 ? null : new RegExpMatchArray(undefined, undefined, found, undefined, undefined);
  }

  // String.prototype.matchAll (§22.1.3.14, §22.2.6.9): a RegExp String
  // Iterator over a copy of the regex that starts at the original's
  // `lastIndex`; each step runs one match.
  private static matchAll(input: string, regexp: RegExp): Generator<RegExpExecArray, void, undefined> {
    if (!regexp.global) {
      throw new TypeError(NOT_GLOBAL_MATCH_ALL);
    }
    const copy = regexp.copy();
    copy.lastIndex = regexp.lastIndex;
    return RegExp.iterate(copy, input);
  }

  // %RegExpStringIteratorPrototype%.next (§22.2.9.2.1) of a global regex.
  private static *iterate(regexp: RegExp, input: string): Generator<RegExpExecArray, void, undefined> {
    for (;;) {
      const match = regexp.exec(input);
      if (match === null) {
        return;
      }
      if (match.index === regexp.lastIndex) {
        regexp.lastIndex = advance(input, regexp.lastIndex, regexp.full());
      }
      yield match;
    }
  }

  // The matcher results String.prototype.replace replaces: one, or all
  // (from 0) for a global regex; `bounds` when no capture is needed.
  private static matches(input: string, regexp: RegExp, bounds: boolean): string[] {
    const found: string[] = [];
    if (regexp.global) {
      regexp.lastIndex = 0;
    }
    for (;;) {
      const caps = regexp.run(input, bounds);
      if (caps.length === 0) {
        break;
      }
      found.push(caps);
      if (!regexp.global) {
        break;
      }
      if (varta.regexp.slot(caps, 1) === varta.regexp.slot(caps, 0)) {
        regexp.lastIndex = advance(input, regexp.lastIndex, regexp.full());
      }
    }
    return found;
  }

  // String.prototype.replace with a replacement template.
  private static replace(input: string, regexp: RegExp, replacement: string): string {
    const plain = replacement.indexOf('$') < 0;
    const found = RegExp.matches(input, regexp, plain || regexp.count === 1);
    if (found.length === 0) {
      return input;
    }
    let result = '';
    let next = 0;
    for (let index = 0; index < found.length; index++) {
      const caps = found[index]!;
      const position = varta.regexp.slot(caps, 0);
      if (position >= next) {
        result =
          result +
          input.slice(next, position) +
          (plain ? replacement : substitute(replacement, caps, input, regexp.count, regexp.names));
        next = varta.regexp.slot(caps, 1);
      }
    }
    return result + input.slice(next);
  }

  // String.prototype.replace with a replacer called with the match array;
  // the compiler adapts the program's callback to this shape.
  private static replaceWith(
    input: string,
    regexp: RegExp,
    replacer: (match: RegExpExecArray) => string,
  ): string {
    const found = RegExp.matches(input, regexp, false);
    if (found.length === 0) {
      return input;
    }
    let result = '';
    let next = 0;
    for (let index = 0; index < found.length; index++) {
      const caps = found[index]!;
      const position = varta.regexp.slot(caps, 0);
      const values = captures(caps, input, regexp.count);
      const replaced = replacer(
        new RegExpExecArray(
          position,
          input,
          values,
          groupsOf(values, regexp.names, regexp.count),
          regexp.indicesFor(caps),
        ),
      );
      if (position >= next) {
        result = result + input.slice(next, position) + replaced;
        next = varta.regexp.slot(caps, 1);
      }
    }
    return result + input.slice(next);
  }

  // String.prototype.replace with a replacer that takes the arguments
  // computed at run time (§22.2.6.11 step 14.k): the match, every capture,
  // the position, the input and, with named groups, the groups object.
  private static replaceCall(
    input: string,
    regexp: RegExp,
    replacer: (args: unknown[]) => string,
  ): string {
    return RegExp.replaceWith(input, regexp, (match: RegExpExecArray): string => {
      const args: unknown[] = [];
      for (const value of match) {
        args.push(value);
      }
      args.push(match.index);
      args.push(input);
      if (regexp.names.length > 0) {
        args.push(match.groups);
      }
      return replacer(args);
    });
  }

  private static replaceAllCall(
    input: string,
    regexp: RegExp,
    replacer: (args: unknown[]) => string,
  ): string {
    if (!regexp.global) {
      throw new TypeError(NOT_GLOBAL_REPLACE_ALL);
    }
    return RegExp.replaceCall(input, regexp, replacer);
  }

  private static replaceAll(input: string, regexp: RegExp, replacement: string): string {
    if (!regexp.global) {
      throw new TypeError(NOT_GLOBAL_REPLACE_ALL);
    }
    return RegExp.replace(input, regexp, replacement);
  }

  private static replaceAllWith(
    input: string,
    regexp: RegExp,
    replacer: (match: RegExpExecArray) => string,
  ): string {
    if (!regexp.global) {
      throw new TypeError(NOT_GLOBAL_REPLACE_ALL);
    }
    return RegExp.replaceWith(input, regexp, replacer);
  }

  // String.prototype.split with a regex separator whose groups always take
  // part (the compiler selects it): every part is a string.
  private static split(input: string, regexp: RegExp, limit: number): string[] {
    const parts: string[] = [];
    for (const part of RegExp.splitParts(input, regexp, limit)) {
      parts.push(part ?? '');
    }
    return parts;
  }

  // String.prototype.split with a regex separator (§22.2.6.14). Matching
  // resumes from each position like the sticky splitter of @@split; the
  // captures are spliced into the result, `undefined` for a group that did
  // not take part.
  private static splitParts(input: string, regexp: RegExp, limit: number): (string | undefined)[] {
    const lim = limitOf(limit);
    const parts: (string | undefined)[] = [];
    if (lim === 0) {
      return parts;
    }
    const size = input.length;
    if (size === 0) {
      if (varta.regexp.exec(regexp.lean, input, 0, true).length === 0) {
        parts.push(input);
      }
      return parts;
    }
    let p = 0;
    let q = 0;
    while (q < size) {
      const caps = varta.regexp.exec(
        regexp.count === 1 ? regexp.lean : regexp.program,
        input,
        q,
        false,
      );
      if (caps.length === 0) {
        break;
      }
      const start = varta.regexp.slot(caps, 0);
      if (start >= size) {
        break;
      }
      const end = Math.min(varta.regexp.slot(caps, 1), size);
      if (end === p) {
        q = advance(input, start, regexp.full());
        continue;
      }
      parts.push(input.slice(p, start));
      if (parts.length === lim) {
        return parts;
      }
      p = end;
      for (let group = 1; group < regexp.count; group++) {
        parts.push(capture(caps, input, group));
        if (parts.length === lim) {
          return parts;
        }
      }
      q = p;
    }
    parts.push(input.slice(p));
    return parts;
  }

  // String.prototype.replace and replaceAll with a string pattern.
  private static replaceText(input: string, pattern: string, replacement: string, all: boolean): string {
    let position = input.indexOf(pattern);
    if (position < 0) {
      return input;
    }
    const step = pattern.length > 0 ? pattern.length : 1;
    let result = '';
    let next = 0;
    while (position >= 0) {
      result = result + input.slice(next, position) + substituteText(replacement, input, position, pattern);
      next = position + pattern.length;
      if (!all) {
        break;
      }
      position = position + step > input.length ? -1 : input.indexOf(pattern, position + step);
    }
    return result + input.slice(next);
  }

  private static replaceTextWith(
    input: string,
    pattern: string,
    replacer: (match: string, offset: number, input: string) => string,
    all: boolean,
  ): string {
    let position = input.indexOf(pattern);
    if (position < 0) {
      return input;
    }
    const step = pattern.length > 0 ? pattern.length : 1;
    const positions: number[] = [];
    while (position >= 0) {
      positions.push(position);
      if (!all) {
        break;
      }
      position = position + step > input.length ? -1 : input.indexOf(pattern, position + step);
    }
    let result = '';
    let next = 0;
    for (let index = 0; index < positions.length; index++) {
      const at = positions[index]!;
      result = result + input.slice(next, at) + replacer(pattern, at, input);
      next = at + pattern.length;
    }
    return result + input.slice(next);
  }
}
