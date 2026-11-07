// Named and duplicate named groups, pattern modifiers, match indices (`d`)
// and the flag accessors (ECMAScript 2025 §22.2.6, §22.2.7.8).
function show(m: RegExpExecArray | null): string {
  if (m === null) {
    return 'null';
  }
  let text = String(m.index);
  for (const value of m) {
    text = text + '|' + (value === undefined ? '~' : value);
  }
  const groups = m.groups;
  if (groups !== undefined) {
    text = text + ' {';
    for (const key of Object.keys(groups)) {
      const value = groups[key];
      text = text + key + '=' + (value === undefined ? '~' : value) + ';';
    }
    text = text + '}';
  }
  return text;
}

function pair(value: [number, number] | undefined): string {
  return value === undefined ? '~' : value[0] + '-' + value[1];
}

function indices(m: RegExpExecArray | null): string {
  if (m === null) {
    return 'null';
  }
  const found = m.indices;
  if (found === undefined) {
    return 'no indices';
  }
  let text = String(found.length);
  for (let index = 0; index < found.length; index++) {
    text = text + ' ' + pair(found[index]);
  }
  const groups = found.groups;
  if (groups !== undefined) {
    for (const key of Object.keys(groups)) {
      text = text + ' ' + key + ':' + pair(groups[key]);
    }
  }
  return text;
}

export function main(): void {
  // Named groups and duplicate names in different alternatives (ES2025).
  console.log(show(/(?<year>\d{4})-(?<month>\d{2})/.exec('on 2026-11-06')));
  console.log(show(/(?<y>\d{4})-\d{2}|\d{2}\/(?<y>\d{4})/.exec('x 11/2026')));
  console.log(show(/(?<y>\d{4})-\d{2}|\d{2}\/(?<y>\d{4})/.exec('2025-10')));
  console.log(show(/(?:(?<a>x)|(?<a>y))\k<a>/.exec('-yy-')));
  console.log(show(/(?<\u0061b>.)(?<\u{63}>.)/.exec('xyz')));
  console.log(show(/\k<n>(?<n>a)/.exec('aa')));
  console.log('a-b'.replace(/(?<x>a)|(?<x>b)/g, '[$<x>]'));
  // Pattern modifiers (ES2025): `i`, `m` and `s` for a group.
  console.log(show(/a(?i:b)c/.exec('aBc')));
  console.log(show(/a(?i:b)c/.exec('aBC')));
  console.log(show(/(?i:a(?-i:b))/.exec('Ab AB')));
  console.log(show(/(?m:^b)/.exec('a\nb')));
  console.log(show(/(?s:.)/.exec('\n')));
  console.log(show(/(?-s:.)/s.exec('\nx')));
  console.log(show(/(?i:(a))\1/.exec('AA aA')));
  console.log(show(/(?i:\b\u212A)/u.exec('xk K\u212A')));
  // Match indices.
  const d = /(?<word>\w+) (\d+)?/d;
  console.log(String(d.hasIndices) + ' ' + d.flags);
  console.log(indices(d.exec('see go 42')));
  console.log(indices(d.exec('stop now')));
  console.log(indices(/a/.exec('a')));
  console.log(indices(/(?<=(a))b(?<c>c)/d.exec('xabc')));
  const matched = 'x1y'.match(/(\d)/d);
  if (matched !== null && matched.indices !== undefined) {
    console.log(pair(matched.indices[0]) + ' ' + pair(matched.indices[1]));
  }
  for (const m of 'a1b22'.matchAll(/\d+/dg)) {
    console.log(indices(m));
  }
  // Flags in canonical order, and the accessors.
  const all = /x/dgimsvy;
  console.log(all.flags + ' ' + String(all.unicodeSets) + ' ' + String(all.unicode));
  console.log(new RegExp('a', 'yvd').flags);
  console.log(/x/u.flags + ' ' + String(/x/v.unicodeSets));
}
