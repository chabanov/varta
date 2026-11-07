// Engine prelude of checked-f64-utf16-v38 (`weak_references`, K9 D5):
// `WeakMap`, `WeakSet`, `WeakRef` and `FinalizationRegistry` (ECMA-262
// §24.3, §24.4, §26.1, §26.2) in typed guest code over ephemeron tables,
// with no host import and no capability; see docs/architecture.md (Weak
// references).
//
// A table (`WeakTable`, created and read only by the `varta.weak.*`
// intrinsics) holds pairs of a key and a value in the managed heap. A key
// is the identity of an object or of a symbol that is not registered
// (CanBeHeldWeakly, §9.13); the collector never traces it. Once marking has
// traced every strong edge, the collector marks the value of every pair
// whose key is marked, to a fixed point (an ephemeron: a value reachable
// only through its key's pair dies with the key), then vacates every pair
// whose key stayed unmarked, before any block is swept.
//
// `WeakMap` and `WeakSet` probe their table linearly from the key's
// identity hash: an empty pair ends a probe, a vacated one (deleted, or
// cleared by a collection) continues it and is reused by an insertion. At
// least half of the pairs stay empty; a full table is rebuilt with only its
// occupied pairs at a quarter of the new capacity at most.
//
// A `WeakRef` is a one-pair table holding its target as both key and value,
// so `deref` reads the value only while the key lives. Creating one and a
// `deref` that finds its target add the target to the kept objects
// (AddToKeptObjects, §9.9.1): a table whose keys the collector marks
// strongly, which the host empties when the microtask checkpoint of the
// current turn has drained (ClearKeptObjects), through the checkpoint export
// that calls `weakCheckpoint`.
//
// A `FinalizationRegistry` keeps its cells in registration order: the
// target of cell `i` is the key of pair `i` of a notifying table (a
// collection that vacates one raises the pending flag), its unregister token
// the key of pair `i` of a second table, and its held value a strong field.
// When the pending flag is up at a checkpoint, every live registry with a
// collected cell queues one cleanup job, in the order of their first
// registrations; the job calls
// the callback for its collected cells, the most recently registered first,
// as V8 does. A callback that throws ends the instance with that exception
// uncaught, as a timer callback does.

// `varta_common::weak::MIN_PAIRS`; a compiler test keeps it in step.
const MIN_PAIRS = 8;
// The most pairs of one table: the v38 array element bound, as for `Map`
// (`value::V38_MAX_ARRAY_ELEMENTS`); a compiler test keeps it in step.
const MAX_PAIRS = 134217728;
// Pair states of `varta.weak.state`.
const EMPTY = 0;
const VACATED = 1;
const OCCUPIED = 2;

// The keys a weak collection admits (TypeScript's `lib.es2023`): objects
// and symbols; a registered symbol is rejected when it is used.
export type WeakKey = object | symbol;

// A table of weak pairs; only the `varta.weak.*` intrinsics create it.
class WeakTable {}

// CanBeHeldWeakly (§9.13): every object, and a symbol that the global
// symbol registry does not hold.
function canBeHeldWeakly(value: WeakKey): boolean {
  if (typeof value === 'symbol') {
    return Symbol.keyFor(value) === undefined;
  }
  return (typeof value === 'object' && value !== null) || typeof value === 'function';
}

// The pair of `table` holding `key`, or -1.
function findPair<K>(table: WeakTable, key: K): number {
  const mask = varta.weak.capacity(table) - 1;
  let at = varta.weak.hash(key) & mask;
  for (let probes = 0; probes <= mask; probes++) {
    const state = varta.weak.state(table, at);
    if (state === EMPTY) {
      return -1;
    }
    if (state === OCCUPIED && varta.weak.holds(table, at, key)) {
      return at;
    }
    at = (at + 1) & mask;
  }
  return -1;
}

// The pair of `table` for `key`: the pair holding it, else the first
// vacated pair of its probe, else its empty pair while the table stays at
// most half used; -1 when the table must grow first.
function placePair<K>(table: WeakTable, key: K): number {
  const capacity = varta.weak.capacity(table);
  const mask = capacity - 1;
  let at = varta.weak.hash(key) & mask;
  let free = -1;
  for (let probes = 0; probes < capacity; probes++) {
    const state = varta.weak.state(table, at);
    if (state === EMPTY) {
      if (free >= 0) {
        return free;
      }
      return (varta.weak.used(table) + 1) * 2 > capacity ? -1 : at;
    }
    if (state === VACATED) {
      if (free < 0) {
        free = at;
      }
    } else if (varta.weak.holds(table, at, key)) {
      return at;
    }
    at = (at + 1) & mask;
  }
  return free;
}

