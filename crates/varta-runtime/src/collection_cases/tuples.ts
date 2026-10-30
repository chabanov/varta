// Tuples: annotations, literals, positional reads, destructuring, optional
// elements, readonly tuples, Object.entries pairs and tuple-to-array use.
type Pair = [string, number];
interface Range {
  bounds: readonly [number, number];
  label?: [string, boolean?];
}

function divide(a: number, b: number): [number, number] {
  return [Math.floor(a / b), a % b];
}

function describe(pair: Pair): string {
  const [name, count] = pair;
  return `${name}:${count + 1}`;
}

export function main(): void {
  const [quotient, remainder] = divide(17, 5);
  console.log(`${quotient} ${remainder}`);
  const pair: Pair = ["apples", 3];
  console.log(describe(pair));
  console.log(`${pair[0].toUpperCase()} ${pair[1] * 2} ${pair.length}`);
  const range: Range = { bounds: [1, 9], label: ["edge"] };
  console.log(`${range.bounds[0]}..${range.bounds[1]}`);
  const label = range.label;
  if (label !== undefined) {
    const flag = label[1];
    console.log(`${label[0]} ${flag === undefined} ${label.length}`);
  }
  const scores: Record<string, number> = { ann: 3, bob: 5 };
  let total = 0;
  for (const [key, value] of Object.entries(scores)) {
    total += value;
    console.log(`${key.padEnd(4, ".")}${value}`);
  }
  console.log(`${total}`);
  const entries = Object.entries(scores);
  const first = entries[0];
  if (first !== undefined) console.log(`${first[0]}=${first[1]}`);
  const list: [string, number][] = [["x", 1], ["y", 2]];
  console.log(list.map((entry) => entry[0] + String(entry[1])).join(" "));
  const mixed: (string | number)[] = pair;
  console.log(`${mixed.length}`);
  for (const part of pair) console.log(String(part));
  console.log(JSON.stringify(pair));
  console.log(JSON.stringify({ bounds: range.bounds }));
  const swapped: [number, string] = [pair[1], pair[0]];
  console.log(`${swapped[0]}${swapped[1]}`);
}
