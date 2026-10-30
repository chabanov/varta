// Module-level `let` bindings hold mutable state shared by every function
// and closure; module constants may hold functions.
let calls = 0;
let last: string | undefined;
const record = (name: string): void => {
  calls++;
  last = name;
};
function report(): string {
  return String(calls) + " " + (last !== undefined ? last : "none");
}
export function main(): void {
  console.log(report());
  record("a");
  record("b");
  console.log(report());
}
