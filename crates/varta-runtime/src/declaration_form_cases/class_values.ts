// Classes are values: a class object of a declared class or of one of
// its subclasses, constructed, called and named through dispatch.
abstract class Shape {
  constructor(public size: number) {}
  abstract area(): number;
  static describe(): string { return 'shape'; }
  static unit(): number { return 1; }
}
class Square extends Shape {
  area(): number { return this.size * this.size; }
  static describe(): string { return 'square'; }
}
class Circle extends Shape {
  area(): number { return Math.round(3 * this.size * this.size); }
  static unit(): number { return 2; }
}
function build(kind: typeof Shape, size: number): string {
  return kind.name + ' ' + kind.describe() + ' ' + String(kind.unit());
}
function make(kind: typeof Square, size: number): Square {
  return new kind(size);
}
function pick(round: boolean): typeof Shape {
  return round ? Circle : Square;
}
export function main(): void {
  console.log(build(Square, 2) + ' | ' + build(Circle, 3));
  const chosen = pick(true);
  console.log(chosen.name + ' ' + String(chosen === Circle) + ' ' + String(chosen !== Square));
  console.log(String(make(Square, 4).area()) + ' ' + typeof chosen);
  const ctor: typeof Circle = Circle;
  console.log(String(new ctor(2).area()) + ' ' + ctor.describe());
}
