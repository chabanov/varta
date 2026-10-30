export function main(): void {
  const counts = new Map<number, number>();
  const xs: number[] = [];
  xs.length = 5000;
  for (let i = 0; i < xs.length; i++) {
    xs[i] = i;
    counts.set(i, i * 2);
  }
  console.log(`${xs.length} ${counts.size} ${xs.slice(4998).join(',')}`);
}
