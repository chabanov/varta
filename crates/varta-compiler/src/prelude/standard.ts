// Engine prelude of checked-f64-utf16-v38 (`standard_library`, K9 D): the
// ECMAScript 2025 Array and String methods that the earlier preludes and
// lowerings do not provide, in typed guest code with no host import and no
// capability. `lower::standard` calls these functions for receivers of the
// matching type; see docs/architecture.md (Standard library).
//
// Arrays may be sparse (v13): `Object.hasOwn(array, index)` tells a hole from
// a present `undefined`, `delete array[index]` makes one, and assigning
// `length` truncates or extends. Every method below follows its ECMA-262
// algorithm step by step on those operations, so holes, live lengths and the
// order of callback calls are the specification's.

// ToIntegerOrInfinity (ECMA-262 §7.1.5) of a number.
function integer(value: number): number {
  if (Number.isNaN(value)) {
    return 0;
  }
  return Math.trunc(value) + 0;
}

// A relative index argument resolved against `length` (the clamping of
// `slice`, `splice`, `copyWithin` and `fill`): `undefined` is `fallback`.
function relative(value: number | undefined, length: number, fallback: number): number {
  if (value === undefined) {
    return fallback;
  }
  const at = integer(value);
  if (at < 0) {
    return Math.max(length + at, 0);
  }
  return Math.min(at, length);
}

// ---- Sorting (ECMA-262 §23.1.3.30 Array.prototype.sort, §23.1.3.34
// toSorted). The comparator order is V8's: holes and `undefined` are taken
// out first (SortIndexedProperties), the rest is sorted by the TimSort of
// V8's `third_party/v8/builtins/array-sort.tq` (CPython's listsort), and the
// sorted values, then the `undefined`s, then the holes are written back. So
// every comparator is called with the same arguments in the same order as in
// Node, and an inconsistent comparator leaves the same order.

const MIN_GALLOP = 7;
const MIN_MERGE = 64;

// The comparator a sort calls, with SortCompare's NaN-as-zero (§23.1.3.30.2).
type Order<T> = (left: T, right: T) => number;

class SortState<T> {
  work: T[];
  order: Order<T>;
  minGallop: number;
  runBase: number[];
  runLength: number[];
  constructor(work: T[], order: Order<T>) {
    this.work = work;
    this.order = order;
    this.minGallop = MIN_GALLOP;
    this.runBase = [];
    this.runLength = [];
  }
  compare(left: T, right: T): number {
    const order = this.order;
    const result = order(left, right);
    return Number.isNaN(result) ? 0 : result;
  }
}

function minRunLength(total: number): number {
  let n = total;
  let r = 0;
  while (n >= MIN_MERGE) {
    r = r | (n & 1);
    n = n >> 1;
  }
  return n + r;
}

// Copy `length` values from `source[from…]` to `target[to…]`; the ranges may
// overlap when both are the same array.
function copyRange<T>(source: T[], from: number, target: T[], to: number, length: number): void {
  if (from < to) {
    for (let index = length - 1; index >= 0; index--) {
      target[to + index] = source[from + index]!;
    }
  } else {
    for (let index = 0; index < length; index++) {
      target[to + index] = source[from + index]!;
    }
  }
}

function reverseRange<T>(work: T[], from: number, to: number): void {
  let low = from;
  let high = to - 1;
  while (low < high) {
    const value = work[low]!;
    work[low] = work[high]!;
    work[high] = value;
    low++;
    high--;
  }
}

// The length of the run starting at `low` (descending runs are reversed).
function countAndMakeRun<T>(state: SortState<T>, lowArg: number, high: number): number {
  const work = state.work;
  const low = lowArg + 1;
  if (low === high) {
    return 1;
  }
  let runLength = 2;
  const elementLow = work[low]!;
  const elementLowPre = work[low - 1]!;
  let order = state.compare(elementLow, elementLowPre);
  const descending = order < 0;
  let previous = elementLow;
  for (let index = low + 1; index < high; index++) {
    const current = work[index]!;
    order = state.compare(current, previous);
    if (descending) {
      if (order >= 0) {
        break;
      }
    } else if (order < 0) {
      break;
    }
    previous = current;
    runLength++;
  }
  if (descending) {
    reverseRange(work, lowArg, lowArg + runLength);
  }
  return runLength;
}

function binaryInsertionSort<T>(state: SortState<T>, low: number, startArg: number, high: number): void {
  const work = state.work;
  let start = low === startArg ? startArg + 1 : startArg;
  for (; start < high; start++) {
    let left = low;
    let right = start;
    const pivot = work[start]!;
    while (left < right) {
      const middle = left + ((right - left) >> 1);
      const order = state.compare(pivot, work[middle]!);
      if (order < 0) {
        right = middle;
      } else {
        left = middle + 1;
      }
    }
    for (let p = start; p > left; p--) {
      work[p] = work[p - 1]!;
    }
    work[left] = pivot;
  }
}

// Where `key` belongs in the sorted `array[base…base+length)`, left of equal
// values, starting the search at `hint`.
function gallopLeft<T>(state: SortState<T>, array: T[], key: T, base: number, length: number, hint: number): number {
  let lastOffset = 0;
  let offset = 1;
  const hintElement = array[base + hint]!;
  let order = state.compare(hintElement, key);
  if (order < 0) {
    const maxOffset = length - hint;
    while (offset < maxOffset) {
      order = state.compare(array[base + hint + offset]!, key);
      if (order >= 0) {
        break;
      }
      lastOffset = offset;
      offset = offset * 2 + 1;
    }
    if (offset > maxOffset) {
      offset = maxOffset;
    }
    lastOffset += hint;
    offset += hint;
  } else {
    const maxOffset = hint + 1;
    while (offset < maxOffset) {
      order = state.compare(array[base + hint - offset]!, key);
      if (order < 0) {
        break;
      }
      lastOffset = offset;
      offset = offset * 2 + 1;
    }
    if (offset > maxOffset) {
      offset = maxOffset;
    }
    const swapped = lastOffset;
    lastOffset = hint - offset;
    offset = hint - swapped;
  }
  lastOffset++;
  while (lastOffset < offset) {
    const middle = lastOffset + ((offset - lastOffset) >> 1);
    order = state.compare(array[base + middle]!, key);
    if (order < 0) {
      lastOffset = middle + 1;
    } else {
      offset = middle;
    }
  }
  return offset;
}

