// Engine prelude of checked-f64-utf16-v37 (`collections`): `Map` and `Set`
// over typed guest storage, with no host import and no capability. The
// compiler loads it when a program names `Map`, `Set`, `ReadonlyMap` or
// `ReadonlySet`, lays every `Map<K, V>`/`Set<T>` out as the private
// `MapStore<K, V>`/`SetStore<T>` record below and calls these generic
// functions for its operations; see docs/architecture.md (Collections).
//
// Entries live in insertion order in parallel vectors. A chained hash index
// finds a key in expected constant time: `varta.collections.hash` hashes a
// string by its code units, a number by its value (so -0 and +0, and every
// NaN, share a hash) and an object by its stable heap address. Keys compare
// by SameValueZero. A deletion only marks its entry (a negated serial);
// insertion compacts the vectors when they are full. Iteration is live: a
// cursor remembers the serial of the last visited entry, so it visits entries
// added during iteration, skips deleted ones and survives compaction and
// `clear`, as in ECMAScript.
//
// `varta.array.get(array, index)` is an element read without the
// `undefined` of a hole: every vector here is filled densely and only read at
// positions below its length (any other read traps).

const EMPTY = -1;
// Entry ceiling of one collection: its vectors share the array element bound
// (value::MAX_ARRAY_ELEMENTS).
const MAX_ENTRIES = 1024;
const HALF = 2;

export interface CollectionIndex {
  // First entry of each hash chain, or EMPTY; a power-of-two count.
  buckets: number[];
  // The next entry of the same chain, or EMPTY.
  chain: number[];
  hashes: number[];
  // Insertion serial of each entry, negated once the entry is deleted.
  serials: number[];
  // Live entries.
  size: number;
  // The last serial issued; never reset, so cursors survive `clear`.
  serial: number;
}

export interface SetStore<T> {
  index: CollectionIndex;
  keys: T[];
}

export interface MapStore<K, V> {
  index: CollectionIndex;
  keys: K[];
  values: V[];
}

function emptyBuckets(count: number): number[] {
  const buckets: number[] = [];
  buckets.length = count;
  return buckets.fill(EMPTY);
}

function createIndex(serial: number): CollectionIndex {
  return {
    buckets: [EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY],
    chain: [],
    hashes: [],
    serials: [],
    size: 0,
    serial,
  };
}

function bucketOf(index: CollectionIndex, hash: number): number {
  return hash & (index.buckets.length - 1);
}

// The position of the live entry whose key is SameValueZero `key`, or EMPTY.
function find<K>(index: CollectionIndex, keys: K[], key: K, hash: number): number {
  let at = varta.array.get(index.buckets, bucketOf(index, hash));
  while (at !== EMPTY) {
    if (varta.array.get(index.serials, at) > 0 && varta.array.get(index.hashes, at) === hash) {
      const candidate = varta.array.get(keys, at);
      if (candidate === key || (candidate !== candidate && key !== key)) {
        return at;
      }
    }
    at = varta.array.get(index.chain, at);
  }
  return EMPTY;
}

// Rebuild the chains of the live entries over `count` buckets.
function rehash(index: CollectionIndex, count: number): void {
  index.buckets = emptyBuckets(count);
  for (let at = 0; at < index.serials.length; at++) {
    if (varta.array.get(index.serials, at) > 0) {
      const bucket = bucketOf(index, varta.array.get(index.hashes, at));
      index.chain[at] = varta.array.get(index.buckets, bucket);
      index.buckets[bucket] = at;
    } else {
      index.chain[at] = EMPTY;
    }
  }
}

// Drop deleted entries in place, keeping order; returns the kept positions so
// that a map moves its values the same way.
function compact<K>(index: CollectionIndex, keys: K[]): number[] {
  const kept: number[] = [];
  const hashes: number[] = [];
  const serials: number[] = [];
  const survivors: K[] = [];
  for (let at = 0; at < index.serials.length; at++) {
    if (varta.array.get(index.serials, at) > 0) {
      kept.push(at);
      hashes.push(varta.array.get(index.hashes, at));
      serials.push(varta.array.get(index.serials, at));
      survivors.push(varta.array.get(keys, at));
    }
  }
  index.hashes = hashes;
  index.serials = serials;
  index.chain = [];
  for (let at = 0; at < serials.length; at++) {
    index.chain.push(EMPTY);
  }
  keys.length = 0;
  for (let at = 0; at < survivors.length; at++) {
    keys.push(varta.array.get(survivors, at));
  }
  rehash(index, index.buckets.length);
  return kept;
}

// Whether a new entry first needs the deleted entries compacted away; throws
// when every entry is live and the vectors are full.
function full<K>(index: CollectionIndex, keys: K[], name: string): boolean {
  if (keys.length < MAX_ENTRIES) {
    return false;
  }
  if (index.size === keys.length) {
    throw new RangeError(`${name} maximum size exceeded`);
  }
  return true;
}

