interface Box { v: number }
function get(this: Box, k: number): number { return this.v * k; }
class C {
  v = 2;
  f = (): number => this.v;
  g = (k: number) => this.v * k;
  h = (prefix: string, times: number = 2) => prefix + String(this.v * times);
  label = (): string => `C${this.v}`;
}
class D extends C {
  w = 5;
  sum = (): number => this.v + this.w;
  twice = () => this.w * 2;
}
export function main(): void {
  const b: Box = { v: 3 };
  console.log(String(get.call(b, 2)));
  const o = { v: 5, get };
  console.log(String(o.get(2)));
  const c = new C();
  const f = c.f;
  console.log(String(f()) + " " + String(c.g(3)) + " " + c.h("x") + " " + c.h("y", 4) + " " + c.label());
  c.v = 10;
  console.log(String(f()));
  const d = new D();
  console.log(String(d.sum()) + " " + String(d.f()) + " " + String(d.twice()));
}
