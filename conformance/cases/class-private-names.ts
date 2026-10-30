// ES private fields and methods with a brand check (admitted by v37
// `class_generics`).
class Counter {
  #count = 0;
  count = 10;
  #bump(): number {
    this.#count += 1;
    return this.#count;
  }
  next(): number {
    return this.#bump() + this.count;
  }
  static has(value: Counter | number[]): boolean {
    return #count in value;
  }
}
export function main(): void {
  const counter = new Counter();
  counter.next();
  console.log(`${counter.next()} ${Counter.has(counter)} ${Counter.has([])}`);
}
