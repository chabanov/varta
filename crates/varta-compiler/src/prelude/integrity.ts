// Engine prelude of checked-f64-utf16-v38 (`integrity_levels`, K9 D2):
// the failures of property writes, additions and deletions on an object
// with an integrity level (ECMA-262 §7.3.15 SetIntegrityLevel; modules are
// strict code, so a failed [[Set]] or [[Delete]] throws), with V8's
// messages, and the array methods that run element by element once their
// receiver has a level. The compiler loads it when a program names
// `Object.freeze`, `Object.seal`, `Object.preventExtensions` or their
// predicates; its write checks (`lower::integrity`) call these functions
// only after the header bits of the written object said the write fails.
// See docs/architecture.md (Integrity levels).

// V8's description of an array receiver.
const ARRAY = '[object Array]';

// [[Set]] of an existing non-writable data property (§10.1.9.2 step 2.a).
export function integrityReadOnly(key: string, object: string): void {
  throw new TypeError(`Cannot assign to read only property '${key}' of object '${object}'`);
}

// [[Set]] that would add a property to a non-extensible object
// (§10.1.6.3 ValidateAndApplyPropertyDescriptor step 2.a).
export function integrityAdd(key: string): void {
  throw new TypeError(`Cannot add property ${key}, object is not extensible`);
}

// A derived class defines its first own field on an instance its base
// constructor made non-extensible (§7.3.33 DefineField).
export function integrityDefined(extensible: boolean, key: string): void {
  if (!extensible) {
    throw new TypeError(`Cannot define property ${key}, object is not extensible`);
  }
}

// [[Delete]] of a non-configurable property (§10.1.10.1 step 4).
export function integrityDelete(key: string, object: string): void {
  throw new TypeError(`Cannot delete property '${key}' of ${object}`);
}

// The array forms, keyed by an element index or `length`.
export function integrityIndexReadOnly(index: number): void {
  integrityReadOnly(`${index}`, ARRAY);
}

export function integrityIndexAdd(index: number): void {
  integrityAdd(`${index}`);
}

export function integrityIndexDelete(index: number): void {
  integrityDelete(`${index}`, ARRAY);
}

export function integrityLengthReadOnly(): void {
  integrityReadOnly('length', ARRAY);
}

// `Object.freeze` and `Object.seal` of a `Uint8Array` with elements
// (§10.4.5.? TypedArray [[DefineOwnProperty]] refuses non-configurable
// elements; V8's messages).
export function integrityBytes(length: number, sealing: boolean): void {
  if (length > 0) {
    throw new TypeError(
      `Cannot ${sealing ? 'seal' : 'freeze'} array buffer views with elements`,
    );
  }
}

// Whether `array` has an own element (§7.3.16 TestIntegrityLevel of an
// array: `length` is never configurable).
export function integrityHasElements<T>(array: T[]): boolean {
  const length = array.length;
  for (let index = 0; index < length; index++) {
    if (Object.hasOwn(array, index)) {
      return true;
    }
  }
  return false;
}

// Array.prototype.reverse (§23.1.3.26) step by step, for a receiver with an
// integrity level: each write and deletion is checked, so the first one that
// fails throws after the earlier ones took effect.
export function integrityReverse<T>(array: T[]): T[] {
  const length = array.length;
  const middle = Math.floor(length / 2);
  let lower = 0;
  while (lower !== middle) {
    const upper = length - lower - 1;
    const lowerExists = Object.hasOwn(array, lower);
    const upperExists = Object.hasOwn(array, upper);
    if (lowerExists && upperExists) {
      const lowerValue = array[lower] as T;
      const upperValue = array[upper] as T;
      array[lower] = upperValue;
      array[upper] = lowerValue;
    } else if (lowerExists) {
      const lowerValue = array[lower] as T;
      delete array[lower];
      array[upper] = lowerValue;
    } else if (upperExists) {
      const upperValue = array[upper] as T;
      array[lower] = upperValue;
      delete array[upper];
    }
    lower++;
  }
  return array;
}

// ToIntegerOrInfinity of an optional relative index, resolved against
// `length` (§23.1.3.7 steps 3–8).
function relative(value: number | undefined, length: number, fallback: number): number {
  if (value === undefined) {
    return fallback;
  }
  const integer = Number.isNaN(value) ? 0 : Math.trunc(value);
  if (integer < 0) {
    return Math.max(length + integer, 0);
  }
  return Math.min(integer, length);
}

// Array.prototype.fill (§23.1.3.7) step by step, for a receiver with an
// integrity level.
export function integrityFill<T>(
  array: T[],
  value: T,
  start: number | undefined,
  end: number | undefined,
): T[] {
  const length = array.length;
  let index = relative(start, length, 0);
  const final = relative(end, length, length);
  while (index < final) {
    array[index] = value;
    index++;
  }
  return array;
}
