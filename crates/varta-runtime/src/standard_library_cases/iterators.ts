// ES2025 iterator helpers (ECMA-262 §27.1.4) and Iterator.from (§27.1.3.2.1):
// laziness, closing the underlying iterator, counters and argument checks.
function* numbers(trace: string[], limit: number): Generator<number, void, undefined> {
  try {
    for (let value = 1; value <= limit; value++) {
      trace.push(`yield ${value}`);
      yield value;
    }
  } finally {
    trace.push("closed");
  }
}

export function main(): void {
  const trace: string[] = [];
  const helper = numbers(trace, 5)
    .map((value, counter) => value * 10 + counter)
    .filter((value) => value % 3 !== 0);
  console.log(`created ${trace.length}`);
  const first = helper.next();
  console.log(`${String(first.value)} ${trace.join(",")}`);
  console.log(helper.toArray().join(","));
  console.log(trace.join(","));

  const closing: string[] = [];
  const taken = numbers(closing, 10).take(2);
  console.log(`${[...taken].join(",")} ${closing.join(",")}`);
  const zero: string[] = [];
  console.log(`${numbers(zero, 3).take(0).toArray().length} ${zero.join(",")}`);
  console.log(numbers([], 5).drop(3).toArray().join(","));
  console.log(numbers([], 3).drop(Infinity).toArray().length);
  console.log(numbers([], 3).take(Infinity).toArray().join(","));
  console.log(numbers([], 3).take(1.5).toArray().join(","));
  for (const limit of [-1, NaN]) {
    try {
      numbers([], 3).take(limit);
    } catch (error) {
      console.log(`${(error as RangeError).name}: ${(error as RangeError).message}`);
    }
    try {
      numbers([], 3).drop(limit);
    } catch (error) {
      console.log(`${(error as RangeError).name}: ${(error as RangeError).message}`);
    }
  }

  const early: string[] = [];
  for (const value of numbers(early, 10).map((value) => value * 2)) {
    if (value > 4) {
      break;
    }
  }
  console.log(early.join(","));

  console.log(numbers([], 4).flatMap((value) => [value, -value]).toArray().join(","));
  console.log(numbers([], 4).reduce((sum, value) => sum + value));
  console.log(numbers([], 4).reduce((text, value, counter) => `${text}${value}@${counter} `, ">"));
  try {
    numbers([], 0).reduce((sum, value) => sum + value);
  } catch (error) {
    console.log(`${(error as TypeError).name}: ${(error as TypeError).message}`);
  }
  const visited: string[] = [];
  numbers([], 3).forEach((value, counter) => {
    visited.push(`${counter}:${value}`);
  });
  console.log(visited.join(" "));
  const someTrace: string[] = [];
  console.log(`${String(numbers(someTrace, 5).some((value) => value === 2))} ${someTrace.join(",")}`);
  console.log(`${String(numbers([], 5).every((value) => value < 9))} ${String(numbers([], 5).every((value) => value < 3))}`);
  console.log(`${String(numbers([], 5).find((value) => value > 3))} ${String(numbers([], 5).find((value) => value > 9))}`);

  // Built-in iterators.
  console.log([10, 20, 30].values().map((value) => value + 1).toArray().join(","));
  console.log(["a", "b", "c"].entries().filter((entry) => entry[0] !== 1).map((entry) => `${entry[0]}${entry[1]}`).toArray().join(","));
  const map = new Map<string, number>([["x", 1], ["y", 2]]);
  console.log(map.keys().map((key) => key.toUpperCase()).toArray().join(","));
  console.log(new Set([3, 4]).values().reduce((sum, value) => sum + value, 0));
  console.log(Iterator.from([1, 2, 3]).drop(1).toArray().join(","));
  console.log(Iterator.from(numbers([], 2)).toArray().join(","));
  console.log(Iterator.from(new Set(["s"])).toArray().join(","));
}
