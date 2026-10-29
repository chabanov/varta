class Shape {
  readonly name: string;
  protected sides: number;
  constructor(name: string, sides: number) {
    this.name = name;
    this.sides = sides;
  }
  area(): number {
    return 0;
  }
  get label(): string {
    return this.name + "/" + String(this.sides);
  }
  describe(): string {
    return this.label + " area " + String(this.area());
  }
  static unit(): Square {
    return new Square(1);
  }
}

class Rect extends Shape {
  width: number;
  height: number;
  constructor(width: number, height: number) {
    super("rect", 4);
    this.width = width;
    this.height = height;
  }
  area(): number {
    return this.width * this.height;
  }
}

class Square extends Rect {
  constructor(size: number) {
    super(size, size);
  }
  describe(): string {
    return "square " + super.describe();
  }
}

export function main(): void {
  const rect = new Rect(2, 3);
  const unit = Shape.unit();
  const shapes: Shape[] = [new Shape("dot", 0), rect, unit];
  for (const shape of shapes) {
    if (shape !== undefined) {
      console.log(shape.describe());
    }
  }
  const widened: Shape = rect;
  console.log(unit instanceof Rect);
  console.log(widened instanceof Square);
  console.log(JSON.stringify(new Rect(1, 2)));
}
