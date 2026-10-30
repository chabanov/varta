// Replacer functions: arrow and function literals and function bindings,
// with the match, captures, position, input and groups String.prototype
// replace passes, for regex and string patterns, global and not.
function say(value: string): void {
  console.log(JSON.stringify(value));
}
function bracket(match: string): string {
  return '[' + match + ']';
}
function tag(match: string, offset: number, input: string): string {
  return `<${match}@${offset}/${input.length}>`;
}
export function main(): void {
  say('a1b22c333'.replace(/\d+/g, (m) => `(${m.length})`));
  say('a1b22c333'.replace(/\d+/, (m) => `(${m.length})`));
  say('a1b22c333'.replaceAll(/\d+/g, bracket));
  say('k1=v1;k2=v2'.replace(/(\w+)=(\w+)/g, (m, key, value) => `${value ?? '?'}=${key ?? '?'}`));
  say('x-y'.replace(/(\w)-(\w)/, (m: string, a: string, b: string) => b + a));
  say('ab'.replace(/(a)|(b)/g, (m, a, b, offset) => `${a ?? '~'}${b ?? '~'}${offset}`));
  say('ab'.replace(/b/, (m, offset, input) => `${offset}:${input}`));
  say('2026-10'.replace(/(?<y>\d+)-(?<m>\d+)/, (all, y, m, offset, input, groups) => {
    const named = groups === undefined ? '-' : (groups['m'] ?? '?') + '/' + (groups['y'] ?? '?');
    return named + ' ' + `${offset}` + ' ' + input;
  }));
  say('aaa'.replace(/a*?/g, function (m) {
    return '<' + m + '>';
  }));
  let calls = 0;
  say('one two three'.replace(/\w+/g, (word) => {
    calls = calls + 1;
    return `${calls}`;
  }));
  say('a.b.c'.replace('.', (m) => m + m));
  say('a.b.c'.replaceAll('.', tag));
  say('abc'.replace('', (m, offset) => `[${offset}]`));
}
