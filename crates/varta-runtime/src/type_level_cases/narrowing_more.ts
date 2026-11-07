// Narrowing a class instance to a subclass, `unknown` to an object type
// and `this is T` method predicates; a lying predicate throws TypeError at
// the narrowed read (Varta's checked deviation: Node would continue).
class Animal {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
  isDog(): this is Dog {
    return this instanceof Dog;
  }
}
class Dog extends Animal {
  bark(): string {
    return `${this.name} woof`;
  }
}
class Cat extends Animal {}
interface Point {
  x: number;
  y: number;
}
function isDog(animal: Animal): animal is Dog {
  return animal instanceof Dog;
}
function isPoint(value: unknown): value is Point {
  return typeof value === "object" && value !== null && "x" in value && "y" in value;
}
function lies(animal: Animal): animal is Dog {
  return animal.name.length > 0;
}

export function main(): void {
  const rex: Animal = new Dog("rex");
  if (isDog(rex)) {
    console.log(rex.bark());
  }
  if (rex.isDog()) {
    console.log(rex.bark());
  }
  const parsed: unknown = JSON.parse('{"x":1,"y":2}');
  if (isPoint(parsed)) {
    console.log(String(parsed.x + parsed.y));
  }
  const tom: Animal = new Cat("tom");
  try {
    if (lies(tom)) {
      console.log(tom.bark());
    }
  } catch (error) {
    console.log(error instanceof TypeError ? "TypeError" : "other");
  }
}
