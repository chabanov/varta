// yield* over generators (next/throw/return forwarding), arrays and
// iterable class instances.
const notes: string[] = [];
function* inner(): Generator<number, string | undefined> {
  try {
    yield 1;
    yield 2;
  } catch (e) {
    notes.push(`inner caught ${e}`);
    yield 99;
  } finally {
    notes.push('inner finally');
  }
  return 'done';
}
function* outer(): Generator<number, string | undefined> {
  notes.push('outer start');
  const r = yield* inner();
  notes.push(`inner returned ${r}`);
  yield* [7, 8];
  return 'outer done';
}
class Tree {
  constructor(public value: number, public children: Tree[]) {}
  *[Symbol.iterator](): Generator<number> {
    yield this.value;
    for (const child of this.children) yield* child;
  }
}
function* echo(): Generator<string, void, string | undefined> {
  let got = yield 'first';
  while (got !== undefined) {
    got = yield `got ${got}`;
  }
}
function* relay(): Generator<string, void, string | undefined> {
  yield* echo();
}
function* withFinally(): Generator<number> {
  try {
    yield* [1, 2, 3];
  } finally {
    notes.push('outer finally');
  }
}
function* sub(): Generator<number, number> {
  try {
    yield 1;
    yield 2;
  } finally {
    notes.push('sub finally');
  }
  return 3;
}
// A void generator delegating to a source with a number return value.
function* top(): Generator<number> {
  const n = yield* sub();
  notes.push(`sub returned ${n}`);
}
export function main(): void {
  console.log([...outer()].join(','));
  console.log(notes.join('; '));
  notes.length = 0;
  const g = outer();
  console.log(`${g.next().value}`);
  console.log(`${g.throw('boom').value}`);
  const last = g.next();
  console.log(`${last.value} ${last.done}`);
  console.log(notes.join('; '));
  notes.length = 0;
  const h = outer();
  h.next();
  const closed = h.return('early');
  console.log(`${closed.value} ${closed.done}`);
  console.log(notes.join('; '));
  notes.length = 0;
  const t = new Tree(1, [new Tree(2, [new Tree(3, [])]), new Tree(4, [])]);
  console.log([...t].join(' '));
  const r = relay();
  console.log(`${r.next().value}`);
  console.log(`${r.next('a').value}`);
  console.log(`${r.next('b').value}`);
  const end = r.next(undefined);
  console.log(`${end.done}`);
  for (const x of withFinally()) {
    if (x === 2) break;
  }
  console.log(notes.join('; '));
  const w = withFinally();
  w.next();
  try {
    w.throw('x');
  } catch (e) {
    console.log(`${e instanceof TypeError} ${(e as TypeError).message}`);
  }
  const v = withFinally();
  v.next();
  const vr = v.return(undefined);
  console.log(`${vr.done} ${vr.value}`);
  notes.length = 0;
  for (const x of top()) {
    if (x === 1) break;
  }
  console.log(`${[...top()].join(',')} ${notes.join('; ')}`);
  const tt = top();
  tt.next();
  const rr = tt.return(undefined);
  console.log(`${rr.done} ${rr.value}`);
}
