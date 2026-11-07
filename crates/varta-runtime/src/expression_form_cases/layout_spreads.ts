interface P {
  a: number;
}
interface PO {
  a?: number;
}
interface Q {
  a: number;
  b: string;
}
const trace: string[] = [];
function copy(p: P): P {
  return { ...p };
}
function around(p: P | undefined, tag: string): PO {
  trace.push("before");
  return { z: 0, ...p, w: tag.length };
}
function mark(): number {
  trace.push("after");
  return 9;
}
function later(p: P): P {
  return { first: trace.length, ...p, last: mark() };
}
export function main(): void {
  const q: Q = { b: "y", a: 1 };
  const c = copy(q);
  console.log(JSON.stringify(c));
  console.log(JSON.stringify(copy({ a: 2 })));
  console.log(JSON.stringify(around(q, "tag")));
  console.log(JSON.stringify(around(undefined, "t")));
  const counted: P = {
    get a(): number {
      trace.push("get a");
      return 5;
    },
  };
  console.log(JSON.stringify(later(counted)));
  console.log(trace.join(","));
  const twice = copy(c);
  console.log(JSON.stringify(twice));
}
