const calls: string[] = [];
interface Counter {
  label: string;
  count: number;
}
interface Inner {
  deep: number;
}
interface Outer {
  first: Inner;
  second: number;
}
function counter(label: string): Counter {
  let count = 0;
  return {
    label,
    get count(): number {
      count++;
      calls.push(label + count);
      return count;
    },
  };
}
export function main(): void {
  const shared = counter("s");
  const pair = { left: shared, right: shared, list: [shared, counter("t"), shared] };
  console.log(JSON.stringify(pair));
  console.log(calls.join(","));
  calls.length = 0;
  const outer: Outer = {
    get first(): Inner {
      calls.push("first");
      return {
        get deep(): number {
          calls.push("deep");
          return 1;
        },
      };
    },
    get second(): number {
      calls.push("second");
      return 2;
    },
  };
  console.log(JSON.stringify(outer));
  console.log(calls.join(","));
  const ordered = {
    z: 1,
    get b(): string {
      return "B";
    },
    a: true,
  };
  console.log(JSON.stringify(ordered));
  console.log(JSON.stringify({ list: [ordered, ordered], plain: { b: "2", "1": "1" } }));
  calls.length = 0;
  const table: Record<string, Counter> = { b: counter("b"), "2": shared, a: shared };
  console.log(JSON.stringify(table));
  console.log(calls.join(","));
}
