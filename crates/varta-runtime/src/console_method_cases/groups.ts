// Group indentation, counters, console.dir and console.table.
class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
}

function main(): void {
  console.group("G");
  console.log("a\nb");
  console.error("err\nnext");
  console.group();
  console.info({ a: 1, b: [1, 2] });
  console.warn("deep");
  console.groupEnd();
  console.groupEnd();
  console.groupEnd();
  console.log("c");
  console.groupCollapsed("C", 1);
  console.table([{ a: 1 }]);
  console.groupEnd();
  console.count();
  console.count();
  console.count("x");
  console.countReset();
  console.count();
  console.countReset("nope");
  console.dir("s");
  console.dir({ a: { b: { c: { d: 1 } } } }, { depth: 0 });
  console.dir({ a: { b: { c: { d: 1 } } } }, { depth: null });
  console.dir([1, 2], { depth: null });
  console.dir({ a: 1, b: "x" }, { compact: false });
  console.dir({ long: "a".repeat(30), other: [1, 2, 3], more: { k: "v" } }, { compact: true, breakLength: 40 });
  console.dir({ b: 2, a: 1, c: [3, 1] }, { sorted: true });
  console.dir([1234567, 0.000123456, 12345.678901], { numericSeparator: true });
  console.dir(Array.from({ length: 30 }, (_, i) => i * 3), { maxArrayLength: 5 });
  console.dir("x".repeat(30), { maxStringLength: 4 });
  console.dir([[1, [2, [3, [4]]]]], { breakLength: 10 });
  const rows: { a: number; b?: string; c?: boolean }[] = [{ a: 1, b: "x" }, { a: 2, c: true }];
  console.table(rows);
  console.table([[1, 2], [3]]);
  console.table({ x: { a: 1 }, y: { a: 2 } }, ["a"]);
  console.table(5);
  console.table("text");
  console.table([1, "a"]);
  console.table([new Point(1, 2), new Point(3, 4)]);
  console.table([{ a: { b: 1, c: 2, d: 3 }, e: [1, 2, 3, 4, 5] }]);
  console.table(new Map<string, number>([["k", 1], ["l", 2]]));
  console.table(new Set<string>(["s", "t"]));
  console.table([{ wide: "日本語" }, { wide: "abc" }]);
  console.table({ 2: { v: 1 }, 1: { v: 2 }, z: { v: 3 } });
  console.table([{ b: 1, 1: 2, a: 3, 0: 4 }]);
}
