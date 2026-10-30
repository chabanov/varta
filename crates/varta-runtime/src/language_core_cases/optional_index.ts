// Index reads whose context admits `undefined` read like JavaScript: a
// return or parameter of type `T | undefined`, an annotated binding, the
// left operand of `??`, an inferred binding or loop variable the code
// compares with `undefined`, and elements of `unknown[]` see `undefined` for
// holes and missing indices.
function first<T>(items: T[]): T | undefined {
  return items[0];
}
function show(value: string | undefined): string {
  return value === undefined ? "none" : value;
}
export function main(): void {
  const empty: number[] = [];
  const holes: number[] = [1, , 3];
  const words: string[] = ["a", "b"];
  console.log(String(first(empty)) + " " + String(first(words)));
  const maybe: number | undefined = holes[1];
  console.log(String(maybe) + " " + String(holes[7] ?? -1) + " " + String(holes[0] ?? -1));
  console.log(show(words[5]) + " " + show(words[1]) + " " + ("xy"[4] ?? "z"));
  const grid: number[][] = [[1], [2, 3]];
  console.log(String(grid[1][1] ?? 0) + " " + String(grid[1][4] ?? 0));
  const item = words[3];
  const label = item === undefined ? "missing" : item;
  let kept = "";
  for (const value of holes) {
    if (value !== undefined) kept += String(value);
  }
  const mixed: unknown[] = ["a", undefined, 1];
  let kinds = "";
  for (const value of mixed) kinds += typeof value + ";";
  console.log(label + " " + kept + " " + kinds + " " + String(typeof mixed[5]));
}
