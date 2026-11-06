// Call signatures, construct signatures, constructor types and the
// polymorphic `this` type.
interface Adder {
  (value: number): number;
}
type Formatter = { (value: number, digits?: number): string };
class Animal {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
  rename(name: string): this {
    this.name = name;
    return this;
  }
}
class Dog extends Animal {
  bark(): string {
    return `${this.name}: woof`;
  }
}
interface AnimalConstructor {
  new (name: string): Animal;
}
type Factory<T> = new (name: string) => T;
class Base {
  id(): number {
    return 0;
  }
}
class Concrete extends Base {
  id(): number {
    return 7;
  }
}

function create<T>(make: Factory<T>, name: string): T {
  return new make(name);
}
function build(make: AnimalConstructor): Animal {
  return new make("built");
}
function first(make: abstract new () => Base): string {
  return typeof make;
}
function makeBase(make: new () => Base): number {
  return new make().id();
}

export function main(): void {
  const add: Adder = (value: number) => value + 1;
  const format: Formatter = (value: number, digits?: number) => value.toFixed(digits ?? 1);
  console.log(`${add(1)} ${format(2)} ${format(2, 3)}`);
  const dog = create(Dog, "rex");
  console.log(dog.bark());
  console.log(build(Animal).name);
  console.log(new Dog("a").rename("b").bark());
  const ctor: typeof Dog = Dog;
  console.log(new ctor("typed").bark());
  console.log(`${first(Concrete)} ${makeBase(Concrete)}`);
}
