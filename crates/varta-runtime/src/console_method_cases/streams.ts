// Standard error: console.error, console.warn, console.trace and
// console.assert write Node's text to standard error.
function nothing(): void {}

function main(): void {
  console.error("err", 1, { a: [1, 2] });
  console.warn("warn %s", "x");
  console.log("out", true);
  console.error({ nested: { deep: { deeper: { deepest: 1 } } } });
  console.warn(new Error("bad"));
  console.trace("x", 1);
  console.trace();
  console.trace({ a: 1 });
  console.assert(true, "no");
  console.assert(1 > 2, "yes %d", 5);
  console.assert(false);
  console.assert(0, { a: 1 });
  console.assert("", "empty", "string");
  console.dirxml("d", 2);
  const values = [1, "two", [3]];
  console.log(...values);
  console.error("spread", ...values, "end");
  console.log(nothing(), void 0);
  console.info("info");
  console.debug("debug");
}
