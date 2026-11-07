// The `docs/language-guide.md` console methods example, verbatim.
function main(): void {
  const parts = ["a", 1];
  console.error("failed:", ...parts); // standard error: failed: a 1
  console.group("request");
  console.warn("slow");      // standard error:   slow
  console.count();           //   default: 1
  console.groupEnd();
  console.assert(1 > 2, "math"); // standard error: Assertion failed: math
  console.dir({ a: { b: { c: 1 } } }, { depth: 0 }); // { a: [Object] }
  console.table([{ a: 1, b: "x" }]);
  // ┌─────────┬───┬─────┐
  // │ (index) │ a │ b   │
  // ├─────────┼───┼─────┤
  // │ 0       │ 1 │ 'x' │
  // └─────────┴───┴─────┘
}
