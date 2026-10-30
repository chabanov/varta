// Engine prelude of checked-f64-utf16-v37 (`collections`): `Uint8Array`
// operations, `TextEncoder`/`TextDecoder` (UTF-8 only) and
// `crypto.getRandomValues`/`crypto.randomUUID`, implemented in typed guest
// code. The compiler loads it when a program names `Uint8Array`,
// `TextEncoder`, `TextDecoder` or `crypto.*`; see docs/architecture.md
// (Collections).
//
// A `Uint8Array` is a fixed-length dense `number[]` of byte values, at most
// MAX_LENGTH elements (the value-layout array bound). `varta.bytes.view`
// and `varta.bytes.adopt` reinterpret between the two without a copy; every
// array this module adopts holds integers in 0..=255. Random bytes come from
// the `Math.random` entropy path (fresh OS entropy, no capability).

const MAX_LENGTH = 1024;
const BYTE = 256;
const MAX_BYTE = 0xff;
// Random bits taken from one Math.random sample (it carries 53).
const RANDOM_BYTES = 6;
const CHUNK = 1024;
const REPLACEMENT = 0xfffd;
const BOM = 0xfeff;
const HIGH_SURROGATE = 0xd800;
const LOW_SURROGATE = 0xdc00;
const SURROGATE_END = 0xe000;
const SURROGATE_BITS = 0x400;
const SUPPLEMENTARY = 0x10000;
const CONTINUATION = 0x80;
const CONTINUATION_MASK = 0x3f;
const CONTINUATION_BITS = 64;
const TWO_BYTE = 0x800;
const LEAD_TWO = 0xc0;
const LEAD_THREE = 0xe0;
const LEAD_FOUR = 0xf0;

function lengthError(length: number): RangeError {
  return new RangeError(`Invalid typed array length: ${String(length)}`);
}

// A `number[]` of `length` zero bytes.
function zeros(length: number): number[] {
  if (!Number.isInteger(length) || length < 0) {
    throw lengthError(length);
  }
  if (length > MAX_LENGTH) {
    throw new RangeError(
      `Uint8Array length ${String(length)} exceeds the value-layout bound of ${String(MAX_LENGTH)} elements`,
    );
  }
  const bytes: number[] = [];
  for (let at = 0; at < length; at++) {
    bytes.push(0);
  }
  return bytes;
}

// ToUint8 of a number.
function uint8(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  const byte = Math.trunc(value) % BYTE;
  return byte < 0 ? byte + BYTE : byte;
}

// A relative index (slice/fill/at arguments) resolved against `length`.
function relative(value: number | undefined, length: number, fallback: number): number {
  if (value === undefined) {
    return fallback;
  }
  const at = Number.isNaN(value) ? 0 : Math.trunc(value);
  if (at < 0) {
    return Math.max(length + at, 0);
  }
  return Math.min(at, length);
}

export function bytesOfLength(length: number): number[] {
  return zeros(length);
}

export function bytesFromArray(values: number[]): number[] {
  const bytes = zeros(values.length);
  for (let at = 0; at < values.length; at++) {
    const value: number | undefined = values[at];
    bytes[at] = value === undefined ? 0 : uint8(value);
  }
  return bytes;
}

export function bytesCopy(source: number[]): number[] {
  const bytes = zeros(source.length);
  for (let at = 0; at < source.length; at++) {
    bytes[at] = source[at]!;
  }
  return bytes;
}

export function bytesToArray(bytes: number[]): number[] {
  return bytesCopy(bytes);
}

export function bytesStore(bytes: number[], index: number, value: number): number {
  if (Number.isInteger(index) && index >= 0 && index < bytes.length) {
    bytes[index] = uint8(value);
  }
  return value;
}

export function bytesSet(bytes: number[], source: number[], offset?: number): void {
  const start = offset === undefined ? 0 : Math.trunc(offset);
  if (!(start >= 0) || start + source.length > bytes.length) {
    throw new RangeError('offset is out of bounds');
  }
  for (let at = 0; at < source.length; at++) {
    const value: number | undefined = source[at];
    bytes[start + at] = value === undefined ? 0 : uint8(value);
  }
}

export function bytesSlice(bytes: number[], start?: number, end?: number): number[] {
  const from = relative(start, bytes.length, 0);
  const to = relative(end, bytes.length, bytes.length);
  const copy = zeros(Math.max(to - from, 0));
  for (let at = from; at < to; at++) {
    copy[at - from] = bytes[at]!;
  }
  return copy;
}

export function bytesFill(bytes: number[], value: number, start?: number, end?: number): number[] {
  const byte = uint8(value);
  const to = relative(end, bytes.length, bytes.length);
  for (let at = relative(start, bytes.length, 0); at < to; at++) {
    bytes[at] = byte;
  }
  return bytes;
}

