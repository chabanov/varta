export function main(): void {
  const pairs = /(?<key>\w)=(\d)/g;
  const iterator = "a=1,b=2,c=3".matchAll(pairs);
  const head = iterator.next();
  console.log(`${String(head.done)} ${head.value?.[0] ?? "~"} ${pairs.lastIndex}`);
  for (const match of iterator) {
    console.log(`${match[0]}@${match.index} ${match.groups?.key ?? "~"}`);
  }
  const digits = [..."x1y2z3".matchAll(/\d/g)].map((match) => match[0]);
  const odd = "x1y2z3".matchAll(/\d/g).filter((match) => match[0] !== "2").map((match) => match.index).toArray();
  console.log(`${digits.join("")} ${odd.join(",")} ${String(iterator.next().done)}`);
}
