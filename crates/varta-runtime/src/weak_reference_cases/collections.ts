// WeakMap and WeakSet (ECMA-262 §24.3, §24.4) without a collection: keys of
// every object kind and unregistered symbols, replacement, deletion,
// growth, constructors and the TypeErrors of CanBeHeldWeakly.
class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
}

function describe(error: unknown): string {
  return error instanceof Error ? `${error.name}: ${error.message}` : 'not an error';
}

export function main(): void {
  const map = new WeakMap<object, string>();
  const a = new Point(1, 2);
  const b = { name: 'b' };
  const list = [1, 2, 3];
  const callback = (): number => 1;
  console.log(String(map.set(a, 'point') === map));
  map.set(b, 'record').set(list, 'array');
  const functions = new WeakMap<() => number, string>();
  functions.set(callback, 'function');
  console.log(`${map.get(a)} ${map.get(b)} ${map.get(list)} ${functions.get(callback)}`);
  console.log(`${functions.has(callback)} ${functions.has((): number => 1)}`);
  console.log(`${map.has(a)} ${map.has({ name: 'b' })} ${map.get(new Point(1, 2))}`);
  map.set(a, 'replaced');
  console.log(String(map.get(a)));
  console.log(`${map.delete(a)} ${map.delete(a)} ${map.has(a)} ${map.get(a)}`);
  map.set(a, 'again');
  console.log(String(map.get(a)));

  const points: Point[] = [];
  const byPoint = new WeakMap<Point, number>();
  for (let i = 0; i < 500; i++) {
    const point = new Point(i, -i);
    points.push(point);
    byPoint.set(point, i * 2);
  }
  let sum = 0;
  for (const point of points) {
    sum += byPoint.get(point) ?? 0;
  }
  console.log(String(sum));
  for (let i = 0; i < 500; i += 2) {
    byPoint.delete(points[i]);
  }
  let left = 0;
  for (const point of points) {
    if (byPoint.has(point)) {
      left++;
    }
  }
  console.log(String(left));

  const s = Symbol('s');
  const symbols = new WeakMap<symbol, number>([[s, 1]]);
  symbols.set(Symbol.iterator, 2);
  console.log(`${symbols.get(s)} ${symbols.get(Symbol.iterator)} ${symbols.has(Symbol('s'))}`);
  console.log(`${symbols.get(Symbol.for('s'))} ${symbols.has(Symbol.for('s'))} ${symbols.delete(Symbol.for('s'))}`);
  try {
    symbols.set(Symbol.for('registered'), 3);
  } catch (error) {
    console.log(describe(error));
  }

  const seen = new WeakSet<object>([a, b]);
  console.log(`${seen.has(a)} ${seen.has(b)} ${seen.has(list)}`);
  console.log(String(seen.add(list) === seen));
  console.log(`${seen.delete(b)} ${seen.delete(b)} ${seen.has(b)} ${seen.has(list)}`);
  const tags = new WeakSet<symbol>();
  tags.add(s);
  console.log(`${tags.has(s)} ${tags.has(Symbol.for('s'))}`);
  try {
    tags.add(Symbol.for('registered'));
  } catch (error) {
    console.log(describe(error));
  }
  try {
    new WeakSet<symbol>([s, Symbol.for('registered')]);
  } catch (error) {
    console.log(describe(error));
  }
  console.log(`${typeof map} ${typeof seen} ${map instanceof WeakMap} ${seen instanceof WeakSet}`);
}
