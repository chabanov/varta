// for...of over generators: exhaustion, continue, break, return and throw
function join(items: string[], sep: string): string {
  let text = "";
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (item !== undefined) {
      text = i === 0 ? item : text + sep + item;
    }
  }
  return text;
}
// out of the loop close the generator so its finally blocks run first.
function* numbers(trace: string[], n: number): Generator<number> {
  try {
    for (let i = 0; i < n; i++) {
      trace.push("yield " + String(i));
      yield i;
    }
  } finally {
    trace.push("closed");
  }
}
function firstOver(trace: string[], limit: number): number {
  for (const value of numbers(trace, 10)) {
    if (value > limit) {
      return value;
    }
  }
  return -1;
}
function* failing(trace: string[]): Generator<number> {
  yield 1;
  trace.push("about to fail");
  throw new TypeError("inner failure");
}
function* pairs(trace: string[]): Generator<string> {
  for (const a of numbers(trace, 2)) {
    for (const b of numbers(trace, 2)) {
      yield String(a) + String(b);
    }
  }
}
export function main(): void {
  const trace: string[] = [];
  let sum = 0;
  for (const v of numbers(trace, 4)) {
    if (v === 1) {
      continue;
    }
    sum += v;
  }
  console.log("sum " + String(sum) + " " + join(trace, ","));
  trace.length = 0;
  for (const v of numbers(trace, 5)) {
    if (v === 2) {
      break;
    }
  }
  console.log("break " + join(trace, ","));
  trace.length = 0;
  console.log("return " + String(firstOver(trace, 1)) + " " + join(trace, ","));
  trace.length = 0;
  try {
    for (const v of numbers(trace, 5)) {
      if (v === 1) {
        throw new RangeError("body failure " + String(v));
      }
    }
  } catch (e) {
    console.log("throw " + (e instanceof RangeError ? e.message : "?") + " " + join(trace, ","));
  }
  trace.length = 0;
  try {
    for (const v of failing(trace)) {
      trace.push("saw " + String(v));
    }
  } catch (e) {
    console.log("generator threw " + (e instanceof TypeError ? e.message : "?") + " " + join(trace, ","));
  }
  trace.length = 0;
  outer: for (const a of numbers(trace, 3)) {
    for (const b of numbers(trace, 3)) {
      if (b === 1) {
        continue outer;
      }
      if (a === 2) {
        break outer;
      }
    }
  }
  console.log("labels " + join(trace, ","));
  trace.length = 0;
  const all: string[] = [];
  for (const p of pairs(trace)) {
    all.push(p);
  }
  console.log("nested " + join(all, ",") + " " + join(trace, ","));
}
