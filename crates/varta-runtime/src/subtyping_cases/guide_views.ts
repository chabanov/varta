interface Readable<T> {
  readonly value: T;
  label: string;
}
interface Signal<T> extends Readable<T> {
  set(v: T): void;
}

function show(r: Readable<number>): string {
  return `${r.label}=${r.value}`;
}

class Cell {
  label = 'cell';
  #v = 1;
  get value(): number {
    return this.#v;
  }
  set(v: number): void {
    this.#v = v;
  }
}

export function main(): void {
  const wide = { label: 'w', value: 2, extra: true };
  const cell = new Cell();
  const signal: Signal<number> = cell; // a class instance as an interface
  signal.set(5);
  const viewed: Readable<number> = wide;
  viewed.label = 'x'; // the same object
  console.log(`${show(wide)} ${show(cell)} ${viewed === wide} ${JSON.stringify(viewed)}`);
  // x=2 cell=5 true {"label":"x","value":2,"extra":true}
}
