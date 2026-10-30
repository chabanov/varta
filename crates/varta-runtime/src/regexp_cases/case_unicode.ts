// Case-insensitive matching (Canonicalize without `u`, simple case folding
// with `u`), the `u` flag over surrogate pairs, Unicode escapes and Annex B
// escapes. Each line is the match index and its elements (JSON).
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
function test(re: RegExp, input: string): void {
  console.log(String(re.test(input)));
}
export function main(): void {
  run(/hello/i, 'Say HeLLo');
  run(/[a-z]+/i, '123ABCdef');
  run(/[^a-z]+/i, 'abcXYZ123');
  test(/ſ/i, 's');
  test(/ſ/iu, 's');
  test(/s/iu, 'ſ');
  test(/K/iu, 'k');
  test(/k/i, 'K');
  test(/\w/iu, 'ſ');
  test(/\W/iu, 'ſ');
  test(/\W/i, 'ſ');
  test(/[^\W]/iu, 'K');
  test(/ı/iu, 'i');
  test(/İ/i, 'i');
  test(/σ/iu, 'ς');
  test(/σ/i, 'ς');
  test(/Σ/i, 'ς');
  test(/ß/i, 'ẞ');
  test(/ß/iu, 'ẞ');
  test(/ǆ/iu, 'ǅ');
  test(/Ꭰ/iu, 'ꭰ');
  test(/[à-þ]/i, 'É');
  test(/µ/i, 'Μ');
  test(/µ/iu, 'μ');
  test(/\u{10400}/iu, '\u{10428}');
  test(/\bK/iu, 'K');
  run(/./u, '😀x');
  run(/./, '😀x');
  run(/^.$/u, '😀');
  run(/^.$/, '😀');
  run(/[😀-😂]+/u, 'a😁😂b');
  run(/\u{1F600}/u, 'x😀');
  run(/😀/u, 'x😀');
  run(/\uD83D/u, '😀\uD83D');
  run(/\uD83D/, '😀');
  run(/[^a]/u, '😀');
  run(/\S+/u, ' 😀b ');
  run(/(?<a>.)(?<b>.)/u, '😀😁');
  run(/a/u, 'b😀a');
  run(/\cA\cz/, '\u0001\u001a');
  run(/\c1/, '\\c1');
  run(/[\c1]/, '\u0011');
  run(/[\c]/, '\\');
  run(/\012\0/, '\n\0');
  run(/\8\9/, '89');
  run(/\x4/, 'x4');
  run(/\u12/, 'u12');
  run(/\a\e\-/, 'ae-');
  run(/[\d-z]+/, '1-z');
  run(/\k/, 'k');
  run(/[Ā-Ă]/, 'ā');
  run(/\u{61}/, 'u'.repeat(61));
  run(/\p/, 'p');
}
