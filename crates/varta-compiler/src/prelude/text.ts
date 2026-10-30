// Engine prelude of checked-f64-utf16-v37 (`collections`): string and array
// methods implemented in typed guest code. The compiler loads this module only
// when a program names one of the methods (`modules::engine::TEXT_METHODS`) or
// `String.fromCharCode`/`String.fromCodePoint`, and calls its functions for
// receivers of the matching type; see docs/architecture.md (Collections).
//
// The `varta.text.*` calls are engine-internal guest operations (no host
// call): `unit` reads one UTF-16 code unit, `fromUnits` builds a string from
// at most MAX_ARRAY_ELEMENTS code units, `indexNonAscii` scans for a unit
// above U+007F and `asciiUppercase` maps a-z. Full Unicode case conversion is
// the pure host operation `varta.text.toUpperCase`/`toLowerCase`.

// Code units per `varta.text.fromUnits` chunk (the array element bound).
const CHUNK = 1024;
const MAX_CODE_POINT = 0x10ffff;
const SURROGATE_OFFSET = 0x10000;
const HIGH_SURROGATE = 0xd800;
const LOW_SURROGATE = 0xdc00;
const SURROGATE_BITS = 0x400;
const UNIT_RANGE = 0x10000;
// The largest array length, 2^32 - 1 (ToUint32 of an omitted split limit).
const MAX_LENGTH = 4294967295;

// Builds a string from code units appended one at a time, in chunks.
class UnitBuffer {
  units: number[];
  text: string;
  constructor() {
    this.units = [];
    this.text = '';
  }
  push(unit: number): void {
    if (this.units.length === CHUNK) {
      this.text += varta.text.fromUnits(this.units);
      this.units = [];
    }
    this.units.push(unit);
  }
  finish(): string {
    return this.text + varta.text.fromUnits(this.units);
  }
}

// ToIntegerOrInfinity of a number.
function integer(value: number): number {
  if (Number.isNaN(value)) {
    return 0;
  }
  return Math.trunc(value) + 0;
}

// ToUint16 of a number.
function uint16(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  const unit = Math.trunc(value) % UNIT_RANGE;
  return unit < 0 ? unit + UNIT_RANGE : unit;
}

// ToUint32 of a number.
function uint32(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  const word = Math.trunc(value) % (MAX_LENGTH + 1);
  return word < 0 ? word + MAX_LENGTH + 1 : word;
}

// The filler of StringPad: `fill` repeated and truncated to `length` units.
function filler(fill: string, length: number): string {
  const repeated = fill.repeat(Math.ceil(length / fill.length));
  return repeated.slice(0, length);
}

export function stringPadStart(value: string, maxLength: number, fillString?: string): string {
  const fill = fillString === undefined ? ' ' : fillString;
  const length = integer(maxLength);
  if (length <= value.length || fill.length === 0) {
    return value;
  }
  return filler(fill, length - value.length) + value;
}

export function stringPadEnd(value: string, maxLength: number, fillString?: string): string {
  const fill = fillString === undefined ? ' ' : fillString;
  const length = integer(maxLength);
  if (length <= value.length || fill.length === 0) {
    return value;
  }
  return value + filler(fill, length - value.length);
}

export function stringToUpperCase(value: string): string {
  if (varta.text.indexNonAscii(value) < 0) {
    return varta.text.asciiUppercase(value);
  }
  return varta.text.toUpperCase(value);
}

export function stringToLowerCase(value: string): string {
  if (varta.text.indexNonAscii(value) < 0) {
    return varta.web.asciiLowercase(value);
  }
  return varta.text.toLowerCase(value);
}

export function stringConcat(value: string, ...strings: string[]): string {
  let result = value;
  for (let index = 0; index < strings.length; index++) {
    result += strings[index]!;
  }
  return result;
}

export function stringCodePointAt(value: string, position?: number): number | undefined {
  const at = position === undefined ? 0 : integer(position);
  if (at < 0 || at >= value.length) {
    return undefined;
  }
  const first = varta.text.unit(value, at);
  if (first < HIGH_SURROGATE || first >= LOW_SURROGATE || at + 1 === value.length) {
    return first;
  }
  const second = varta.text.unit(value, at + 1);
  if (second < LOW_SURROGATE || second >= LOW_SURROGATE + SURROGATE_BITS) {
    return first;
  }
  return (first - HIGH_SURROGATE) * SURROGATE_BITS + (second - LOW_SURROGATE) + SURROGATE_OFFSET;
}

// String.prototype.split with a string separator (a RegExp separator belongs
// to the RegExp stream).
export function stringSplit(value: string, separator?: string, limit?: number): string[] {
  const parts: string[] = [];
  const bound = limit === undefined ? MAX_LENGTH : uint32(limit);
  if (bound === 0) {
    return parts;
  }
  if (separator === undefined) {
    parts.push(value);
    return parts;
  }
  if (value.length === 0) {
    if (separator.length > 0) {
      parts.push(value);
    }
    return parts;
  }
  if (separator.length === 0) {
    for (let index = 0; index < value.length && parts.length < bound; index++) {
      parts.push(value.slice(index, index + 1));
    }
    return parts;
  }
  let start = 0;
  let found = value.indexOf(separator, start);
  while (found >= 0) {
    parts.push(value.slice(start, found));
    if (parts.length === bound) {
      return parts;
    }
    start = found + separator.length;
    found = value.indexOf(separator, start);
  }
  parts.push(value.slice(start));
  return parts;
}

export function stringFromCharCode(...codes: number[]): string {
  const buffer = new UnitBuffer();
  for (let index = 0; index < codes.length; index++) {
    buffer.push(uint16(codes[index]!));
  }
  return buffer.finish();
}

export function stringFromCodePoint(...codePoints: number[]): string {
  const buffer = new UnitBuffer();
  for (let index = 0; index < codePoints.length; index++) {
    const point = codePoints[index]!;
    if (!Number.isInteger(point) || point < 0 || point > MAX_CODE_POINT) {
      throw new RangeError(`Invalid code point ${String(point)}`);
    }
    if (point < SURROGATE_OFFSET) {
      buffer.push(point);
    } else {
      const offset = point - SURROGATE_OFFSET;
      buffer.push(HIGH_SURROGATE + Math.floor(offset / SURROGATE_BITS));
      buffer.push(LOW_SURROGATE + (offset % SURROGATE_BITS));
    }
  }
  return buffer.finish();
}

// Array.prototype.join; `undefined` and `null` elements (and holes) join as
// empty strings, every other element through String.
function joined<T>(value: T): string {
  if (value === undefined || value === null) {
    return '';
  }
  return String(value);
}

export function arrayJoin<T>(array: T[], separator?: string): string {
  const between = separator === undefined ? ',' : separator;
  let result = '';
  for (let index = 0; index < array.length; index++) {
    if (index > 0) {
      result += between;
    }
    result += joined(array[index]);
  }
  return result;
}
