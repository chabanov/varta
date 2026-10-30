// Engine prelude of checked-f64-utf16-v37 (`net_request`): a WHATWG `URL`
// and `URLSearchParams` subset in typed guest code, with no host import and
// no capability. The compiler adds it to a program only when a module names
// `URL` or `URLSearchParams`; see docs/architecture.md (Network requests).
//
// Subset: the special schemes http, https, ws and wss (other schemes throw
// `TypeError: Invalid URL`), ASCII hosts (lowercased; IPv6 literals kept as
// written; no IDNA and no IPv4 number normalization), ports, dot segments,
// relative references against a base, and the WHATWG percent-encode sets
// (non-ASCII text is percent-encoded as UTF-8). `URLSearchParams` decodes
// `+` and percent escapes of printable ASCII; other escapes stay as written
// until a code-unit constructor (`String.fromCharCode`) is available.
// `varta.text.unit` reads one UTF-16 code unit without a host call.

const INVALID_URL = 'Invalid URL';
const HEX = '0123456789ABCDEF';
// Printable ASCII from U+0020, for decoding percent escapes.
const PRINTABLE = ' !"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~';
const FIRST_PRINTABLE = 0x20;
const LAST_PRINTABLE = 0x7e;
const SCHEMES = ['http:', 'https:', 'ws:', 'wss:'];
const DEFAULT_PORTS = ['80', '443', '80', '443'];

// WHATWG percent-encode sets beyond the C0 control set (below U+0020 and
// above U+007E): fragment, special-query, path and
// application/x-www-form-urlencoded (everything but ASCII alphanumerics and
// `*-._`, with space as `+`).
const FRAGMENT_SET = [0x20, 0x22, 0x3c, 0x3e, 0x60];
const QUERY_SET = [0x20, 0x22, 0x23, 0x3c, 0x3e, 0x27];
const PATH_SET = [0x20, 0x22, 0x23, 0x3c, 0x3e, 0x3f, 0x5e, 0x60, 0x7b, 0x7d];
const USERINFO_SET = [
  0x20, 0x22, 0x23, 0x3c, 0x3e, 0x3f, 0x5e, 0x60, 0x7b, 0x7d, 0x2f, 0x3a, 0x3b, 0x3d, 0x40, 0x5b, 0x5c, 0x5d, 0x7c,
];
const FORM_KEEP = [0x2a, 0x2d, 0x2e, 0x5f];
// Host code points that make a host invalid (WHATWG forbidden host code points).
const FORBIDDEN_HOST = [0x00, 0x20, 0x23, 0x2f, 0x3a, 0x3c, 0x3e, 0x3f, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x7c, 0x25];

function invalid(): TypeError {
  return new TypeError(INVALID_URL);
}

function isAlpha(unit: number): boolean {
  return (unit >= 0x41 && unit <= 0x5a) || (unit >= 0x61 && unit <= 0x7a);
}

function isDigit(unit: number): boolean {
  return unit >= 0x30 && unit <= 0x39;
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

// Percent-encode `text` with the C0 control set plus `set`; with `form`, the
// application/x-www-form-urlencoded set instead. Runs of kept units are
// copied as slices. A lone surrogate encodes U+FFFD.
function encode(text: string, set: number[], form: boolean): string {
  let out = '';
  let start = 0;
  let i = 0;
  while (i < text.length) {
    const unit = varta.text.unit(text, i);
    const keep = form
      ? isAlpha(unit) || isDigit(unit) || FORM_KEEP.indexOf(unit) >= 0
      : unit >= FIRST_PRINTABLE && unit <= LAST_PRINTABLE && set.indexOf(unit) < 0;
    if (keep) {
      i += 1;
      continue;
    }
    out += text.slice(start, i);
    let point = unit;
    let width = 1;
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = i + 1;
      const low = varta.text.unit(text, next);
      if (low >= 0xdc00 && low <= 0xdfff) {
        point = 0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
        width = 2;
      } else {
        point = 0xfffd;
      }
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      point = 0xfffd;
    }
    out += form && unit === 0x20 ? '+' : codePointEscape(point);
    i += width;
    start = i;
  }
  return start === 0 ? text : out + text.slice(start);
}

