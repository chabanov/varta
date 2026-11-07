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
  const date = /(?<y>\d{4})-(?<m>\d{2})|(?<m>\d{2})\/(?<y>\d{4})/;
  for (const text of ["2026-11", "11/2026"]) {
    const found = date.exec(text);
    console.log(`${found?.groups?.y ?? "-"} ${found?.groups?.m ?? "-"}`);
  }
  console.log("11/2026".replace(date, "$<y>.$<m>"));
  console.log(joined("aBc aBC".match(/a(?i:b)c/g)));
  console.log(joined("Ab AB".match(/(?i:a(?-i:b))/g)));
  console.log("a\nb".replace(/(?m:^)/g, ">").split("\n").join("|"));
}
