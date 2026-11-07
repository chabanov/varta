// The regex string methods and `lastIndex` with the v38 pattern language
// (ECMAScript 2025 §22.1.3, §22.2.6).
function list(parts: string[]): string {
  return parts.map((part) => '<' + part + '>').join('');
}

export function main(): void {
  // replace and replaceAll: templates, named templates and replacer functions.
  console.log('2026-11-06'.replace(/(?<y>\d+)-(?<m>\d+)-(?<d>\d+)/, '$<d>.$<m>.$<y>'));
  console.log('aaa bbb'.replace(/(\w)\1+/g, '$1'));
  console.log('John Smith'.replace(/(?<first>\w+)\s(?<last>\w+)/, (m, first, last) => last + ', ' + first));
  console.log(
    'x=1, y=22'.replaceAll(/(?<k>\w)=(?<v>\d+)/g, (match, k, v, offset, input, groups) =>
      k + ':' + String(Number(v) * 2) + '@' + String(offset) + (groups.k === k ? '' : '!'),
    ),
  );
  console.log('price: 10 USD, 20 EUR'.replace(/\d+(?= EUR)/, '[$&]'));
  console.log('aXbXc'.replace(/(?<=X)./g, (m) => m.toUpperCase()));
  console.log('abc'.replace(/(?:)/g, '-'));
  console.log('😀😀'.replace(/(?:)/gv, '.'));
  console.log('AbC'.replace(/(?i:[a-c])/g, '*'));
  console.log("it's 'quoted' and \"double\"".replace(/(['"])(.*?)\1/g, '«$2»'));
  // split with captures and lookaround separators.
  console.log(list('one1two22three'.split(/(\d+)/)));
  console.log(list('camelCaseString'.split(/(?=[A-Z])/)));
  console.log(list('a, b,c ,d'.split(/\s*,\s*/)));
  console.log(list('abcabc'.split(/(?<=b)/)));
  console.log(list('a1b2c3'.split(/(\d)/, 4)));
  console.log(list('😀a😀'.split(/(?:)/v)));
  // search, match and matchAll.
  console.log(String('hello world'.search(/(?<=o )w/)));
  const words = 'the cat sat on the mat'.match(/\b(\w)at\b/g);
  const found: string[] = [];
  if (words !== null) {
    for (const word of words) {
      found.push(word ?? '');
    }
  }
  console.log(list(found));
  let pairs = '';
  for (const m of 'k1=v1;k2=v2'.matchAll(/(?<key>\w+)=(?<value>\w+)/g)) {
    pairs = pairs + m.index + ':' + (m.groups?.key ?? '') + '->' + (m.groups?.value ?? '') + ' ';
  }
  console.log(pairs);
  // lastIndex with global and sticky regexps.
  const sticky = /\d+(?=,|$)/y;
  sticky.lastIndex = 2;
  const first = sticky.exec('1,22,333');
  console.log((first === null ? 'null' : first[0]) + ' ' + String(sticky.lastIndex));
  console.log(String(sticky.test('1,22,333')) + ' ' + String(sticky.lastIndex));
  const global = /(?<!\d)\d/g;
  let seen = '';
  while (global.test('a1 b23 c4')) {
    seen = seen + String(global.lastIndex) + ' ';
  }
  console.log(seen + String(global.lastIndex));
  const unicodeSets = /\p{Emoji_Presentation}/gv;
  unicodeSets.lastIndex = 1;
  const inside = unicodeSets.exec('😀x');
  console.log((inside === null ? 'null' : String(inside.index)) + ' ' + String(unicodeSets.lastIndex));
}
