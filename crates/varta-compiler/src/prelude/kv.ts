// Engine prelude of checked-f64-utf16-v38 (`kv`): Deno KV compatible
// storage. The compiler adds it to a module that names `varta.kv.open` (bound
// under that dotted name, which no source declaration can spell) or one of its
// exported types; see docs/architecture.md (KV storage).
//
// The runtime does the storage on Deno KV's own SQLite backend: the
// `varta.kv.*` intrinsics below exist only in engine preludes.
// `varta.kv.openFile` checks the exact `kv.read` grant (and `kv.write` for a
// writable database) and opens the pinned file; `varta.kv.stage` hands one
// key or value to the host, which encodes it at once (V8 serializer bytes, a
// KvU64 or raw bytes, chosen by the value's static type); `varta.kv.read`
// and `varta.kv.commit` run one snapshot read or one atomic write over staged
// ids; `varta.kv.key<T>` and `varta.kv.value<T>` decode an entry of the last
// read strictly into `T`, like checked JSON parsing. A failed operation
// leaves `<ErrorName>\n<message>` in `varta.kv.text()`. Every operation
// completes within the call, so each promise below settles in the same turn.

// The descriptor and result codes of `varta_common::kv`; a compiler test
// keeps them in step.
const DESCRIPTOR_VERSION = 'varta-kv-1';
const SEPARATOR = '\n';
const FAILED = -1;
const CHECK_FAILED = 0;
const COMMITTED = 1;
const SELECTOR_KEY = 0;
const SELECTOR_PREFIX = 1;
const SELECTOR_PREFIX_START = 2;
const SELECTOR_PREFIX_END = 3;
const SELECTOR_START_END = 4;
const MUTATION_SET = 0;
const MUTATION_DELETE = 1;
const MUTATION_SUM = 2;
const MUTATION_MIN = 3;
const MUTATION_MAX = 4;
const FIELD_VERSIONSTAMP = 0;
const FIELD_ENCODING = 1;
const FIELD_U64 = 2;
const FIELD_BYTES = 3;
const FIELD_CURSOR = 4;
const FIELD_RANGE = 5;
const ENCODING_U64 = '2';
const ENCODING_BYTES = '3';
const DEFAULT_BATCH_SIZE = 100;
const MAX_DEFAULT_BATCH_SIZE = 500;
const NOT_ALLOWED = 'NotAllowedError';
const NOT_FOUND = 'NotFoundError';
const RANGE_ERROR = 'RangeError';
const MAX_U64 = 18446744073709551615n;

export type KvKeyPart = string | number;
export type KvKey = KvKeyPart[];

export interface KvEntry<T> {
  key: KvKey;
  value: T;
  versionstamp: string;
}

// Deno's `KvEntryMaybe<T>` is the union of an entry and the missing entry;
// here one record whose `value` and `versionstamp` are null when missing.
export interface KvEntryMaybe<T> {
  key: KvKey;
  value: T | null;
  versionstamp: string | null;
}

export interface KvListSelector {
  prefix?: KvKey;
  start?: KvKey;
  end?: KvKey;
}

export interface KvListOptions {
  limit?: number;
  cursor?: string;
  reverse?: boolean;
  batchSize?: number;
}

export interface KvSetOptions {
  expireIn?: number;
}

export interface AtomicCheck {
  key: KvKey;
  versionstamp: string | null;
}

export interface KvCommitResult {
  ok: true;
  versionstamp: string;
}

export interface KvCommitError {
  ok: false;
}

// Throw the error a failed operation reports.
function raise(): never {
  const text = varta.kv.text();
  const at = text.indexOf(SEPARATOR);
  const name = text.slice(0, at);
  const message = text.slice(at + 1);
  if (name === NOT_ALLOWED || name === NOT_FOUND) {
    throw new DOMException(message, name);
  }
  if (name === RANGE_ERROR) {
    throw new RangeError(message);
  }
  throw new TypeError(message);
}

function stageKey(key: KvKey): number {
  const id = varta.kv.stage(key);
  if (id === FAILED) {
    raise();
  }
  return id;
}

