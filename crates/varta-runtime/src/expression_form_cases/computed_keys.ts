const K = "alpha";
const N = 3;
export function main(): void {
  const k = "a";
  const o = { [k]: 1, [K]: 2, [N]: 4, ["lit"]: 5, [`tpl`]: 6, [7]: 8 };
  console.log(String(o[k] + o[K] + o[N] + o.lit + o.tpl + o[7]));
  console.log(String(o.alpha));
  console.log(Object.keys(o).join(","));
  console.log(JSON.stringify(o));
}