// The occupied pairs of `table`.
function occupied(table: WeakTable): number {
  const capacity = varta.weak.capacity(table);
  let count = 0;
  for (let at = 0; at < capacity; at++) {
    if (varta.weak.state(table, at) === OCCUPIED) {
      count++;
    }
  }
  return count;
}

// A table of the kind of `table` holding its occupied pairs at a quarter of
// its capacity at most. A collection the allocation runs vacates pairs of
// `table`, never occupies one, and nothing allocates while the pairs move.
function grown(table: WeakTable, what: string): WeakTable {
  const live = occupied(table);
  let size = MIN_PAIRS;
  while (size < live * 4) {
    size *= 2;
  }
  if (size > MAX_PAIRS) {
    throw new RangeError(`${what} maximum size exceeded`);
  }
  const next = varta.weak.like(table, size);
  const capacity = varta.weak.capacity(table);
  const mask = size - 1;
  for (let at = 0; at < capacity; at++) {
    if (varta.weak.state(table, at) === OCCUPIED) {
      let to = varta.weak.slotHash(table, at) & mask;
      while (varta.weak.state(next, to) !== EMPTY) {
        to = (to + 1) & mask;
      }
      varta.weak.move(table, at, next, to);
    }
  }
  return next;
}

// Occupy the pair of `key` with `value`: the table, or the grown table of
// the same kind that holds it.
function insert<K, V>(table: WeakTable, key: K, value: V, what: string): WeakTable {
  let target = table;
  let at = placePair(target, key);
  if (at < 0) {
    target = grown(target, what);
    at = placePair(target, key);
  }
  varta.weak.put(target, at, key, value);
  return target;
}

// The kept objects of the current turn (AddToKeptObjects, ClearKeptObjects).
class Kept {
  table: WeakTable = varta.weak.kept(MIN_PAIRS);
}
const KEPT = new Kept();

function keep<T>(target: T): void {
  if (findPair(KEPT.table, target) < 0) {
    KEPT.table = insert(KEPT.table, target, false, 'WeakRef');
  }
}

export class WeakMap<K extends WeakKey, V> {
  private table: WeakTable;
  constructor(entries?: (readonly [K, V])[] | null) {
    let table = varta.weak.table<V>(MIN_PAIRS);
    if (entries !== undefined && entries !== null) {
      for (const entry of entries) {
        if (!canBeHeldWeakly(entry[0])) {
          throw new TypeError('Invalid value used as weak map key');
        }
        table = insert(table, entry[0], entry[1], 'WeakMap');
      }
    }
    this.table = table;
  }
  get(key: K): V | undefined {
    const at = findPair(this.table, key);
    if (at < 0) {
      return undefined;
    }
    return varta.weak.value<V>(this.table, at);
  }
  has(key: K): boolean {
    return findPair(this.table, key) >= 0;
  }
  set(key: K, value: V): this {
    if (!canBeHeldWeakly(key)) {
      throw new TypeError('Invalid value used as weak map key');
    }
    this.table = insert(this.table, key, value, 'WeakMap');
    return this;
  }
  delete(key: K): boolean {
    const at = findPair(this.table, key);
    if (at < 0) {
      return false;
    }
    varta.weak.remove(this.table, at);
    return true;
  }
}

export class WeakSet<T extends WeakKey> {
  private table: WeakTable;
  constructor(values?: T[] | null) {
    let table = varta.weak.table<boolean>(MIN_PAIRS);
    if (values !== undefined && values !== null) {
      for (const value of values) {
        if (!canBeHeldWeakly(value)) {
          throw new TypeError('Invalid value used in weak set');
        }
        table = insert(table, value, true, 'WeakSet');
      }
    }
    this.table = table;
  }
  add(value: T): this {
    if (!canBeHeldWeakly(value)) {
      throw new TypeError('Invalid value used in weak set');
    }
    this.table = insert(this.table, value, true, 'WeakSet');
    return this;
  }
  has(value: T): boolean {
    return findPair(this.table, value) >= 0;
  }
  delete(value: T): boolean {
    const at = findPair(this.table, value);
    if (at < 0) {
      return false;
    }
    varta.weak.remove(this.table, at);
    return true;
  }
}

