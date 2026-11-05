// A union of two record interfaces passed where a record type with their
// shared field is expected. TypeScript accepts it and Node prints `1 2`; v38
// views a single record or class instance as a narrower record, but does not
// convert a union of record shapes to one record type.
interface A {
  type: 'a';
  n: number;
}
interface B {
  type: 'b';
  n: number;
}

function pick(first: boolean): A | B {
  return first ? { type: 'a', n: 1 } : { type: 'b', n: 2 };
}

function show(value: { n: number }): string {
  return `${value.n}`;
}

export function main(): void {
  console.log(`${show(pick(true))} ${show(pick(false))}`);
}