export function bytesIndexOf(bytes: number[], value: number, fromIndex?: number): number {
  for (let at = relative(fromIndex, bytes.length, 0); at < bytes.length; at++) {
    if (bytes[at]! === value) {
      return at;
    }
  }
  return -1;
}

export function bytesIncludes(bytes: number[], value: number, fromIndex?: number): boolean {
  return bytesIndexOf(bytes, value, fromIndex) >= 0;
}

export function bytesAt(bytes: number[], index: number): number | undefined {
  const at = Number.isNaN(index) ? 0 : Math.trunc(index);
  return bytes[at < 0 ? bytes.length + at : at];
}

export function bytesJoin(bytes: number[], separator?: string): string {
  const between = separator === undefined ? ',' : separator;
  let text = '';
  for (let at = 0; at < bytes.length; at++) {
    if (at > 0) {
      text += between;
    }
    text += String(bytes[at]!);
  }
  return text;
}

// Builds a string from UTF-16 code units in chunks of at most CHUNK units.
class Units {
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
  point(codePoint: number): void {
    if (codePoint < SUPPLEMENTARY) {
      this.push(codePoint);
      return;
    }
    const offset = codePoint - SUPPLEMENTARY;
    this.push(HIGH_SURROGATE + Math.floor(offset / SURROGATE_BITS));
    this.push(LOW_SURROGATE + (offset % SURROGATE_BITS));
  }
  finish(): string {
    return this.text + varta.text.fromUnits(this.units);
  }
}

// Append the UTF-8 encoding of `codePoint` to `bytes`.
function encodePoint(bytes: number[], codePoint: number): void {
  const count =
    codePoint < CONTINUATION ? 1 : codePoint < TWO_BYTE ? 2 : codePoint < SUPPLEMENTARY ? 3 : 4;
  if (bytes.length + count > MAX_LENGTH) {
    throw new RangeError(
      `TextEncoder.encode output exceeds the Uint8Array bound of ${String(MAX_LENGTH)} bytes`,
    );
  }
  if (count === 1) {
    bytes.push(codePoint);
    return;
  }
  const lead = count === 2 ? LEAD_TWO : count === 3 ? LEAD_THREE : LEAD_FOUR;
  let scale = 1;
  for (let at = 1; at < count; at++) {
    scale *= CONTINUATION_BITS;
  }
  bytes.push(lead + Math.floor(codePoint / scale));
  for (let at = 1; at < count; at++) {
    scale /= CONTINUATION_BITS;
    bytes.push(CONTINUATION + (Math.floor(codePoint / scale) % CONTINUATION_BITS));
  }
}

export class TextEncoder {
  readonly encoding: string;
  constructor() {
    this.encoding = 'utf-8';
  }
  // UTF-8 bytes of `input`; a lone surrogate encodes as U+FFFD.
  encode(input?: string): Uint8Array {
    const text = input === undefined ? '' : input;
    const bytes: number[] = [];
    for (let at = 0; at < text.length; at++) {
      const unit = varta.text.unit(text, at);
      if (unit >= HIGH_SURROGATE && unit < LOW_SURROGATE && at + 1 < text.length) {
        const next = varta.text.unit(text, at + 1);
        if (next >= LOW_SURROGATE && next < SURROGATE_END) {
          encodePoint(
            bytes,
            (unit - HIGH_SURROGATE) * SURROGATE_BITS + (next - LOW_SURROGATE) + SUPPLEMENTARY,
          );
          at++;
          continue;
        }
      }
      encodePoint(bytes, unit >= HIGH_SURROGATE && unit < SURROGATE_END ? REPLACEMENT : unit);
    }
    return varta.bytes.adopt(bytes);
  }
}

export interface TextDecoderOptions {
  fatal?: boolean;
  ignoreBOM?: boolean;
}
export interface TextDecodeOptions {
  stream?: boolean;
}

const UTF8_LABELS = ['unicode-1-1-utf-8', 'unicode11utf8', 'unicode20utf8', 'utf-8', 'utf8', 'x-unicode20utf8'];

