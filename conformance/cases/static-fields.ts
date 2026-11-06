class Ids {
  static next = 1;
  static readonly prefix: string = 'id-';
  static take(): string { const id = this.prefix + String(this.next); Ids.next++; return id; }
}
class Shape {
  static describe(): string { return 'shape ' + this.kind(); }
  static kind(): string { return 'generic'; }
}
class Circle extends Shape {
  static kind(): string { return 'circle'; }
}
export function main(): void {
  console.log(Ids.take() + ' ' + Ids.take() + ' ' + String(Ids.next));
  console.log(Shape.describe() + ' / ' + Circle.describe());
}
