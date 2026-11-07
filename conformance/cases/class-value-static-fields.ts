// A static field read through a class value is the field the class
// declares or inherits.
class Base {
  static count: number = 1;
  static label = 'base';
}
class Derived extends Base {
  static count: number = 2;
}
function describe(kind: typeof Base): string {
  return kind.name + ' ' + String(kind.count) + ' ' + kind.label;
}
export function main(): void {
  Base.label = 'shared';
  console.log(describe(Base) + ' | ' + describe(Derived));
}
