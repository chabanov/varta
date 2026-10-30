// Engine prelude of checked-f64-utf16-v35 (`web_headers`): the WHATWG Fetch
// `Headers` class, implemented in typed guest code with no host import and no
// capability. The compiler adds it to a program only when a module names
// `Headers` or `HeadersInit`; see docs/architecture.md (Web types).
//
// Names and values are WebIDL ByteStrings; names must be HTTP tokens and
// values are normalized by stripping leading and trailing HTTP whitespace,
// then may not contain NUL, CR or LF. Iteration uses the sorted and combined
// list, with every `set-cookie` value as its own entry.
//
// The header list is a chain of entries kept sorted by lowercase name, and
// entries with the same name keep their insertion order. Only that per-name
// order is observable (`get`, `getSetCookie`, iteration), so the list never
// needs sorting when it is read, and inserting or removing moves no entry.

export type HeadersInit = Headers | Record<string, string> | string[][];

const SET_COOKIE = 'set-cookie';
const SEPARATOR = ', ';

// The `varta.web.*` calls are engine-internal guest scans over UTF-16 code
// units (no host call); they exist only in engine prelude modules.

function byteString(input: string): void {
  const index = varta.web.indexAboveByte(input);
  if (index >= 0) {
    throw new TypeError(
      `Cannot convert argument to a ByteString because the character at index ${index} has a value of ${input.charCodeAt(index)} which is greater than 255.`,
    );
  }
}

function checkName(prefix: string, name: string): void {
  if (name.length === 0 || varta.web.indexNonToken(name) >= 0) {
    throw new TypeError(`${prefix}: "${name}" is an invalid header name.`);
  }
}

function checkValue(prefix: string, value: string): void {
  if (varta.web.indexForbiddenValue(value) >= 0) {
    throw new TypeError(`${prefix}: "${value}" is an invalid header value.`);
  }
}

class HeaderEntry {
  readonly name: string;
  value: string;
  next: HeaderEntry | null;
  constructor(name: string, value: string, next: HeaderEntry | null) {
    this.name = name;
    this.value = value;
    this.next = next;
  }
}

// Insert after every entry whose name is not greater (stable by name) and
// return the new first entry.
function insert(first: HeaderEntry | null, name: string, value: string): HeaderEntry {
  if (!(first instanceof HeaderEntry) || first.name > name) {
    return new HeaderEntry(name, value, first);
  }
  let at = first;
  let next = at.next;
  while (next instanceof HeaderEntry && next.name <= name) {
    at = next;
    next = at.next;
  }
  at.next = new HeaderEntry(name, value, next);
  return first;
}

// Normalize and validate an already ByteString-converted header, then insert it.
function append(first: HeaderEntry | null, name: string, value: string): HeaderEntry {
  const normalized = varta.web.trimHttpWhitespace(value);
  checkName('Headers.append', name);
  checkValue('Headers.append', normalized);
  return insert(first, varta.web.asciiLowercase(name), normalized);
}

// The first entry named `name`, or null.
function find(first: HeaderEntry | null, name: string): HeaderEntry | null {
  let at = first;
  while (at instanceof HeaderEntry) {
    if (at.name >= name) {
      return at.name === name ? at : null;
    }
    at = at.next;
  }
  return null;
}

// The entry after the run of entries named like `start`.
function runEnd(start: HeaderEntry): HeaderEntry | null {
  let next = start.next;
  while (next instanceof HeaderEntry && next.name === start.name) {
    next = next.next;
  }
  return next;
}

// The run of `start` with its values combined.
function combined(start: HeaderEntry): string {
  let value = start.value;
  let next = start.next;
  while (next instanceof HeaderEntry && next.name === start.name) {
    value = value + SEPARATOR + next.value;
    next = next.next;
  }
  return value;
}

// The sort-and-combine list: one entry per name with its values combined,
// except that each `set-cookie` value is its own entry.
function sortedEntries(first: HeaderEntry | null): string[][] {
  const entries: string[][] = [];
  let at = first;
  while (at instanceof HeaderEntry) {
    if (at.name === SET_COOKIE) {
      entries.push([at.name, at.value]);
      at = at.next;
    } else {
      entries.push([at.name, combined(at)]);
      at = runEnd(at);
    }
  }
  return entries;
}

export class Headers {
  private first: HeaderEntry | null;
  // Counts mutations, so that forEach rereads the list only after a change.
  private version: number;

