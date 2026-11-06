// String.prototype.at (§22.1.3.1), substr (Annex B §B.2.2.1), isWellFormed
// and toWellFormed (§22.1.3.10, §22.1.3.32), concat and the locale-free case
// mappings.
export function main(): void {
  const text = "héllo";
  console.log(`${text.at(0)} ${text.at(-1)} ${String(text.at(5))} ${String(text.at(-6))} ${text.at(1.7)}`);
  console.log(`[${"abcdef".substr(1, 3)}] [${"abcdef".substr(-3)}] [${"abcdef".substr(-3, 2)}] [${"abcdef".substr(4, -1)}] [${"abcdef".substr(9)}]`);
  console.log(`[${"abcdef".substr(-Infinity, 2)}] [${"abcdef".substr(NaN, 2)}] [${"abcdef".substr(2, Infinity)}]`);
  const lone = "a\ud800b";
  const pair = "a😀b";
  console.log(`${String(lone.isWellFormed())} ${String(pair.isWellFormed())} ${String("\udc00".isWellFormed())}`);
  const fixed = lone.toWellFormed();
  console.log(`${fixed.length} ${fixed.charCodeAt(1).toString(16)} ${pair.toWellFormed() === pair ? "same" : "changed"}`);
  console.log("\udc00x\ud800".toWellFormed().split("").map((unit) => unit.charCodeAt(0).toString(16)).join(" "));
  console.log("a".concat("b", "c"));
  console.log(`${"Straße".toLocaleUpperCase()} ${"ÀB".toLocaleLowerCase()}`);
}
