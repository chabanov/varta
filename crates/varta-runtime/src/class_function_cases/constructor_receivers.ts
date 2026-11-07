let retained: Ready[] = [];
let identities: (() => Ready)[] = [];
let delayed: Promise<string>[] = [];
function retain(value: Ready): void { retained.push(value); }
async function observe(value: Ready): Promise<string> {
 await Promise.resolve(0);
 return value.text;
}
class Ready {
 self: Ready = this;
 text: string = 'live';
 #secret: number = 2;
 n: number;
 fn: (this: Ready) => number = function(this: Ready): number { return this.n + 1; };
 constructor(n: number) {
  this.n = n;
  retain(this);
  identities.push(() => this);
  delayed.push(observe(this));
  const alias = this;
  const method = this.read;
  console.log(this.self === alias, this.doubled, this['read'](), method.call(this), this.fn());
  console.log(this.#secretValue());
  this.update(n + 2);
  this.doubled = (n + 2) * 2;
  console.log(this.read());
 }
 read(): number { return this.n; }
 get doubled(): number { return this.n * 2; }
 set doubled(value: number) { this.n = value / 2; }
 #secretValue(): number { return this.n + this.#secret; }
 update(n: number): void { this.n = n; }
}
class Parent { base: number = 4; }
let leaf: Leaf | undefined;
class Leaf extends Parent {
 child: string;
 constructor() {
  super();
  this.child = 'leaf';
  leaf = this;
  console.log(this.summary());
 }
 summary(): string { return this.child + this.base; }
}
let failed: Failed | undefined;
class Failed {
 text: string = 'retained';
 constructor() {
  failed = this;
  throw new Error('after publication');
 }
}
class Generic<T> {
 value: T;
 constructor(value: T) {
  this.value = value;
  const same = this;
  console.log(same === this, this.get());
 }
 get(): T { return this.value; }
}
class RedeclaredBase { n: number = 2; }
function inspect(value: Redeclared): number { return value.read() + 1; }
class Redeclared extends RedeclaredBase {
 n: number = inspect(this);
 read(): number { return this.n; }
}
export async function main(): Promise<void> {
 const first = new Ready(3);
 const second = new Ready(8);
 const secondIdentity = identities[1]!;
 console.log(retained[0] === first, secondIdentity() === second, first.n, second.n);
 const derived = new Leaf();
 console.log(leaf === derived, derived.child);
 try { new Failed(); } catch (error) { console.log(error instanceof Error); }
 if (failed !== undefined) console.log(failed.text);
 new Generic<number>(11);
 new Generic<string>('generic');
 const redeclared = new Redeclared();
 console.log(redeclared.n);
 const result = await Promise.all(delayed);
 const firstIdentity = identities[0]!;
 console.log(result[0], result[1], firstIdentity() === first);
}
