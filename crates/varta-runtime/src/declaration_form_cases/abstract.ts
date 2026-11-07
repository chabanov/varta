// Abstract classes, methods, getters and fields; abstract classes as types.
abstract class Shape {
  abstract readonly kind: string;
  constructor(protected scale: number) {}
  abstract area(): number;
  abstract get sides(): number;
  describe(): string {
    return this.kind + ' with ' + String(this.sides) + ' sides and area ' + String(this.area() * this.scale);
  }
}
class Square extends Shape {
  readonly kind: string = 'square';
  constructor(private side: number) { super(1); }
  area(): number { return this.side * this.side; }
  get sides(): number { return 4; }
}
abstract class Polygon extends Shape {
  get sides(): number { return this.count; }
  constructor(protected count: number) { super(2); }
}
class Triangle extends Polygon {
  readonly kind: string = 'triangle';
  constructor(private base: number, private height: number) { super(3); }
  area(): number { return this.base * this.height / 2; }
  override describe(): string { return 'polygon: ' + super.describe(); }
}
function total(shapes: Shape[]): number {
  let sum = 0;
  for (const shape of shapes) sum += shape.area();
  return sum;
}
export function main(): void {
  const shapes: Shape[] = [new Square(3), new Triangle(4, 5)];
  for (const shape of shapes) console.log(shape.describe());
  console.log(String(total(shapes)));
  const s: Shape = shapes[1];
  console.log(String(s instanceof Polygon) + ' ' + String(s instanceof Square));
}
