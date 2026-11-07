// Match arrays are arrays, matchAll is a lazy RegExp String Iterator and
// split keeps `undefined` for groups that did not take part (ECMAScript
// 2025 §22.2.7.2, §22.2.9, §22.2.6.14, §23.1.3).
function show(values: (string | undefined)[]): string {
  return values.map((value) => (value === undefined ? '~' : '<' + value + '>')).join('');
}

function first(text: string): RegExpExecArray | null {
  return /(\w)(\w)/.exec(text);
}

function elements(): void {
  const m = /(\d+)-(x)?(?<word>\w+)/d.exec('id 42-abc!');
  if (m === null) {
    return;
  }
  console.log(String(m.length) + ' ' + m.join('|') + ' ' + String(m.index) + ' ' + m.input);
  console.log(show(m.slice(1)) + ' ' + String(m.at(-1)) + ' ' + String(m.includes(undefined)));
  console.log(String(m.indexOf('abc')) + ' ' + String(m.findIndex((value) => value === undefined)));
  console.log(m.map((value, index) => String(index) + ':' + (value ?? '~')).join(','));
  console.log(m.filter((value) => value !== undefined).length);
  let seen = '';
  m.forEach((value) => {
    seen = seen + (value ?? '_');
  });
  console.log(seen + ' ' + String(m.some((value) => value === 'abc')) + ' ' + String(m.every((value) => value !== '')));
  console.log(String(m.reduce((total, value) => total + (value ?? '').length, 0)));
  console.log([...m].length + ' ' + Array.from(m).length + ' ' + String(Array.isArray(m)));
  const [whole, digits, , word] = m;
  console.log(whole + ' ' + (digits ?? '') + ' ' + (word ?? ''));
  console.log(JSON.stringify(m.groups) + ' ' + JSON.stringify(m) + ' ' + String(Array.isArray(/z/.exec('a'))) + String(Array.isArray(/a/.exec('a'))));
  const sorted = m.toSorted();
  console.log(show(sorted) + ' ' + show(m.toReversed()) + ' ' + m.entries().next().done);
  m.push('tail');
  console.log(String(m.length) + ' ' + m[0] + ' ' + String(m[4]));
  m.reverse();
  console.log(String(m[0]) + ' ' + m.join('/'));
  const indices = m.indices;
  if (indices !== undefined) {
    console.log(String(indices.length) + ' ' + indices.map((pair) => (pair === undefined ? '~' : pair.join('-'))).join(','));
  }
  const g = 'a1b22c333'.match(/\d+/g);
  console.log(g === null ? 'null' : g.join('+') + ' ' + String(g.index) + ' ' + String(g.length));
  // matchAll is a lazy iterator.
  const re = /(?<k>\w)=(\d)/g;
  const iterator = 'a=1,b=2,c=3'.matchAll(re);
  const head = iterator.next();
  console.log(String(head.done) + ' ' + (head.value === undefined ? '' : head.value[0]) + ' ' + String(re.lastIndex));
  for (const match of iterator) {
    console.log(match[0] + '@' + String(match.index) + ' ' + (match.groups?.k ?? ''));
  }
  console.log(String(iterator.next().done));
  const keys = [...'x1y2'.matchAll(/[a-z](\d)/g)].map((match) => match[1] ?? '');
  console.log(keys.join(','));
  const helpers = 'p1q2r3'.matchAll(/\d/g).map((match) => match[0]).filter((digit) => digit !== '2').toArray();
  console.log(helpers.join(';'));
  // split yields undefined for groups that did not take part.
  console.log(show('a1b-c2d'.split(/(\d)|-/)));
  console.log(show('a,b;c'.split(/(,)|(;)/)));
  const parts: string[] = 'one two  three'.split(/\s+/);
  console.log(parts.length + ' ' + parts.join('|'));
  const sep = /(x)?y/;
  console.log(show('1y2xy3'.split(sep)));
  console.log(show('1y2xy3'.split(sep, 2)));
}

function receivers(): void {
  console.log(/(a)(b)/.exec('ab')!.join('|'));
  console.log('xy'.match(/(x)(y)/)?.slice(1).join('+') ?? 'none');
  console.log(first('pq')?.map((value) => (value ?? '').toUpperCase()).join('') ?? 'none');
  const all = 'k1 k2'.match(/k\d/g);
  console.log(String(all?.length) + ' ' + String(all?.includes('k2')));
  const m = first('mn');
  if (m) {
    console.log(m.length + ' ' + m.indexOf('n') + ' ' + m.lastIndexOf('m'));
  }
}

function writes(): void {
  const m = /(a)(b)?/.exec('xa');
  if (m !== null) {
    m[1] = 'z';
    m[2] = 'y';
    console.log(m.join('|'));
    m.length = 1;
    console.log(String(m.length) + ' ' + m[0]);
    m.index = 7;
    m.input = 'changed';
    console.log(String(m.index) + ' ' + m.input);
    m[0] = 'w';
    console.log(m[0]);
  }
}

export function main(): void {
  elements();
  receivers();
  writes();
}