function hexValue(unit: number): number {
  if (isDigit(unit)) {
    return unit - 0x30;
  }
  if (unit >= 0x41 && unit <= 0x46) {
    return unit - 0x37;
  }
  if (unit >= 0x61 && unit <= 0x66) {
    return unit - 0x57;
  }
  return -1;
}

// application/x-www-form-urlencoded decoding of one name or value.
function decodeForm(text: string): string {
  let out = '';
  let start = 0;
  let i = 0;
  while (i < text.length) {
    const unit = varta.text.unit(text, i);
    if (unit === 0x2b) {
      out += text.slice(start, i) + ' ';
      i += 1;
      start = i;
      continue;
    }
    if (unit === 0x25) {
      const a = i + 1;
      const b = i + 2;
      const high = hexValue(varta.text.unit(text, a));
      const low = hexValue(varta.text.unit(text, b));
      const byte = high * 16 + low;
      if (high >= 0 && low >= 0 && byte >= FIRST_PRINTABLE && byte <= LAST_PRINTABLE) {
        out += text.slice(start, i) + PRINTABLE.slice(byte - FIRST_PRINTABLE, byte - FIRST_PRINTABLE + 1);
        i += 3;
        start = i;
        continue;
      }
    }
    i += 1;
  }
  return start === 0 ? text : out + text.slice(start);
}

// Remove leading and trailing C0 controls and spaces, and every tab or newline.
function clean(input: string): string {
  let start = 0;
  let end = input.length;
  while (start < end && varta.text.unit(input, start) <= FIRST_PRINTABLE) {
    start += 1;
  }
  while (end > start) {
    const last = end - 1;
    if (varta.text.unit(input, last) > FIRST_PRINTABLE) {
      break;
    }
    end -= 1;
  }
  let out = '';
  let from = start;
  for (let i = start; i < end; i++) {
    const unit = varta.text.unit(input, i);
    if (unit === 0x09 || unit === 0x0a || unit === 0x0d) {
      out += input.slice(from, i);
      from = i + 1;
    }
  }
  return out + input.slice(from, end);
}

// The scheme with its colon (lowercased) if `input` starts with one, else ''.
function schemeOf(input: string): string {
  if (input.length === 0 || !isAlpha(varta.text.unit(input, 0))) {
    return '';
  }
  for (let i = 1; i < input.length; i++) {
    const unit = varta.text.unit(input, i);
    if (unit === 0x3a) {
      return varta.web.asciiLowercase(input.slice(0, i + 1));
    }
    if (!(isAlpha(unit) || isDigit(unit) || unit === 0x2b || unit === 0x2d || unit === 0x2e)) {
      return '';
    }
  }
  return '';
}

function isSlash(unit: number): boolean {
  return unit === 0x2f || unit === 0x5c;
}

// The index of the first unit at or after `from` that ends an authority.
function authorityEnd(input: string, from: number): number {
  for (let i = from; i < input.length; i++) {
    const unit = varta.text.unit(input, i);
    if (isSlash(unit) || unit === 0x3f || unit === 0x23) {
      return i;
    }
  }
  return input.length;
}

// The parsed parts of a URL; `path` starts with `/`, `query` and `fragment`
// are null when absent.
class Parts {
  scheme: string;
  username: string;
  password: string;
  host: string;
  port: string;
  path: string;
  query: string | null;
  fragment: string | null;
  constructor(scheme: string) {
    this.scheme = scheme;
    this.username = '';
    this.password = '';
    this.host = '';
    this.port = '';
    this.path = '/';
    this.query = null;
    this.fragment = null;
  }
}

