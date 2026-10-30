// RegExp.prototype.exec over literals, classes, escapes, quantifiers,
// alternation priority, groups, capture resets, empty iterations, anchors,
// word boundaries, multiline, dotAll and sticky/global lastIndex. Each line
// is the match index and its elements (JSON, `~` for undefined).
function show(m: RegExpExecArray | null): string {
  if (m === null) {
    return 'null';
  }
  let text = `${m.index}`;
  for (let i = 0; i < m.length; i++) {
    const v = m[i];
    text = text + '|' + (v === undefined ? '~' : JSON.stringify(v));
  }
  return text;
}
function run(re: RegExp, input: string): void {
  console.log(show(re.exec(input)));
}
// Every match of a global or sticky regex with the lastIndex after it.
function all(re: RegExp, input: string): void {
  let text = '';
  for (let n = 0; n < 12; n++) {
    const m = re.exec(input);
    text = text + show(m) + '@' + `${re.lastIndex}` + ' ';
    if (m === null) {
      break;
    }
    if (m[0].length === 0) {
      re.lastIndex = re.lastIndex + 1;
    }
  }
  console.log(text);
}
export function main(): void {
  run(/abc/, 'xxabcxx');
  run(/abc/, 'ab');
  run(/a.c/, 'a\nc abc');
  run(/a.c/s, 'a\nc');
  run(/[a-c]+/, 'zzbcabd');
  run(/[^a-c]+/, 'abcxyzab');
  run(/[\d.]+/, 'v1.25.3-beta');
  run(/\w+@\w+\.com/, 'mail bob@example.com now');
  run(/\s+/, 'a \t\n b');
  run(/\S+\s\D\W/, 'ab 1! x !');
  run(/a*/, 'bbb');
  run(/a*?b/, 'aaab');
  run(/a+?/, 'aaa');
  run(/a{2,3}/, 'aaaa');
  run(/a{2,3}?/, 'aaaa');
  run(/a{2}/, 'a aa');
  run(/a{2,}/, 'aaaaa');
  run(/x{0}y/, 'xy');
  run(/(a|ab)(c|bcd)(d*)/, 'abcd');
  run(/a|ab/, 'abc');
  run(/(?:ab|a)c/, 'abc');
  run(/(a)|(b)/, 'b');
  run(/((a)|b)+/, 'ab');
  run(/(?:(a)|b)+/, 'ab');
  run(/(z)((a+)?(b+)?(c))*/, 'zaacbbbcac');
  run(/(a*)*/, 'b');
  run(/(a*)+/, 'b');
  run(/(a?)?/, '');
  run(/(?:(a)|b?){1,2}/, 'a');
  run(/(?:a|())*/, 'aa');
  run(/(a|)*b/, 'aab');
  run(/(?:x*)*y/, 'xxy');
  run(/(?<year>\d{4})-(?<month>\d\d)/, 'on 2026-10-29');
  run(/^abc/, 'abcabc');
  run(/^abc/, 'xabc');
  run(/abc$/, 'abcabc');
  run(/^$/, '');
  run(/^b/m, 'a\nb');
  run(/a$/m, 'a\nb');
  run(/^\s*$/m, 'x\n  \ny');
  run(/\bfoo\b/, 'a foo b');
  run(/\bfoo\b/, 'afoob');
  run(/\Boo\B/, 'fooo');
  run(/\b/, '  ');
  run(/[\b]/, 'a\bb');
  run(/\x41B\103\0/, 'ABC\0');
  run(/\cJ/, 'a\nb');
  run(/[\-\]]+/, 'a-]-b');
  run(/a]{/, 'a]{');
  run(/a{1/, 'a{1');
  run(/\/+$/, 'https://host/v1///');
  run(/^\/+/, '//path');
  all(/a/g, 'banana');
  all(/an/y, 'ananxan');
  all(/a*/g, 'baab');
  all(/(?:)/g, 'ab');
  all(/\d+/g, 'a1b22c333');
}
