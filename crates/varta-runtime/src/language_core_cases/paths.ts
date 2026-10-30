// Property-path narrowing: guards, joins, switch, classes and invalidation.
interface Inner { label?: string; count: number | undefined }
interface Options { signal?: Inner; retry?: number; mode: "a" | "b" | "c" }
class Holder {
  current: Inner | undefined;
  name: string | null = null;
  constructor(current: Inner | undefined) {
    this.current = current;
  }
  describe(): string {
    if (this.current !== undefined && this.current.label !== undefined) {
      return "label " + this.current.label;
    }
    if (this.name === null) return "unnamed";
    return "named " + this.name;
  }
}
function retries(options: Options): number {
  const retry = options.retry ?? 2;
  if (options.retry !== undefined && options.retry > 5) return options.retry * 10;
  return retry;
}
function signal(options: Options): string {
  if (!options.signal) return "none";
  const count = options.signal.count;
  if (typeof options.signal.count === "number") return "count " + String(options.signal.count + count!);
  return "no count";
}
function chained(options: Options): string {
  if (options.signal?.label) return options.signal.label + "!";
  return "-";
}
function modes(options: Options): string {
  switch (options.mode) {
    case "a":
      return "A";
    case "b": {
      const only: "b" = options.mode;
      return only.length > 0 ? "B" : "?";
    }
    default: {
      const rest: "c" = options.mode;
      return rest;
    }
  }
}
function joined(options: Options, flag: boolean): number {
  if (flag) {
    if (options.retry === undefined) return -1;
  } else {
    if (options.retry === undefined) return -2;
  }
  return options.retry;
}
function ternary(options: Options): number {
  return options.retry !== undefined ? options.retry + 1 : 0;
}
export function main(): void {
  console.log(new Holder({ label: "x", count: 1 }).describe() + " " + new Holder(undefined).describe());
  console.log(String(retries({ mode: "a", retry: 7 })) + " " + String(retries({ mode: "a" })));
  console.log(signal({ mode: "a" }) + "|" + signal({ mode: "a", signal: { count: 4 } }) + "|" + signal({ mode: "b", signal: { count: undefined } }));
  console.log(chained({ mode: "a", signal: { label: "L", count: 0 } }) + chained({ mode: "a" }));
  console.log(modes({ mode: "a" }) + modes({ mode: "b" }) + modes({ mode: "c" }));
  console.log(String(joined({ mode: "a", retry: 3 }, true)) + String(joined({ mode: "a" }, false)));
  console.log(String(ternary({ mode: "a", retry: 1 })));
}
