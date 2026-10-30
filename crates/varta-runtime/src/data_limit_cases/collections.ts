// V38 `data_limits`: Map, Set, Uint8Array, text codecs and dictionaries past
// 1024 entries.
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

export function main(): void {
  const map = new Map<string, number>();
  for (let i = 0; i < 3000; i++) {
    map.set(name('k', i), i);
  }
  for (let i = 0; i < 3000; i += 3) {
    map.delete(name('k', i));
  }
  let visited = 0;
  for (const [, value] of map) {
    visited += value;
  }
  console.log(`map ${map.size} ${map.get('k2999')} ${map.has('k3')} ${visited}`);
  const set = new Set<number>();
  for (let i = 0; i < 4000; i++) {
    set.add(mod(i, 2500));
  }
  console.log(`set ${set.size} ${set.has(2499)} ${set.has(2500)}`);
  const bytes = new Uint8Array(5000);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = mod(i, 251);
  }
  let check = 0;
  for (const byte of bytes) {
    check = mod(check * 31 + byte, 65521);
  }
  console.log(`bytes ${bytes.length} ${bytes[4999]} ${check}`);
  const text = 'é'.repeat(1500);
  const encoded = new TextEncoder().encode(text);
  const decoded = new TextDecoder().decode(encoded);
  console.log(`text ${encoded.length} ${decoded.length} ${decoded === text}`);
  const scores: Record<string, number> = {};
  for (let i = 0; i < 1100; i++) {
    scores[name('p', i)] = i;
  }
  for (let i = 0; i < 1100; i += 2) {
    delete scores[name('p', i)];
  }
  const names = Object.keys(scores);
  console.log(`dictionary ${names.length} ${names[0]} ${names[549]} ${scores['p1099']} ${'p4' in scores}`);
}