  constructor(init?: HeadersInit) {
    let first: HeaderEntry | null = null;
    if (init instanceof Headers) {
      // The source is sorted already: copy it in order.
      let last: HeaderEntry | null = null;
      let at = init.first;
      while (at instanceof HeaderEntry) {
        const entry = new HeaderEntry(at.name, at.value, null);
        if (last instanceof HeaderEntry) {
          last.next = entry;
        } else {
          first = entry;
        }
        last = entry;
        at = at.next;
      }
    } else if (init) {
      if (Array.isArray(init)) {
        // WebIDL converts every pair before the list is filled.
        const pairs: string[][] = [];
        for (let index = 0; index < init.length; index++) {
          if (!Object.hasOwn(init, index)) {
            throw new TypeError(`Headers constructor: init[${index}] (undefined) is not iterable.`);
          }
          const pair = init[index]!;
          const converted: string[] = [];
          for (let part = 0; part < pair.length; part++) {
            const text = Object.hasOwn(pair, part) ? pair[part]! : 'undefined';
            byteString(text);
            converted.push(text);
          }
          pairs.push(converted);
        }
        for (let index = 0; index < pairs.length; index++) {
          const pair = pairs[index]!;
          if (pair.length !== 2) {
            throw new TypeError(
              `Headers constructor: expected name/value pair to be length 2, found ${pair.length}.`,
            );
          }
          first = append(first, pair[0]!, pair[1]!);
        }
      } else {
        const keys = Object.keys(init);
        for (let index = 0; index < keys.length; index++) {
          const key = keys[index]!;
          byteString(key);
          byteString(init[key]!);
        }
        for (let index = 0; index < keys.length; index++) {
          const key = keys[index]!;
          first = append(first, key, init[key]!);
        }
      }
    }
    this.first = first;
    this.version = 0;
  }

  append(name: string, value: string): void {
    byteString(name);
    byteString(value);
    this.first = append(this.first, name, value);
    this.version++;
  }

  delete(name: string): void {
    byteString(name);
    checkName('Headers.delete', name);
    const lowered = varta.web.asciiLowercase(name);
    let previous: HeaderEntry | null = null;
    let at = this.first;
    while (at instanceof HeaderEntry && at.name < lowered) {
      previous = at;
      at = at.next;
    }
    if (!(at instanceof HeaderEntry) || at.name !== lowered) {
      return;
    }
    const after = runEnd(at);
    if (previous instanceof HeaderEntry) {
      previous.next = after;
    } else {
      this.first = after;
    }
    this.version++;
  }

  get(name: string): string | null {
    byteString(name);
    checkName('Headers.get', name);
    const start = find(this.first, varta.web.asciiLowercase(name));
    return start instanceof HeaderEntry ? combined(start) : null;
  }

  getSetCookie(): string[] {
    const cookies: string[] = [];
    let at = find(this.first, SET_COOKIE);
    while (at instanceof HeaderEntry && at.name === SET_COOKIE) {
      cookies.push(at.value);
      at = at.next;
    }
    return cookies;
  }

  has(name: string): boolean {
    byteString(name);
    checkName('Headers.has', name);
    return find(this.first, varta.web.asciiLowercase(name)) instanceof HeaderEntry;
  }

  set(name: string, value: string): void {
    byteString(name);
    byteString(value);
    const normalized = varta.web.trimHttpWhitespace(value);
    checkName('Headers.set', name);
    checkValue('Headers.set', normalized);
    const lowered = varta.web.asciiLowercase(name);
    const start = find(this.first, lowered);
    if (start instanceof HeaderEntry) {
      start.value = normalized;
      start.next = runEnd(start);
    } else {
      this.first = insert(this.first, lowered, normalized);
    }
    this.version++;
  }

  // Like the WebIDL pair iterator, each step reads the current sorted list,
  // so changes made by the callback affect the remaining steps.
  forEach(callback: (value: string, key: string, parent: Headers) => void): void {
    let entries = sortedEntries(this.first);
    let version = this.version;
    for (let index = 0; ; index++) {
      if (version !== this.version) {
        entries = sortedEntries(this.first);
        version = this.version;
      }
      if (index >= entries.length) {
        return;
      }
      const entry = entries[index]!;
      callback(entry[1]!, entry[0]!, this);
    }
  }

  // Fresh snapshot arrays of the sorted and combined list.
  entries(): string[][] {
    return sortedEntries(this.first);
  }

  keys(): string[] {
    const keys: string[] = [];
    let at = this.first;
    while (at instanceof HeaderEntry) {
      keys.push(at.name);
      at = at.name === SET_COOKIE ? at.next : runEnd(at);
    }
    return keys;
  }

  values(): string[] {
    const values: string[] = [];
    let at = this.first;
    while (at instanceof HeaderEntry) {
      if (at.name === SET_COOKIE) {
        values.push(at.value);
        at = at.next;
      } else {
        values.push(combined(at));
        at = runEnd(at);
      }
    }
    return values;
  }
}
