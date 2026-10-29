// Index signatures and Record<string, T>: own-key order puts canonical array
// indices first; absent keys read as undefined; delete and `in` see own keys.
type Counts = Record<string, number>;
interface Labels {
  [key: string]: string;
}
export function main(): void {
  const counts: Counts = {};
  for (const word of ["b", "a", "b", "2", "1"]) {
    if (word !== undefined) {
      counts[word] = (counts[word] ?? 0) + 1;
    }
  }
  console.log(JSON.stringify(counts));
  console.log(JSON.stringify(Object.keys(counts)));
  delete counts.a;
  console.log(String("a" in counts) + " " + String(counts["b"]) + " " + String(counts["z"]));
  const labels: Labels = { x: "ex" };
  labels.y = "why";
  console.log(JSON.stringify(Object.entries(labels)));
}
