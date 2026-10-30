// Engine prelude of checked-f64-utf16-v37 (`regexp`): `RegExp`, its match
// arrays and the regex string methods, implemented in typed guest code with
// no host import and no capability. The compiler adds it to a program only
// when a module names `RegExp`, `RegExpExecArray` or `RegExpMatchArray`,
// writes a regular expression literal or calls `match`, `matchAll`,
// `replace`, `replaceAll`, `search` or `split`; see docs/architecture.md
// (RegExp).
//
// Patterns are compiled ahead of time: the compiler turns every literal (and
// every `new RegExp` with constant arguments) into a matcher program, a
// static string, and constructs the instance through `RegExp.compiled`. The
// `varta.regexp.*` calls are engine-internal guest operations (no host call)
// that exist only in engine prelude modules: `exec` runs the linear-time
// matcher over UTF-16 code units, `slot` reads one capture position of its
// result and `unit` reads one code unit.

const NOT_GLOBAL_MATCH_ALL = 'String.prototype.matchAll called with a non-global RegExp argument';
const NOT_GLOBAL_REPLACE_ALL = 'replaceAll must be called with a global RegExp';
const SPLIT_UNDEFINED =
  'RegExp split capture group did not participate (Varta types split results as string[])';
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
// names for groups 1.., or -1.
function groupIndex(names: string, count: number, name: string): number {
  let start = 0;
  for (let group = 1; group < count; group++) {
    let end = names.indexOf(',', start);
    if (end < 0) {
      end = names.length;
    }
    if (end > start && names.slice(start, end) === name) {
      return group;
    }
    start = end + 1;
  }
  return -1;
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
      groups[names.slice(start, end)] = values[group];
    }
    start = end + 1;
  }
  return groups;
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
        const group = groupIndex(names, count, replacement.slice(dollar + 2, close));
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

export class RegExpExecArray {
  readonly index: number;
  readonly input: string;
  readonly groups: Record<string, string | undefined> | undefined;
  readonly length: number;
  // Element 0, always a string, and every element (the compiler reads
  // `match[0]` from `first` and other elements from `values`).
  private readonly first: string;
  private readonly values: (string | undefined)[];

  constructor(
    index: number,
    input: string,
    values: (string | undefined)[],
    groups: Record<string, string | undefined> | undefined,
  ) {
    this.index = index;
    this.input = input;
    this.groups = groups;
    this.length = values.length;
    this.first = values[0] ?? '';
    this.values = values;
  }
}

export class RegExpMatchArray {
  readonly index: number | undefined;
  readonly input: string | undefined;
  readonly groups: Record<string, string | undefined> | undefined;
  readonly length: number;
  private readonly first: string;
  private readonly values: (string | undefined)[];

  constructor(
    index: number | undefined,
    input: string | undefined,
    values: (string | undefined)[],
    groups: Record<string, string | undefined> | undefined,
  ) {
    this.index = index;
    this.input = input;
    this.groups = groups;
    this.length = values.length;
    this.first = values[0] ?? '';
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

  // Construction is compiler-only: `RegExp.compiled` is the target of every
  // literal and constant `new RegExp`.
  constructor(
    source: string,
    flags: string,
    bits: number,
    program: string,
    lean: string,
    names: string,
    count: number,
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
    this.hasIndices = false;
    this.unicodeSets = false;
    this.program = program;
    this.lean = lean;
    this.names = names;
    this.count = count;
    this.bits = bits;
  }

  private static compiled(
    source: string,
    flags: string,
    bits: number,
    program: string,
    lean: string,
    names: string,
    count: number,
  ): RegExp {
    return new RegExp(source, flags, bits, program, lean, names, count);
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
    );
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
        regexp.lastIndex = advance(input, regexp.lastIndex, regexp.unicode);
      }
    }
    return found.length === 0 ? null : new RegExpMatchArray(undefined, undefined, found, undefined);
  }

  // String.prototype.matchAll: every match of a copy of the regex that
  // starts at the original's `lastIndex`, as a fresh array.
  private static matchAll(input: string, regexp: RegExp): RegExpExecArray[] {
    if (!regexp.global) {
      throw new TypeError(NOT_GLOBAL_MATCH_ALL);
    }
    const copy = new RegExp(
      regexp.source,
      regexp.flags,
      regexp.bits,
      regexp.program,
      regexp.lean,
      regexp.names,
      regexp.count,
    );
    copy.lastIndex = regexp.lastIndex;
    const found: RegExpExecArray[] = [];
    for (;;) {
      const caps = copy.run(input, false);
      if (caps.length === 0) {
        break;
      }
      const start = varta.regexp.slot(caps, 0);
      const values = captures(caps, input, copy.count);
      found.push(new RegExpExecArray(start, input, values, groupsOf(values, copy.names, copy.count)));
      if (varta.regexp.slot(caps, 1) === start) {
        copy.lastIndex = advance(input, copy.lastIndex, copy.unicode);
      }
    }
    return found;
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
        regexp.lastIndex = advance(input, regexp.lastIndex, regexp.unicode);
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
        new RegExpExecArray(position, input, values, groupsOf(values, regexp.names, regexp.count)),
      );
      if (position >= next) {
        result = result + input.slice(next, position) + replaced;
        next = varta.regexp.slot(caps, 1);
      }
    }
    return result + input.slice(next);
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

  // String.prototype.split with a regex separator. Matching resumes from
  // each position like the sticky splitter of @@split; captures are spliced
  // into the result.
  private static split(input: string, regexp: RegExp, limit: number): string[] {
    const lim = limitOf(limit);
    const parts: string[] = [];
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
        q = advance(input, start, regexp.unicode);
        continue;
      }
      parts.push(input.slice(p, start));
      if (parts.length === lim) {
        return parts;
      }
      p = end;
      for (let group = 1; group < regexp.count; group++) {
        const start = varta.regexp.slot(caps, 2 * group);
        if (start < 0) {
          throw new TypeError(SPLIT_UNDEFINED);
        }
        parts.push(input.slice(start, varta.regexp.slot(caps, 2 * group + 1)));
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
