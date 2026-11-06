// Math.max / Math.min with spread arguments (ECMA-262 §21.3.2.24–25).
export function main(): void {
  const values = [3, 9, -2, 7];
  console.log(`${Math.max(...values)} ${Math.min(...values)}`);
  console.log(`${Math.max(...values, 12)} ${Math.min(0, ...values, ...[-5, 1])}`);
  const empty: number[] = [];
  console.log(`${Math.max(...empty)} ${Math.min(...empty)}`);
  console.log(`${Math.max(...[1, NaN, 3])} ${Math.min(...[NaN])}`);
  console.log(`${Object.is(Math.max(...[-0, 0]), 0)} ${Object.is(Math.min(...[0, -0]), -0)} ${Object.is(Math.max(...[-0]), -0)}`);
}
