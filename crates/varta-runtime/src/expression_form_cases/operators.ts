class Base {
  kind: string;
  constructor() {
    this.kind = new.target === Base ? "direct" : "derived";
    if (new.target !== Derived) { console.log("not Derived"); }
  }
}
class Derived extends Base {}
class Other {}
function plain(): string { return new.target === undefined ? "call" : "new"; }
interface Opt { a?: number; b: string; f?: (x: number) => number; g: (x: number) => number }
interface Box { v?: number; s: string; c: number }
export function main(): void {
  console.log(new Base().kind + " " + new Derived().kind + " " + plain());
  let i = 0;
  const x = (i++, i++, i);
  console.log(String(x) + " " + String(i));
  for (let a = 0, b = 10; a < b; a += 3, b -= 3) { console.log(`${a} ${b}`); }
  let n = 1;
  const u = void (n = 2);
  console.log(String(u) + " " + String(n) + " " + String(void 0));
  const o: Opt = { a: 1, b: "s", g: (k: number): number => k + 1 };
  console.log(String(delete o.a) + " " + String("a" in o) + " " + String("b" in o));
  console.log(String(o.f?.(2)) + " " + String(o.g?.(2)));
  const arr: number[][] | undefined = [[1, 2]];
  console.log(String(arr?.[0]?.[1]));
  const none: Opt | undefined = undefined;
  console.log(String(none?.g(1)));
  let m: number | undefined = undefined;
  m ??= 5;
  let z = 0;
  z ||= 7;
  let y = 1;
  y &&= 9;
  let p = 2;
  p **= 10;
  console.log(`${m} ${z} ${y} ${p}`);
  const box: Box = { s: "", c: 3 };
  box.v ??= 4; box.s ||= "x"; box.c &&= 6; box.c **= 2;
  console.log(`${box.v} ${box.s} ${box.c}`);
  const e: Error = new TypeError("t");
  const d: Base = new Derived();
  const unknownValue: unknown = new Derived();
  console.log(String(e instanceof TypeError) + " " + String(e instanceof RangeError) + " " +
    String(d instanceof Derived) + " " + String(unknownValue instanceof Base) + " " + String(unknownValue instanceof Other));
  let count = 0;
  outer: for (let r = 0; r < 3; r++) {
    for (let c = 0; c < 3; c++) {
      if (c === 1) continue outer;
      if (r === 2) break outer;
      count++;
    }
  }
  block: {
    count += 10;
    if (count > 0) break block;
    count = -1;
  }
  console.log(String(count));
  for (const v of [1, 2, 3, 4]) {
    switch (v) {
      case 1:
      case 2:
        console.log("small " + String(v));
      case 3:
        console.log("fall " + String(v));
        break;
      default:
        console.log("big " + String(v));
    }
  }
}
