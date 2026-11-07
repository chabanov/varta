// Class expressions bound by const declarations, at module level and in
// functions, named and anonymous.
const Point = class {
  constructor(public x: number, public y: number) {}
  sum(): number { return this.x + this.y; }
};
const Labeled = class Label {
  constructor(public text: string) {}
  copy(): Label { return new Label(this.text + '*'); }
};
export function main(): void {
  const Pair = class PairImpl {
    a = 1;
    twice(): number { return this.a * 2; }
    clone(): PairImpl { const c = new PairImpl(); c.a = this.a + 1; return c; }
  };
  const Anonymous = class { value = 'anon'; };
  const p = new Pair();
  console.log(String(new Point(1, 2).sum()) + ' ' + String(p.twice()) + ' ' + String(p.clone().twice()));
  console.log(new Labeled('x').copy().copy().text + ' ' + new Anonymous().value);
  const points = [new Point(0, 1), new Point(2, 3)];
  console.log(String(points[1].sum() + points.length));
}
