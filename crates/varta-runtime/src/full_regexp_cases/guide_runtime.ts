export function main(): void {
  const words = ["cat", "dog"];
  const animals = new RegExp(`\\b(${words.map((word) => RegExp.escape(word)).join("|")})s?\\b`, "gi");
  console.log(animals.source); // \b(\x63at|\x64og)s?\b
  const found = animals.exec("Dogs and a cat");
  console.log(found?.slice(1).join(",")); // Dog
  animals.lastIndex = 0; // exec advanced it
  console.log([..."cats, dog".matchAll(animals)].map((match) => match.index).join(" ")); // 0 6
  console.log("a1b-c".split(/(\d)|-/).map((part) => part ?? "~").join("|")); // a|1|b|~|c
  try {
    new RegExp("(", "u");
  } catch (error) {
    if (error instanceof SyntaxError) console.log(error.message); // Invalid regular expression: /(/u: Unterminated group
  }
}
