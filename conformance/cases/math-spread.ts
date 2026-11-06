export function main(): void {
  const values = [4, -2, 9];
  console.log(`${Math.max(...values)} ${Math.min(...values, -5)}`);
  const none: number[] = [];
  console.log(`${Math.max(...none)} ${Math.min(...none)}`);
}
