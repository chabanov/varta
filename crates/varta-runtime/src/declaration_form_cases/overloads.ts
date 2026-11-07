// Function, method, static and constructor overloads.
function pad(x: string): string;
function pad(x: number, width: number): string;
function pad(x: string | number, width?: number): string {
  if (typeof x === 'string') return '[' + x + ']';
  return String(x).padStart(width ?? 0, '0');
}
function id(x: string): string;
function id(x: number): number;
function id(x: boolean): boolean;
function id(x: string | number | boolean): string | number | boolean { return x; }
function sum(...values: number[]): number;
function sum(values: number[], start: number): number;
function sum(first: number | number[], ...rest: number[]): number {
  if (typeof first === 'number') { let s = first; for (const v of rest) s += v; return s; }
  let s = rest[0] ?? 0;
  for (const v of first) s += v;
  return s;
}
class Store {
  private items: string[] = [];
  constructor();
  constructor(first: string);
  constructor(first?: string) { if (first !== undefined) this.items.push(first); }
  get(index: number): string;
  get(key: string): number;
  get(k: number | string): string | number {
    return typeof k === 'number' ? this.items[k] ?? '?' : this.items.indexOf(k);
  }
  static of(item: string): Store;
  static of(item: number): Store;
  static of(item: string | number): Store { return new Store(String(item)); }
}
class Tagged extends Store {
  tag(): string { return 'tagged ' + this.get(0); }
}
export function main(): void {
  console.log(pad('a') + ' ' + pad(7, 3));
  console.log(id('s').toUpperCase() + ' ' + String(id(2) * 10) + ' ' + String(!id(true)));
  console.log(String(sum(1, 2, 3)) + ' ' + String(sum([4, 5], 6)));
  const store = new Store('first');
  console.log(store.get(0) + ' ' + String(store.get('first') + 1) + ' ' + store.get(5));
  console.log(Store.of(42).get(0) + ' ' + String(new Store().get('x')));
  console.log(new Tagged('t').tag());
}
