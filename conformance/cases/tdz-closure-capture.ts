// A closure may capture a let/const binding before its declaration runs; a
// read through it before the initialization throws ReferenceError, and after
// the initialization it reads the value. Nested function declarations are
// hoisted to the start of their block.
function probe(): string {
  const read = (): number => value;
  let first = "";
  try {
    read();
  } catch (e) {
    if (e instanceof Error) first = e.name;
  }
  const value = 7;
  return first + " " + String(read()) + " " + String(twice(value));
  function twice(n: number): number {
    return n * 2;
  }
}
export function main(): void {
  console.log(probe());
}