// Where `key` belongs in the sorted `array[base…base+length)`, right of equal
// values, starting the search at `hint`.
function gallopRight<T>(state: SortState<T>, array: T[], key: T, base: number, length: number, hint: number): number {
  let lastOffset = 0;
  let offset = 1;
  const hintElement = array[base + hint]!;
  let order = state.compare(key, hintElement);
  if (order < 0) {
    const maxOffset = hint + 1;
    while (offset < maxOffset) {
      order = state.compare(key, array[base + hint - offset]!);
      if (order >= 0) {
        break;
      }
      lastOffset = offset;
      offset = offset * 2 + 1;
    }
    if (offset > maxOffset) {
      offset = maxOffset;
    }
    const swapped = lastOffset;
    lastOffset = hint - offset;
    offset = hint - swapped;
  } else {
    const maxOffset = length - hint;
    while (offset < maxOffset) {
      order = state.compare(key, array[base + hint + offset]!);
      if (order < 0) {
        break;
      }
      lastOffset = offset;
      offset = offset * 2 + 1;
    }
    if (offset > maxOffset) {
      offset = maxOffset;
    }
    lastOffset += hint;
    offset += hint;
  }
  lastOffset++;
  while (lastOffset < offset) {
    const middle = lastOffset + ((offset - lastOffset) >> 1);
    order = state.compare(key, array[base + middle]!);
    if (order < 0) {
      offset = middle;
    } else {
      lastOffset = middle + 1;
    }
  }
  return offset;
}

function slice<T>(work: T[], from: number, length: number): T[] {
  const copy: T[] = [];
  for (let index = 0; index < length; index++) {
    copy.push(work[from + index]!);
  }
  return copy;
}

// Merge the adjacent runs at `baseA` and `baseB` with run A no longer than
// run B, copying run A to a temporary array.
function mergeLow<T>(state: SortState<T>, baseA: number, lengthAArg: number, baseB: number, lengthBArg: number): void {
  const work = state.work;
  let lengthA = lengthAArg;
  let lengthB = lengthBArg;
  const temp = slice(work, baseA, lengthA);
  let dest = baseA;
  let cursorTemp = 0;
  let cursorB = baseB;
  work[dest++] = work[cursorB++]!;
  // 0: succeed, 1: copy B.
  let outcome = 0;
  merge: {
    if (--lengthB === 0) {
      break merge;
    }
    if (lengthA === 1) {
      outcome = 1;
      break merge;
    }
    let minGallop = state.minGallop;
    for (;;) {
      let winsA = 0;
      let winsB = 0;
      for (;;) {
        const order = state.compare(work[cursorB]!, temp[cursorTemp]!);
        if (order < 0) {
          work[dest++] = work[cursorB++]!;
          winsB++;
          lengthB--;
          winsA = 0;
          if (lengthB === 0) {
            break merge;
          }
          if (winsB >= minGallop) {
            break;
          }
        } else {
          work[dest++] = temp[cursorTemp++]!;
          winsA++;
          lengthA--;
          winsB = 0;
          if (lengthA === 1) {
            outcome = 1;
            break merge;
          }
          if (winsA >= minGallop) {
            break;
          }
        }
      }
      minGallop++;
      let first = true;
      while (winsA >= MIN_GALLOP || winsB >= MIN_GALLOP || first) {
        first = false;
        minGallop = Math.max(1, minGallop - 1);
        state.minGallop = minGallop;
        winsA = gallopRight<T>(state, temp, work[cursorB]!, cursorTemp, lengthA, 0);
        if (winsA > 0) {
          copyRange(temp, cursorTemp, work, dest, winsA);
          dest += winsA;
          cursorTemp += winsA;
          lengthA -= winsA;
          if (lengthA === 1) {
            outcome = 1;
            break merge;
          }
          if (lengthA === 0) {
            break merge;
          }
        }
        work[dest++] = work[cursorB++]!;
        if (--lengthB === 0) {
          break merge;
        }
        winsB = gallopLeft<T>(state, work, temp[cursorTemp]!, cursorB, lengthB, 0);
        if (winsB > 0) {
          copyRange(work, cursorB, work, dest, winsB);
          dest += winsB;
          cursorB += winsB;
          lengthB -= winsB;
          if (lengthB === 0) {
            break merge;
          }
        }
        work[dest++] = temp[cursorTemp++]!;
        if (--lengthA === 1) {
          outcome = 1;
          break merge;
        }
      }
      minGallop++;
      state.minGallop = minGallop;
    }
  }
  if (outcome === 0) {
    if (lengthA > 0) {
      copyRange(temp, cursorTemp, work, dest, lengthA);
    }
  } else {
    copyRange(work, cursorB, work, dest, lengthB);
    work[dest + lengthB] = temp[cursorTemp]!;
  }
}

