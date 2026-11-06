// ES2023–ES2025 Array.prototype copying, splicing, flattening and iterator
// methods (ECMA-262 §23.1.3) and Array.of / Array.from (§23.1.2).
interface Point {
  x: number;
  y: number;
}

function show(values: (number | undefined)[]): string {
  const parts: string[] = [];
  for (let index = 0; index < values.length; index++) {
    parts.push(Object.hasOwn(values, index) ? String(values[index]) : "_");
  }
  return `[${parts.join(",")}] ${values.length}`;
}

export function main(): void {
  const base = [1, 2, 3, 4, 5];
  console.log(base.toReversed().join(","));
  console.log(base.with(0, 9).join(","));
  console.log(base.with(-1, 7).join(","));
  try {
    base.with(5, 0);
  } catch (error) {
    console.log(`${(error as RangeError).name}: ${(error as RangeError).message}`);
  }
  try {
    base.with(-6, 0);
  } catch (error) {
    console.log(`${(error as RangeError).name}: ${(error as RangeError).message}`);
  }
  console.log(base.toSpliced().join(","));
  console.log(base.toSpliced(2).join(","));
  console.log(base.toSpliced(-2, 1).join(","));
  console.log(base.toSpliced(1, 0, 8, 9).join(","));
  console.log(base.toSpliced(1, 10, 0).join(","));
  console.log(base.join(","));

  // splice: removed elements, shifts both ways, holes move as holes.
  const a = [1, 2, 3, 4, 5, 6];
  console.log(`${a.splice(1, 2).join(",")} | ${a.join(",")}`);
  console.log(`${a.splice(1, 0, 7, 8, 9).join(",")} | ${a.join(",")}`);
  console.log(`${a.splice(-2).join(",")} | ${a.join(",")}`);
  console.log(`${a.splice(0, 2, 0).join(",")} | ${a.join(",")}`);
  console.log(`${a.splice().length} | ${a.join(",")}`);
  const sparse: (number | undefined)[] = [1, 2];
  sparse[4] = 5;
  const removed = sparse.splice(1, 1, 10, 11);
  console.log(`${show(removed)} ${show(sparse)}`);

  // copyWithin, overlapping both ways and with holes.
  console.log([1, 2, 3, 4, 5].copyWithin(0, 3).join(","));
  console.log([1, 2, 3, 4, 5].copyWithin(1, 0, 3).join(","));
  console.log([1, 2, 3, 4, 5].copyWithin(-2, -4, -3).join(","));
  const gaps: (number | undefined)[] = [1];
  gaps[3] = 4;
  gaps.copyWithin(1, 2);
  console.log(show(gaps));

  // shift / unshift.
  const queue = [1, 2, 3];
  console.log(`${String(queue.shift())} ${queue.join(",")}`);
  console.log(`${queue.unshift(7, 8)} ${queue.join(",")}`);
  const empty: number[] = [];
  console.log(`${String(empty.shift())} ${empty.unshift()} ${empty.length}`);

  // concat: arrays spread, values append, holes stay holes.
  const left = [1, 2];
  console.log(left.concat([3], 4, [5, 6]).join(","));
  console.log(String(left.concat() === left));
  const holey: (number | undefined)[] = [1];
  holey[2] = 3;
  console.log(show(holey.concat([4])));
  const nested: number[][] = [[1], [2]];
  console.log(JSON.stringify(nested.concat([[3, 4]])));

  // flat and flatMap.
  const deep: number[][][] = [[[1, 2]], [[3], [4, 5]]];
  console.log(JSON.stringify(deep.flat()));
  console.log(JSON.stringify(deep.flat(2)));
  console.log(JSON.stringify(deep.flat(Infinity)));
  console.log(JSON.stringify(deep.flat(0)));
  const mixed: (number | number[])[] = [1, [2, 3], 4, [5]];
  console.log(mixed.flat().join(","));
  const words = ["it's Sunny", "in", "California"];
  console.log(words.flatMap((word) => word.split(" ")).join("|"));
  console.log([1, 2, 3].flatMap((value, index) => [value, index]).join(","));
  console.log([1, 2, 3].flatMap((value) => value * 2).join(","));
  const points: Point[] = [{ x: 1, y: 2 }, { x: 3, y: 4 }];
  console.log(points.flatMap((point) => [point.x, point.y]).join(","));

  // keys / values / entries are live iterators.
  const live = [1, 2];
  const seen: number[] = [];
  for (const value of live.values()) {
    seen.push(value);
    if (value === 1) {
      live.push(3);
    }
  }
  console.log(seen.join(","));
  console.log([...["a", "b", "c"].keys()].join(","));
  for (const [index, value] of ["x", "y"].entries()) {
    console.log(`${index}=${value}`);
  }
  const iterator = [10, 20].values();
  const first = iterator.next();
  console.log(`${String(first.value)} ${String(first.done)}`);

  // at.
  console.log(`${String(base.at(0))} ${String(base.at(-1))} ${String(base.at(9))} ${String(base.at(-0.5))}`);

  // Array.of and Array.from.
  console.log(JSON.stringify(Array.of(7, 8, 9)));
  console.log(JSON.stringify(Array.of<string>()));
  console.log(Array.from([1, 2, 3], (value, index) => value * 10 + index).join(","));
  console.log(Array.from({ length: 4 }, (_, index) => index * index).join(","));
  console.log(Array.from(new Set(["a", "b", "a"]), (value) => value.toUpperCase()).join(","));
  console.log(Array.from("héllo", (char) => char.charCodeAt(0)).join(","));
  try {
    Array.from({ length: -1 }, () => 0);
  } catch (error) {
    console.log(`${(error as RangeError).name}: ${(error as RangeError).message}`);
  }
}
