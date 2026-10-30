// RegExp and match arrays as ordinary values: fields, unions, closures,
// destructuring, `for…of`, `instanceof`, lastIndex writes and constant
// `new RegExp`, plus the observable properties of a RegExp.
class Rule {
  readonly pattern: RegExp;
  readonly label: string;
  constructor(pattern: RegExp, label: string) {
    this.pattern = pattern;
    this.label = label;
  }
}
function classify(rules: Rule[], text: string): string {
  for (let i = 0; i < rules.length; i++) {
    const rule = rules[i]!;
    if (rule.pattern.test(text)) {
      return rule.label;
    }
  }
  return 'other';
}
function first(m: RegExpExecArray | null): string {
  return m === null ? 'none' : m[0];
}
export function main(): void {
  const rules = [
    new Rule(/^\d+$/, 'number'),
    new Rule(/^[a-z_]\w*$/i, 'identifier'),
    new Rule(new RegExp('^\\s*$'), 'blank'),
  ];
  console.log(classify(rules, '1234') + ' ' + classify(rules, 'Foo_1') + ' ' + classify(rules, '  ') + ' ' + classify(rules, '1a'));
  const re = /(\w+)@(\w+)\.(\w+)/g;
  console.log(re.source + ' ' + re.flags + ' ' + String(re.global) + String(re.ignoreCase) + String(re.multiline) + String(re.sticky) + String(re.unicode) + String(re.dotAll) + String(re.hasIndices));
  const dynamic = new RegExp('a/b', 'gimsuy');
  console.log(dynamic.source + ' ' + dynamic.flags + ' ' + new RegExp('').source);
  console.log(String(re instanceof RegExp));
  const m = re.exec('mail: ann@example.org, bob@test.dev');
  if (m !== null) {
    const [all, user, host] = m;
    console.log(`${all ?? '-'} ${user ?? '-'} ${host ?? '-'} ${m.index} ${m.input.length} ${re.lastIndex}`);
    let parts = '';
    for (const part of m) {
      parts = parts + (part ?? '~') + ';';
    }
    console.log(parts);
  }
  const again = re.exec('mail: ann@example.org, bob@test.dev');
  console.log(first(again) + ' ' + `${re.lastIndex}`);
  re.lastIndex = 0;
  console.log(first(re.exec('x@y.z')));
  const make = (suffix: string): ((text: string) => boolean) => {
    const pattern = /\.(ts|js)$/;
    return (text: string): boolean => pattern.test(text + suffix);
  };
  const isSource = make('');
  console.log(String(isSource('main.ts')) + String(isSource('main.rs')));
  let found: RegExpExecArray | null = null;
  const words = ['alpha', 'beta42', 'gamma'];
  for (let i = 0; i < words.length; i++) {
    const hit = /\d+/.exec(words[i]!);
    if (hit !== null) {
      found = hit;
    }
  }
  console.log(first(found));
  const value: RegExp | string = Math.random() < 2 ? /z+/ : 'z';
  if (value instanceof RegExp) {
    console.log(first(value.exec('azzz')));
  }
  const sticky = /a/y;
  sticky.lastIndex = 1;
  console.log(String(sticky.test('ba')) + `${sticky.lastIndex}` + String(sticky.test('ba')) + `${sticky.lastIndex}`);
  const global = /o/g;
  global.lastIndex = 99;
  console.log(String(global.test('foo')) + `${global.lastIndex}`);
}