function stageValue<T>(value: T): number {
  const id = varta.kv.stage(value);
  if (id === FAILED) {
    raise();
  }
  return id;
}

function stageU64(value: KvU64): number {
  const id = varta.kv.stage(value);
  if (id === FAILED) {
    raise();
  }
  return id;
}

// The decimal text of a KvU64 (its stored form, `v_encoding` 2).
export function u64Text(value: KvU64): string {
  return value.value.toString();
}

// A stored KvU64 of entry `index` of the last read.
export function u64At(index: number): KvU64 {
  if (varta.kv.entry(index, FIELD_ENCODING) !== ENCODING_U64) {
    throw new TypeError('the stored value is not a KvU64');
  }
  return new KvU64(BigInt(varta.kv.entry(index, FIELD_U64)));
}

// A stored Uint8Array (`v_encoding` 3) of entry `index` of the last read.
export function bytesAt(index: number): Uint8Array {
  if (varta.kv.entry(index, FIELD_ENCODING) !== ENCODING_BYTES) {
    throw new TypeError('the stored value is not a Uint8Array');
  }
  return varta.bytes.adopt(varta.text.units(varta.kv.entry(index, FIELD_BYTES)));
}

function range(
  selector: number,
  first: number,
  second: string,
  limit: number,
  reverse: boolean,
  cursor: string,
): string {
  return (
    selector + SEPARATOR + first + SEPARATOR + second + SEPARATOR + limit + SEPARATOR +
    (reverse ? '1' : '0') + SEPARATOR + cursor
  );
}

// Run one snapshot read; returns its entry count.
function snapshot(id: number, ranges: string[]): number {
  let text = DESCRIPTOR_VERSION + SEPARATOR + id + SEPARATOR + ranges.length;
  for (const item of ranges) {
    text += SEPARATOR + item;
  }
  const count = varta.kv.read(text);
  if (count === FAILED) {
    raise();
  }
  return count;
}

function entryAt<T>(index: number): KvEntry<T> {
  return {
    key: varta.kv.key<KvKey>(index),
    value: varta.kv.value<T>(index),
    versionstamp: varta.kv.entry(index, FIELD_VERSIONSTAMP),
  };
}

function validateExpireIn(expireIn: number | undefined): void {
  if (expireIn !== undefined && (!Number.isInteger(expireIn) || expireIn < 0)) {
    throw new TypeError(`expireIn must be a non-negative integer: received ${expireIn}`);
  }
}

export class KvU64 {
  readonly value: bigint;

  constructor(value: bigint) {
    if (value < 0n) {
      throw new RangeError(`Value must be a positive bigint: received ${value}`);
    }
    if (value > MAX_U64) {
      throw new RangeError('Value must fit in a 64-bit unsigned integer');
    }
    this.value = value;
  }

  toString(): string {
    return this.value.toString();
  }
}

// One mutation of an atomic operation; its key and value are staged only
// when the operation commits, so no other read can interleave.
interface Mutation {
  code: number;
  key: KvKey;
  value: () => number;
  hasValue: boolean;
  expireIn: string;
}

function noValue(): number {
  return FAILED;
}

function commitWrite(id: number, checks: AtomicCheck[], mutations: Mutation[]): number {
  let text = DESCRIPTOR_VERSION + SEPARATOR + id + SEPARATOR + checks.length;
  for (const check of checks) {
    const versionstamp = check.versionstamp;
    text += SEPARATOR + stageKey(check.key) + SEPARATOR + (versionstamp === null ? '' : versionstamp);
  }
  text += SEPARATOR + mutations.length;
  for (const mutation of mutations) {
    const key = stageKey(mutation.key);
    const value = mutation.hasValue ? '' + mutation.value() : '';
    text += SEPARATOR + mutation.code + SEPARATOR + key + SEPARATOR + value + SEPARATOR + mutation.expireIn;
  }
  const outcome = varta.kv.commit(text);
  if (outcome === FAILED) {
    raise();
  }
  return outcome;
}

export class AtomicOperation {
  #id: number;
  #checks: AtomicCheck[] = [];
  #mutations: Mutation[] = [];

