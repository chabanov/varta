const calls: string[] = [];
interface Point {
  readonly x: number;
  y: number;
}
class Holder {
  #secret = 1;
  name: string;
  point: Point;
  constructor(name: string, point: Point) {
    this.name = name;
    this.point = point;
  }
  get doubled(): number {
    return this.#secret * 2;
  }
}
class Pair extends Holder {
  other: Point | null = null;
}
function makePoint(label: string): Point {
  return {
    get x(): number {
      calls.push(label);
      return 1;
    },
    y: 2,
  };
}
export function main(): void {
  const holder = new Holder("h", makePoint("a"));
  console.log(JSON.stringify(holder));
  console.log(calls.join(","));
  const pair = new Pair("p", makePoint("b"));
  pair.other = pair.point;
  console.log(JSON.stringify([pair, holder]));
  console.log(calls.join(","));
}
