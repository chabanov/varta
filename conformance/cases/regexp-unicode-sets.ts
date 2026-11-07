function joined(found: RegExpMatchArray | null): string {
  let text = "";
  if (found !== null) {
    for (const part of found) {
      text += (text === "" ? "" : ",") + (part ?? "");
    }
  }
  return text;
}

export function main(): void {
  console.log(joined("héllo wörld 42 мир".match(/\p{L}+/gu)));
  console.log("abc αβγ".match(/\p{Script=Greek}+/u)?.[0] ?? "");
  console.log(joined("hello world".match(/[[a-z]--[aeiou]]+/gv)));
  console.log(joined("aαbβ".match(/[\p{L}&&\p{Script=Greek}]/gv)));
  console.log(joined("abcdxab".match(/[\q{abc|d}x]/gv)));
  console.log(joined("a👍🏽b🇺🇦c👨‍👩‍👧".match(/\p{RGI_Emoji}/gv)));
  console.log(joined("aZ_1".match(/[\w--[a-z]]/giv)));
}
