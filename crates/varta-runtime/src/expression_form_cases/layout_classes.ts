interface Named {
  name: string;
}
class Person {
  #secret = 1;
  age?: number;
  name: string;
  constructor(name: string) {
    this.name = name;
  }
  greet(): string {
    return "hi " + this.name + String(this.#secret);
  }
}
class Admin extends Person {
  level = 3;
}
function copy(named: Named): string {
  return JSON.stringify({ ...named, tag: true });
}
function rest(named: Named): string {
  const { name, ...others } = named;
  return name + " " + JSON.stringify(others);
}
export function main(): void {
  const person = new Person("p");
  console.log(copy(person));
  person.age = 30;
  console.log(copy(person));
  console.log(copy(new Admin("a")));
  console.log(rest(new Admin("b")));
  console.log(copy({ name: "plain" }));
}
