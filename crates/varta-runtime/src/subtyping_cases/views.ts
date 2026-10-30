interface Readable<T> {
  readonly value: T;
  label: string;
}
interface Signal<T> extends Readable<T> {
  set(v: T): void;
  count: number;
}
interface Named {
  label: string;
  note?: string;
}
interface Holder {
  inner: Readable<number>;
  tag: string;
}

function show(r: Readable<number>): string {
  return r.label + "=" + r.value;
}
function rename(n: Named, label: string): void {
  n.label = label;
}
function noted(n: Named): string {
  return n.note === undefined ? n.label + ":none" : n.label + ":" + n.note;
}
function maybe(r: Readable<number> | undefined): number {
  return r === undefined ? -1 : r.value;
}

class Counter {
  label: string;
  #n = 0;
  constructor(label: string) {
    this.label = label;
  }
  get value(): number {
    return this.#n;
  }
  set(v: number): void {
    this.#n = v;
  }
  get count(): number {
    return this.#n * 2;
  }
}
class Loud extends Counter {
  get value(): number {
    return this.count * 5;
  }
}

function bump(s: Signal<number>, v: number): number {
  s.set(v);
  return s.count;
}

export function main(): void {
  const s: Signal<number> = {
    value: 3,
    label: "a",
    count: 1,
    set(v: number): void {
      console.log("set " + v);
    },
  };
  console.log(show(s));
  const r: Readable<number> = s;
  console.log(String(r.label) + " " + String(r.value));
  rename(s, "b");
  console.log(String(s.label) + " " + String(show(s)));
  const plain = { label: "p", extra: true };
  console.log(String(noted(plain)) + " " + String(noted({ label: "q", note: "n" })));
  rename(plain, "p2");
  console.log(String(plain.label) + " " + String(JSON.stringify(plain)));
  const viewed: Named = plain;
  const hasLabel = "label" in viewed;
  const hasNote = "note" in viewed;
  console.log(JSON.stringify(viewed) + " " + hasLabel + " " + hasNote);
  console.log(String(maybe(s)) + " " + String(maybe(undefined)));
  const holder: Holder = { inner: s, tag: "h" };
  console.log(String(holder.inner.label) + " " + String(show(holder.inner)));
  const c = new Counter("c");
  c.set(4);
  console.log(String(show(c)) + " " + String(bump(c, 5)) + " " + String(show(c)));
  const l = new Loud("l");
  l.set(2);
  console.log(String(show(l)) + " " + String(bump(l, 3)));
  const list: Readable<number>[] = [s, c, l, { value: 9, label: "lit" }];
  let total = "";
  for (const item of list) total += show(item) + ";";
  console.log(total);
  const sig: Signal<number> = c;
  sig.set(7);
  console.log(String(c.value) + " " + String(sig.count) + " " + String(sig.label));
  console.log(`${viewed === plain}`);
}