// Merge the adjacent runs at `baseA` and `baseB` with run A longer than run
// B, copying run B to a temporary array and merging from the end.
function mergeHigh<T>(state: SortState<T>, baseA: number, lengthAArg: number, baseB: number, lengthBArg: number): void {
  const work = state.work;
  let lengthA = lengthAArg;
  let lengthB = lengthBArg;
  const temp = slice(work, baseB, lengthB);
  let dest = baseB + lengthB - 1;
  let cursorTemp = lengthB - 1;
  let cursorA = baseA + lengthA - 1;
  work[dest--] = work[cursorA--]!;
  // 0: succeed, 1: copy A.
  let outcome = 0;
  merge: {
    if (--lengthA === 0) {
      break merge;
    }
    if (lengthB === 1) {
      outcome = 1;
      break merge;
    }
    let minGallop = state.minGallop;
    for (;;) {
      let winsA = 0;
      let winsB = 0;
      for (;;) {
        const order = state.compare(temp[cursorTemp]!, work[cursorA]!);
        if (order < 0) {
          work[dest--] = work[cursorA--]!;
          winsA++;
          lengthA--;
          winsB = 0;
          if (lengthA === 0) {
            break merge;
          }
          if (winsA >= minGallop) {
            break;
          }
        } else {
          work[dest--] = temp[cursorTemp--]!;
          winsB++;
          lengthB--;
          winsA = 0;
          if (lengthB === 1) {
            outcome = 1;
            break merge;
          }
          if (winsB >= minGallop) {
            break;
          }
        }
      }
      minGallop++;
      let first = true;
      while (winsA >= MIN_GALLOP || winsB >= MIN_GALLOP || first) {
        first = false;
        minGallop = Math.max(1, minGallop - 1);
        state.minGallop = minGallop;
        const k = gallopRight<T>(state, work, temp[cursorTemp]!, baseA, lengthA, lengthA - 1);
        winsA = lengthA - k;
        if (winsA > 0) {
          dest -= winsA;
          cursorA -= winsA;
          copyRange(work, cursorA + 1, work, dest + 1, winsA);
          lengthA -= winsA;
          if (lengthA === 0) {
            break merge;
          }
        }
        work[dest--] = temp[cursorTemp--]!;
        if (--lengthB === 1) {
          outcome = 1;
          break merge;
        }
        const j = gallopLeft<T>(state, temp, work[cursorA]!, 0, lengthB, lengthB - 1);
        winsB = lengthB - j;
        if (winsB > 0) {
          dest -= winsB;
          cursorTemp -= winsB;
          copyRange(temp, cursorTemp + 1, work, dest + 1, winsB);
          lengthB -= winsB;
          if (lengthB === 1) {
            outcome = 1;
            break merge;
          }
          if (lengthB === 0) {
            break merge;
          }
        }
        work[dest--] = work[cursorA--]!;
        if (--lengthA === 0) {
          break merge;
        }
      }
      minGallop++;
      state.minGallop = minGallop;
    }
  }
  if (outcome === 0) {
    if (lengthB > 0) {
      copyRange(temp, 0, work, dest - (lengthB - 1), lengthB);
    }
  } else {
    dest -= lengthA;
    cursorA -= lengthA;
    copyRange(work, cursorA + 1, work, dest + 1, lengthA);
    work[dest] = temp[cursorTemp]!;
  }
}

function mergeAt<T>(state: SortState<T>, at: number): void {
  const bases = state.runBase;
  const lengths = state.runLength;
  const size = lengths.length;
  let baseA = bases[at]!;
  let lengthA = lengths[at]!;
  const baseB = bases[at + 1]!;
  let lengthB = lengths[at + 1]!;
  lengths[at] = lengthA + lengthB;
  if (at === size - 3) {
    bases[at + 1] = bases[at + 2]!;
    lengths[at + 1] = lengths[at + 2]!;
  }
  bases.pop();
  lengths.pop();
  const work = state.work;
  const k = gallopRight<T>(state, work, work[baseB]!, baseA, lengthA, 0);
  baseA += k;
  lengthA -= k;
  if (lengthA === 0) {
    return;
  }
  lengthB = gallopLeft<T>(state, work, work[baseA + lengthA - 1]!, baseB, lengthB, lengthB - 1);
  if (lengthB === 0) {
    return;
  }
  if (lengthA <= lengthB) {
    mergeLow<T>(state, baseA, lengthA, baseB, lengthB);
  } else {
    mergeHigh<T>(state, baseA, lengthA, baseB, lengthB);
  }
}

function invariantHolds(lengths: number[], n: number): boolean {
  if (n < 2) {
    return true;
  }
  return lengths[n - 2]! > lengths[n - 1]! + lengths[n]!;
}

function mergeCollapse<T>(state: SortState<T>): void {
  const lengths = state.runLength;
  while (lengths.length > 1) {
    let n = lengths.length - 2;
    if (!invariantHolds(lengths, n + 1) || !invariantHolds(lengths, n)) {
      if (lengths[n - 1]! < lengths[n + 1]!) {
        n--;
      }
      mergeAt<T>(state, n);
    } else if (lengths[n]! <= lengths[n + 1]!) {
      mergeAt<T>(state, n);
    } else {
      break;
    }
  }
}

function mergeForceCollapse<T>(state: SortState<T>): void {
  const lengths = state.runLength;
  while (lengths.length > 1) {
    let n = lengths.length - 2;
    if (n > 0 && lengths[n - 1]! < lengths[n + 1]!) {
      n--;
    }
    mergeAt<T>(state, n);
  }
}

