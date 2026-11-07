// A temporal-dead-zone access throws a ReferenceError instance; a var read
// before its definition is undefined.
function read(): number { return limit; }
function attempt(): string {
  try {
    return String(read());
  } catch (e) {
    return (e instanceof ReferenceError ? 'ReferenceError' : 'other') + ' ' + String(e instanceof Error);
  }
}
function peek(): string { return String(late); }
const early = attempt();
const before = peek();
const limit = 3;
var late: number | undefined = 4;
export function main(): void {
  console.log(early + ' ' + attempt() + ' ' + before + ' ' + peek());
}
