// Backreferences, lookaround, quantifier semantics and captures under the
// v38 backtracking matcher (ECMAScript 2025 §22.2.2).
function show(m: RegExpExecArray | null): string {
  if (m === null) {
    return 'null';
  }
  let text = String(m.index);
  for (const value of m) {
    text = text + '|' + (value === undefined ? '~' : '"' + value + '"');
  }
  return text;
}

export function main(): void {
  // Numbered and named backreferences (§22.2.2.7.2 BackreferenceMatcher).
  console.log(show(/(a+)b\1/.exec('xaaabaa')));
  console.log(show(/(\w)(\w)\2\1/.exec('xabba abba')));
  console.log(show(/(?<q>['"]).*?\k<q>/.exec(`say "hi" or 'yo'`)));
  console.log(show(/\1(a)/.exec('aa')));
  console.log(show(/(a)|\1b/.exec('b')));
  console.log(show(/(?:(a)|b)\1c/.exec('bc')));
  console.log(show(/(a)\1/i.exec('aA')));
  console.log(show(/(ſ)\1/iu.exec('ſs')));
  console.log(show(/(ſ)\1/i.exec('ſs')));
  console.log(show(/(abc)\1/i.exec('ABCabc')));
  console.log(show(/<(\w+)>.*?<\/\1>/.exec('<b>x</i><i>y</i>')));
  console.log(show(/(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)\10/.exec('abcdefghijj')));
  console.log(show(/(a)\10/.exec('a\b')));
  // Lookahead and lookbehind (§22.2.2.4).
  console.log(show(/\d+(?=px)/.exec('10em 20px')));
  console.log(show(/\d+(?!px|\d)/.exec('10px 20em')));
  console.log(show(/(?<=\$)\d+(\.\d+)?/.exec('cost: $42.50')));
  console.log(show(/(?<!\$)\b\d+/.exec('$5 and 7')));
  console.log(show(/(?=(a+))a*b\1/.exec('baaabac')));
  console.log(show(/(?=(a+))/.exec('baaabac')));
  console.log(show(/(.*?)a(?!(a+)b\2c)\2(.*)/.exec('baaabaac')));
  console.log(show(/(?<=(\d+)(\d+))$/.exec('1053')));
  console.log(show(/(?<=\1(a))b/.exec('aab')));
  console.log(show(/(?<!(a))b/.exec('ab cb')));
  console.log(show(/(?<=a(?=b)b)c/.exec('abc')));
  console.log(show(/(?=a)*a/.exec('a')));
  console.log(show(/(?!a)?b/.exec('ab')));
  console.log(show(/^(?:(?=(\w))\1)+$/.exec('abc')));
  // Quantifiers, empty checks and captures of iterations (RepeatMatcher).
  console.log(show(/(z)((a+)?(b+)?(c))*/.exec('zaacbbbcac')));
  console.log(show(/(a*)*/.exec('b')));
  console.log(show(/(a*)+/.exec('b')));
  console.log(show(/(a|ab)(c|bcd)(d*)/.exec('abcd')));
  console.log(show(/((a)|b)+/.exec('ab')));
  console.log(show(/(?:(a)|(b))+/.exec('ab')));
  console.log(show(/(a{2,3}?)(a*)/.exec('aaaaa')));
  console.log(show(/(.){0}x/.exec('ax')));
  console.log(show(/^(?:a|ab)(?:c|bcd)(?:d*)$/.exec('abcd')));
  console.log(show(/(\d{1,3})(?=(\d{3})+$)/.exec('1234567')));
  console.log(show(/a{2,}?b/.exec('aaaab')));
  console.log(show(/(?:a?){3}b/.exec('aab')));
  console.log(show(/(a?){2,}b/.exec('ab')));
  console.log(show(/x(?:(a)|b)*?y/.exec('xabay')));
  console.log(show(/[^]+?(?=;)/.exec('k=v;w')));
  console.log(show(/(?:)/.exec('')));
}
