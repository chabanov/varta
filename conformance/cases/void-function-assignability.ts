// A function returning a value is assignable to a function type with the
// same parameters and a void result; the result is ignored.
function each(xs: number[], visit: (x: number) => void): void {
  for (const x of xs) visit(x);
}
export function main(): void {
  const seen: number[] = [];
  const record = (x: number): number => seen.push(x * 2);
  each([1, 2, 3], record);
  const callback: (x: number) => void = record;
  callback(5);
  console.log(String(seen.length) + " " + String(seen[3]));
}