function timSort<T>(state: SortState<T>): void {
  const length = state.work.length;
  if (length < 2) {
    return;
  }
  let remaining = length;
  let low = 0;
  const minRun = minRunLength(remaining);
  while (remaining !== 0) {
    let run = countAndMakeRun<T>(state, low, low + remaining);
    if (run < minRun) {
      const forced = Math.min(minRun, remaining);
      binaryInsertionSort<T>(state, low, low + run, low + forced);
      run = forced;
    }
    state.runBase.push(low);
    state.runLength.push(run);
    mergeCollapse<T>(state);
    low += run;
    remaining -= run;
  }
  mergeForceCollapse<T>(state);
}

// SortCompare without a comparator (§23.1.3.30.2): the String forms by code
// units. Only primitive elements reach it (`lower::standard` refuses others).
function defaultOrder<T>(left: T, right: T): number {
  const x = String(left);
  const y = String(right);
  if (x < y) {
    return -1;
  }
  if (x > y) {
    return 1;
  }
  return 0;
}

// Sort `array` in place: present values, then `undefined`s, then holes.
function sortInPlace<T>(array: T[], order: Order<T>): T[] {
  const length = array.length;
  const work: T[] = [];
  // The present `undefined` values, kept as values of `T`.
  const undefineds: T[] = [];
  for (let index = 0; index < length; index++) {
    if (Object.hasOwn(array, index)) {
      const value = array[index] as T;
      if (value === undefined) {
        undefineds.push(value);
      } else {
        work.push(value);
      }
    }
  }
  timSort<T>(new SortState<T>(work, order));
  let index = 0;
  for (; index < work.length; index++) {
    array[index] = work[index]!;
  }
  for (let count = 0; count < undefineds.length; count++) {
    array[index] = undefineds[count] as T;
    index++;
  }
  for (; index < length; index++) {
    delete array[index];
  }
  return array;
}

// A dense copy of `array` with its holes read as `undefined`.
function denseCopy<T>(array: T[]): T[] {
  const copy: T[] = [];
  const length = array.length;
  for (let index = 0; index < length; index++) {
    copy.push(array[index] as T);
  }
  return copy;
}

export function arraySort<T>(array: T[], compare: (left: T, right: T) => number): T[] {
  return sortInPlace(array, compare);
}

export function arraySortDefault<T>(array: T[]): T[] {
  return sortInPlace(array, (left: T, right: T) => defaultOrder<T>(left, right));
}

export function arrayToSorted<T>(array: T[], compare: (left: T, right: T) => number): T[] {
  return sortInPlace(denseCopy(array), compare);
}

export function arrayToSortedDefault<T>(array: T[]): T[] {
  return sortInPlace(denseCopy(array), (left: T, right: T) => defaultOrder<T>(left, right));
}

// ---- Copying methods (ES2023, §23.1.3.33–§23.1.3.39)

export function arrayToReversed<T>(array: T[]): T[] {
  const result: T[] = [];
  for (let index = array.length - 1; index >= 0; index--) {
    result.push(array[index] as T);
  }
  return result;
}

export function arrayWith<T>(array: T[], index: number, value: T): T[] {
  const length = array.length;
  const relativeIndex = integer(index);
  const actual = relativeIndex >= 0 ? relativeIndex : length + relativeIndex;
  if (actual >= length || actual < 0) {
    throw new RangeError(`Invalid index : ${index}`);
  }
  const result: T[] = [];
  for (let at = 0; at < length; at++) {
    result.push(at === actual ? value : (array[at] as T));
  }
  return result;
}

// `supplied` counts the source arguments, since `toSpliced()` and
// `toSpliced(undefined)` differ (§23.1.3.35 steps 5–9).
export function arrayToSpliced<T>(
  array: T[],
  supplied: number,
  start?: number,
  skip?: number,
  ...items: T[]
): T[] {
  const length = array.length;
  const actualStart = relative(start, length, 0);
  let actualSkip = 0;
  if (supplied === 1) {
    actualSkip = length - actualStart;
  } else if (supplied > 1) {
    actualSkip = Math.min(Math.max(skip === undefined ? 0 : integer(skip), 0), length - actualStart);
  }
  const result: T[] = [];
  for (let index = 0; index < actualStart; index++) {
    result.push(array[index] as T);
  }
  for (let index = 0; index < items.length; index++) {
    result.push(items[index]!);
  }
  for (let index = actualStart + actualSkip; index < length; index++) {
    result.push(array[index] as T);
  }
  return result;
}

// ---- Mutating methods

// Array.prototype.splice (§23.1.3.31): holes move as holes.
export function arraySplice<T>(
  array: T[],
  supplied: number,
  start?: number,
  skip?: number,
  ...items: T[]
): T[] {
  const length = array.length;
  const actualStart = relative(start, length, 0);
  let actualSkip = 0;
  if (supplied === 1) {
    actualSkip = length - actualStart;
  } else if (supplied > 1) {
    actualSkip = Math.min(Math.max(skip === undefined ? 0 : integer(skip), 0), length - actualStart);
  }
  const removed: T[] = [];
  for (let k = 0; k < actualSkip; k++) {
    const from = actualStart + k;
    if (Object.hasOwn(array, from)) {
      removed[k] = array[from] as T;
    }
  }
  removed.length = actualSkip;
  const itemCount = items.length;
  if (itemCount < actualSkip) {
    for (let k = actualStart; k < length - actualSkip; k++) {
      const from = k + actualSkip;
      const to = k + itemCount;
      if (Object.hasOwn(array, from)) {
        array[to] = array[from] as T;
      } else {
        delete array[to];
      }
    }
    for (let k = length; k > length - actualSkip + itemCount; k--) {
      delete array[k - 1];
    }
  } else if (itemCount > actualSkip) {
    for (let k = length - actualSkip; k > actualStart; k--) {
      const from = k + actualSkip - 1;
      const to = k + itemCount - 1;
      if (Object.hasOwn(array, from)) {
        array[to] = array[from] as T;
      } else {
        delete array[to];
      }
    }
  }
  for (let index = 0; index < itemCount; index++) {
    array[actualStart + index] = items[index]!;
  }
  array.length = length - actualSkip + itemCount;
  return removed;
}

