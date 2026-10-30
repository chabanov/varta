// Engine prelude of checked-f64-utf16-v38 (`web_bytes`): `ArrayBuffer`,
// `Blob` and `FormData` in typed guest code with no host import. The
// compiler loads it when a module names one of its exports; see
// docs/architecture.md (Web bytes).
//
// A `Blob` holds its bytes as an immutable byte string: one UTF-16 code unit
// (0..=0xFF) per byte, the form network chunks and byte request bodies take
// (`fetch`), so concatenating, slicing and sending a blob copies code units
// without a per-byte step. An `ArrayBuffer` owns the `number[]` storage that
// `new Uint8Array(buffer)` views (the same storage, so writes through the view
// are visible in the buffer). `FormData` keeps its entry list and serializes
// to `multipart/form-data` with a deterministic boundary.

const MIN_TYPE_UNIT = 0x20;
const MAX_TYPE_UNIT = 0x7e;
const UPPER_A = 0x41;
const UPPER_Z = 0x5a;
const CASE_OFFSET = 0x20;
const HIGH_SURROGATE = 0xd800;
const LOW_SURROGATE = 0xdc00;
const SURROGATE_END = 0xe000;
const REPLACEMENT = 0xfffd;
const DEFAULT_FILENAME = 'blob';
const OCTET_STREAM = 'application/octet-stream';
const CRLF = '\r\n';
const BOUNDARY_PREFIX = '----formdata-varta-';
const BOUNDARY_DIGITS = 11;
// Boundaries tried before a body is refused: each collision with the body's
// bytes moves to the next counter value.
const MAX_BOUNDARY_ATTEMPTS = 1000;

// Hands storage or bytes to the next instance the prelude builds; program
// code can never fill it, so its buffers and blobs always own fresh state.
const HANDOFF: { storage: number[] | null; bytes: string | null } = {
  storage: null,
  bytes: null,
};

// A WebIDL USVString: lone surrogates become U+FFFD.
function scalarString(text: string): string {
  let clean = true;
  for (let at = 0; at < text.length; at++) {
    const unit = varta.text.unit(text, at);
    if (unit >= HIGH_SURROGATE && unit < SURROGATE_END) {
      clean = false;
      break;
    }
  }
  if (clean) {
    return text;
  }
  const units: number[] = [];
  for (let at = 0; at < text.length; at++) {
    const unit = varta.text.unit(text, at);
    if (unit >= HIGH_SURROGATE && unit < LOW_SURROGATE && at + 1 < text.length) {
      const next = varta.text.unit(text, at + 1);
      if (next >= LOW_SURROGATE && next < SURROGATE_END) {
        units.push(unit);
        units.push(next);
        at++;
        continue;
      }
    }
    units.push(unit >= HIGH_SURROGATE && unit < SURROGATE_END ? REPLACEMENT : unit);
  }
  return varta.text.fromUnits(units);
}

// The UTF-8 bytes of `text` as a byte string (lone surrogates as U+FFFD).
function utf8Bytes(text: string): string {
  for (let at = 0; at < text.length; at++) {
    if (varta.text.unit(text, at) > MAX_TYPE_UNIT) {
      return varta.text.fromUnits(varta.bytes.view(new TextEncoder().encode(text)));
    }
  }
  // ASCII text is its own byte string.
  return text;
}

// The WHATWG blob type: empty when a code unit is outside U+0020..U+007E,
// otherwise ASCII-lowercased.
function blobType(type: string | undefined): string {
  if (type === undefined) {
    return '';
  }
  const units: number[] = [];
  for (let at = 0; at < type.length; at++) {
    const unit = varta.text.unit(type, at);
    if (unit < MIN_TYPE_UNIT || unit > MAX_TYPE_UNIT) {
      return '';
    }
    units.push(unit >= UPPER_A && unit <= UPPER_Z ? unit + CASE_OFFSET : unit);
  }
  return varta.text.fromUnits(units);
}

// A relative position (slice arguments) resolved against `length`.
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

export class ArrayBuffer {
  private readonly storage: number[];

  constructor(length: number) {
    const handed = HANDOFF.storage;
    HANDOFF.storage = null;
    if (handed === null && (!Number.isInteger(length) || length < 0)) {
      throw new RangeError('Array buffer allocation failed');
    }
    this.storage = handed ?? varta.bytes.view(new Uint8Array(length));
  }

  get byteLength(): number {
    return this.storage.length;
  }

