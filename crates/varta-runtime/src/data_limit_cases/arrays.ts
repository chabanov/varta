// V38 `data_limits`: dense, resizable and sparse arrays past 1024 elements.
// Decimal names without a host conversion per call (the production budget
// allows 1024 host calls per execution).
// Integer remainder in guest arithmetic (`%` is a host operation, and the
// production budget allows 1024 host calls per execution).
function mod(value: number, divisor: number): number {
  return value - Math.floor(value / divisor) * divisor;
}
const DIGITS: string[] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
function name(prefix: string, value: number): string {
  let text = '';
  let rest = value;
  do {
    text = DIGITS[mod(rest, 10)] + text;
    rest = Math.floor(rest / 10);
  } while (rest > 0);
  return prefix + text;
}

function total(...values: number[]): number {
  let sum = 0;
  for (const value of values) {
    sum += value;
  }
  return sum;
}

export function main(): void {
  const xs: number[] = [];
  for (let i = 0; i < 5000; i++) {
    xs.push(i);
  }
  let sum = 0;
  for (let i = 0; i < xs.length; i++) {
    sum += xs[i];
  }
  console.log(`dense ${xs.length} ${sum} ${xs[4999]}`);
  // Indexed extension creates holes far past the old 1024 bitmap.
  xs[7000] = 7;
  console.log(`extended ${xs.length} ${xs.at(6000) === undefined} ${xs[7000]}`);
  console.log(`present ${xs.indexOf(7)} ${xs.includes(4999)}`);
  delete xs[10];
  console.log(`deleted ${xs.length} ${xs.at(10) === undefined} ${xs.indexOf(11)}`);
  xs.length = 3000;
  console.log(`shrunk ${xs.length} ${xs[2999]} ${xs.lastIndexOf(2999)}`);
  xs.length = 2048;
  const tail = xs.slice(2000);
  console.log(`slice ${tail.length} ${tail[0]} ${tail[47]}`);
  const keys = Object.keys(xs);
  console.log(`keys ${keys.length} ${keys[9]} ${keys[2046]}`);
  console.log(`spread ${total(...xs.slice(11, 1511))}`);
  const strings: string[] = [];
  for (let i = 0; i < 3000; i++) {
    strings.push(name('s', i));
  }
  // Garbage between pushes forces collections while the vector is live.
  let garbage = '';
  for (let i = 0; i < 200; i++) {
    garbage = name('', garbage.length) + ':' + strings[i * 7];
  }
  console.log(`strings ${strings.length} ${strings[2999]} ${strings.join('').length} ${garbage}`);
  const sparse: number[] = [];
  sparse[4095] = 1;
  sparse.push(2);
  console.log(`sparse ${sparse.length} ${sparse.filter((v) => v > 0).length} ${sparse.pop()}`);
  const mapped = xs.map((v) => v * 2);
  console.log(`mapped ${mapped.length} ${mapped[2047]} ${mapped.reduce((a, b) => a + b, 0)}`);
}