// Array.prototype.copyWithin (§23.1.3.4).
export function arrayCopyWithin<T>(array: T[], target: number, start?: number, end?: number): T[] {
  const length = array.length;
  let to = relative(target, length, 0);
  let from = relative(start, length, 0);
  const final = relative(end, length, length);
  let count = Math.min(final - from, length - to);
  let direction = 1;
  if (from < to && to < from + count) {
    direction = -1;
    from = from + count - 1;
    to = to + count - 1;
  }
  while (count > 0) {
    if (Object.hasOwn(array, from)) {
      array[to] = array[from] as T;
    } else {
      delete array[to];
    }
    from += direction;
    to += direction;
    count--;
  }
  return array;
}

// Array.prototype.shift (§23.1.3.27).
export function arrayShift<T>(array: T[]): T | undefined {
  const length = array.length;
  if (length === 0) {
    return undefined;
  }
  const first = array[0];
  for (let k = 1; k < length; k++) {
    if (Object.hasOwn(array, k)) {
      array[k - 1] = array[k] as T;
    } else {
      delete array[k - 1];
    }
  }
  delete array[length - 1];
  array.length = length - 1;
  return first;
}

// Array.prototype.unshift (§23.1.3.37).
export function arrayUnshift<T>(array: T[], ...items: T[]): number {
  const length = array.length;
  const count = items.length;
  if (count > 0) {
    for (let k = length; k > 0; k--) {
      const from = k - 1;
      const to = k + count - 1;
      if (Object.hasOwn(array, from)) {
        array[to] = array[from] as T;
      } else {
        delete array[to];
      }
    }
    for (let index = 0; index < count; index++) {
      array[index] = items[index]!;
    }
  }
  array.length = length + count;
  return length + count;
}

// ---- Concatenation and flattening

// Array.prototype.concat (§23.1.3.1) after `lower::standard` wrapped every
// non-array argument as a one-element array: holes stay holes.
export function arrayConcat<T>(array: T[], parts: T[][]): T[] {
  const result: T[] = [];
  let n = 0;
  for (let part = -1; part < parts.length; part++) {
    const source = part < 0 ? array : parts[part]!;
    const length = source.length;
    for (let index = 0; index < length; index++) {
      if (Object.hasOwn(source, index)) {
        result[n] = source[index] as T;
      }
      n++;
    }
  }
  result.length = n;
  return result;
}

// One FlattenIntoArray level (§23.1.3.13.1) of an array of arrays: holes of
// either level are skipped.
export function arrayFlat<T>(array: T[][]): T[] {
  const result: T[] = [];
  const length = array.length;
  for (let index = 0; index < length; index++) {
    if (Object.hasOwn(array, index)) {
      const inner = array[index]!;
      const innerLength = inner.length;
      for (let at = 0; at < innerLength; at++) {
        if (Object.hasOwn(inner, at)) {
          result.push(inner[at] as T);
        }
      }
    }
  }
  return result;
}

// `flat` of an array without array elements (or depth 0): a copy without
// holes.
export function arrayCompact<T>(array: T[]): T[] {
  const result: T[] = [];
  const length = array.length;
  for (let index = 0; index < length; index++) {
    if (Object.hasOwn(array, index)) {
      result.push(array[index] as T);
    }
  }
  return result;
}

// One flattening level of an array whose elements are values or arrays.
export function arrayFlatMixed<T>(array: (T | T[])[]): T[] {
  const result: T[] = [];
  const length = array.length;
  for (let index = 0; index < length; index++) {
    if (Object.hasOwn(array, index)) {
      const element = array[index]!;
      if (Array.isArray(element)) {
        const innerLength = element.length;
        for (let at = 0; at < innerLength; at++) {
          if (Object.hasOwn(element, at)) {
            result.push(element[at] as T);
          }
        }
      } else {
        result.push(element);
      }
    }
  }
  return result;
}

// Array.prototype.flatMap (§23.1.3.14) with a callback returning arrays.
export function arrayFlatMap<T, U>(array: T[], callback: (value: T, index: number, array: T[]) => U[]): U[] {
  const result: U[] = [];
  const length = array.length;
  for (let index = 0; index < length; index++) {
    if (Object.hasOwn(array, index)) {
      const inner = callback(array[index] as T, index, array);
      const innerLength = inner.length;
      for (let at = 0; at < innerLength; at++) {
        if (Object.hasOwn(inner, at)) {
          result.push(inner[at] as U);
        }
      }
    }
  }
  return result;
}

// Array.prototype.flatMap with a callback returning non-array values.
export function arrayFlatMapValue<T, U>(array: T[], callback: (value: T, index: number, array: T[]) => U): U[] {
  const result: U[] = [];
  const length = array.length;
  for (let index = 0; index < length; index++) {
    if (Object.hasOwn(array, index)) {
      result.push(callback(array[index] as T, index, array));
    }
  }
  return result;
}

// ---- Iterators (§23.1.5): the length is read at every step, so elements
// pushed during iteration are visited; holes read as `undefined`.

export function* arrayValues<T>(array: T[]): Generator<T, void, undefined> {
  for (let index = 0; index < array.length; index++) {
    yield array[index] as T;
  }
}

export function* arrayKeys<T>(array: T[]): Generator<number, void, undefined> {
  for (let index = 0; index < array.length; index++) {
    yield index;
  }
}

