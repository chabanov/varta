const Vector = class {
  constructor(public x: number, public y: number) {}
  plus(other: { x: number; y: number }): string { return String(this.x + other.x) + ',' + String(this.y + other.y); }
};
export function main(): void {
  const Pair = class PairOf {
    constructor(public left: string, public right: string) {}
    swap(): PairOf { return new PairOf(this.right, this.left); }
  };
  const pair = new Pair('a', 'b').swap();
  console.log(new Vector(1, 2).plus(new Vector(3, 4)) + ' ' + pair.left + pair.right);
}
