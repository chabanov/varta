// Patterns only known at run time (ECMAScript 2025 §22.2.4.1, §22.2.3.3,
// §22.2.5.1): `new RegExp` and `RegExp(…)` with computed patterns and flags,
// `new RegExp(regexp, flags)`, `source`/`flags` round trips, `RegExp.escape`
// and V8's SyntaxError messages, compiled in the guest by the linked pattern
// compiler.
function show(re: RegExp): string {
  return '/' + re.source + '/' + re.flags + ' ' + String(re.lastIndex);
}

function attempt(pattern: string, flags: string): string {
  try {
    return show(new RegExp(pattern, flags));
  } catch (error) {
    if (error instanceof SyntaxError) {
      return error.name + ': ' + error.message;
    }
    return 'other';
  }
}

function build(parts: string[]): string {
  return parts.join('');
}

export function main(): void {
  // Computed patterns and flags.
  const re = new RegExp(build(['a', '(b)', '+']), 'g');
  console.log(show(re));
  const m = re.exec('xxabbbab');
  console.log(m === null ? 'null' : m[0] + ' ' + String(m.index) + ' ' + String(re.lastIndex));
  const named = new RegExp(build(['(?<year>\\d{4})', '-', '(?<month>\\d{2})']), build(['d', 'u']));
  const date = named.exec('on 2026-11 ok');
  console.log(date === null ? 'null' : (date.groups?.year ?? '') + '/' + (date.groups?.month ?? ''));
  const span = date?.indices?.groups?.month;
  console.log(span === undefined ? 'none' : String(span[0]) + '-' + String(span[1]));
  // `source` escapes `/` and line terminators; the empty pattern is `(?:)`.
  console.log(show(new RegExp(build(['a/b', '\n', '[/]', '\u2028']))));
  console.log(show(new RegExp(build([]))));
  console.log(show(RegExp(build(['x', 'y']))));
  // A RegExp pattern keeps its source; flags replace its flags.
  re.lastIndex = 3;
  const copy = new RegExp(re);
  console.log(show(copy) + ' ' + String(copy === re));
  console.log(show(new RegExp(re, 'iy')));
  console.log(String(RegExp(re) === re) + ' ' + String(RegExp(re, 'g') === re));
  const slashes = new RegExp(build(['a/', '(']).slice(0, 2), 'm');
  console.log(show(new RegExp(slashes, 'u')));
  // Round trips through `source` and `flags`.
  for (const literal of [/[/]\/x/gimsuy, /\p{L}+(?<=a)/dv, /(?:)/, /\n\u2028/s]) {
    const again = new RegExp(literal.source, literal.flags);
    console.log(show(again) + ' ' + String(again.source === literal.source && again.flags === literal.flags));
  }
  // RegExp.escape (ES2025).
  console.log(RegExp.escape('a.b-c 1') + ' ' + RegExp.escape('$100 (net)'));
  console.log(RegExp.escape('😀\u00a0\u2028\ud800x,=<>#&!%:;@~\'`"') + ' ' + RegExp.escape('_1\t\n\u000b\f\r'));
  console.log(RegExp.escape('') + '|' + RegExp.escape('9') + '|' + RegExp.escape('ä^$\\.*+?()[]{}|/'));
  const literal = new RegExp(RegExp.escape('1+1=2?'), 'g');
  console.log('1+1=2? and 1+1=2?'.replace(literal, '<$&>'));
  // Unicode properties, sets and case folding compiled at run time.
  const props = new RegExp(build(['\\p{Script=Greek}+', '|', '\\p{RGI_Emoji}']), 'gv');
  let found = '';
  for (const item of 'abc αβγ 👍🏽 x 🇺🇦'.matchAll(props)) {
    found = found + '|' + item[0];
  }
  console.log(found);
  const folded = new RegExp(build(['straße', '|', 'ǆ', '|', '[\\w--\\d]+']), 'iv');
  console.log(String(folded.test('STRASSE')) + ' ' + String(folded.test('ǅ')) + ' ' + String(folded.test('ſ')));
  const lone = new RegExp('\ud800', 'u');
  console.log(String(lone.test('a\ud800b')) + ' ' + String(lone.test('\ud800\udc00')));
  // Early errors at run time carry V8's messages.
  for (const [pattern, flags] of [
    ['(', ''],
    ['a/(', 'gi'],
    ['[', 'v'],
    ['a', 'gg'],
    ['a', 'uv'],
    ['a', 'x'],
    ['\\p{Nope}', 'u'],
    ['(?<a>x)(?<a>y)', ''],
    ['\\k<b>(?<a>x)', ''],
    ['[a-z&&[aeiou]]', 'v'],
    ['a{2,1}', ''],
    ['\ud800(', 'y'],
  ]) {
    console.log(attempt(pattern ?? '', flags ?? ''));
  }
}
