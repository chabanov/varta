// Generic methods (v37 `class_generics`): instance, static and async methods
// with constraints, defaults, inference from arguments and from the
// contextual result type, explicit type arguments, overrides dispatched per
// instantiation and calls through `this` and `super`.
interface Spec {
  path: string;
  value: number;
}
interface User {
  id: number;
  name: string;
}
class Transport {
  count: number = 0;
  async request<T>(spec: Spec, make: (n: number) => T): Promise<T> {
    this.count = this.count + 1;
    return make(spec.value);
  }
  wrap<T, U = T[]>(value: T, f: (v: T) => U): U {
    return f(value);
  }
  static pair<A, B>(a: A, b: B): { a: A; b: B } {
    return { a, b };
  }
  first<T>(items: T[]): T | undefined {
    return items[0];
  }
  loud<T extends Animal>(a: T): string {
    const said = a.speak('!');
    return `${said}${said.length}`;
  }
  count3<T>(...items: T[]): number {
    return items.length + this.first(items.length > 0 ? [1] : [0])!;
  }
}
class Resource {
  protected readonly client: Transport;
  constructor(client: Transport) {
    this.client = client;
  }
  user(id: number): Promise<User> {
    return this.client.request({ path: '/u', value: id }, (n: number): User => ({ id: n, name: `u${n}` }));
  }
  label(id: number): Promise<string> {
    return this.client.request<string>({ path: '/l', value: id }, (n: number): string => `#${n}`);
  }
  async both(id: number): Promise<string> {
    const user = await this.user(id);
    const label = await this.label(id + 1);
    return `${user.name}/${label}`;
  }
}
class Animal {
  speak<T>(x: T): string {
    return `animal ${String(x)}`;
  }
  describe<T>(x: T): string {
    return `[${this.speak(x)}]`;
  }
}
class Dog extends Animal {
  speak<T>(x: T): string {
    return `dog ${super.speak<T>(x)}`;
  }
}
class Puppy extends Dog {}
export async function main(): Promise<void> {
  const t = new Transport();
  const r = new Resource(t);
  const u = await r.user(7);
  const l = await r.label(3);
  console.log(`${u.id} ${u.name} ${l} ${t.count} ${await r.both(1)} ${t.count}`);
  console.log(JSON.stringify(t.wrap(2, (v: number): number[] => [v, v])));
  console.log(JSON.stringify(t.wrap<string, string>('a', (v: string): string => v + v)));
  console.log(JSON.stringify(Transport.pair('x', 1)));
  console.log(`${t.first([1, 2]) ?? 0} ${t.first<string>([]) ?? 'none'} ${t.loud(new Dog())}`);
  console.log(`${t.loud(new Animal())} ${t.count3('a', 'b')} ${t.count3<number>()}`);
  const animals: Animal[] = [new Animal(), new Dog(), new Puppy()];
  for (let i = 0; i < animals.length; i++) {
    const animal = animals[i]!;
    console.log(`${animal.speak(i)} | ${animal.speak('s')} | ${animal.describe(true)}`);
  }
}