function parseHost(parts: Parts, text: string): void {
  const userEnd = text.lastIndexOf('@');
  let hostPort = text;
  if (userEnd >= 0) {
    const userinfo = text.slice(0, userEnd);
    hostPort = text.slice(userEnd + 1);
    const colon = userinfo.indexOf(':');
    parts.username = encode(colon < 0 ? userinfo : userinfo.slice(0, colon), USERINFO_SET, false);
    parts.password = colon < 0 ? '' : encode(userinfo.slice(colon + 1), USERINFO_SET, false);
  }
  let host = hostPort;
  let port = '';
  const close = hostPort.lastIndexOf(']');
  const colon = hostPort.lastIndexOf(':');
  if (colon > close) {
    host = hostPort.slice(0, colon);
    port = hostPort.slice(colon + 1);
  }
  if (host.length === 0) {
    throw invalid();
  }
  if (varta.text.unit(host, 0) === 0x5b) {
    if (close !== host.length - 1) {
      throw invalid();
    }
  } else {
    for (let i = 0; i < host.length; i++) {
      const unit = varta.text.unit(host, i);
      // Non-ASCII hosts need IDNA, which this subset does not implement.
      if (unit < FIRST_PRINTABLE || unit > LAST_PRINTABLE || FORBIDDEN_HOST.indexOf(unit) >= 0) {
        throw invalid();
      }
    }
  }
  parts.host = varta.web.asciiLowercase(host);
  for (let i = 0; i < port.length; i++) {
    if (!isDigit(varta.text.unit(port, i))) {
      throw invalid();
    }
  }
  if (port.length > 0) {
    const value = Number(port);
    if (value > 65535) {
      throw invalid();
    }
    const normalized = String(value);
    const index = SCHEMES.indexOf(parts.scheme);
    parts.port = index >= 0 && DEFAULT_PORTS[index] === normalized ? '' : normalized;
  }
}

// Resolve `.` and `..` segments of a `/`-separated path (backslashes count
// as slashes in special URLs) and percent-encode each segment.
function normalizePath(path: string): string {
  const segments: string[] = [];
  // The leading slash opens the first segment.
  const first = path.length === 0 ? 0 : varta.text.unit(path, 0);
  const begin = isSlash(first) ? 1 : 0;
  let start = begin;
  let trailing = false;
  for (let i = begin; i <= path.length; i++) {
    if (i < path.length && !isSlash(varta.text.unit(path, i))) {
      continue;
    }
    const segment = path.slice(start, i);
    start = i + 1;
    const last = i === path.length;
    const lower = varta.web.asciiLowercase(segment);
    if (lower === '..' || lower === '.%2e' || lower === '%2e.' || lower === '%2e%2e') {
      segments.pop();
      trailing = last;
    } else if (lower === '.' || lower === '%2e') {
      trailing = last;
    } else {
      segments.push(encode(segment, PATH_SET, false));
      trailing = false;
    }
  }
  let out = '';
  for (let i = 0; i < segments.length; i++) {
    out += '/' + (segments[i] ?? '');
  }
  return trailing || out.length === 0 ? out + '/' : out;
}

// Split `rest` (after the authority) into path, query and fragment.
function parseTail(parts: Parts, rest: string, keepPath: boolean): void {
  let text = rest;
  const hash = text.indexOf('#');
  if (hash >= 0) {
    parts.fragment = encode(text.slice(hash + 1), FRAGMENT_SET, false);
    text = text.slice(0, hash);
  }
  const question = text.indexOf('?');
  if (question >= 0) {
    parts.query = encode(text.slice(question + 1), QUERY_SET, false);
    text = text.slice(0, question);
  }
  if (!keepPath) {
    parts.path = normalizePath(text);
  }
}