  constructor(id: number) {
    this.#id = id;
  }

  check(...checks: AtomicCheck[]): AtomicOperation {
    for (const check of checks) {
      this.#checks.push({ key: check.key, versionstamp: check.versionstamp });
    }
    return this;
  }

  set<T>(key: KvKey, value: T, options?: KvSetOptions): AtomicOperation {
    const expireIn = options?.expireIn;
    validateExpireIn(expireIn);
    this.#mutations.push({
      code: MUTATION_SET,
      key,
      value: () => stageValue(value),
      hasValue: true,
      expireIn: expireIn === undefined ? '' : '' + expireIn,
    });
    return this;
  }

  delete(key: KvKey): AtomicOperation {
    this.#mutations.push({ code: MUTATION_DELETE, key, value: noValue, hasValue: false, expireIn: '' });
    return this;
  }

  sum(key: KvKey, n: bigint): AtomicOperation {
    return this.#counter(MUTATION_SUM, key, n);
  }

  min(key: KvKey, n: bigint): AtomicOperation {
    return this.#counter(MUTATION_MIN, key, n);
  }

  max(key: KvKey, n: bigint): AtomicOperation {
    return this.#counter(MUTATION_MAX, key, n);
  }

  #counter(code: number, key: KvKey, n: bigint): AtomicOperation {
    const operand = new KvU64(n);
    this.#mutations.push({ code, key, value: () => stageU64(operand), hasValue: true, expireIn: '' });
    return this;
  }

  async commit(): Promise<KvCommitResult | KvCommitError> {
    const outcome = commitWrite(this.#id, this.#checks, this.#mutations);
    if (outcome === CHECK_FAILED) {
      return { ok: false };
    }
    return { ok: true, versionstamp: varta.kv.text() };
  }
}

export class KvListIterator<T> {
  #id: number;
  #selector: number;
  #first: KvKey;
  #second: KvKey | null;
  #limit: number;
  #batchSize: number;
  #reverse: boolean;
  #cursor: string;
  #started: boolean = false;

  constructor(
    id: number,
    selector: number,
    first: KvKey,
    second: KvKey | null,
    limit: number,
    batchSize: number,
    reverse: boolean,
    cursor: string,
  ) {
    this.#id = id;
    this.#selector = selector;
    this.#first = first;
    this.#second = second;
    this.#limit = limit;
    this.#batchSize = batchSize;
    this.#reverse = reverse;
    this.#cursor = cursor;
  }

  get cursor(): string {
    if (!this.#started && this.#cursor === '') {
      throw new Error('Cannot get cursor before first iteration');
    }
    return this.#cursor;
  }

  async *[Symbol.asyncIterator](): AsyncGenerator<KvEntry<T>> {
    let count = 0;
    while (true) {
      const first = stageKey(this.#first);
      const pair = this.#second;
      const second = pair === null ? '' : '' + stageKey(pair);
      const found = snapshot(this.#id, [
        range(this.#selector, first, second, this.#batchSize, this.#reverse, this.#cursor),
      ]);
      // Decode the whole batch first: a read inside the loop body replaces
      // the runtime's entries.
      const batch: KvEntry<T>[] = [];
      const cursors: string[] = [];
      for (let index = 0; index < found; index++) {
        batch.push(entryAt<T>(index));
        cursors.push(varta.kv.entry(index, FIELD_CURSOR));
      }
      for (let index = 0; index < batch.length; index++) {
        if (this.#limit > 0 && count >= this.#limit) {
          return;
        }
        this.#started = true;
        this.#cursor = cursors[index];
        count++;
        yield batch[index];
      }
      if (this.#limit > 0 && count >= this.#limit) {
        return;
      }
      if (found < this.#batchSize) {
        // Exhausted: Deno's cursor is then empty.
        this.#started = true;
        this.#cursor = '';
        return;
      }
    }
  }
}

export class Kv {
  #id: number;

  constructor(id: number) {
    this.#id = id;
  }