  slice(begin?: number, end?: number): ArrayBuffer {
    const from = relative(begin, this.storage.length, 0);
    const to = relative(end, this.storage.length, this.storage.length);
    return bufferOf(this.storage.slice(from, Math.max(from, to)));
  }

  // `Uint8Array` is the only `ArrayBufferView` of the profile.
  static isView(value: unknown): boolean {
    return false;
  }

  // Engine-internal: the storage a `Uint8Array` view shares.
  storageOf(): number[] {
    return this.storage;
  }

  // Engine-internal: a buffer over `storage` (shared, not copied).
  adopt(storage: number[]): ArrayBuffer {
    return bufferOf(storage);
  }
}

function bufferOf(storage: number[]): ArrayBuffer {
  HANDOFF.storage = storage;
  return new ArrayBuffer(0);
}

// `new Uint8Array(buffer)` (a compiler hook): a view sharing the buffer's
// storage.
export function viewOf(buffer: ArrayBuffer): number[] {
  return buffer.storageOf();
}

// `ArrayBufferView` of TypeScript's DOM library: `Uint8Array` is the only
// typed array of the profile (no `DataView`, no other element types).
export type ArrayBufferView = Uint8Array;

export type BlobPart = string | Blob | ArrayBuffer | Uint8Array;

export interface BlobPropertyBag {
  type?: string;
}

export class Blob {
  private readonly data: string;
  readonly type: string;

  constructor(blobParts?: BlobPart[], options?: BlobPropertyBag) {
    const handed = HANDOFF.bytes;
    HANDOFF.bytes = null;
    let data = handed ?? '';
    if (handed === null && blobParts !== undefined) {
      for (const part of blobParts) {
        if (typeof part === 'string') {
          data += utf8Bytes(part);
        } else if (part instanceof Blob) {
          data += part.byteString();
        } else if (part instanceof ArrayBuffer) {
          data += varta.text.fromUnits(part.storageOf());
        } else {
          data += varta.text.fromUnits(varta.bytes.view(part));
        }
      }
    }
    this.data = data;
    this.type = blobType(options?.type);
  }

  get size(): number {
    return this.data.length;
  }

  slice(start?: number, end?: number, contentType?: string): Blob {
    const from = relative(start, this.data.length, 0);
    const to = relative(end, this.data.length, this.data.length);
    return blobOf(this.data.slice(from, Math.max(from, to)), blobType(contentType));
  }

  async arrayBuffer(): Promise<ArrayBuffer> {
    return bufferOf(varta.text.units(this.data));
  }

  async bytes(): Promise<Uint8Array> {
    return varta.bytes.adopt(varta.text.units(this.data));
  }

  async text(): Promise<string> {
    return new TextDecoder().decode(varta.bytes.adopt(varta.text.units(this.data)));
  }

  // Engine-internal: the bytes, one code unit each.
  byteString(): string {
    return this.data;
  }

  // Engine-internal: a blob of the byte string `bytes` with an already
  // normalized type.
  adopt(bytes: string, type: string): Blob {
    return blobOf(bytes, type);
  }

  // Engine-internal: the UTF-8 bytes of `text` as a byte string.
  encode(text: string): string {
    return utf8Bytes(text);
  }
}

function blobOf(bytes: string, type: string): Blob {
  HANDOFF.bytes = bytes;
  return new Blob(undefined, { type: type });
}

export type FormDataEntryValue = string | Blob;

class FormEntry {
  readonly name: string;
  readonly value: FormDataEntryValue;
  // The file name of a blob entry (WHATWG: the `File` it becomes).
  readonly filename: string;
  constructor(name: string, value: FormDataEntryValue, filename: string) {
    this.name = name;
    this.value = value;
    this.filename = filename;
  }
}

function entry(name: string, value: FormDataEntryValue, filename: string | undefined, method: string): FormEntry {
  if (typeof value === 'string') {
    if (filename !== undefined) {
      throw new TypeError(`Failed to execute '${method}' on 'FormData': parameter 2 is not of type 'Blob'.`);
    }
    return new FormEntry(scalarString(name), scalarString(value), '');
  }
  return new FormEntry(
    scalarString(name),
    value,
    filename === undefined ? DEFAULT_FILENAME : scalarString(filename),
  );
}

export class FormData {
  private list: FormEntry[];

  constructor() {
    this.list = [];
  }

  append(name: string, value: FormDataEntryValue, filename?: string): void {
    this.list.push(entry(name, value, filename, 'append'));
  }

