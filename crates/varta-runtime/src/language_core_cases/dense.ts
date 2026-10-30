// Dense element typing: for...of and xs[i] have the element type.
interface Item { id: string; n: number }
function total(xs: number[]): number {
  let sum = 0;
  for (const x of xs) sum += x;
  for (let i = 0; i < xs.length; i++) sum += xs[i];
  return sum;
}
function ids(items: Item[]): string {
  let out = "";
  for (const item of items) out += item.id + ":" + String(item.n) + ";";
  const first: Item = items[0];
  return out + " first=" + first.id;
}
function chars(s: string): string {
  let out = "";
  for (let i = 0; i < s.length; i++) {
    const c: string = s[i];
    if (c !== "-") out += c;
  }
  return out;
}
function maybe(xs: (number | undefined)[]): string {
  let out = "";
  for (const x of xs) out += String(x) + ",";
  return out + String(xs[5]);
}
function nested(rows: number[][]): number {
  let n = 0;
  for (const row of rows) for (const cell of row) n += cell;
  return n + rows[1][0];
}
function guarded(xs: string[], i: number): string {
  try {
    return xs[i];
  } catch (e) {
    return e instanceof TypeError ? "TypeError" : "other";
  }
}
export function main(): void {
  console.log(String(total([1, 2, 3])));
  console.log(ids([{ id: "a", n: 1 }, { id: "b", n: 2 }]));
  console.log(chars("a-b-c"));
  console.log(maybe([1, undefined, 3]));
  console.log(String(nested([[1, 2], [3]])));
  console.log(guarded(["x"], 0));
  for (const word of ["lit", "eral"]) console.log(word + "!");
}
