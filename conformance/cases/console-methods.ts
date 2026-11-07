// Node's console methods (checked profile v38): standard error, groups,
// counters, console.table, console.assert and console.dir options. Node 24
// prints the same text; `varta error:`/`varta warn:` lines are its
// standard error.
export function main(): void {
  console.error("e", { a: 1 });
  console.warn("w %d", 4);
  console.group("g");
  console.count();
  console.count();
  console.groupEnd();
  console.table([[1, 2]]);
  console.assert(false, "no");
  console.dir({ a: { b: { c: {} } } }, { depth: 0 });
  console.trace();
}
