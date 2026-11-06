// Engine prelude of checked-f64-utf16-v38 (`collection_hashing`): `Map` and
// `Set` over typed guest storage, with no host import and no capability. It
// replaces the v37 `collections` prelude under v38 and keeps its exported
// functions and storage interface names, so `lower::collections` calls it the
// same way; see docs/architecture.md (Collection hashing).
//
// Entries live in insertion order in parallel vectors (keys, values and
// serials). An open-addressing table of entry positions finds a key in
// expected constant time, with the dictionary table encoding: a hole is an
// empty slot, a present +0 a tombstone and a present `p + 1` the entry at
// position `p`. The probes are allocation-free guest helpers selected by the
// key type (`varta.collections.lookup`, `insert`, `unlink`, `indexKeys`): strings
// and BigInts hash their code units (FNV-1a) and compare by content, numbers
// hash their value (every NaN and both zeros alike) and compare by
// SameValueZero, and booleans and references hash their slot bits (a stable
// heap address for references) and compare by identity.
//
// A deletion tombstones the entry's slot, negates its serial and releases its
// key and value (both vector positions become holes). An insertion that
// would pass half the table's capacity, or a deletion that leaves more
// deleted than live entries, rebuilds: the live entries move to dense
// vectors in order and a fresh table of three slots per entry indexes them,
// so every operation costs amortized constant time. Iteration is live: a
// cursor remembers the serial of the last visited entry, so it visits
// entries added during iteration, skips deleted ones and survives rebuilds
// and `clear`, as in ECMAScript. `first` bounds the deleted prefix, so
// repeatedly taking the first entry of a queue-like collection stays cheap.
//
// `varta.array.get(array, index)` is an element read without the
// `undefined` of a hole: the prelude reads only live positions, which are
// present.

const NONE = -1;
// Slots of the largest table: the array element bound of the profile
// (value::MAX_ARRAY_ELEMENTS until lifted). A table holds at most half as
// many entries.
const MAX_TABLE = 1024;
const TABLE_MIN = 8;
const SLOTS_PER_ENTRY = 3;
const LOAD = 2;
const HALF = 2;