// The WHATWG UTF-8 decoder, resumable across `decode(…, { stream: true })`.
export class TextDecoder {
  readonly encoding: string;
  readonly fatal: boolean;
  readonly ignoreBOM: boolean;
  private needed: number;
  private seen: number;
  private codePoint: number;
  private lower: number;
  private upper: number;
  private started: boolean;
  constructor(label?: string, options?: TextDecoderOptions) {
    const name = label === undefined ? 'utf-8' : label.trim().toLowerCase();
    if (!UTF8_LABELS.includes(name)) {
      throw new RangeError(`The "${label === undefined ? '' : label}" encoding is not supported`);
    }
    this.encoding = 'utf-8';
    this.fatal = options?.fatal === true;
    this.ignoreBOM = options?.ignoreBOM === true;
    this.needed = 0;
    this.seen = 0;
    this.codePoint = 0;
    this.lower = CONTINUATION;
    this.upper = MAX_BYTE - CONTINUATION_BITS;
    this.started = false;
  }
  private reset(): void {
    this.needed = 0;
    this.seen = 0;
    this.codePoint = 0;
    this.lower = CONTINUATION;
    this.upper = MAX_BYTE - CONTINUATION_BITS;
  }
  private invalid(units: Units): void {
    this.reset();
    if (this.fatal) {
      this.started = false;
      throw new TypeError('The encoded data was not valid for encoding utf-8');
    }
    this.emit(units, REPLACEMENT);
  }
  private emit(units: Units, codePoint: number): void {
    if (!this.started) {
      this.started = true;
      if (codePoint === BOM && !this.ignoreBOM) {
        return;
      }
    }
    units.point(codePoint);
  }
  decode(input?: Uint8Array, options?: TextDecodeOptions): string {
    const bytes: number[] = input === undefined ? [] : varta.bytes.view(input);
    const units = new Units();
    let at = 0;
    while (at < bytes.length) {
      const byte = bytes[at]!;
      if (this.needed === 0) {
        at++;
        if (byte < CONTINUATION) {
          this.emit(units, byte);
        } else if (byte >= 0xc2 && byte <= 0xdf) {
          this.needed = 1;
          this.codePoint = byte - LEAD_TWO;
        } else if (byte >= LEAD_THREE && byte <= 0xef) {
          if (byte === LEAD_THREE) {
            this.lower = 0xa0;
          }
          if (byte === 0xed) {
            this.upper = 0x9f;
          }
          this.needed = 2;
          this.codePoint = byte - LEAD_THREE;
        } else if (byte >= LEAD_FOUR && byte <= 0xf4) {
          if (byte === LEAD_FOUR) {
            this.lower = 0x90;
          }
          if (byte === 0xf4) {
            this.upper = 0x8f;
          }
          this.needed = 3;
          this.codePoint = byte - LEAD_FOUR;
        } else {
          this.invalid(units);
        }
        continue;
      }
      if (byte < this.lower || byte > this.upper) {
        // The byte is not consumed: it starts the next sequence.
        this.invalid(units);
        continue;
      }
      at++;
      this.lower = CONTINUATION;
      this.upper = MAX_BYTE - CONTINUATION_BITS;
      this.codePoint = this.codePoint * CONTINUATION_BITS + (byte & CONTINUATION_MASK);
      this.seen++;
      if (this.seen === this.needed) {
        const codePoint = this.codePoint;
        this.reset();
        this.emit(units, codePoint);
      }
    }
    if (options?.stream !== true) {
      if (this.needed !== 0) {
        this.invalid(units);
      }
      this.started = false;
    }
    return units.finish();
  }
}

// Fill with fresh entropy: six bytes per 48-bit slice of a Math.random sample.
export function cryptoGetRandomValues(array: Uint8Array): Uint8Array {
  const bytes = varta.bytes.view(array);
  let at = 0;
  while (at < bytes.length) {
    let sample = Math.floor(Math.random() * 2 ** (8 * RANDOM_BYTES));
    for (let taken = 0; taken < RANDOM_BYTES && at < bytes.length; taken++) {
      bytes[at] = sample % BYTE;
      sample = Math.floor(sample / BYTE);
      at++;
    }
  }
  return array;
}

const HEX = '0123456789abcdef';
const UUID_BYTES = 16;
const VERSION_BYTE = 6;
const VARIANT_BYTE = 8;
const NIBBLE = 16;
const DASH = 0x2d;

// An RFC 9562 version 4 UUID from 16 random bytes.
export function cryptoRandomUUID(): string {
  const bytes = varta.bytes.view(cryptoGetRandomValues(new Uint8Array(UUID_BYTES)));
  bytes[VERSION_BYTE] = (bytes[VERSION_BYTE]! & 0x0f) | 0x40;
  bytes[VARIANT_BYTE] = (bytes[VARIANT_BYTE]! & 0x3f) | 0x80;
  const text = new Units();
  for (let at = 0; at < UUID_BYTES; at++) {
    if (at === 4 || at === 6 || at === 8 || at === 10) {
      text.push(DASH);
    }
    const byte = bytes[at]!;
    text.push(varta.text.unit(HEX, Math.floor(byte / NIBBLE)));
    text.push(varta.text.unit(HEX, byte % NIBBLE));
  }
  return text.finish();
}
