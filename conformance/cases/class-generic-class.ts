// A generic class instantiated from a constructor argument and a generic
// base (admitted by v37 `class_generics`).
class Page<T> {
  items: T[];
  constructor(items: T[]) {
    this.items = items;
  }
  first(): T | undefined {
    return this.items[0];
  }
}
class Labeled<T> extends Page<T> {
  label: string = 'page';
}
export function main(): void {
  const page = new Page(['a', 'b']);
  const labeled = new Labeled<number>([3]);
  console.log(`${page.first() ?? ''} ${labeled.first() ?? 0} ${labeled.label}`);
}
