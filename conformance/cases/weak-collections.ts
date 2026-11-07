// WeakMap and WeakSet (ECMA-262 §24.3, §24.4): object and unregistered
// symbol keys, replacement and deletion, and CanBeHeldWeakly's TypeError
// for a registered symbol.
class Key {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
}

export function main(): void {
  const map = new WeakMap<object, number>();
  const a = new Key('a');
  const b = { b: 1 };
  map.set(a, 1).set(b, 2);
  map.set(a, 3);
  console.log(`${map.get(a)} ${map.get(b)} ${map.has(new Key('a'))} ${map.delete(b)} ${map.has(b)}`);
  const tag = Symbol('tag');
  const set = new WeakSet<symbol>([tag]);
  console.log(`${set.has(tag)} ${set.has(Symbol('tag'))} ${set.delete(tag)} ${set.has(tag)}`);
  try {
    set.add(Symbol.for('tag'));
  } catch (error) {
    console.log(error instanceof TypeError ? error.message : 'no TypeError');
  }
}