export class WeakRef<T extends WeakKey> {
  // One pair: the target as its own key and value.
  private table: WeakTable;
  constructor(target: T) {
    if (!canBeHeldWeakly(target)) {
      throw new TypeError('WeakRef: invalid target');
    }
    this.table = varta.weak.table<T>(1);
    varta.weak.put(this.table, 0, target, target);
    keep(target);
  }
  deref(): T | undefined {
    if (varta.weak.state(this.table, 0) !== OCCUPIED) {
      return undefined;
    }
    const target = varta.weak.value<T>(this.table, 0);
    keep(target);
    return target;
  }
}

// The live registries in the order of their first registration: pair `i`
// holds a registry as its key and, as its value, the closure that queues
// its cleanup job when one of its cells was collected.
class Registries {
  table: WeakTable = varta.weak.table<() => void>(MIN_PAIRS);
  count = 0;
}
const REGISTRIES = new Registries();

function enlist<R>(registry: R, due: () => void): void {
  if (REGISTRIES.count === varta.weak.capacity(REGISTRIES.table)) {
    // Compact the live registries, in order, into a table with room.
    const table = REGISTRIES.table;
    const live = occupied(table);
    let size = MIN_PAIRS;
    while (size < live * 2 + 1) {
      size *= 2;
    }
    const next = varta.weak.like(table, size);
    let count = 0;
    for (let at = 0; at < REGISTRIES.count; at++) {
      if (varta.weak.state(table, at) === OCCUPIED) {
        varta.weak.move(table, at, next, count);
        count++;
      }
    }
    REGISTRIES.table = next;
    REGISTRIES.count = count;
  }
  varta.weak.put(REGISTRIES.table, REGISTRIES.count, registry, due);
  REGISTRIES.count++;
}

// The host's call at the end of every microtask checkpoint: empty the kept
// objects, then let every live registry queue its cleanup job if a
// collection vacated one of its targets.
export function weakCheckpoint(): void {
  const kept = KEPT.table;
  if (varta.weak.used(kept) > 0) {
    if (varta.weak.capacity(kept) > MIN_PAIRS) {
      KEPT.table = varta.weak.kept(MIN_PAIRS);
    } else {
      varta.weak.clear(kept);
    }
  }
  if (!varta.weak.pending()) {
    return;
  }
  const table = REGISTRIES.table;
  const count = REGISTRIES.count;
  for (let at = 0; at < count; at++) {
    if (varta.weak.state(table, at) === OCCUPIED) {
      const due = varta.weak.value<() => void>(table, at);
      due();
    }
  }
}

// One registered cell's held value.
class Cell<T> {
  readonly held: T;
  constructor(held: T) {
    this.held = held;
  }
}

