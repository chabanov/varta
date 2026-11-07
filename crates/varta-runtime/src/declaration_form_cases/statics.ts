// Static fields, readonly and private statics, static inheritance and
// `this` in static methods.
class Registry {
  static count = 0;
  static readonly label: string = 'registry';
  private static secret = 42;
  protected static names: string[] = [];
  static current: Registry | null = null;
  id: number;
  constructor(public name: string) {
    Registry.count++;
    this.id = Registry.count;
    Registry.names.push(name);
  }
  static reset(): void { this.count = 0; Registry.names = []; }
  static reveal(): number { return Registry.secret + this.count; }
  static all(): string { return this.names.join(','); }
  static make(name: string): Registry {
    const made = new Registry(name);
    Registry.current = made;
    return made;
  }
}
class Special extends Registry {
  static extra = Registry.count + 100;
  static describe(): string { return 'special of ' + String(Special.count) + ' ' + Special.names.join('+'); }
}
class Base {
  static create(): string { return 'made by ' + this.kind() + ' ' + this.tag; }
  static kind(): string { return 'base'; }
  static tag = 'B';
}
class Derived extends Base {
  static kind(): string { return 'derived'; }
}
class Leaf extends Derived {}
export function main(): void {
  new Registry('a');
  Registry.make('b');
  console.log(String(Registry.count) + ' ' + Registry.label + ' ' + String(Registry.reveal()));
  console.log(Registry.all() + ' ' + (Registry.current === null ? 'none' : Registry.current.name));
  console.log(String(Special.extra) + ' ' + Special.describe());
  const s = new Special('c');
  console.log(String(s.id) + ' ' + String(Special.count) + ' ' + Registry.all());
  Registry.reset();
  console.log(String(Registry.count) + ' ' + String(new Special('d').id));
  console.log(Base.create() + ' / ' + Derived.create() + ' / ' + Leaf.create());
}
