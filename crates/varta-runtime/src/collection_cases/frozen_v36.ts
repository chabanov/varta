// A v36 program over the paths the v37 collections stream touches: member
// calls named like the v37 built-ins on a class, Object.entries pairs and
// array destructuring, for-of over arrays, unions of arrays, module constants
// and string methods. Its v36 bytes are frozen.
const LIMITS = [3, 5];
class Bag {
  items: string[];
  constructor() {
    this.items = [];
  }
  add(item: string): Bag {
    this.items.push(item);
    return this;
  }
  has(item: string): boolean {
    return this.items.includes(item);
  }
  join(separator: string): string {
    let text = "";
    for (let index = 0; index < this.items.length; index++) {
      text += (index > 0 ? separator : "") + this.items[index]!;
    }
    return text;
  }
  split(): number {
    return this.items.length;
  }
}
export function main(): void {
  const bag = new Bag();
  bag.add("a").add("b");
  console.log(`${bag.join("+")} ${bag.has("a")} ${bag.split()}`);
  const scores: Record<string, number> = { x: 1, y: 2 };
  for (const [key, value] of Object.entries(scores)) {
    console.log(`${String(key)}=${String(value)}`);
  }
  const pair = [1, 2];
  const [first, second] = pair;
  console.log(`${first} ${second} ${LIMITS.length}`);
  const values: (number[] | undefined)[] = [[1], undefined];
  for (const value of values) {
    console.log(`${value === undefined}`);
  }
  console.log("  text ".trim().slice(1).indexOf("x"));
}
