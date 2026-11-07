// Getter/setter pairs, setter-only accessors, compound assignment and
// updates through accessors, inheritance and auto-accessors.
class Temperature {
  private celsius = 0;
  get fahrenheit(): number { return this.celsius * 9 / 5 + 32; }
  set fahrenheit(value: number) { this.celsius = (value - 32) * 5 / 9; }
  set kelvin(value: number) { this.celsius = value - 273.15; }
  get value(): number { return this.celsius; }
}
class Named {
  private parts: string[] = [];
  protected log: string[] = [];
  get name(): string { return this.parts.join(' '); }
  set name(value: string) { this.log.push('set ' + value); this.parts = value.split(' '); }
  history(): string { return this.log.join('; '); }
}
class Person extends Named {
  greeting(): string { return 'hello ' + this.name; }
}
class Counter {
  accessor count = 1;
  accessor label: string = 'n';
  static accessor shared = 'x';
}
export function main(): void {
  const t = new Temperature();
  t.fahrenheit = 212;
  console.log(String(t.value) + ' ' + String(t.fahrenheit));
  t.kelvin = 273.15;
  console.log(String(t.fahrenheit));
  t.fahrenheit += 18;
  console.log(String(t.value));
  const before = t.fahrenheit++;
  console.log(String(before) + ' ' + String(t.fahrenheit));
  const after = --t.fahrenheit;
  console.log(String(after));
  const assigned = (t.fahrenheit = 50);
  console.log(String(assigned) + ' ' + String(t.value));
  const p = new Person();
  p.name = 'Ada Lovelace';
  console.log(p.greeting());
  p.name += ' Byron';
  console.log(p.name + ' | ' + p.history());
  const c = new Counter();
  c.count += 2;
  c.count++;
  c.label = c.label + String(c.count);
  Counter.shared = Counter.shared + 'y';
  console.log(String(c.count) + ' ' + c.label + ' ' + Counter.shared);
}
