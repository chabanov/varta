export function main(): void {
  const parts = "a1b-c".split(/(\d)|-/);
  console.log(parts.map((part) => (part === undefined ? "~" : part)).join("|"));
  const built = new RegExp(["(\\w)", "(\\d)"].join(""), "g");
  console.log("a1 b2".replace(built, (match: string, letter: string, digit: string, offset: number) => `${digit}${letter}${offset}`));
  console.log("x=1;y=2".replace(/(?<k>\w)=(?<v>\d)/g, (match, k, v, offset, input, groups) => `${groups.v ?? ""}${groups.k ?? ""}`));
  console.log("a1 b2".replace(built, (match, ...rest: unknown[]) => `${match}:${rest.length}`));
  const sticky = /a/y;
  sticky.lastIndex = 2;
  console.log(`${"aaba".replace(sticky, "-")} ${sticky.lastIndex}`);
}
