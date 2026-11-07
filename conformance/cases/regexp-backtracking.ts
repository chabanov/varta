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
  const tag = /<(\w+)>(.*?)<\/\1>/.exec("<b>x</i><i>y</i>");
  console.log(tag === null ? "null" : `${tag.index} ${tag[1]} ${tag[2]}`);
  console.log("aaa bbb cd".replace(/(\w)\1+/g, "$1"));
  console.log(joined("10px 20em 30px".match(/\d+(?=px)/g)));
  console.log(joined("$5 and 7 and $9".match(/(?<!\$)\b\d+/g)));
  console.log("cost: $42.50".match(/(?<=\$)\d+(\.\d+)?/)?.[0] ?? "");
  console.log("1234567".replace(/\B(?=(\d{3})+(?!\d))/g, ","));
  const quoted = /(?<q>['"])(.*?)\k<q>/.exec(`say "hi" or 'yo'`);
  console.log(quoted?.[2] ?? "");
}
