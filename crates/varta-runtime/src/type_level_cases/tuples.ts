// Named, optional and rest tuple elements and variadic tuple types.
type Entry = [name: string, age?: number];
type Numbers = [first: number, ...rest: number[]];
type Concat<A extends unknown[], B extends unknown[]> = [...A, ...B];
type Prefixed<T extends unknown[]> = [string, ...T];
type Empty = [];

function total(values: Numbers): number {
  let sum = 0;
  for (const value of values) {
    sum += value;
  }
  return sum;
}
function describe(entry: Entry): string {
  const [name, age] = entry;
  return age === undefined ? name : `${name}(${age})`;
}

export function main(): void {
  const short: Entry = ["Ada"];
  const full: Entry = ["Bob", 42];
  console.log(`${describe(short)} ${describe(full)} ${short.length} ${full.length}`);
  const numbers: Numbers = [1, 2, 3, 4];
  const one: Numbers = [5];
  console.log(`${total(numbers)} ${total(one)} ${numbers[0]} ${numbers[3]}`);
  const joined: Concat<[number, boolean], [string]> = [1, true, "z"];
  console.log(`${joined[0]} ${joined[1]} ${joined[2]} ${joined.length}`);
  const prefixed: Prefixed<[number]> = ["p", 2];
  const empty: Empty = [];
  console.log(`${prefixed.join(":")} ${empty.length}`);
}
