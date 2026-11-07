function tag(s: TemplateStringsArray, ...v: number[]): string {
  return s.join("|") + ":" + v.join(",") + ":" + String(s.length);
}
function upper(strings: TemplateStringsArray, ...values: string[]): string {
  let out = "";
  strings.forEach((part, i) => { out += part + (i < values.length ? values[i].toUpperCase() : ""); });
  return out;
}
function count(strings: TemplateStringsArray): number { return strings.length; }
let calls = 0;
function next(): number { calls++; return calls; }
export function main(): void {
  console.log(tag`a${1}b${2}c`);
  console.log(tag``);
  console.log(tag`${next()}${next()}`);
  console.log(upper`hello ${"world"} and ${"you"}!`);
  console.log(String(count`x-y-z`));
  console.log(String.raw`x\ny${3}z\u0041`);
  console.log(String.raw`C:\path\${"file"}.txt`);
  console.log(String.raw``.length.toString());
}
