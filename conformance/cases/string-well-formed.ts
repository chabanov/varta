export function main(): void {
  const lone = "a\ud800";
  console.log(`${String(lone.isWellFormed())} ${String("ok".isWellFormed())}`);
  console.log(lone.toWellFormed().charCodeAt(1).toString(16));
  console.log(`${"hello".at(-1)} ${String("hello".at(9))}`);
  console.log("abcdef".substr(-4, 2));
}