export class FinalizationRegistry<T> {
  private readonly cleanup: (heldValue: T) => void;
  // Pair `i`: the target of cell `i` (notifying), and its token or empty.
  private targets: WeakTable;
  private tokens: WeakTable;
  // Cell `i`, or undefined once it was unregistered or cleaned up.
  private cells: (Cell<T> | undefined)[];
  private scheduled: boolean;
  // Whether the registry is on the list of registries (from its first cell).
  private enlisted: boolean;
  constructor(cleanupCallback: (heldValue: T) => void) {
    this.cleanup = cleanupCallback;
    this.targets = varta.weak.targets(MIN_PAIRS);
    this.tokens = varta.weak.table<boolean>(MIN_PAIRS);
    this.cells = [];
    this.scheduled = false;
    this.enlisted = false;
  }
  register(target: WeakKey, heldValue: T, unregisterToken?: WeakKey): void {
    if (!canBeHeldWeakly(target)) {
      throw new TypeError('FinalizationRegistry.prototype.register: invalid target');
    }
    if (varta.weak.same(target, heldValue)) {
      throw new TypeError(
        'FinalizationRegistry.prototype.register: target and holdings must not be same',
      );
    }
    if (unregisterToken !== undefined && !canBeHeldWeakly(unregisterToken)) {
      throw new TypeError(`Invalid unregisterToken ('${tokenText(unregisterToken)}')`);
    }
    if (!this.enlisted) {
      this.enlisted = true;
      enlist(this, () => this.due());
    }
    if (this.cells.length === varta.weak.capacity(this.targets)) {
      this.compact();
    }
    const at = this.cells.length;
    varta.weak.put(this.targets, at, target, false);
    if (unregisterToken !== undefined) {
      varta.weak.put(this.tokens, at, unregisterToken, false);
    }
    this.cells.push(new Cell<T>(heldValue));
  }
  unregister(unregisterToken: WeakKey): boolean {
    if (!canBeHeldWeakly(unregisterToken)) {
      throw new TypeError(`Invalid unregisterToken ('${tokenText(unregisterToken)}')`);
    }
    let removed = false;
    for (let at = 0; at < this.cells.length; at++) {
      if (
        this.cells[at] !== undefined &&
        varta.weak.state(this.tokens, at) === OCCUPIED &&
        varta.weak.holds(this.tokens, at, unregisterToken)
      ) {
        this.release(at);
        removed = true;
      }
    }
    return removed;
  }
  // Cell `at` leaves the registry.
  private release(at: number): void {
    this.cells[at] = undefined;
    if (varta.weak.state(this.targets, at) === OCCUPIED) {
      varta.weak.remove(this.targets, at);
    }
    if (varta.weak.state(this.tokens, at) === OCCUPIED) {
      varta.weak.remove(this.tokens, at);
    }
  }
  // Move the remaining cells, in order, to the front of tables with room.
  private compact(): void {
    let live = 0;
    for (let at = 0; at < this.cells.length; at++) {
      if (this.cells[at] !== undefined) {
        live++;
      }
    }
    let size = MIN_PAIRS;
    while (size < live * 2 + 1) {
      size *= 2;
    }
    if (size > MAX_PAIRS) {
      throw new RangeError('FinalizationRegistry maximum size exceeded');
    }
    const targets = varta.weak.like(this.targets, size);
    const tokens = varta.weak.like(this.tokens, size);
    const cells: (Cell<T> | undefined)[] = [];
    // Nothing allocates while the pairs move: the cells array grows after.
    let count = 0;
    for (let at = 0; at < this.cells.length; at++) {
      if (this.cells[at] !== undefined) {
        if (varta.weak.state(this.targets, at) === OCCUPIED) {
          varta.weak.move(this.targets, at, targets, count);
        }
        if (varta.weak.state(this.tokens, at) === OCCUPIED) {
          varta.weak.move(this.tokens, at, tokens, count);
        }
        count++;
      }
    }
    for (let at = 0; at < this.cells.length; at++) {
      const cell = this.cells[at];
      if (cell !== undefined) {
        cells.push(cell);
      }
    }
    this.targets = targets;
    this.tokens = tokens;
    this.cells = cells;
  }
  // Whether a cell's target was collected.
  private collected(at: number): boolean {
    return this.cells[at] !== undefined && varta.weak.state(this.targets, at) !== OCCUPIED;
  }
  // Queue the cleanup job when a cell's target was collected.
  private due(): void {
    if (this.scheduled) {
      return;
    }
    for (let at = 0; at < this.cells.length; at++) {
      if (this.collected(at)) {
        this.scheduled = true;
        this.cleanupJob();
        return;
      }
    }
  }
  // CleanupFinalizationRegistry (§9.13), as a job of its own: while a cell's
  // target is collected, remove the cell and call the callback with its held
  // value, the most recently registered cell first. A callback may register
  // (and compact the cells, which only moves a cell to a lower position) or
  // unregister cells; a collection during a callback adds collected cells,
  // which the next pass finds.
  private async cleanupJob(): Promise<void> {
    await Promise.resolve();
    this.scheduled = false;
    let found = true;
    while (found) {
      found = false;
      for (let at = this.cells.length - 1; at >= 0; at--) {
        if (at >= this.cells.length) {
          continue;
        }
        const cell = this.cells[at];
        if (cell !== undefined && varta.weak.state(this.targets, at) !== OCCUPIED) {
          found = true;
          this.release(at);
          try {
            this.cleanup(cell.held);
          } catch (error) {
            varta.weak.uncaught(error);
          }
        }
      }
    }
  }
}

// The text of an unregister token in its TypeError, as V8 writes it (only a
// registered symbol is rejected).
function tokenText(token: WeakKey): string {
  if (typeof token === 'symbol') {
    return token.toString();
  }
  return '';
}
