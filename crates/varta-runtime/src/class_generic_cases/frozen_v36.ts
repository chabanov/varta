// A v36 program over the class features the v37 class-generics stream
// touches (methods, overrides, statics, getters, async methods, function
// fields, interfaces and generic functions). Its v36 bytes are frozen.
interface Named {
  name: string;
  run: (x: number) => number;
}
class Shape {
  protected readonly label: string;
  private hits: number = 0;
  constructor(label: string) {
    this.label = label;
  }
  area(): number {
    return 0;
  }
  get title(): string {
    return `${this.label}:${this.area()}`;
  }
  touch(): number {
    this.hits = this.hits + 1;
    return this.hits;
  }
  static unit(): Shape {
    return new Shape('unit');
  }
  async later(n: number): Promise<number> {
    return n + this.area();
  }
}
class Square extends Shape {
  side: number;
  constructor(side: number) {
    super('square');
    this.side = side;
  }
  area(): number {
    return this.side * this.side;
  }
}
function first<T>(items: T[]): T | undefined {
  return items[0];
}
async function run(shapes: Shape[], named: Named): Promise<void> {
  for (let i = 0; i < shapes.length; i++) {
    const shape: Shape = shapes[i]!;
    const later = await shape.later(1);
    console.log(`${shape.title} ${shape.touch()} ${later}`);
  }
  console.log(`${named.name} ${named.run(3)} ${first([4, 5]) ?? 0} ${Shape.unit().title}`);
}
export async function main(): Promise<void> {
  await run([new Shape('dot'), new Square(3)], { name: 'n', run: (x: number): number => x * 2 });
}