export function* arrayEntries<T>(array: T[]): Generator<[number, T], void, undefined> {
  for (let index = 0; index < array.length; index++) {
    yield [index, array[index] as T];
  }
}

// ---- Constructors

// Array.of (§23.1.2.3).
export function arrayOf<T>(...items: T[]): T[] {
  return items;
}

// Array.from({ length }, callback) (§23.1.2.1 steps 7–12 for an array-like
// without elements): every index maps `undefined`. The length is
// ToLength'd, and `new Array(len)` refuses one above 2^32 - 1.
export function arrayFromLength<U>(length: number, callback: (value: undefined, index: number) => U): U[] {
  const count = Math.max(integer(length), 0);
  if (count > 4294967295) {
    throw new RangeError('Invalid array length');
  }
  const result: U[] = [];
  for (let index = 0; index < count; index++) {
    result.push(callback(undefined, index));
  }
  return result;
}

// Array.from(items, callback) after the items were collected (§23.1.2.1
// steps 5.k and 6–7): the callback maps each value with its index.
export function arrayMapIndexed<T, U>(values: T[], callback: (value: T, index: number) => U): U[] {
  const result: U[] = [];
  for (let index = 0; index < values.length; index++) {
    result.push(callback(values[index] as T, index));
  }
  return result;
}

// ---- Math with spread arguments (§21.3.2.24–25): NaN wins, and +0 is
// larger than -0. Holes of a spread source read as `undefined`, whose
// ToNumber is NaN.

export function mathMaxOf(parts: number[][]): number {
  let result = -Infinity;
  for (let part = 0; part < parts.length; part++) {
    const values = parts[part]!;
    for (let index = 0; index < values.length; index++) {
      const value = Object.hasOwn(values, index) ? values[index]! : NaN;
      if (Number.isNaN(value) || Number.isNaN(result)) {
        result = NaN;
      } else if (value > result || (value === 0 && result === 0 && Object.is(result, -0))) {
        result = value;
      }
    }
  }
  return result;
}

export function mathMinOf(parts: number[][]): number {
  let result = Infinity;
  for (let part = 0; part < parts.length; part++) {
    const values = parts[part]!;
    for (let index = 0; index < values.length; index++) {
      const value = Object.hasOwn(values, index) ? values[index]! : NaN;
      if (Number.isNaN(value) || Number.isNaN(result)) {
        result = NaN;
      } else if (value < result || (value === 0 && result === 0 && Object.is(value, -0))) {
        result = value;
      }
    }
  }
  return result;
}

// ---- Objects

// Object.groupBy (§20.1.2.12) with property keys.
export function objectGroupBy<T, K>(items: T[], callback: (value: T, index: number) => K): Record<string, T[]> {
  const groups: Record<string, T[]> = {};
  for (let index = 0; index < items.length; index++) {
    const value = items[index] as T;
    const key = String(callback(value, index));
    const group = groups[key];
    if (group === undefined) {
      groups[key] = [value];
    } else {
      group.push(value);
    }
  }
  return groups;
}

// Object.fromEntries (§20.1.2.7) of `[key, value]` pairs.
export function objectFromEntries<K, V>(entries: [K, V][]): Record<string, V> {
  const result: Record<string, V> = {};
  for (let index = 0; index < entries.length; index++) {
    const entry = entries[index]!;
    result[String(entry[0])] = entry[1];
  }
  return result;
}

// ---- Strings

// String.prototype.at (§22.1.3.1).
export function stringAt(value: string, index?: number): string | undefined {
  const length = value.length;
  const relativeIndex = index === undefined ? 0 : integer(index);
  const k = relativeIndex >= 0 ? relativeIndex : length + relativeIndex;
  if (k < 0 || k >= length) {
    return undefined;
  }
  return value.charAt(k);
}

// String.prototype.toLocaleUpperCase and toLocaleLowerCase without a locale
// (§22.1.3.27–28, ECMA-402 §19.1.2–3 with the root locale): the
// locale-independent case mappings.
export function stringToLocaleUpperCase(value: string): string {
  return value.toUpperCase();
}

export function stringToLocaleLowerCase(value: string): string {
  return value.toLowerCase();
}

// String.prototype.substr (Annex B §B.2.2.1).
export function stringSubstr(value: string, start: number, length?: number): string {
  const size = value.length;
  let from = integer(start);
  if (from === -Infinity) {
    from = 0;
  } else if (from < 0) {
    from = Math.max(size + from, 0);
  } else {
    from = Math.min(from, size);
  }
  const count = length === undefined ? size : integer(length);
  const end = Math.min(from + Math.max(count, 0), size);
  if (from >= end) {
    return '';
  }
  return value.substring(from, end);
}

// String.prototype.normalize (§22.1.3.15): the form word of the linked
// normalizer is the form's index in NFC, NFD, NFKC, NFKD
// (`varta_common::text::NormalizationForm`).
export function stringNormalize(value: string, form?: string): string {
  const name = form === undefined ? 'NFC' : form;
  const word = name === 'NFC' ? 0 : name === 'NFD' ? 1 : name === 'NFKC' ? 2 : name === 'NFKD' ? 3 : -1;
  if (word < 0) {
    throw new RangeError('The normalization form should be one of NFC, NFD, NFKC, NFKD.');
  }
  return varta.text.normalize(value, word);
}

// String.prototype.isWellFormed (§22.1.3.10): no lone surrogate.
export function stringIsWellFormed(value: string): boolean {
  const length = value.length;
  for (let index = 0; index < length; index++) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = index + 1 < length ? value.charCodeAt(index + 1) : 0;
      if (next >= 0xdc00 && next <= 0xdfff) {
        index++;
      } else {
        return false;
      }
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      return false;
    }
  }
  return true;
}

