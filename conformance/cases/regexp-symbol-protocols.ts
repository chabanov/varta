class Digits {
  [Symbol.replace](text: string, replacement: string): string {
    return text.replace(/\d/g, replacement);
  }
  [Symbol.split](text: string, limit?: number): string[] {
    return text.split(/\d/, limit);
  }
}

export function main(): void {
  const digits = new Digits();
  console.log(`${"a1b2".replace(digits, "#")} ${"a1b2c".split(digits).join("|")}`);
  const re = /(\d)/g;
  console.log(`${re[Symbol.replace]("a1b2", "<$1>")} ${re[Symbol.search]("ab3")}`);
  console.log([...re[Symbol.matchAll]("a1b2")].map((match) => match.index).join(","));
}
