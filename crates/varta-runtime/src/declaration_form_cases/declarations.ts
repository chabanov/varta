// declare fields, override, parameter properties with defaults and
// implements of several interfaces.
interface Named { name(): string }
interface Sized { readonly size: number }
class Base {
  value: number = 1;
  greet(): string { return 'base'; }
  constructor(public readonly id: number = 0, protected tag = 'base') {}
}
class Child extends Base implements Named, Sized {
  declare value: number;
  readonly size = 3;
  override greet(): string { return 'child+' + super.greet(); }
  name(): string { return this.tag + String(this.id); }
}
export function main(): void {
  const c = new Child(7, 'c');
  const named: Named = c;
  const sized: Sized = c;
  console.log(c.greet() + ' ' + String(c.value) + ' ' + named.name() + ' ' + String(sized.size));
  const b = new Base();
  console.log(String(b.id) + ' ' + b.greet());
}
