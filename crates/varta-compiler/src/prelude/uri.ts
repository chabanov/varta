// Engine prelude of checked-f64-utf16-v38 (`structural_subtyping`): the
// ECMAScript URI functions `encodeURIComponent` and `encodeURI` in typed
// guest code, with no host import and no capability. The compiler adds it to
// a program only when a module names one of them; see docs/architecture.md
// (Structural subtyping, pilot items).
//
// Text is percent-encoded as UTF-8; a lone surrogate throws `URIError`
// ("URI malformed"), as in V8. Varta has no native `URIError`: the prelude
// declares it as an `Error` subclass. `varta.text.unit` reads one UTF-16 code
// unit without a host call.

const HEX = '0123456789ABCDEF';
const MALFORMED = 'URI malformed';
// Unreserved marks kept by both functions: - _ . ! ~ * ' ( ).
const MARKS = [0x2d, 0x5f, 0x2e, 0x21, 0x7e, 0x2a, 0x27, 0x28, 0x29];
// Reserved characters and `#`, which `encodeURI` also keeps: ; / ? : @ & = + $ , #.
const RESERVED = [0x3b, 0x2f, 0x3f, 0x3a, 0x40, 0x26, 0x3d, 0x2b, 0x24, 0x2c, 0x23];

export class URIError extends Error {
  constructor(message?: string) {
    super(message);
    this.name = 'URIError';
  }
}

function alphanumeric(unit: number): boolean {
  return (unit >= 0x41 && unit <= 0x5a) || (unit >= 0x61 && unit <= 0x7a) || (unit >= 0x30 && unit <= 0x39);
}

function byteEscape(byte: number): string {
  return '%' + HEX.slice(byte >> 4, (byte >> 4) + 1) + HEX.slice(byte & 15, (byte & 15) + 1);
}

// The UTF-8 percent escapes of one code point.
function codePointEscape(point: number): string {
  if (point < 0x80) {
    return byteEscape(point);
  }
  if (point < 0x800) {
    return byteEscape(0xc0 | (point >> 6)) + byteEscape(0x80 | (point & 63));
  }
  if (point < 0x10000) {
    return byteEscape(0xe0 | (point >> 12)) + byteEscape(0x80 | ((point >> 6) & 63)) + byteEscape(0x80 | (point & 63));
  }
  return (
    byteEscape(0xf0 | (point >> 18)) +
    byteEscape(0x80 | ((point >> 12) & 63)) +
    byteEscape(0x80 | ((point >> 6) & 63)) +
    byteEscape(0x80 | (point & 63))
  );
}

// ECMAScript Encode: keep alphanumerics, the marks and (with `reserved`) the
// reserved set; escape every other code point. Runs of kept units are
// copied as slices.
function encode(text: string, reserved: boolean): string {
  let out = '';
  let start = 0;
  let i = 0;
  while (i < text.length) {
    const unit = varta.text.unit(text, i);
    if (alphanumeric(unit) || MARKS.indexOf(unit) >= 0 || (reserved && RESERVED.indexOf(unit) >= 0)) {
      i += 1;
      continue;
    }
    out += text.slice(start, i);
    let point = unit;
    let width = 1;
    if (unit >= 0xdc00 && unit <= 0xdfff) {
      throw new URIError(MALFORMED);
    }
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const low = varta.text.unit(text, i + 1);
      if (!(low >= 0xdc00 && low <= 0xdfff)) {
        throw new URIError(MALFORMED);
      }
      point = 0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
      width = 2;
    }
    out += codePointEscape(point);
    i += width;
    start = i;
  }
  return start === 0 ? text : out + text.slice(start);
}

export function encodeURIComponent(value: string | number | boolean): string {
  return encode(String(value), false);
}

export function encodeURI(value: string): string {
  return encode(value, true);
}
