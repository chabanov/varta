// Callable objects with properties (K9 A step 4): functions are objects
// with own properties, from callable object types, intersections,
// Object.assign and expando assignments.
interface Counter {
  (step: number): number;
  count: number;
  label?: string;
}
type Greeter = ((name: string) => string) & { greeting: string };
type Formatter = (value: number) => string;

function tick(): number {
  tick.calls += 1;
  return tick.calls;
}
tick.calls = 0;
tick.unit = "ms";

function makeCounter(): Counter {
  const counter: Counter = ((step: number) => {
    counter.count += step;
    return counter.count;
  }) as Counter;
  counter.count = 0;
  return counter;
}

function total(counters: Counter[]): number {
  return counters.reduce((sum, counter) => sum + counter.count, 0);
}

export function main(): void {
  const counter = makeCounter();
  counter(2);
  counter(3);
  console.log(`${counter.count} ${counter.label ?? "none"} ${typeof counter}`);
  counter.label = "clicks";
  console.log(`${Object.keys(counter).join(",")} ${"label" in counter} ${total([counter, makeCounter()])}`);
  const greet: Greeter = ((name: string) => `${greet.greeting}, ${name}`) as Greeter;
  greet.greeting = "Hi";
  console.log(greet("Ada"));
  delete counter.label;
  console.log(`${Object.keys(counter).join(",")} ${JSON.stringify({ f: counter, n: 1 })} ${JSON.stringify([counter])}`);
  tick();
  tick();
  console.log(`${tick.calls} ${tick.unit} ${Object.keys(tick).join(",")}`);
  const format = Object.assign((value: number) => `${value * 2}px`, { suffix: "px", scale: 2 });
  console.log(`${format(3)} ${format.scale} ${Object.keys(format).join(",")}`);
  const plain: Formatter = format;
  const held: unknown = plain;
  const back = held as Formatter;
  console.log(`${plain(4)} ${back(5)} ${back === format}`);
  const same = Object.assign(tick, { extra: true });
  console.log(`${same === tick} ${same.extra} ${Object.keys(tick).join(",")}`);
  for (const key in format) {
    console.log(key);
  }
}
