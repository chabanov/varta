// `super` in static methods: a parent's static method runs with the calling
// subclass as `this`, through any depth of `super` calls.
class Base {
  static label = 'base';
  static describe(): string { return 'I am ' + this.label + ' (' + this.kind() + ')'; }
  static kind(): string { return 'B'; }
  static plain(): string { return 'plain'; }
}
class Middle extends Base {
  static label = 'middle';
  static describe(): string { return 'middle says ' + super.describe() + ' ' + super.plain(); }
  static kind(): string { return 'M'; }
}
class Leaf extends Middle {
  static describe(): string { return 'leaf: ' + super.describe() + ' ' + super.label; }
  static kind(): string { return 'L'; }
}
export function main(): void {
  console.log(Base.describe());
  console.log(Middle.describe());
  console.log(Leaf.describe());
}
