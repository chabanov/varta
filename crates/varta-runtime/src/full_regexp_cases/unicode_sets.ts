// Unicode property escapes and the `v` flag's class set notation and
// properties of strings (ECMAScript 2025 §22.2.1, §22.2.2.9).
function all(pattern: RegExp, text: string): string {
  let out = '';
  for (const m of text.matchAll(pattern)) {
    out = out + '[' + m[0] + ']';
  }
  return out;
}

export function main(): void {
  // Property escapes.
  console.log(all(/\p{Lu}/gu, 'aBcÄdΣ'));
  console.log(all(/\p{L}+/gu, 'héllo wörld 42 мир'));
  console.log(all(/\P{L}/gu, 'a1b-c'));
  console.log(all(/\p{Script=Greek}+/gu, 'abc αβγ def'));
  console.log(all(/\p{sc=Cyrillic}/gu, 'aжb'));
  console.log(all(/\p{scx=Hira}/gu, 'あーa'));
  console.log(all(/\p{Nd}/gu, '1٣x'));
  console.log(all(/\p{General_Category=Decimal_Number}/gu, '7a'));
  console.log(all(/\p{Emoji_Presentation}/gu, 'a😀b✨'));
  console.log(all(/\p{ASCII_Hex_Digit}+/gu, 'beef cafe xyz'));
  console.log(all(/[\p{L}\p{N}]+/gu, 'a1-b2 c'));
  console.log(all(/[^\p{L}]/gu, 'ab1c'));
  console.log(all(/\p{Lu}/giu, 'aB'));
  console.log(all(/\P{Lu}/giu, 'aB1'));
  console.log(all(/\p{Any}/gu, 'a😀'));
  console.log(all(/\p{ASCII}/gu, 'aé'));
  console.log(all(/\p{Assigned}/gu, 'a͸b'));
  console.log(all(/\p{LC}/gu, 'aǅ1'));
  // Class set operations.
  console.log(all(/[\p{L}--[a-z]]/gv, 'aBcD'));
  console.log(all(/[\p{L}&&\p{Script=Greek}]/gv, 'aαbβ'));
  console.log(all(/[[a-z]--[aeiou]]+/gv, 'hello world'));
  console.log(all(/[\w--\d]+/gv, 'ab12cd'));
  console.log(all(/[[a-z]&&[^aeiou]&&[^xyz]]/gv, 'abxyc'));
  console.log(all(/[\q{abc|d}x]/gv, 'abcdxab'));
  console.log(all(/[\q{abc|ab|a}]/gv, 'abcaba'));
  console.log(all(/[^\q{a}b]/gv, 'abc'));
  console.log(all(/[\q{}a]/gv, 'ab'));
  console.log(all(/[\p{L}--\q{a}]/gv, 'ab'));
  console.log(all(/[\-\&\!]/gv, 'a-&!'));
  console.log(all(/[a-c]/giv, 'ABCD'));
  console.log(all(/[\w--[a-z]]/giv, 'aZ_1'));
  console.log(all(/[^a]/giv, 'aAb'));
  console.log(all(/\P{Ll}/giv, 'aA1'));
  // Properties of strings.
  console.log(all(/\p{RGI_Emoji}/gv, 'a👍🏽b🇺🇦c👨‍👩‍👧d#️⃣'));
  console.log(all(/\p{RGI_Emoji_Flag_Sequence}/gv, '🇯🇵🇫🇷x'));
  console.log(all(/\p{Emoji_Keycap_Sequence}/gv, '1️⃣2'));
  console.log(all(/\p{RGI_Emoji_Modifier_Sequence}/gv, '👋🏿👋'));
  console.log(all(/\p{RGI_Emoji_Tag_Sequence}/gv, '🏴󠁧󠁢󠁳󠁣󠁴󠁿'));
  console.log(all(/\p{RGI_Emoji_ZWJ_Sequence}/gv, '👩‍💻 🧑‍🚀'));
  console.log(all(/\p{Basic_Emoji}/gv, '⌚x☺️'));
  console.log(all(/[\p{RGI_Emoji}--\q{😀}]/gv, '😀😁'));
  console.log(String(/^\p{RGI_Emoji}$/v.test('👨‍👩‍👧')));
  // `u` and `v` read code points; lone surrogates match themselves.
  console.log(all(/./gv, 'a😀'));
  console.log(all(/[😀-😂]/gv, '😁x😂'));
  console.log(String(/\ud83d/v.test('😀')) + ' ' + String(/\ud83d/.test('😀')));
}
