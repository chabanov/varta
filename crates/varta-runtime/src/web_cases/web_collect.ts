// Live Headers, signals and their reasons survive collections forced by
// garbage strings, garbage Headers and garbage controllers; the header lists,
// reasons and captured instances are traced exactly (v35 Web types).
function churn(): void {
  let payload = 'x';
  for (let i = 0; i < 14; i++) {
    payload = payload + payload;
  }
  for (let i = 0; i < 350; i++) {
    const values = [payload + '!', payload + '?'];
    values.slice();
  }
}

class Holder {
  readonly list: Headers[] = [];
  readonly signal: AbortSignal;
  constructor(signal: AbortSignal) {
    this.signal = signal;
  }
  reader(): () => string {
    return () => `${this.list.length} ${this.signal.aborted}`;
  }
}

const NAMES = ['X-One', 'x-two', 'X-Three'];

function garbage(i: number): void {
  const name = NAMES[i < 1 ? 0 : i < 2 ? 1 : 2]!;
  const headers = new Headers([[name, 'v' + name]]);
  headers.set('Y', name + name);
  const controller = new AbortController();
  controller.abort('reason ' + name);
  headers.get(name);
}

function dump(headers: Headers): string {
  let out = '';
  for (const [name, value] of headers) {
    out = out + `[${name}=${value}]`;
  }
  return out;
}

export function main(): void {
  const init: Record<string, string> = {};
  init['Content-' + 'Type'] = 'text/' + 'plain';
  const held = new Headers(init);
  held.append('Accept', 'a' + '/b');
  const controller = new AbortController();
  controller.abort(new Error('kept' + '!'));
  const holder = new Holder(controller.signal);
  holder.list.push(new Headers(held));
  holder.list[0]!.append('x-copy', 'c' + 'd');
  const read = holder.reader();
  churn();
  // Each iteration makes two host calls (Array.isArray and the unknown
  // reason check), well inside the production host-call budget.
  for (let i = 0; i < 300; i++) {
    garbage(i < 3 ? i : 2);
  }
  churn();
  console.log(dump(held));
  console.log(dump(holder.list[0]!));
  console.log(read());
  const reason = holder.signal.reason;
  if (reason instanceof Error) console.log(reason.message);
}
