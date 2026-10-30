// Guards on a property path narrow later reads of the same path until a
// call or property write may change it: `!== undefined`, `typeof`, truthiness
// and an optional chain.
interface Inner {
  text?: string;
}
interface Outer {
  count?: number;
  inner?: Inner;
}
function describe(o: Outer): string {
  let out = "";
  if (o.count !== undefined) out += String(o.count + 1);
  if (o.inner !== undefined && typeof o.inner.text === "string") out += o.inner.text;
  if (o.inner?.text) out += String(o.inner.text.length);
  return out;
}
export function main(): void {
  console.log(describe({ count: 1, inner: { text: "abc" } }) + "|" + describe({}));
}
