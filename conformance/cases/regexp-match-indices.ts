export function main(): void {
  const word = /(?<word>\w+) (\d+)?/d;
  console.log(`${String(word.hasIndices)} ${word.flags}`);
  const found = word.exec("see go 42");
  const indices = found?.indices;
  if (indices !== undefined) {
    const whole = indices[0];
    const second = indices[2];
    const named = indices.groups?.word;
    console.log(`${whole[0]}-${whole[1]} ${second === undefined ? "~" : second[0]} ${named === undefined ? "~" : named[1]}`);
  }
  let spans = "";
  for (const m of "a1b22".matchAll(/\d+/dg)) {
    const span = m.indices?.[0];
    spans += span === undefined ? "~" : `${span[0]}-${span[1]};`;
  }
  console.log(spans);
}
