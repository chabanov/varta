// Generic overload signatures: each candidate infers its type arguments
// from the call's arguments, as tsc does, and the call takes the first
// candidate that accepts them; the implementation's result is checked
// against the selected signature's instantiated result type.
function first<T>(items: T[]): T | undefined;
function first(text: string): string;
function first(value: unknown): unknown {
  if (typeof value === 'string') return value.charAt(0);
  return Array.isArray(value) ? value[0] : undefined;
}
interface Pair<A, B> { first: A; second: B }
function pair<A, B>(a: A, b: B): Pair<A, B>;
function pair<A>(a: A): Pair<A, A>;
function pair(a: unknown, b?: unknown): unknown {
  return { first: a, second: b === undefined ? a : b };
}
function label<T extends string>(value: T): T;
function label(value: number): string;
function label(value: string | number): string {
  return typeof value === 'string' ? value : 'n' + String(value);
}
export function main(): void {
  const n = first([3, 4]);
  const s = first('hey');
  const w = first(['x', 'y']);
  console.log(String(n) + ' ' + s + ' ' + String(w));
  const p = pair(1, 'one');
  const q = pair(true);
  console.log(String(p.first + 1) + p.second.toUpperCase() + ' ' + String(q.first && q.second));
  const a = label('a');
  console.log(a + ' ' + label(2));
}
