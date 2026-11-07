// `infer` in declared generic arguments, union patterns and optional tuple
// elements; tuple elements after a rest element.
interface Box<T> {
  value: T;
}
type Unbox<T> = T extends Box<infer U> ? U : never;
type Defined<T> = T extends infer U | undefined ? U : T;
type Second<T> = T extends [unknown, (infer S)?] ? S : never;
type Init<T extends unknown[]> = T extends [...infer I, unknown] ? I : [];
type LastOf = [...string[], number];
type Last<T> = T extends [...string[], infer L] ? L : never;

function count(values: LastOf): string {
  return `${values.length}:${values[values.length - 1]}`;
}

export function main(): void {
  const boxed: Unbox<Box<string>> = "boxed";
  const defined: Defined<number | undefined> = 4;
  const second: Second<[a: string, b?: number]> = 5;
  const init: Init<[1, 2, 3]> = [1, 2];
  const tail: LastOf = ["a", "b", 3];
  const last: Last<LastOf> = 7;
  console.log(`${boxed} ${defined} ${second} ${init.length} ${tail.length} ${last}`);
  console.log(count([1]));
  console.log(count(["x", "y", 2]));
}
