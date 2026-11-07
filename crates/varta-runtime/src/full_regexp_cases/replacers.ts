// Replacer functions with every argument form, for patterns written in
// place and built at run time, and `lastIndex` under the regex methods
// (ECMAScript 2025 §22.2.6.11, §22.1.3.19, §22.2.7.2).
function swap(match: string, first: string, second: string, offset: number, input: string): string {
  return second + first + '@' + String(offset) + '/' + String(input.length);
}

function run(regexp: RegExp): string {
  return 'a1 b2'.replace(regexp, (match, ...rest) => match + ':' + String(rest.length) + typeof rest[0]);
}

export function main(): void {
  const built = new RegExp(['(\\w)', '(\\d)'].join(''), 'g');
  console.log('a1 b2'.replace(built, swap));
  console.log('a1 b2'.replace(built, (match: string, letter: string, digit: string, offset: number) => digit + letter + String(offset)));
  console.log('a1 b2'.replace(built, (match, letter, digit) => match + String(letter) + String(digit)));
  const named = new RegExp('(?<key>\\w)=(?<value>\\d)', 'g');
  console.log(
    'x=1;y=2'.replace(named, (match: string, key: string, value: string, offset: number, input: string, groups: Record<string, string | undefined>) =>
      (groups['value'] ?? '') + (groups['key'] ?? '') + String(offset) + String(input.length),
    ),
  );
  console.log(run(/(\w)(\d)/));
  console.log(run(new RegExp('\\w\\d', 'g')));
  console.log('aXbX'.replaceAll(new RegExp('x', 'gi'), (match, ...args: unknown[]) => '[' + match + String(args.length) + ']'));
  console.log('2026-11-06'.replace(/(?<y>\d+)-(?<m>\d+)-(?<d>\d+)/, (...args: unknown[]) => String(args.length) + typeof args[6]));
  // lastIndex: global and sticky reads and writes (§22.2.7.2).
  const g = /a/g;
  g.lastIndex = 1.7;
  console.log(String(g.test('aba')) + ' ' + String(g.lastIndex));
  g.lastIndex = -5;
  console.log(String(g.exec('aba')?.index) + ' ' + String(g.lastIndex));
  g.lastIndex = 10;
  console.log(String(g.test('aba')) + ' ' + String(g.lastIndex));
  g.lastIndex = NaN;
  console.log(String(g.test('aba')) + ' ' + String(g.lastIndex));
  const y = /b/y;
  y.lastIndex = 1;
  console.log(String(y.test('abab')) + ' ' + String(y.lastIndex) + ' ' + String(y.test('abab')) + ' ' + String(y.lastIndex));
  const plain = /b/;
  plain.lastIndex = 3;
  console.log(String(plain.exec('abab')?.index) + ' ' + String(plain.lastIndex));
  const replacing = /b/g;
  replacing.lastIndex = 3;
  console.log('abab'.replace(replacing, '-') + ' ' + String(replacing.lastIndex));
  const stickyReplace = /a/y;
  stickyReplace.lastIndex = 2;
  console.log('aaba'.replace(stickyReplace, '-') + ' ' + String(stickyReplace.lastIndex));
  const searching = /b/g;
  searching.lastIndex = 3;
  console.log(String('abab'.search(searching)) + ' ' + String(searching.lastIndex));
  const matching = /b/g;
  matching.lastIndex = 3;
  console.log(String('abab'.match(matching)?.length) + ' ' + String(matching.lastIndex));
  const splitting = /b/y;
  splitting.lastIndex = 3;
  console.log('abab'.split(splitting).join('|') + ' ' + String(splitting.lastIndex));
  const all = /a/g;
  all.lastIndex = 2;
  const iterator = 'aaa'.matchAll(all);
  all.lastIndex = 0;
  console.log([...iterator].map((match) => match.index).join(',') + ' ' + String(all.lastIndex));
  const empty = /(?:)/gu;
  console.log('😀x'.replace(empty, '.') + ' ' + String(empty.lastIndex));
}