function parse(input: string, base: Parts | null): Parts {
  const text = clean(input);
  const scheme = schemeOf(text);
  if (scheme !== '' && SCHEMES.indexOf(scheme) < 0) {
    throw invalid();
  }
  if (scheme !== '') {
    const parts = new Parts(scheme);
    let at = scheme.length;
    while (at < text.length && isSlash(varta.text.unit(text, at))) {
      at += 1;
    }
    const end = authorityEnd(text, at);
    parseHost(parts, text.slice(at, end));
    parseTail(parts, text.slice(end), false);
    return parts;
  }
  if (base === null) {
    throw invalid();
  }
  const parts = new Parts(base.scheme);
  parts.username = base.username;
  parts.password = base.password;
  parts.host = base.host;
  parts.port = base.port;
  const first = text.length === 0 ? -1 : varta.text.unit(text, 0);
  const second = text.length < 2 ? -1 : varta.text.unit(text, 1);
  if (isSlash(first) && isSlash(second)) {
    const end = authorityEnd(text, 2);
    parts.username = '';
    parts.password = '';
    parts.port = '';
    parseHost(parts, text.slice(2, end));
    parseTail(parts, text.slice(end), false);
  } else if (isSlash(first)) {
    parseTail(parts, text, false);
  } else if (first === 0x3f) {
    parts.path = base.path;
    parseTail(parts, text, true);
  } else if (first === 0x23) {
    parts.path = base.path;
    parts.query = base.query;
    parseTail(parts, text, true);
  } else if (text.length === 0) {
    parts.path = base.path;
    parts.query = base.query;
  } else {
    const directory = base.path.slice(0, base.path.lastIndexOf('/') + 1);
    parseTail(parts, directory + text, false);
  }
  return parts;
}

// A query list as two parallel arrays (no tuple types).
interface QueryList {
  names: string[];
  items: string[];
}

function parseQuery(query: string): QueryList {
  const list: QueryList = { names: [], items: [] };
  let text = query;
  if (text.startsWith('?')) {
    text = text.slice(1);
  }
  let start = 0;
  for (let i = 0; i <= text.length; i++) {
    if (i < text.length && varta.text.unit(text, i) !== 0x26) {
      continue;
    }
    const pair = text.slice(start, i);
    start = i + 1;
    if (pair.length === 0) {
      continue;
    }
    const equals = pair.indexOf('=');
    list.names.push(decodeForm(equals < 0 ? pair : pair.slice(0, equals)));
    list.items.push(equals < 0 ? '' : decodeForm(pair.slice(equals + 1)));
  }
  return list;
}

export class URLSearchParams {
  private names: string[];
  private items: string[];
  private owner: URL | null;

  constructor(init?: string | Record<string, string> | string[][]) {
    this.names = [];
    this.items = [];
    this.owner = null;
    if (typeof init === 'string') {
      const list = parseQuery(init);
      this.names = list.names;
      this.items = list.items;
    } else if (Array.isArray(init)) {
      for (let i = 0; i < init.length; i++) {
        const pair = init[i];
        if (pair === undefined || pair.length !== 2) {
          throw new TypeError('Each query pair must be an iterable [name, value] tuple');
        }
        this.names.push(pair[0] ?? '');
        this.items.push(pair[1] ?? '');
      }
    } else if (init !== undefined) {
      const keys = Object.keys(init);
      for (let i = 0; i < keys.length; i++) {
        const key = keys[i] ?? '';
        this.names.push(key);
        this.items.push(init[key] ?? '');
      }
    }
  }

  // Engine-internal: replace the list with the parsed `query`.
  parse(query: string): void {
    const list = parseQuery(query);
    this.names = list.names;
    this.items = list.items;
  }

  // Engine-internal: the URL whose query this object reflects.
  attach(owner: URL): void {
    this.owner = owner;
  }

  private changed(): void {
    const owner = this.owner;
    if (owner !== null) {
      owner.adoptQuery(this.toString());
    }
  }

  get size(): number {
    return this.names.length;
  }

  append(name: string, value: string): void {
    this.names.push(name);
    this.items.push(value);
    this.changed();
  }

  delete(name: string): void {
    const names: string[] = [];
    const items: string[] = [];
    for (let i = 0; i < this.names.length; i++) {
      const key = this.names[i] ?? '';
      if (key !== name) {
        names.push(key);
        items.push(this.items[i] ?? '');
      }
    }
    this.names = names;
    this.items = items;
    this.changed();
  }

  get(name: string): string | null {
    const index = this.names.indexOf(name);
    return index < 0 ? null : (this.items[index] ?? null);
  }