// String.prototype.toWellFormed (§22.1.3.32): lone surrogates become U+FFFD.
export function stringToWellFormed(value: string): string {
  if (stringIsWellFormed(value)) {
    return value;
  }
  let result = '';
  let start = 0;
  const length = value.length;
  for (let index = 0; index < length; index++) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = index + 1 < length ? value.charCodeAt(index + 1) : 0;
      if (next >= 0xdc00 && next <= 0xdfff) {
        index++;
        continue;
      }
    } else if (unit < 0xdc00 || unit > 0xdfff) {
      continue;
    }
    result += value.slice(start, index) + '�';
    start = index + 1;
  }
  return result + value.slice(start);
}

// ---- JSON.stringify replacer lists and indentation (§25.5.2). The compact
// text that the engine serializer produced is the specification's output
// with an empty gap and no property list; both options are deterministic
// rewrites of it: a property list keeps, in list order, the members of every
// object whose keys it names, and a gap breaks every non-empty object and
// array onto indented lines with `": "` between key and value.

const QUOTE = 0x22;
const BACKSLASH = 0x5c;
const COMMA = 0x2c;
const COLON = 0x3a;
const OPEN_BRACE = 0x7b;
const CLOSE_BRACE = 0x7d;
const OPEN_BRACKET = 0x5b;
const CLOSE_BRACKET = 0x5d;
const MAX_GAP = 10;

class JsonText {
  text: string;
  at: number;
  constructor(text: string) {
    this.text = text;
    this.at = 0;
  }
  unit(): number {
    return this.text.charCodeAt(this.at);
  }
  // The string token at the cursor, quotes included.
  token(): string {
    const start = this.at;
    this.at++;
    while (this.unit() !== QUOTE) {
      if (this.unit() === BACKSLASH) {
        this.at++;
      }
      this.at++;
    }
    this.at++;
    return this.text.slice(start, this.at);
  }
  // The primitive literal at the cursor (number, true, false, null).
  literal(): string {
    const start = this.at;
    while (this.at < this.text.length) {
      const unit = this.unit();
      if (unit === COMMA || unit === CLOSE_BRACE || unit === CLOSE_BRACKET) {
        break;
      }
      this.at++;
    }
    return this.text.slice(start, this.at);
  }
}

// The value at the cursor with every object's members selected by `keys`
// (canonical JSON string tokens, in list order).
function selectValue(json: JsonText, keys: string[]): string {
  const unit = json.unit();
  if (unit === QUOTE) {
    return json.token();
  }
  if (unit === OPEN_BRACKET) {
    json.at++;
    const elements: string[] = [];
    while (json.unit() !== CLOSE_BRACKET) {
      elements.push(selectValue(json, keys));
      if (json.unit() === COMMA) {
        json.at++;
      }
    }
    json.at++;
    return `[${elements.join(',')}]`;
  }
  if (unit === OPEN_BRACE) {
    json.at++;
    const names: string[] = [];
    const values: string[] = [];
    while (json.unit() !== CLOSE_BRACE) {
      names.push(json.token());
      json.at++;
      values.push(selectValue(json, keys));
      if (json.unit() === COMMA) {
        json.at++;
      }
    }
    json.at++;
    const members: string[] = [];
    for (let index = 0; index < keys.length; index++) {
      const key = keys[index]!;
      const found = names.indexOf(key);
      if (found >= 0) {
        members.push(`${key}:${values[found]!}`);
      }
    }
    return `{${members.join(',')}}`;
  }
  return json.literal();
}

// JSON.stringify with a property list: the list's String forms, without
// duplicates, as canonical string tokens.
export function jsonSelect<K>(text: string, list: K[]): string {
  const keys: string[] = [];
  for (let index = 0; index < list.length; index++) {
    const key = JSON.stringify(String(list[index]));
    if (keys.indexOf(key) < 0) {
      keys.push(key);
    }
  }
  return selectValue(new JsonText(text), keys);
}

// The gap of a numeric `space` argument (§25.5.2.1 step 7).
export function jsonGapNumber(space: number): string {
  const count = Math.min(MAX_GAP, integer(space));
  return count >= 1 ? ' '.repeat(count) : '';
}

// The gap of a string `space` argument (§25.5.2.1 step 8).
export function jsonGapString(space: string): string {
  return space.slice(0, MAX_GAP);
}

// Indent compact JSON `text` with `gap` (SerializeJSONObject and
// SerializeJSONArray with a non-empty gap).
export function jsonIndent(text: string, gap: string): string {
  if (gap.length === 0) {
    return text;
  }
  let result = '';
  let indent = '';
  let start = 0;
  const length = text.length;
  for (let index = 0; index < length; index++) {
    const unit = text.charCodeAt(index);
    if (unit === QUOTE) {
      index++;
      while (text.charCodeAt(index) !== QUOTE) {
        if (text.charCodeAt(index) === BACKSLASH) {
          index++;
        }
        index++;
      }
      continue;
    }
    if (unit === OPEN_BRACE || unit === OPEN_BRACKET) {
      const next = text.charCodeAt(index + 1);
      if (next === CLOSE_BRACE || next === CLOSE_BRACKET) {
        index++;
        continue;
      }
      indent += gap;
      result += `${text.slice(start, index + 1)}\n${indent}`;
      start = index + 1;
    } else if (unit === CLOSE_BRACE || unit === CLOSE_BRACKET) {
      indent = indent.slice(0, indent.length - gap.length);
      result += `${text.slice(start, index)}\n${indent}`;
      start = index;
    } else if (unit === COMMA) {
      result += `${text.slice(start, index + 1)}\n${indent}`;
      start = index + 1;
    } else if (unit === COLON) {
      result += `${text.slice(start, index + 1)} `;
      start = index + 1;
    }
  }
  return result + text.slice(start);
}

