// `this` in static methods called through class values and constructor
// types: the class the value holds is `this`, also for a subclass the
// module never names.
class Animal {
  constructor(public name: string) {}
  static create(name: string): Animal { return new this(name); }
  static kind(): string { return 'animal'; }
  static label(): string { return this.kind() + ':' + this.name; }
  speak(): string { return this.name + ' ...'; }
}
class Dog extends Animal {
  static kind(): string { return 'dog'; }
  speak(): string { return this.name + ' woof'; }
}
const DEFAULT = Dog;
function create<T>(make: new (name: string) => T, name: string): T { return new make(name); }
function adopt(make: new (name: string) => Animal, name: string): string { return new make(name).speak(); }
function labelOf(kind: typeof Animal): string { return kind.label(); }
export function main(): void {
  const kind: typeof Animal = DEFAULT;
  console.log(kind.create('rex').speak() + ' | ' + labelOf(Animal) + ' | ' + labelOf(kind));
  console.log(create(kind, 'fido').speak() + ' | ' + adopt(kind, 'max'));
  const other = Animal;
  console.log(String(other === kind) + ' ' + other.name + ' ' + DEFAULT.name);
}