// Append a new entry for `key` (absent) at the end of the vectors.
function append<K>(index: CollectionIndex, keys: K[], key: K, hash: number): void {
  const at = keys.length;
  keys[at] = key;
  index.hashes[at] = hash;
  index.serial = index.serial + 1;
  index.serials[at] = index.serial;
  index.chain[at] = EMPTY;
  index.size = index.size + 1;
  if (keys.length > index.buckets.length && index.buckets.length < MAX_ENTRIES) {
    rehash(index, index.buckets.length * HALF);
    return;
  }
  const bucket = bucketOf(index, hash);
  index.chain[at] = varta.array.get(index.buckets, bucket);
  index.buckets[bucket] = at;
}

function remove(index: CollectionIndex, at: number): void {
  index.serials[at] = -varta.array.get(index.serials, at);
  index.size = index.size - 1;
}

// The first position whose entry was inserted after serial `last`, resuming
// at `at` when no entry moved since the cursor read position `at - 1`.
function resume(index: CollectionIndex, at: number, last: number): number {
  const serials = index.serials;
  if (last === 0) {
    return 0;
  }
  if (at > 0 && at <= serials.length && Math.abs(varta.array.get(serials, at - 1)) === last) {
    return at;
  }
  let low = 0;
  let high = serials.length;
  while (low < high) {
    const middle = Math.floor((low + high) / HALF);
    if (Math.abs(varta.array.get(serials, middle)) <= last) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

// The first live position at or after `at`, or EMPTY.
function live(index: CollectionIndex, at: number): number {
  let next = at;
  while (next < index.serials.length) {
    if (varta.array.get(index.serials, next) > 0) {
      return next;
    }
    next++;
  }
  return EMPTY;
}

// ---- Set<T>

export function setCreate<T>(): SetStore<T> {
  const keys: T[] = [];
  return { index: createIndex(0), keys };
}

export function setHas<T>(set: SetStore<T>, value: T): boolean {
  return find(set.index, set.keys, value, varta.collections.hash(value)) !== EMPTY;
}

export function setAdd<T>(set: SetStore<T>, value: T): void {
  const key = varta.collections.key(value);
  const hash = varta.collections.hash(key);
  if (find(set.index, set.keys, key, hash) !== EMPTY) {
    return;
  }
  if (full(set.index, set.keys, 'Set')) {
    compact(set.index, set.keys);
  }
  append(set.index, set.keys, key, hash);
}

export function setDelete<T>(set: SetStore<T>, value: T): boolean {
  const at = find(set.index, set.keys, value, varta.collections.hash(value));
  if (at === EMPTY) {
    return false;
  }
  remove(set.index, at);
  return true;
}

export function setClear<T>(set: SetStore<T>): void {
  set.index = createIndex(set.index.serial);
  set.keys = [];
}

export function setSize<T>(set: SetStore<T>): number {
  return set.index.size;
}

export function setFromArray<T>(values: T[]): SetStore<T> {
  const set = setCreate<T>();
  for (let at = 0; at < values.length; at++) {
    setAdd(set, varta.array.get(values, at));
  }
  return set;
}

export function* setValues<T>(set: SetStore<T>): Generator<T, void, undefined> {
  let at = 0;
  let last = 0;
  for (;;) {
    at = live(set.index, resume(set.index, at, last));
    if (at === EMPTY) {
      return;
    }
    last = varta.array.get(set.index.serials, at);
    const value = varta.array.get(set.keys, at);
    at++;
    yield value;
  }
}

export function* setEntries<T>(set: SetStore<T>): Generator<[T, T], void, undefined> {
  for (const value of setValues(set)) {
    yield [value, value];
  }
}

export function setCopy<T>(source: SetStore<T>): SetStore<T> {
  const set = setCreate<T>();
  for (const value of setValues(source)) {
    setAdd(set, value);
  }
  return set;
}

export function setToArray<T>(set: SetStore<T>): T[] {
  const values: T[] = [];
  for (const value of setValues(set)) {
    values.push(value);
  }
  return values;
}

export function setForEach<T>(set: SetStore<T>, callback: (value: T, key: T) => void): void {
  for (const value of setValues(set)) {
    callback(value, value);
  }
}

// ---- Map<K, V>

export function mapCreate<K, V>(): MapStore<K, V> {
  const keys: K[] = [];
  const values: V[] = [];
  return { index: createIndex(0), keys, values };
}

export function mapHas<K, V>(map: MapStore<K, V>, key: K): boolean {
  return find(map.index, map.keys, key, varta.collections.hash(key)) !== EMPTY;
}

export function mapGet<K, V>(map: MapStore<K, V>, key: K): V | undefined {
  const at = find(map.index, map.keys, key, varta.collections.hash(key));
  if (at === EMPTY) {
    return undefined;
  }
  return varta.array.get(map.values, at);
}

export function mapSet<K, V>(map: MapStore<K, V>, key: K, value: V): void {
  const normalized = varta.collections.key(key);
  const hash = varta.collections.hash(normalized);
  const at = find(map.index, map.keys, normalized, hash);
  if (at !== EMPTY) {
    map.values[at] = value;
    return;
  }
  if (full(map.index, map.keys, 'Map')) {
    const kept = compact(map.index, map.keys);
    const values: V[] = [];
    for (let position = 0; position < kept.length; position++) {
      values.push(varta.array.get(map.values, varta.array.get(kept, position)));
    }
    map.values = values;
  }
  append(map.index, map.keys, normalized, hash);
  map.values[map.values.length] = value;
}

export function mapDelete<K, V>(map: MapStore<K, V>, key: K): boolean {
  const at = find(map.index, map.keys, key, varta.collections.hash(key));
  if (at === EMPTY) {
    return false;
  }
  remove(map.index, at);
  return true;
}

export function mapClear<K, V>(map: MapStore<K, V>): void {
  map.index = createIndex(map.index.serial);
  map.keys = [];
  map.values = [];
}

export function mapSize<K, V>(map: MapStore<K, V>): number {
  return map.index.size;
}

export function mapFromEntries<K, V>(entries: [K, V][]): MapStore<K, V> {
  const map = mapCreate<K, V>();
  for (let at = 0; at < entries.length; at++) {
    const entry = varta.array.get(entries, at);
    mapSet(map, entry[0], entry[1]);
  }
  return map;
}

// The live positions of a map, in order (one cursor for all three iterators).
function* mapPositions<K, V>(map: MapStore<K, V>): Generator<number, void, undefined> {
  let at = 0;
  let last = 0;
  for (;;) {
    at = live(map.index, resume(map.index, at, last));
    if (at === EMPTY) {
      return;
    }
    last = varta.array.get(map.index.serials, at);
    const position = at;
    at++;
    yield position;
  }
}

export function* mapKeys<K, V>(map: MapStore<K, V>): Generator<K, void, undefined> {
  for (const at of mapPositions(map)) {
    yield varta.array.get(map.keys, at);
  }
}

export function* mapValues<K, V>(map: MapStore<K, V>): Generator<V, void, undefined> {
  for (const at of mapPositions(map)) {
    yield varta.array.get(map.values, at);
  }
}

export function* mapEntries<K, V>(map: MapStore<K, V>): Generator<[K, V], void, undefined> {
  for (const at of mapPositions(map)) {
    yield [varta.array.get(map.keys, at), varta.array.get(map.values, at)];
  }
}

export function mapCopy<K, V>(source: MapStore<K, V>): MapStore<K, V> {
  const map = mapCreate<K, V>();
  for (const at of mapPositions(source)) {
    mapSet(map, varta.array.get(source.keys, at), varta.array.get(source.values, at));
  }
  return map;
}

export function mapToArray<K, V>(map: MapStore<K, V>): [K, V][] {
  const entries: [K, V][] = [];
  for (const at of mapPositions(map)) {
    entries.push([varta.array.get(map.keys, at), varta.array.get(map.values, at)]);
  }
  return entries;
}

export function mapForEach<K, V>(map: MapStore<K, V>, callback: (value: V, key: K) => void): void {
  for (const at of mapPositions(map)) {
    callback(varta.array.get(map.values, at), varta.array.get(map.keys, at));
  }
}

// `[...iterator]` of a collection iterator.
export function drain<T>(values: Generator<T, void, undefined>): T[] {
  const drained: T[] = [];
  for (const value of values) {
    drained.push(value);
  }
  return drained;
}

// ---- key hashing (`varta.collections.hash` selects one by the key type)

const FNV_OFFSET = 0x811c9dc5;
const FNV_PRIME = 0x01000193;
const MIX = 0x9e3779b1;
const FINAL = 0x85ebca6b;
const WORD = 4294967296;

export function hashString(key: string): number {
  let hash = FNV_OFFSET | 0;
  for (let at = 0; at < key.length; at++) {
    hash = Math.imul(hash ^ varta.text.unit(key, at), FNV_PRIME);
  }
  return hash >>> 0;
}

export function hashNumber(key: number): number {
  if (key !== key) {
    return 0;
  }
  const low = key | 0;
  if (low === key) {
    return low >>> 0;
  }
  const high = Math.floor(key / WORD) | 0;
  const fraction = ((key - Math.floor(key)) * WORD) | 0;
  return Math.imul(low ^ Math.imul(high, MIX) ^ fraction, FINAL) >>> 0;
}

export function hashBoolean(key: boolean): number {
  return key ? 1 : HALF;
}