// ---- Iterator helpers (ES2025 §27.1.4.2–§27.1.4.13). A helper is a guest
// generator over the underlying generator: it starts nothing until its first
// `next`, pulls one value per step, and its `return` (or a `break` of the
// loop consuming it) closes the underlying generator through the `for...of`
// it runs. Arguments are validated when the helper is created, as in the
// specification, so the exported functions are not generators themselves.

// The counter limit of `take` and `drop` (§27.1.4.3 steps 3–6).
function counterLimit(limit: number): number {
  if (Number.isNaN(limit)) {
    throw new RangeError(`${limit} must be positive`);
  }
  const integerLimit = integer(limit);
  if (integerLimit < 0) {
    throw new RangeError(`${limit} must be positive`);
  }
  return integerLimit;
}

function* mapped<T, U>(
  source: Generator<T, void, undefined>,
  callback: (value: T, counter: number) => U,
): Generator<U, void, undefined> {
  let counter = 0;
  for (const value of source) {
    yield callback(value, counter);
    counter++;
  }
}

export function iteratorMap<T, U>(
  source: Generator<T, void, undefined>,
  callback: (value: T, counter: number) => U,
): Generator<U, void, undefined> {
  return mapped<T, U>(source, callback);
}

function* filtered<T>(
  source: Generator<T, void, undefined>,
  predicate: (value: T, counter: number) => boolean,
): Generator<T, void, undefined> {
  let counter = 0;
  for (const value of source) {
    if (predicate(value, counter)) {
      yield value;
    }
    counter++;
  }
}

export function iteratorFilter<T>(
  source: Generator<T, void, undefined>,
  predicate: (value: T, counter: number) => boolean,
): Generator<T, void, undefined> {
  return filtered<T>(source, predicate);
}

function* taken<T>(source: Generator<T, void, undefined>, limit: number): Generator<T, void, undefined> {
  if (limit === 0) {
    // Close the underlying iterator without pulling a value.
    source.return();
    return;
  }
  let remaining = limit;
  for (const value of source) {
    yield value;
    remaining--;
    if (remaining === 0) {
      return;
    }
  }
}

export function iteratorTake<T>(source: Generator<T, void, undefined>, limit: number): Generator<T, void, undefined> {
  return taken<T>(source, counterLimit(limit));
}

function* dropped<T>(source: Generator<T, void, undefined>, limit: number): Generator<T, void, undefined> {
  let remaining = limit;
  for (const value of source) {
    if (remaining > 0) {
      remaining--;
      continue;
    }
    yield value;
  }
}

export function iteratorDrop<T>(source: Generator<T, void, undefined>, limit: number): Generator<T, void, undefined> {
  return dropped<T>(source, counterLimit(limit));
}

function* flatMappedArrays<T, U>(
  source: Generator<T, void, undefined>,
  callback: (value: T, counter: number) => U[],
): Generator<U, void, undefined> {
  let counter = 0;
  for (const value of source) {
    const inner = callback(value, counter);
    counter++;
    for (let index = 0; index < inner.length; index++) {
      yield inner[index] as U;
    }
  }
}

export function iteratorFlatMapArrays<T, U>(
  source: Generator<T, void, undefined>,
  callback: (value: T, counter: number) => U[],
): Generator<U, void, undefined> {
  return flatMappedArrays<T, U>(source, callback);
}

export function iteratorReduce<T, A>(
  source: Generator<T, void, undefined>,
  callback: (accumulator: A, value: T, counter: number) => A,
  initial: A,
): A {
  let accumulator = initial;
  let counter = 0;
  for (const value of source) {
    accumulator = callback(accumulator, value, counter);
    counter++;
  }
  return accumulator;
}

export function iteratorReduceFirst<T>(
  source: Generator<T, void, undefined>,
  callback: (accumulator: T, value: T, counter: number) => T,
): T {
  let accumulator: T[] = [];
  let counter = 0;
  for (const value of source) {
    if (counter === 0) {
      accumulator = [value];
    } else {
      accumulator = [callback(accumulator[0]!, value, counter)];
    }
    counter++;
  }
  if (counter === 0) {
    throw new TypeError('Reduce of a done iterator with no initial value');
  }
  return accumulator[0]!;
}

export function iteratorToArray<T>(source: Generator<T, void, undefined>): T[] {
  const result: T[] = [];
  for (const value of source) {
    result.push(value);
  }
  return result;
}

export function iteratorForEach<T>(
  source: Generator<T, void, undefined>,
  callback: (value: T, counter: number) => void,
): void {
  let counter = 0;
  for (const value of source) {
    callback(value, counter);
    counter++;
  }
}

export function iteratorSome<T>(
  source: Generator<T, void, undefined>,
  predicate: (value: T, counter: number) => boolean,
): boolean {
  let counter = 0;
  for (const value of source) {
    if (predicate(value, counter)) {
      return true;
    }
    counter++;
  }
  return false;
}

export function iteratorEvery<T>(
  source: Generator<T, void, undefined>,
  predicate: (value: T, counter: number) => boolean,
): boolean {
  let counter = 0;
  for (const value of source) {
    if (!predicate(value, counter)) {
      return false;
    }
    counter++;
  }
  return true;
}

export function iteratorFind<T>(
  source: Generator<T, void, undefined>,
  predicate: (value: T, counter: number) => boolean,
): T | undefined {
  let counter = 0;
  for (const value of source) {
    if (predicate(value, counter)) {
      return value;
    }
    counter++;
  }
  return undefined;
}
