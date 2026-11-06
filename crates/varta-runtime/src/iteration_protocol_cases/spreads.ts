// Array literal spreads and Array.from over arrays, generators, collections
// and iterable class instances; generator methods with parameters.
class Base {
  protected values: string[] = ['a', 'b'];
  *[Symbol.iterator](): Generator<string> {
    for (const v of this.values) yield v;
  }
  *pairs(prefix: string): Generator<string, number> {
    let n = 0;
    for (const v of this.values) {
      yield `${prefix}${v}`;
      n++;
    }
    return n;
  }
}
class Upper extends Base {
  *[Symbol.iterator](): Generator<string> {
    for (const v of this.values) yield v.toUpperCase();
  }
}
function* counted(notes: string[]): Generator<number> {
  try {
    notes.push('start');
    yield 1;
    yield 2;
  } finally {
    notes.push('closed');
  }
}
function show(items: Base): string {
  return [...items].join('');
}
export function main(): void {
  console.log(show(new Base()));
  console.log(show(new Upper()));
  const u = new Upper();
  const g = u.pairs('>');
  console.log(`${[...g].join(' ')}`);
  const again = u.pairs('#');
  let step = again.next();
  while (!step.done) {
    console.log(`${step.value}`);
    step = again.next();
  }
  console.log(`returned ${step.value}`);
  const notes: string[] = [];
  const all = [...counted(notes)];
  console.log(`${all.join(',')} ${notes.join(',')}`);
  const trace2: string[] = [];
  for (const x of counted(trace2)) {
    if (x === 1) break;
  }
  console.log(trace2.join(','));
  const maybe: (number | undefined)[] = [1, undefined, 3];
  console.log(`${[...maybe].length} ${[...maybe][1]}`);
  const nested = [...[...[1, 2]], ...[[3]][0]];
  console.log(nested.join('-'));
  const widened: (number | string)[] = [...[1, 2], 'x'];
  console.log(widened.join(''));
  const inferred = ['s', ...[1, 2]];
  console.log(`${inferred.length} ${typeof inferred[0]} ${typeof inferred[1]}`);
  const big: number[] = [];
  for (let i = 0; i < 5000; i++) big.push(i);
  const copy = [...big, ...big.map((x) => `${x}`)];
  console.log(`${copy.length} ${copy[4999]} ${copy[9999]}`);
  const fromGen = Array.from(counted([]));
  console.log(`${fromGen.length}`);
  const text = new Set<string>(['x', 'y']);
  console.log([...text, ...new Base()].join(''));
}
