// Module evaluation with effects: initializers call functions and construct
// objects; static fields and static blocks run where their class is defined.
const events: string[] = [];
function note(text: string): number { events.push(text); return events.length; }
const first = note('first');
class Registry {
  static entries: string[] = [];
  static readonly created = note('Registry fields');
  static instance = new Registry('root');
  static {
    Registry.entries.push('block');
    this.entries.push('this block');
    note('Registry block');
  }
  constructor(public name: string) { Registry.entries.push('new ' + name); }
  static count(): number { return Registry.entries.length; }
}
const second = note('second');
const total = Registry.count() * 10;
class Config {
  static values = new Map<string, number>([['a', 1]]);
  static { Config.values.set('b', first + second); }
}
export function main(): void {
  console.log(events.join(', '));
  console.log(Registry.entries.join('|') + ' ' + String(total) + ' ' + Registry.instance.name);
  console.log(String(Config.values.get('b')) + ' ' + String(Registry.created));
}
