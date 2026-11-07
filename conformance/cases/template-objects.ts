function show(strings: TemplateStringsArray, ...values: number[]): string {
  return JSON.stringify(strings.raw) + " " + JSON.stringify(strings) + " " + String(values.length);
}
const seen: TemplateStringsArray[] = [];
function keep(strings: TemplateStringsArray, ...values: number[]): number {
  seen.push(strings);
  return values.length;
}
function site(n: number): number {
  return keep`a${n}b`;
}
function rawOnly(strings: TemplateStringsArray, ...values: number[]): string {
  let out = "";
  for (const part of strings.raw) {
    out += "[" + part + "]";
  }
  return out + String(strings.raw.length) + String(values.length);
}
function joined(strings: TemplateStringsArray, ...values: number[]): string {
  let out = strings.raw[0];
  for (let i = 0; i < values.length; i++) {
    out += String(values[i]) + strings.raw[i + 1];
  }
  return out;
}
function nested(n: number): string {
  return show`[${n}]`;
}
export function main(): void {
  console.log(show`a\nb${1}c\u0041\\d`);
  console.log(show`line1
line2${2}\t`);
  console.log(rawOnly`x\`y${0}\${z}`);
  for (let i = 0; i < 3; i++) {
    site(i);
  }
  console.log(String(seen[0] === seen[1]) + " " + String(seen[1] === seen[2]));
  keep`a${1}b`;
  console.log(String(seen[2] === seen[3]) + " " + String(seen.length));
  console.log(String(seen[0].raw === seen[1].raw) + " " + String(seen[0].raw === seen[3].raw));
  console.log(seen[0].raw[0] + " " + seen[3].raw[1] + " " + String(seen[3].length));
  const copy: readonly string[] = seen[0];
  console.log(copy.join("+") + " " + String(copy === seen[0]));
  console.log(seen[0].slice(1).join("-") + " " + seen[0].map((s) => s.toUpperCase()).join(""));
  console.log(joined`p${1}q${2}r\n`);
  console.log(nested(1) + " " + nested(2));
  const tags = [show, show];
  console.log(tags[0]`x${9}`);
}
