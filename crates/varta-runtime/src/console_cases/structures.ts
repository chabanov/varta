class Point { x: number; y: number; #secret = 1; constructor(x: number, y: number) { this.x = x; this.y = y; } }
class Plain extends Error {}
class Coded extends Error {
  code: string;
  constructor(message: string, code: string) { super(message); this.code = code; }
}
interface Tree { label: string; children: Tree[] }
export function main(): void {
  console.info('info', 1);
  console.debug('debug', [1]);
  console.log(new Point(1, 2), [new Point(3, 4), new Point(5, 6)]);
  console.log(new Plain('p'), new Coded('bad thing', 'E_BAD'));
  console.log(new AggregateError([1, new Error('e')], 'agg'));
  console.log(new Error('outer', { cause: new Error('inner') }));
  console.log({ err: new Error('nested\nline') });
  const t: Tree = { label: 'root', children: [{ label: 'a', children: [{ label: 'a1', children: [] }] }, { label: 'b', children: [] }] };
  console.log(t);
  console.log(JSON.parse('{"a":[{"b":{"c":{"d":{"e":1}}}}],"f":"g"}') as unknown);
  const strs: string[] = [];
  for (let i = 0; i < 26; i++) strs.push('item' + i);
  console.log(strs);
  const mixed: (number | string)[] = [1, 'two', 3, 'four', 5, 'six', 7, 'eight'];
  console.log(mixed);
  const nums = [1.5, -2.25, 3, 400000, 5e-7, 6, 7, 8, 9, 10, 11];
  console.log(nums);
  const m = new Map<string, number[]>();
  m.set('first', [1, 2, 3]);
  m.set('second', [4, 5]);
  console.log(m);
  const s = new Set<string>(['a', 'b']);
  s.delete('a');
  s.add('c');
  console.log(s, { s });
  console.log([[1, 2, [3, 4, [5, [6]]]], { a: { b: { c: {} } } }]);
  console.log({ a: { b: { c: [] as number[] } } });
  console.log([undefined, null, true, 0n, -0, '']);
  const sparse: number[] = [];
  sparse[2] = 1;
  sparse[5] = 2;
  const holes: number[] = []; holes.length = 4;
  console.log(sparse, holes);
  console.log('%s|%s|%s', { a: { b: 1 } }, [1, [2, [3]]], new Error('e'));
  console.log('%d|%d|%d|%d', '0x10', true, null, undefined);
  console.log('%i|%f|%i', ' 12px', '1e3x', -3.9);
  console.log('%O', { deep: { er: { est: { x: 1 } } } });
  console.log('%o', { deep: { er: { est: { x: 1 } } } });
  console.log('%j', { n: undefined, d: [1] });
  console.log('trailing %s');
  console.log('%s and %s', 'one');
  console.log('a', 'b', 'c');
  console.log({ list: [(): void => {}] });
  const p = Promise.resolve({ v: [1, 2] });
  console.log(p);
  console.log({ p });
  console.log(new Uint8Array(120));
  console.log({ bytes: new Uint8Array([255, 0, 128]) });
  const date = new Date(Date.UTC(2024, 1, 29, 13, 5, 9, 7));
  console.log(date, { date }, [date]);
  console.log({ 'key with space': 1, 'quote\'s': 2, 'ünï': 3, '': 4 });
  console.log(['x'.repeat(30), 'y'.repeat(30), 'z'.repeat(30)]);
  console.log({ s: 'it\'s "q"' }, ['`b` it\'s "q"']);
  console.log(['\u0001\u007f\u0085', 'tab\there']);
}
