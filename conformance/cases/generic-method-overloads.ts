// Generic overload signatures of methods infer their type arguments per
// call, also in an instance of a generic class, whose static overloads
// resolve as well.
interface Pair<A, B> { first: A; second: B }
class Box<V> {
  constructor(private value: V) {}
  with<R>(other: R): Pair<V, R>;
  with(): Pair<V, V>;
  with(other?: unknown): unknown {
    return { first: this.value, second: other === undefined ? this.value : other };
  }
  static of(value: number, scale: number): Box<number>;
  static of(value: string): Box<string>;
  static of(value: number | string, scale?: number): unknown {
    return typeof value === 'number' ? new Box(value * (scale ?? 1)) : new Box(value);
  }
}
export function main(): void {
  const box = Box.of(3, 2);
  const mixed = box.with('x');
  const same = Box.of('s').with();
  console.log(String(mixed.first + 1) + mixed.second.toUpperCase() + ' ' + same.first + same.second);
}
