// The temporal dead zone of module bindings: code that runs during module
// evaluation and reads (or writes) a binding before its initialization
// throws ReferenceError; after the initialization it sees the value.
function limits(): number[] { return LIMITS; }
function measure(): string {
  try {
    return String(limits().length);
  } catch (e) {
    return e instanceof ReferenceError ? e.name + ': ' + e.message : 'other';
  }
}
const early = measure();
const LIMITS: number[] = [3, 4];
const late = measure();
const greet = (name: string): string => prefix + name;
const prefix: string = 'hi ';
let counter = 0;
function bump(): number { counter += STEP; return counter; }
function tryBump(): string {
  try { return String(bump()); } catch (e) { return e instanceof Error ? e.message : '?'; }
}
const before = tryBump();
const STEP = 2;
const after = tryBump();
function reset(): void { total = 0; }
function tryReset(): string {
  try { reset(); return 'reset'; } catch (e) { return e instanceof Error ? e.message : '?'; }
}
const resetEarly = tryReset();
let total = 5;
const resetLate = tryReset();
const fact = (n: number): number => n < 2 ? 1 : n * fact(n - 1);
function local(): string {
  const read = (): number => value;
  let text = '';
  try {
    text = String(read());
  } catch (e) {
    text = e instanceof ReferenceError && e instanceof Error ? 'local ' + e.message : 'other';
  }
  const value = 1;
  return text + ' ' + String(read());
}
export function main(): void {
  console.log(early + ' | ' + late);
  console.log(greet('there'));
  console.log(before + ' | ' + after + ' | ' + String(counter));
  console.log(resetEarly + ' | ' + resetLate + ' ' + String(total));
  console.log(String(fact(5)));
  console.log(local());
  const made = new ReferenceError('made');
  const other = new EvalError('e');
  console.log(made.name + ' ' + made.message + ' ' + String(other instanceof ReferenceError) + ' ' + other.name + ' ' + typeof ReferenceError);
}
