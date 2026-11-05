// A `Uint8Array` held by `unknown` (v38 `byte_values`): it stays the same
// object (writes through either reference are seen by both), `typeof`,
// `instanceof`, `ArrayBuffer.isView`, `Array.isArray`, truthiness, `String`
// and `JSON.stringify` answer as in Node, and it narrows back to the same
// bytes. Records, arrays, dictionaries and unions that reach one serialize
// its index keys. Comparisons with values that cannot hold a `Uint8Array`
// (primitives, records) answer as in Node; two values that may both hold
// one are not compared (a compile error, `sdk_path_tests`).
interface Upload {
  data: Uint8Array;
  size: number;
}

function kind(v: unknown): string {
  const parts: string[] = [typeof v];
  parts.push(String(v instanceof Uint8Array));
  parts.push(String(ArrayBuffer.isView(v)));
  parts.push(String(v instanceof ArrayBuffer));
  parts.push(String(v instanceof Blob));
  parts.push(String(Array.isArray(v)));
  parts.push(String(Boolean(v)));
  return parts.join(' ');
}

function bytesOf(v: unknown): string {
  if (v instanceof Uint8Array) {
    return `${String(v.length)}:${v.join('.')}`;
  }
  return 'not bytes';
}

function pick(flag: boolean): Uint8Array | string {
  return flag ? new Uint8Array([5, 6]) : 'text';
}

export function main(): void {
  const bytes = new Uint8Array([1, 2, 255]);
  const held: unknown = bytes;
  console.log(kind(held));
  console.log(kind(new Uint8Array(0)));
  console.log(kind(new ArrayBuffer(1)));
  console.log(kind([1, 2]));
  console.log(kind({ a: 1 }));
  console.log(String(held) + ' ' + String(new Uint8Array(0) as unknown) + '|');
  console.log(JSON.stringify(held));
  console.log(JSON.stringify(new Uint8Array(0) as unknown));
  console.log([held === undefined, held === null, held !== 'x', held === 1].map((b) => String(b)).join(' '));
  console.log(bytesOf(held) + ' ' + bytesOf([1, 2]) + ' ' + bytesOf('x'));
  if (held instanceof Uint8Array) {
    held[0] = 9;
  }
  console.log(String(bytes[0]));
  const back = held as Uint8Array;
  back[1] = 8;
  console.log(bytes.join(','));

  const upload: Upload = { data: bytes, size: 3 };
  const boxed: unknown = upload;
  const other: Upload = { data: bytes, size: 3 };
  console.log(String(boxed === upload) + ' ' + String(boxed === other));
  console.log(JSON.stringify(boxed));
  console.log(JSON.stringify(upload));
  console.log(JSON.stringify([bytes, new ArrayBuffer(2), new Uint8Array([0])]));
  const record: Record<string, unknown> = { a: bytes, b: 'x' };
  console.log(JSON.stringify(record));
  console.log(JSON.stringify(Object.values(record)));
  const entries = Object.entries(boxed as Record<string, unknown>);
  console.log(entries.map((entry) => `${entry[0]}=${bytesOf(entry[1])}`).join(' '));
  const union: unknown = pick(true);
  console.log(kind(union) + ' ' + JSON.stringify(union) + ' ' + JSON.stringify(pick(false) as unknown));
  const list: unknown[] = [bytes, 'x', 1];
  console.log(list.map((item) => typeof item + ':' + String(item instanceof Uint8Array)).join(' '));
  console.log(JSON.stringify(list));
}
