// Engine prelude of checked-f64-utf16-v37 (`collections`): `btoa` and `atob`
// (the HTML standard's forgiving base64 over Latin-1 code units), in typed
// guest code. The compiler loads it when a program names `btoa` or `atob`.
// Failures throw a `DOMException` named `InvalidCharacterError`.

const BYTE = 256;
const MAX_BYTE = 0xff;
const CHUNK = 1024;
const BASE64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const PAD = 0x3d;
const SEXTET = 64;

// Builds a string from code units in chunks of at most CHUNK units.
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
  finish(): string {
    return this.text + varta.text.fromUnits(this.units);
  }
}

export function btoa(data: string): string {
  const units: number[] = [];
  for (let at = 0; at < data.length; at++) {
    const unit = varta.text.unit(data, at);
    if (unit > MAX_BYTE) {
      throw new DOMException('Invalid character', 'InvalidCharacterError');
    }
    units.push(unit);
  }
  const text = new Units();
  for (let at = 0; at < units.length; at += 3) {
    const first = units[at]!;
    const second = at + 1 < units.length ? units[at + 1]! : 0;
    const third = at + 2 < units.length ? units[at + 2]! : 0;
    const word = first * BYTE * BYTE + second * BYTE + third;
    text.push(varta.text.unit(BASE64, Math.floor(word / (SEXTET * SEXTET * SEXTET)) % SEXTET));
    text.push(varta.text.unit(BASE64, Math.floor(word / (SEXTET * SEXTET)) % SEXTET));
    text.push(at + 1 < units.length ? varta.text.unit(BASE64, Math.floor(word / SEXTET) % SEXTET) : PAD);
    text.push(at + 2 < units.length ? varta.text.unit(BASE64, word % SEXTET) : PAD);
  }
  return text.finish();
}

// The value of one base64 alphabet unit, or -1.
function sextet(unit: number): number {
  if (unit >= 0x41 && unit <= 0x5a) {
    return unit - 0x41;
  }
  if (unit >= 0x61 && unit <= 0x7a) {
    return unit - 0x61 + 26;
  }
  if (unit >= 0x30 && unit <= 0x39) {
    return unit - 0x30 + 52;
  }
  if (unit === 0x2b) {
    return 62;
  }
  if (unit === 0x2f) {
    return 63;
  }
  return -1;
}

// The forgiving-base64 decode of the HTML standard.
export function atob(data: string): string {
  const values: number[] = [];
  let padding = 0;
  for (let at = 0; at < data.length; at++) {
    const unit = varta.text.unit(data, at);
    if (unit === 0x20 || unit === 0x09 || unit === 0x0a || unit === 0x0c || unit === 0x0d) {
      continue;
    }
    if (unit === PAD) {
      padding++;
      continue;
    }
    const value = sextet(unit);
    if (value < 0 || padding > 0) {
      throw new DOMException(
        value < 0 ? 'Invalid character' : 'The string to be decoded is not correctly encoded.',
        'InvalidCharacterError',
      );
    }
    values.push(value);
  }
  const total = values.length + padding;
  if (padding > 2 || (padding > 0 && total % 4 !== 0) || values.length % 4 === 1) {
    throw new DOMException('The string to be decoded is not correctly encoded.', 'InvalidCharacterError');
  }
  const units = new Units();
  let buffer = 0;
  let bits = 0;
  for (let at = 0; at < values.length; at++) {
    buffer = (buffer * SEXTET + values[at]!) % (BYTE * BYTE * BYTE);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      units.push(Math.floor(buffer / 2 ** bits) % BYTE);
    }
  }
  return units.finish();
}