  async get<T>(key: KvKey): Promise<KvEntryMaybe<T>> {
    const found = snapshot(this.#id, [range(SELECTOR_KEY, stageKey(key), '', 1, false, '')]);
    if (found === 0) {
      return { key, value: null, versionstamp: null };
    }
    const entry = entryAt<T>(0);
    return { key: entry.key, value: entry.value, versionstamp: entry.versionstamp };
  }

  async getMany<T>(keys: KvKey[]): Promise<KvEntryMaybe<T>[]> {
    const ranges: string[] = [];
    for (const key of keys) {
      ranges.push(range(SELECTOR_KEY, stageKey(key), '', 1, false, ''));
    }
    const found = snapshot(this.#id, ranges);
    const entries: KvEntryMaybe<T>[] = [];
    for (const key of keys) {
      entries.push({ key, value: null, versionstamp: null });
    }
    for (let index = 0; index < found; index++) {
      const entry = entryAt<T>(index);
      entries[Number(varta.kv.entry(index, FIELD_RANGE))] = {
        key: entry.key,
        value: entry.value,
        versionstamp: entry.versionstamp,
      };
    }
    return entries;
  }

  list<T>(selector: KvListSelector, options?: KvListOptions): KvListIterator<T> {
    const limit = options?.limit;
    if (limit !== undefined && (!Number.isInteger(limit) || limit <= 0)) {
      throw new Error(`Limit must be a positive integer: received ${limit}`);
    }
    const explicit = options?.batchSize;
    let batchSize = explicit ?? limit ?? DEFAULT_BATCH_SIZE;
    if (!Number.isInteger(batchSize) || batchSize <= 0) {
      throw new Error(`batchSize must be a positive integer: received ${batchSize}`);
    }
    if (explicit === undefined && batchSize > MAX_DEFAULT_BATCH_SIZE) {
      batchSize = MAX_DEFAULT_BATCH_SIZE;
    }
    const prefix = selector.prefix;
    const start = selector.start;
    const end = selector.end;
    let code = SELECTOR_START_END;
    let first: KvKey = [];
    let second: KvKey | null = null;
    if (prefix !== undefined) {
      if (start !== undefined && end !== undefined) {
        throw new TypeError(
          "Selector can not specify both 'start' and 'end' key when specifying 'prefix'",
        );
      }
      first = prefix;
      if (start !== undefined) {
        code = SELECTOR_PREFIX_START;
        second = start;
      } else if (end !== undefined) {
        code = SELECTOR_PREFIX_END;
        second = end;
      } else {
        code = SELECTOR_PREFIX;
      }
    } else if (start !== undefined && end !== undefined) {
      first = start;
      second = end;
    } else {
      throw new TypeError("Selector must specify either 'prefix' or both 'start' and 'end' key");
    }
    return new KvListIterator<T>(
      this.#id,
      code,
      first,
      second,
      limit ?? 0,
      batchSize,
      options?.reverse ?? false,
      options?.cursor ?? '',
    );
  }

  async set<T>(key: KvKey, value: T, options?: KvSetOptions): Promise<KvCommitResult> {
    const expireIn = options?.expireIn;
    validateExpireIn(expireIn);
    const outcome = commitWrite(this.#id, [], [
      {
        code: MUTATION_SET,
        key,
        value: () => stageValue(value),
        hasValue: true,
        expireIn: expireIn === undefined ? '' : '' + expireIn,
      },
    ]);
    if (outcome !== COMMITTED) {
      throw new TypeError('Failed to set value');
    }
    return { ok: true, versionstamp: varta.kv.text() };
  }

  async delete(key: KvKey): Promise<void> {
    const outcome = commitWrite(this.#id, [], [
      { code: MUTATION_DELETE, key, value: noValue, hasValue: false, expireIn: '' },
    ]);
    if (outcome !== COMMITTED) {
      throw new TypeError('Failed to set value');
    }
  }

  atomic(): AtomicOperation {
    return new AtomicOperation(this.#id);
  }

  close(): void {
    varta.kv.release(this.#id);
  }
}

export async function kvOpen(path: string): Promise<Kv> {
  const id = varta.kv.openFile(path);
  if (id === FAILED) {
    raise();
  }
  return new Kv(id);
}
