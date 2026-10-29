// Dynamic-key records: index signatures and Record<string, T>.
interface Labels {
  [key: string]: string;
}
type Counts = Record<string, number>;
function tally(words: string[]): Counts {
  const counts: Counts = {};
  for (const word of words) {
    if (word !== undefined) {
      counts[word] = (counts[word] ?? 0) + 1;
    }
  }
  return counts;
}
function describe(labels: Labels, key: string): string {
  const value = labels[key];
  return value === undefined ? key + "=?" : key + "=" + value;
}
export function main(): void {
  const labels: Labels = { b: "bee", a: "ay", "2": "two", "10": "ten" };
  labels["1"] = "one";
  labels.c = "see";
  labels.a = "ay!";
  console.log(JSON.stringify(labels));
  console.log(JSON.stringify(Object.keys(labels)));
  console.log(describe(labels, "a") + " " + describe(labels, "z"));
  console.log(String("c" in labels) + " " + String("z" in labels) + " " + String("toString" in labels));
  console.log(String(Object.hasOwn(labels, "b")) + " " + String(Object.hasOwn(labels, "toString")));
  console.log(String(delete labels.b) + " " + String(delete labels["missing"]));
  console.log(JSON.stringify(Object.entries(labels)));
  console.log(JSON.stringify(Object.values(labels)));
  const counts = tally(["x", "y", "x", "0", "x", "7", "y"]);
  console.log(JSON.stringify(counts));
  let total = 0;
  for (const key of Object.keys(counts)) {
    if (key === undefined) {
      continue;
    }
    const n = counts[key];
    if (n !== undefined) {
      total = total + n;
    }
  }
  console.log(String(total) + " " + typeof counts);
  const nested: Record<string, number[]> = {};
  nested["evens"] = [2, 4];
  nested["odds"] = [1];
  const odds = nested["odds"];
  if (odds !== undefined) {
    odds.push(3);
  }
  console.log(JSON.stringify(nested));
  const same = labels;
  same["late"] = "!";
  console.log(String(same === labels) + " " + String(Object.keys(labels).length));
}
