// Strings spread, convert and delegate their code points (lone surrogates
// stay single units), as the String iterator yields them.
function* letters(text: string): Generator<string> {
  yield* text;
  yield '|';
}
export function main(): void {
  const s = 'a😀b\uD800c\uDC00';
  const points = [...s];
  console.log(`${points.length} ${s.length}`);
  console.log(points.map((p) => `${p.length}:${p.charCodeAt(0)}`).join(' '));
  console.log([...'héllo', ...''].reverse().join(''));
  console.log(Array.from('xy😀').length.toString());
  console.log([...letters('ab😀')].join(','));
  const words = ['one', 'two'];
  console.log([...words[0], ...words[1]].join('-'));
}