export interface CollectionIndex {
  // The open-addressing table; its capacity is its power-of-two length.
  table: number[];
  // Insertion serial of each position, negated once its entry is deleted.
  serials: number[];
  // Live entries.
  size: number;
  // The last serial issued; never reset, so cursors survive `clear`.
  serial: number;
  // Every position below `first` holds a deleted entry.
  first: number;
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

function createIndex(serial: number): CollectionIndex {
  return {
    table: varta.collections.table(TABLE_MIN),
    serials: [],
    size: 0,
    serial,
    first: 0,
  };
}

// The table capacity for `entries` entries: three slots per entry, a power
// of two from TABLE_MIN up to MAX_TABLE.
function capacityFor(entries: number): number {
  let capacity = TABLE_MIN;
  while (capacity < entries * SLOTS_PER_ENTRY && capacity < MAX_TABLE) {
    capacity = capacity * HALF;
  }
  return capacity;
}

// Throws when one more live entry would pass half the largest table.
function admit(index: CollectionIndex, name: string): void {
  if (LOAD * (index.size + 1) > MAX_TABLE) {
    throw new RangeError(`${name} maximum size exceeded`);
  }
}

// Whether a deletion left more deleted than live entries (beyond a small
// table's worth), so that the vectors are compacted.
function sparse(index: CollectionIndex): boolean {
  const deleted = index.serials.length - index.size;
  return deleted > TABLE_MIN && deleted > index.size;
}

// Mark the entry at `position`, whose slot was just tombstoned, deleted.
function remove(index: CollectionIndex, position: number): void {
  index.serials[position] = -varta.array.get(index.serials, position);
  index.size = index.size - 1;
}

// The serials of the live entries in order, with `first` reset.
function liveSerials(index: CollectionIndex): number[] {
  const serials: number[] = [];
  for (let at = index.first; at < index.serials.length; at++) {
    const serial = varta.array.get(index.serials, at);
    if (serial > 0) {
      serials.push(serial);
    }
  }
  return serials;
}

// A fresh table for `entries` entries indexing the dense `keys`.
function reindex<K>(index: CollectionIndex, keys: K[], entries: number): void {
  index.table = varta.collections.table(capacityFor(entries));
  varta.collections.indexKeys(keys, index.table);
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

// The first live position at or after `at`, or NONE. A search from the
// deleted prefix advances `first` past the deleted entries it crossed.
function live(index: CollectionIndex, at: number): number {
  const serials = index.serials;
  const fromFirst = at <= index.first;
  let next = fromFirst ? index.first : at;
  while (next < serials.length && varta.array.get(serials, next) < 0) {
    next++;
  }
  if (fromFirst) {
    index.first = next;
  }
  return next < serials.length ? next : NONE;
}

// ---- Set<T>

export function setCreate<T>(): SetStore<T> {
  const keys: T[] = [];
  return { index: createIndex(0), keys };
}

export function setHas<T>(set: SetStore<T>, value: T): boolean {
  return varta.collections.lookup(set.keys, set.index.table, value) !== NONE;
}

// Drop the deleted entries and index the live ones for `entries` entries.
function setRebuild<T>(set: SetStore<T>, entries: number): void {
  const index = set.index;
  if (index.size < index.serials.length) {
    const keys: T[] = [];
    for (let at = index.first; at < index.serials.length; at++) {
      if (varta.array.get(index.serials, at) > 0) {
        keys.push(varta.array.get(set.keys, at));
      }
    }
    index.serials = liveSerials(index);
    index.first = 0;
    set.keys = keys;
  }
  reindex(index, set.keys, entries);
}

// Make room for one more entry of a crowded set unless `key` is present;
// whether it was absent.
function setRoom<T>(set: SetStore<T>, key: T): boolean {
  if (varta.collections.lookup(set.keys, set.index.table, key) !== NONE) {
    return false;
  }
  admit(set.index, 'Set');
  setRebuild(set, set.index.size + 1);
  return true;
}

export function setAdd<T>(set: SetStore<T>, value: T): void {
  const key = varta.collections.key(value);
  // When one more entry position would pass half the table's capacity, the
  // insertion first rebuilds (an `insert` probe needs an empty slot). The
  // check is inline: it runs on every insertion.
  if (LOAD * (set.index.serials.length + 1) > set.index.table.length && !setRoom(set, key)) {
    return;
  }
  // A miss references the next position, where the entry is appended.
  if (varta.collections.insert(set.keys, set.index.table, key, set.keys.length) === NONE) {
    set.keys.push(key);
    set.index.serial = set.index.serial + 1;
    set.index.serials.push(set.index.serial);
    set.index.size = set.index.size + 1;
  }
}

export function setDelete<T>(set: SetStore<T>, value: T): boolean {
  const position = varta.collections.unlink(set.keys, set.index.table, value);
  if (position === NONE) {
    return false;
  }
  remove(set.index, position);
  delete set.keys[position];
  if (sparse(set.index)) {
    setRebuild(set, set.index.size);
  }
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
    if (at === NONE) {
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

// ---- ES2025 Set methods (`standard_library`; ECMA-262 §24.2.4). Every
// argument is a `Set`, whose `size`, `has` and `keys` are the built-in ones,
// so GetSetRecord observes nothing; the iteration order follows the spec's
// choice between this set's and the other set's elements.

export function setUnion<T>(set: SetStore<T>, other: SetStore<T>): SetStore<T> {
  const result = setCopy(set);
  for (const value of setValues(other)) {
    setAdd(result, value);
  }
  return result;
}

export function setIntersection<T>(set: SetStore<T>, other: SetStore<T>): SetStore<T> {
  const result = setCreate<T>();
  if (setSize(set) <= setSize(other)) {
    for (const value of setValues(set)) {
      if (setHas(other, value)) {
        setAdd(result, value);
      }
    }
  } else {
    for (const value of setValues(other)) {
      if (setHas(set, value)) {
        setAdd(result, value);
      }
    }
  }
  return result;
}

export function setDifference<T>(set: SetStore<T>, other: SetStore<T>): SetStore<T> {
  const result = setCopy(set);
  if (setSize(set) <= setSize(other)) {
    for (const value of setValues(set)) {
      if (setHas(other, value)) {
        setDelete(result, value);
      }
    }
  } else {
    for (const value of setValues(other)) {
      setDelete(result, value);
    }
  }
  return result;
}

export function setSymmetricDifference<T>(set: SetStore<T>, other: SetStore<T>): SetStore<T> {
  const result = setCopy(set);
  for (const value of setValues(other)) {
    if (setHas(set, value)) {
      setDelete(result, value);
    } else {
      setAdd(result, value);
    }
  }
  return result;
}

export function setIsSubsetOf<T>(set: SetStore<T>, other: SetStore<T>): boolean {
  if (setSize(set) > setSize(other)) {
    return false;
  }
  for (const value of setValues(set)) {
    if (!setHas(other, value)) {
      return false;
    }
  }
  return true;
}

export function setIsSupersetOf<T>(set: SetStore<T>, other: SetStore<T>): boolean {
  if (setSize(set) < setSize(other)) {
    return false;
  }
  for (const value of setValues(other)) {
    if (!setHas(set, value)) {
      return false;
    }
  }
  return true;
}

export function setIsDisjointFrom<T>(set: SetStore<T>, other: SetStore<T>): boolean {
  if (setSize(set) <= setSize(other)) {
    for (const value of setValues(set)) {
      if (setHas(other, value)) {
        return false;
      }
    }
  } else {
    for (const value of setValues(other)) {
      if (setHas(set, value)) {
        return false;
      }
    }
  }
  return true;
}

// ---- Map<K, V>

// Map.groupBy (ECMA-262 §24.1.2.1, GroupBy with the `zero` key coercion):
// each group is an array of the items, in order, under the callback's key.
export function mapGroupBy<K, T>(
  items: T[],
  callback: (value: T, index: number) => K,
): MapStore<K, T[]> {
  const groups = mapCreate<K, T[]>();
  for (let index = 0; index < items.length; index++) {
    const value = items[index]!;
    const key = callback(value, index);
    const group = mapGet(groups, key);
    if (group === undefined) {
      mapSet(groups, key, [value]);
    } else {
      group.push(value);
    }
  }
  return groups;
}

export function mapCreate<K, V>(): MapStore<K, V> {
  const keys: K[] = [];
  const values: V[] = [];
  return { index: createIndex(0), keys, values };
}

export function mapHas<K, V>(map: MapStore<K, V>, key: K): boolean {
  return varta.collections.lookup(map.keys, map.index.table, key) !== NONE;
}

export function mapGet<K, V>(map: MapStore<K, V>, key: K): V | undefined {
  const position = varta.collections.lookup(map.keys, map.index.table, key);
  if (position === NONE) {
    return undefined;
  }
  return varta.array.get(map.values, position);
}

// Drop the deleted entries and index the live ones for `entries` entries.
function mapRebuild<K, V>(map: MapStore<K, V>, entries: number): void {
  const index = map.index;
  if (index.size < index.serials.length) {
    const keys: K[] = [];
    const values: V[] = [];
    for (let at = index.first; at < index.serials.length; at++) {
      if (varta.array.get(index.serials, at) > 0) {
        keys.push(varta.array.get(map.keys, at));
        values.push(varta.array.get(map.values, at));
      }
    }
    index.serials = liveSerials(index);
    index.first = 0;
    map.keys = keys;
    map.values = values;
  }
  reindex(index, map.keys, entries);
}

// Make room for one more entry of a crowded map unless `key` is present
// (then its value is replaced); whether it was absent.
function mapRoom<K, V>(map: MapStore<K, V>, key: K, value: V): boolean {
  const found = varta.collections.lookup(map.keys, map.index.table, key);
  if (found !== NONE) {
    map.values[found] = value;
    return false;
  }
  admit(map.index, 'Map');
  mapRebuild(map, map.index.size + 1);
  return true;
}

export function mapSet<K, V>(map: MapStore<K, V>, key: K, value: V): void {
  const normalized = varta.collections.key(key);
  // As in `setAdd`.
  if (LOAD * (map.index.serials.length + 1) > map.index.table.length && !mapRoom(map, normalized, value)) {
    return;
  }
  // A miss references the next position, where the entry is appended.
  const found = varta.collections.insert(map.keys, map.index.table, normalized, map.keys.length);
  if (found !== NONE) {
    map.values[found] = value;
    return;
  }
  map.keys.push(normalized);
  map.values.push(value);
  map.index.serial = map.index.serial + 1;
  map.index.serials.push(map.index.serial);
  map.index.size = map.index.size + 1;
}

export function mapDelete<K, V>(map: MapStore<K, V>, key: K): boolean {
  const position = varta.collections.unlink(map.keys, map.index.table, key);
  if (position === NONE) {
    return false;
  }
  remove(map.index, position);
  delete map.keys[position];
  delete map.values[position];
  if (sparse(map.index)) {
    mapRebuild(map, map.index.size);
  }
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
    if (at === NONE) {
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
