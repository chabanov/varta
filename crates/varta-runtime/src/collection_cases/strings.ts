export function main(): void {
  console.log("5".padStart(3, "0"));
  console.log("abc".padEnd(8, "12"));
  console.log("x".padStart(4));
  console.log("abc".padStart(2, "z"));
  console.log("ab".padStart(5, ""));
  console.log("Hello, World".toUpperCase());
  console.log("ÀÉÎ straße".toLowerCase());
  console.log("straße".toUpperCase());
  console.log("a,b,,c".split(",").length);
  console.log("a,b,,c".split(",").join("|"));
  console.log("abc".split("").join("-"));
  console.log("a1b1c".split("1", 2).join("+"));
  console.log("".split(",").length);
  console.log("".split("").length);
  console.log("abc".split(undefined).length);
  console.log("a".concat("b", "c"));
  console.log([1, 2.5, -0].join());
  console.log(["x", "y"].join(" and "));
  const parts: (string | undefined)[] = ["p", undefined, "q"];
  console.log(parts.join("/"));
  console.log(String.fromCharCode(72, 105, 65601));
  console.log(String.fromCodePoint(0x1f600, 65));
  console.log("😀".codePointAt(0));
  console.log("😀".codePointAt(1));
  console.log(`${"ab".codePointAt(5) === undefined}`);
  console.log((255).toString(16).toUpperCase().padStart(4, "0"));
  try {
    String.fromCodePoint(1.5);
  } catch (e) {
    if (e instanceof RangeError) console.log(e.message);
  }
}
