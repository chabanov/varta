export function main(): void {
  const word = ["\\b", "(\\w)", "\\w*"].join("");
  const re = new RegExp(word, "g");
  console.log(`${re.source} ${re.flags} ${"one two".replace(re, "$1")}`);
  const copy = new RegExp(re, "iy");
  console.log(`${copy.source} ${copy.flags} ${String(RegExp(re) === re)}`);
  console.log(RegExp.escape("1.5*(x)"));
  try {
    new RegExp(word + "(", "u");
  } catch (error) {
    if (error instanceof SyntaxError) {
      console.log(error.message);
    }
  }
}
