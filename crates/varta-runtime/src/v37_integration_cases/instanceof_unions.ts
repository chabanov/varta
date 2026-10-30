// `instanceof` over v37 unions of several classes (language core): every
// member that is the tested class or a subclass is tested by descriptor,
// including generic class instances and `#x in` brand checks.
class Base {
  base = 1;
}
class Left extends Base {
  left = 2;
}
class Right extends Base {
  right = 3;
}
class Other {
  other = 4;
}
class Box<T> {
  #value: T;
  constructor(value: T) {
    this.#value = value;
  }
  owns(value: Box<T> | Other): boolean {
    return #value in value;
  }
}
type Shape = Left | Right | Other;
function kind(shape: Shape): string {
  if (shape instanceof Base) {
    return shape instanceof Left ? 'left' : 'base';
  }
  return 'other';
}
export function main(): void {
  const shapes: Shape[] = [new Left(), new Right(), new Other()];
  console.log(shapes.map(kind).join(' '));
  const box = new Box<string>('x');
  console.log(`${box.owns(box)} ${box.owns(new Other())}`);
}
