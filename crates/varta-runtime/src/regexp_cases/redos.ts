// Patterns that backtrack catastrophically in backtracking engines; the
// linear-time matcher answers them like Node. The inputs stay short enough
// for Node itself to answer (it backtracks exponentially on them).
export function main(): void {
  const a20 = 'a'.repeat(20) + '!';
  console.log(
    [
      /^(a+)+$/.test(a20),
      /(a|a)*b/.test(a20),
      /(a|aa)+c/.test(a20),
      /^(\w+\s?)*$/.test(a20),
      /(x+x+)+y/.test('x'.repeat(12)),
      /\s+$/.test(' '.repeat(200) + 'x'),
      /(a+)+!/.test(a20),
    ]
      .map((value) => String(value))
      .reduce((text, value) => (text === '' ? value : text + ' ' + value), ''),
  );
  const plain = 'a'.repeat(20);
  console.log(
    String(/^(a+)+$/.test(plain)) +
      ' ' +
      String(/(.*a){5}/.test(plain)) +
      ' ' +
      String(/(a?){20}a{20}/.test(plain)),
  );
}
