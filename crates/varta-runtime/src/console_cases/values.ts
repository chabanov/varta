class Foo { a = 1; b = 'x'; }
class Empty {}
class MyError extends Error { code = 5; }
class Named extends Error {
  constructor(m: string) { super(m); this.name = 'Named'; }
}
class Box<T> { value: T; constructor(value: T) { this.value = value; } }
interface Node1 { name: string; next: Node1 | null }
export function main(): void {
  console.log(true, false, 1, -0, 1.5, NaN, -Infinity, 10n, -3n);
  console.log(undefined, null, 'top', ['nested', "it's"]);
  console.log();
  console.log('one string');
  console.log({ a: [1, 2, { b: { c: { d: 1 } } }], s: "it's" });
  console.log(new Foo(), new Empty(), [new Foo()], { f: { g: { h: new Foo() } } });
  console.log(new MyError('m'));
  console.log(new Named('n'));
  console.log(new Error('x', { cause: 1 }));
  console.log(new TypeError('bad'), [new RangeError('r')], { e: new Error('') });
  console.log(new Box(3), new Box('s'));
  console.log(new Map<string, unknown>([["a", 1], ["b", { x: 1 }]]), new Set([1, 2, 3]), new Map<string, number>(), new Set<string>());
  console.log(new Date(0), [new Date(NaN)], { d: new Date(86400000) });
  console.log(new Uint8Array([1, 2, 3]), new Uint8Array(0));
  console.log(Promise.resolve(4));
  const xs: number[] = [];
  for (let i = 0; i < 30; i++) xs.push(i * 7);
  console.log(xs);
  const big: number[] = [];
  for (let i = 0; i < 120; i++) big.push(i);
  console.log(big);
  const words = ['apple', 'banana', 'cherry', 'dates', 'elderberry', 'fig', 'grape', 'honeydew'];
  console.log(words);
  console.log({ alpha: 'aaaaaaaaaaaaaaaa', beta: 'bbbbbbbbbbbbbbbbbbbb', gamma: 'cccccccccccccccccc', delta: 1 });
  const a: Node1 = { name: 'a', next: null };
  const b: Node1 = { name: 'b', next: a };
  a.next = b;
  console.log(a);
  console.log('%s is %d years and %i, %f', 'Bob', 42.5, 42.5, '3.25');
  console.log('%o and %O', [1, 2], { k: [3] });
  console.log('%j|%j', { a: 1 }, 'x');
  console.log('100%', 5, '%%');
  console.log('%c styled', 'color: red');
  const fmt: string = 'dyn %s!';
  console.log(fmt, 'value');
  console.log({ 'a-b': 1, 2: 'two', _ok: true, $no: null });
  console.log({ long: 'x'.repeat(100) });
  console.log(['a\nb', 'line one\nline two\nline three\nline four and more text here to be long']);
  console.log(() => 1, [() => 2]);
  console.log({ u: undefined, n: null, nested: [[[[1]]]] });
}
