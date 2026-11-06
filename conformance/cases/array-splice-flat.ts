export function main(): void {
  const values = [1, 2, 3, 4, 5, 6];
  const removed = values.splice(1, 2, 9);
  console.log(`${removed.join(",")} | ${values.join(",")}`);
  console.log([1, 2, 3, 4, 5].copyWithin(0, 3).join(","));
  const queue = [1, 2];
  console.log(`${String(queue.shift())} ${queue.unshift(7, 8)} ${queue.join(",")}`);
  console.log([1, 2].concat([3], 4).join(","));
  const nested: number[][][] = [[[1]], [[2, 3]]];
  console.log(JSON.stringify(nested.flat()));
  console.log(nested.flat(Infinity).join(","));
  console.log(["a b", "c"].flatMap((text) => text.split(" ")).join("|"));
}
