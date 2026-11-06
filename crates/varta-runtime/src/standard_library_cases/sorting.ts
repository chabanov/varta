// Array.prototype.sort and toSorted (ECMA-262 §23.1.3.30, §23.1.3.34): the
// comparator's call sequence, stability, holes and undefined, galloping merges.
let seed = 12345;
function random(limit: number): number {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed % limit;
}

function checksum(values: number[]): number {
  let sum = 0;
  for (let index = 0; index < values.length; index++) {
    sum = (sum * 31 + values[index]! + index) % 1000000007;
  }
  return sum;
}

interface Item {
  key: number;
  order: number;
}

export function main(): void {
  // Call sequence of a small sort.
  const calls: string[] = [];
  const small = [5, 1, 4, 2, 3];
  small.sort((a, b) => {
    calls.push(`${a}:${b}`);
    return a - b;
  });
  console.log(small.join(","));
  console.log(calls.join(" "));

  // Many sizes, random data with runs: the comparator count and checksum.
  for (const size of [0, 1, 2, 31, 32, 33, 63, 64, 65, 100, 257, 1000]) {
    const values: number[] = [];
    for (let index = 0; index < size; index++) {
      values.push(index % 7 === 0 ? random(10) : random(1000));
    }
    // Some ascending and descending runs to exercise galloping.
    for (let index = 0; index + 40 < size; index += 97) {
      for (let k = 0; k < 40; k++) {
        values[index + k] = index % 2 === 0 ? k * 3 : 400 - k;
      }
    }
    let count = 0;
    const sorted = values.toSorted((a, b) => {
      count++;
      return a - b;
    });
    console.log(`${size} ${count} ${checksum(sorted)} ${checksum(values)}`);
  }

  // Stability with equal keys.
  const items: Item[] = [];
  for (let index = 0; index < 300; index++) {
    items.push({ key: random(5), order: index });
  }
  items.sort((a, b) => a.key - b.key);
  let stable = true;
  for (let index = 1; index < items.length; index++) {
    const previous = items[index - 1]!;
    const current = items[index]!;
    if (previous.key === current.key && previous.order > current.order) {
      stable = false;
    }
  }
  console.log(`stable ${String(stable)} ${items[0]!.order} ${items[299]!.order}`);

  // An inconsistent comparator leaves V8's order.
  const odd: number[] = [];
  for (let index = 0; index < 80; index++) {
    odd.push(random(50));
  }
  odd.sort((a, b) => (a + b) % 3 - 1);
  console.log(odd.join(","));

  // NaN results compare as equal.
  const nan = [3, 1, 2];
  nan.sort(() => NaN);
  console.log(nan.join(","));

  // Default order: String forms by code units.
  const numbers = [10, 9, 1, 100, -1, 25];
  numbers.sort();
  console.log(numbers.join(","));
  const words = ["b", "B", "a", "é", "Z", "aa", ""];
  console.log(words.toSorted().join("|"));
  console.log(words.join("|"));

  // undefined goes last; holes after.
  const sparse: (number | undefined)[] = [3, undefined, 1, undefined, 2];
  sparse.sort((a, b) => a! - b!);
  console.log(sparse.map((v) => String(v)).join(","));
  const holes: number[] = [4, 2];
  holes[5] = 1;
  holes.sort((a, b) => a - b);
  console.log(`${holes.length} ${String(Object.hasOwn(holes, 2))} ${String(Object.hasOwn(holes, 3))} ${holes[0]},${holes[1]},${holes[2]}`);
  const copied: (number | undefined)[] = [4, 2];
  copied[3] = 1;
  const result = copied.toSorted((a, b) => a! - b!);
  console.log(`${result.length} ${String(result[3])} ${String(Object.hasOwn(result, 3))}`);

  // sort returns the receiver.
  const same = [2, 1];
  console.log(String(same.sort((a, b) => a - b) === same));
  console.log(String(same.toSorted((a, b) => a - b) === same));
}
