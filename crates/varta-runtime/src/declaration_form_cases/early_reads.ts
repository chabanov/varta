// Early reads of a `var` and of a static field: JavaScript reads
// `undefined` before the definition. A declared type that admits
// `undefined` sees it, as in Node; any other type throws TypeError at that
// read (the approved checked read at TypeScript's unsoundness: Node prints
// `undefined` and `NaN` on the two `TypeError` lines), and a write before the
// definition defines the binding.
function readCount(): number { return count; }
function readLabel(): string { return String(label); }
function attempt(read: () => number): string {
  try {
    return String(read());
  } catch (e) {
    return e instanceof TypeError ? 'TypeError' : 'other';
  }
}
const first = attempt(readCount);
const label0 = readLabel();
var count: number = 1;
var label: string | undefined = 'set';
const second = attempt(readCount);
const label1 = readLabel();
function assignTotal(): string { total = 5; return String(total); }
const totalEarly = assignTotal();
var total: number = 7;
var early: number | undefined = later;
var later: number | undefined = 2;
class Config {
  static seen: string = Config.peek();
  static checked: string = Config.check();
  static peek(): string { return String(Config.limit); }
  static check(): string {
    try {
      return String(Config.size + 1);
    } catch (e) {
      return e instanceof TypeError ? 'TypeError' : 'other';
    }
  }
  static limit: number | undefined = 3;
  static size: number = 4;
}
export function main(): void {
  console.log(first + ' ' + second);
  console.log(label0 + ' ' + label1);
  console.log(totalEarly + ' ' + String(total));
  console.log(String(early) + ' ' + String(later));
  console.log(Config.seen + ' ' + Config.checked + ' ' + String(Config.limit) + ' ' + String(Config.size));
}
