// `for...of` over `T[]` and `xs[i]` have the element type T, as TypeScript
// types them without noUncheckedIndexedAccess.
function sum(xs: number[]): number {
  let total = 0;
  for (const x of xs) total += x;
  for (let i = 0; i < xs.length; i++) total += xs[i] * 10;
  return total;
}
export function main(): void {
  const words: string[] = ["ab", "c"];
  const first: string = words[0];
  console.log(String(sum([1, 2])) + " " + first + String(words[1].length));
}
