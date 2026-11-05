// `instanceof Map`/`Set`/`Uint8Array` of closed unions, as uarp-sdk 0.13.2's
// byte inputs (core/json.ts BinaryInput) are told apart.
type Bytes = Blob | ArrayBuffer | Uint8Array | string;
function kind(value: Bytes): string {
  if (value instanceof Uint8Array) return 'u8 ' + String(value.length);
  if (value instanceof Blob) return 'blob ' + String(value.size);
  if (value instanceof ArrayBuffer) return 'buffer ' + String(value.byteLength);
  return 'string ' + value;
}
function negated(value: Uint8Array | string): string {
  if (!(value instanceof Uint8Array)) return value.toUpperCase();
  return String(value[0]);
}
function map(value: Map<string, number> | string | undefined): string {
  if (value instanceof Map) return 'map ' + String(value.size);
  return value ?? 'none';
}
function set(value: Set<number> | number): string {
  return value instanceof Set ? 'set ' + String(value.has(2)) : 'number ' + String(value);
}
export function main(): void {
  console.log([kind(new Uint8Array([1, 2])), kind(new Blob(['abc'])), kind(new ArrayBuffer(4)), kind('s')].join(', '));
  console.log(negated('x') + ' ' + negated(new Uint8Array([7])));
  console.log(map(new Map<string, number>([['a', 1]])) + ', ' + map('s') + ', ' + map(undefined));
  console.log(set(new Set<number>([2])) + ', ' + set(3));
  const bytes = new Uint8Array(1);
  const text = 'abc';
  console.log(String(bytes instanceof Uint8Array) + ' ' + String(text instanceof Map) + ' ' + String(bytes instanceof Set));
}