  set(name: string, value: FormDataEntryValue, filename?: string): void {
    const replacement = entry(name, value, filename, 'set');
    const kept: FormEntry[] = [];
    let placed = false;
    for (const item of this.list) {
      if (item.name !== replacement.name) {
        kept.push(item);
      } else if (!placed) {
        kept.push(replacement);
        placed = true;
      }
    }
    if (!placed) {
      kept.push(replacement);
    }
    this.list = kept;
  }

  delete(name: string): void {
    const key = scalarString(name);
    this.list = this.list.filter((item) => item.name !== key);
  }

  get(name: string): FormDataEntryValue | null {
    const key = scalarString(name);
    for (const item of this.list) {
      if (item.name === key) {
        return item.value;
      }
    }
    return null;
  }

  getAll(name: string): FormDataEntryValue[] {
    const key = scalarString(name);
    const values: FormDataEntryValue[] = [];
    for (const item of this.list) {
      if (item.name === key) {
        values.push(item.value);
      }
    }
    return values;
  }

  has(name: string): boolean {
    const key = scalarString(name);
    return this.list.some((item) => item.name === key);
  }

  // Snapshots, in entry order.
  keys(): string[] {
    return this.list.map((item) => item.name);
  }

  values(): FormDataEntryValue[] {
    return this.list.map((item) => item.value);
  }

  entries(): [string, FormDataEntryValue][] {
    return this.list.map((item): [string, FormDataEntryValue] => [item.name, item.value]);
  }

  // Engine-internal: the `multipart/form-data` body of the entries.
  multipart(): MultipartBody {
    return multipartBody(this.list);
  }
}

// Fetch's multipart newline normalization: CR, LF and CRLF become CRLF.
function normalizeLinefeeds(text: string): string {
  if (text.indexOf('\r') < 0 && text.indexOf('\n') < 0) {
    return text;
  }
  let out = '';
  for (let at = 0; at < text.length; at++) {
    const unit = text.charAt(at);
    if (unit === '\r') {
      out += CRLF;
      if (at + 1 < text.length && text.charAt(at + 1) === '\n') {
        at++;
      }
    } else if (unit === '\n') {
      out += CRLF;
    } else {
      out += unit;
    }
  }
  return out;
}

// The field-name escape of a multipart header (Node's `%0A`, `%0D`, `%22`).
function escapeName(text: string): string {
  let out = '';
  for (let at = 0; at < text.length; at++) {
    const unit = text.charAt(at);
    out += unit === '\n' ? '%0A' : unit === '\r' ? '%0D' : unit === '"' ? '%22' : unit;
  }
  return out;
}

// One serialized body: the bytes (one code unit each) and its boundary.
interface MultipartBody {
  bytes: string;
  boundary: string;
}

function boundaryOf(attempt: number): string {
  return BOUNDARY_PREFIX + String(attempt).padStart(BOUNDARY_DIGITS, '0');
}

// `multipart/form-data` (RFC 7578) in Node's layout. The boundary is
// deterministic: the first counter value whose boundary occurs in no part
// (names, file names, types, values and blob bytes), so equal forms
// serialize to equal bytes.
function multipartBody(list: FormEntry[]): MultipartBody {
  const parts: string[] = [];
  for (const item of list) {
    const name = escapeName(normalizeLinefeeds(item.name));
    const value = item.value;
    if (typeof value === 'string') {
      parts.push(
        utf8Bytes(`Content-Disposition: form-data; name="${name}"${CRLF}${CRLF}${normalizeLinefeeds(value)}`),
      );
    } else {
      const type = value.type === '' ? OCTET_STREAM : value.type;
      const head = utf8Bytes(
        `Content-Disposition: form-data; name="${name}"; filename="${escapeName(item.filename)}"${CRLF}Content-Type: ${type}${CRLF}${CRLF}`,
      );
      parts.push(head + value.byteString());
    }
  }
  for (let attempt = 0; attempt < MAX_BOUNDARY_ATTEMPTS; attempt++) {
    const boundary = boundaryOf(attempt);
    if (parts.some((part) => part.indexOf(boundary) >= 0)) {
      continue;
    }
    let bytes = '';
    for (const part of parts) {
      bytes += `--${boundary}${CRLF}` + part + CRLF;
    }
    bytes += `--${boundary}--${CRLF}`;
    return { bytes: bytes, boundary: boundary };
  }
  throw new RangeError('FormData parts collide with every multipart boundary');
}
