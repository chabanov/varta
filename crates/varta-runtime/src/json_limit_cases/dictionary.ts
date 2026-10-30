// V38 `data_limits`: a 10k-key dictionary (past the 1024-entry ceiling of
// earlier profiles) round-trips through checked JSON, keeping its own-key
// order: array-index keys ascending first, then the others in input order.
// Keys are built by concatenation only, so no host call runs per key.
const LETTERS = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];

function chunk(prefix: string): string {
  let text = "";
  for (const first of LETTERS) {
    for (const second of LETTERS) {
      text += ',"' + prefix + first + second + '":1';
    }
  }
  return text;
}

export function main(): void {
  let entries = "";
  for (const first of LETTERS) {
    for (const second of LETTERS) {
      entries += chunk(first + second);
    }
  }
  const text = '{"7":7,' + entries.slice(1) + ',"3":3}';
  const scores = varta.json.parse<Record<string, number>>(text);
  const out = JSON.stringify(scores);
  console.log(`${text.length} ${out.length} ${out.slice(0, 20)} ${out.slice(out.length - 20)}`);
}