  getAll(name: string): string[] {
    const values: string[] = [];
    for (let i = 0; i < this.names.length; i++) {
      if (this.names[i] === name) {
        values.push(this.items[i] ?? '');
      }
    }
    return values;
  }

  has(name: string): boolean {
    return this.names.indexOf(name) >= 0;
  }

  set(name: string, value: string): void {
    const index = this.names.indexOf(name);
    if (index < 0) {
      this.append(name, value);
      return;
    }
    const names: string[] = [];
    const items: string[] = [];
    for (let i = 0; i < this.names.length; i++) {
      const key = this.names[i] ?? '';
      if (i === index) {
        names.push(key);
        items.push(value);
      } else if (key !== name) {
        names.push(key);
        items.push(this.items[i] ?? '');
      }
    }
    this.names = names;
    this.items = items;
    this.changed();
  }

  forEach(callback: (value: string, key: string) => void): void {
    for (let i = 0; i < this.names.length; i++) {
      callback(this.items[i] ?? '', this.names[i] ?? '');
    }
  }

  // Snapshots, as for `Headers` (no tuple types).
  entries(): string[][] {
    const pairs: string[][] = [];
    for (let i = 0; i < this.names.length; i++) {
      pairs.push([this.names[i] ?? '', this.items[i] ?? '']);
    }
    return pairs;
  }

  keys(): string[] {
    return this.names.slice();
  }

  values(): string[] {
    return this.items.slice();
  }

  toString(): string {
    let out = '';
    for (let i = 0; i < this.names.length; i++) {
      if (i > 0) {
        out += '&';
      }
      out += encode(this.names[i] ?? '', FORM_KEEP, true) + '=' + encode(this.items[i] ?? '', FORM_KEEP, true);
    }
    return out;
  }
}

export class URL {
  private readonly parts: Parts;
  private params: URLSearchParams | null;

  constructor(url: string, base?: string | URL) {
    let parsedBase: Parts | null = null;
    if (base instanceof URL) {
      parsedBase = base.parts;
    } else if (base !== undefined) {
      parsedBase = parse(base, null);
    }
    this.parts = parse(url, parsedBase);
    this.params = null;
  }

  static canParse(url: string, base?: string): boolean {
    try {
      const parsedBase = base === undefined ? null : parse(base, null);
      parse(url, parsedBase);
      return true;
    } catch (error) {
      return false;
    }
  }

  get protocol(): string {
    return this.parts.scheme;
  }

  get username(): string {
    return this.parts.username;
  }

  get password(): string {
    return this.parts.password;
  }

  get hostname(): string {
    return this.parts.host;
  }

  get port(): string {
    return this.parts.port;
  }

  get host(): string {
    return this.parts.port === '' ? this.parts.host : this.parts.host + ':' + this.parts.port;
  }

  get origin(): string {
    return this.parts.scheme + '//' + this.host;
  }

  get pathname(): string {
    return this.parts.path;
  }

  get search(): string {
    const query = this.parts.query;
    return query === null || query === '' ? '' : '?' + query;
  }

  get hash(): string {
    const fragment = this.parts.fragment;
    return fragment === null || fragment === '' ? '' : '#' + fragment;
  }

  get searchParams(): URLSearchParams {
    const existing = this.params;
    if (existing !== null) {
      return existing;
    }
    const params = new URLSearchParams(this.parts.query ?? '');
    params.attach(this);
    this.params = params;
    return params;
  }

  // Engine-internal: the serialized query of the attached `searchParams`.
  adoptQuery(query: string): void {
    this.parts.query = query === '' ? null : query;
  }

  get href(): string {
    const parts = this.parts;
    let userinfo = '';
    if (parts.username !== '' || parts.password !== '') {
      userinfo = parts.username + (parts.password === '' ? '' : ':' + parts.password) + '@';
    }
    const query = parts.query === null ? '' : '?' + parts.query;
    const fragment = parts.fragment === null ? '' : '#' + parts.fragment;
    return parts.scheme + '//' + userinfo + this.host + parts.path + query + fragment;
  }

  toString(): string {
    return this.href;
  }
}
