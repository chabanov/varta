// The regex string methods: search, match (with and without `g`), matchAll,
// replace and replaceAll with replacement templates, split with captures and
// limits, and the string-pattern forms of split, replace and replaceAll.
function show(m: RegExpMatchArray | null): string {
  if (m === null) {
    return 'null';
  }
  let text = m.index === undefined ? '-' : `${m.index}`;
  for (let i = 0; i < m.length; i++) {
    const v = m[i];
    text = text + '|' + (v === undefined ? '~' : JSON.stringify(v));
  }
  return text;
}
function say(value: string | number): void {
  console.log(typeof value === 'string' ? JSON.stringify(value) : `${value}`);
}
export function main(): void {
  say('line1\r\nline2'.search(/\r\n|\r|\n/));
  say('abc'.search(/x/));
  const sticky = /b/y;
  sticky.lastIndex = 1;
  say('ab'.search(sticky));
  say(sticky.lastIndex);
  say('a.b'.search('.'));
  console.log(show('2026-10-29'.match(/(\d+)-(\d+)/)));
  console.log(show('a1b22c333'.match(/\d+/g)));
  console.log(show('abc'.match(/x/g)));
  console.log(show('aaa'.match(/a*?/g)));
  console.log(show('x😀y'.match(/(?:)/gu)));
  const named = 'k=v'.match(/(?<key>\w+)=(?<value>\w+)/);
  const groups = named === null ? undefined : named.groups;
  if (groups !== undefined) {
    say(groups['key'] ?? '?');
    say(groups['value'] ?? '?');
    say(Object.keys(groups).length);
  }
  let all = '';
  // `for…of` element types include undefined until dense for…of typing.
  for (const each of 'a1b22c333'.matchAll(/[a-z](\d+)/g)) {
    const m = each!;
    all = all + `${m.index}:${m[0]}:${m[1] ?? '~'} `;
  }
  say(all);
  const counter = /o/g;
  counter.lastIndex = 5;
  let count = 0;
  for (const m of 'foo boo'.matchAll(counter)) {
    count = count + m!.length;
  }
  say(count);
  say(counter.lastIndex);
  say('path///'.replace(/\/+$/, ''));
  say('//path'.replace(/^\/+/, ''));
  say('a-b-c'.replace(/-/, '+'));
  say('a-b-c'.replace(/-/g, '+'));
  say('a-b-c'.replaceAll(/-/g, '+'));
  say('John Smith'.replace(/(\w+)\s(\w+)/, '$2, $1'));
  say('abc'.replace(/b/, "[$&|$`|$'|$$|$0|$1|$<n>]"));
  say('abc'.replace(/(?<n>b)/, '[$<n>|$<m>|$2|$01|$10|$<n]'));
  say('x'.replace(/(x)/, '$11$01$1'));
  say('aaa'.replace(/a*?/g, '-'));
  say('aaba'.replace(/a/gy, 'x'));
  say('😀😀'.replace(/(?:)/gu, '.'));
  say('😀😀'.replace(/(?:)/g, '.'));
  say(JSON.stringify('a,b,,c'.split(',')));
  say(JSON.stringify('a,b,,c'.split(',', 2)));
  say(JSON.stringify('abc'.split('')));
  say(JSON.stringify(''.split(',')));
  say(JSON.stringify(''.split('')));
  say(JSON.stringify('abc'.split()));
  say(JSON.stringify('abc'.split(undefined, 0)));
  say(JSON.stringify('a1b22c'.split(/\d+/)));
  say(JSON.stringify('a1b22c'.split(/(\d)+/)));
  say(JSON.stringify('a, b ,c'.split(/\s*,\s*/)));
  say(JSON.stringify('abc'.split(/(?:)/)));
  say(JSON.stringify('😀x'.split(/(?:)/u)));
  say(JSON.stringify('😀x'.split(/(?:)/)));
  say(JSON.stringify(''.split(/x/)));
  say(JSON.stringify(''.split(/(?:)/)));
  say(JSON.stringify('a1b2c3'.split(/\d/, 2)));
  say(JSON.stringify('test'.split(/(?:)/y)));
  say('a.b.c'.replace('.', '!'));
  say('a.b.c'.replaceAll('.', '!'));
  say('abc'.replace('b', '[$&$`$\'$$$1]'));
  say('aaa'.replaceAll('', '-'));
  say('abc'.replace('', '-'));
  say('xyz'.replace('q', '-'));
}
