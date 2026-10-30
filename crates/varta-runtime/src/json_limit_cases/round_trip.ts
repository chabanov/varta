// V38 `data_limits`: number arrays of 10k and 100k elements round-trip
// through checked JSON parsing and serialization.
function source(count: number): string {
  return "[" + "1.5,".repeat(count - 1) + "-2e-7]";
}

export function main(): void {
  for (const count of [10000, 100000]) {
    const text = source(count);
    const values = varta.json.parse<number[]>(text);
    const out = JSON.stringify(values);
    console.log(`${count} ${values.length} ${out.length} ${out === text}`);
  }
}
