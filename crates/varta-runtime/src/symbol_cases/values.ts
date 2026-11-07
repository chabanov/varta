// Symbols as values: record fields, array elements, unions with typeof
// narrowing, unknown cells, Map and Set keys, and JSON (§25.5.2.2
// SerializeJSONProperty skips symbol values: omitted in objects, null in
// arrays, undefined at the top level).
interface Tagged {
  name: string;
  tag: symbol;
}

function describe(value: string | symbol): string {
  if (typeof value === 'symbol') {
    return `symbol ${String(value.description)}`;
  }
  return `string ${value}`;
}

function kind(value: unknown): string {
  if (typeof value === 'symbol') {
    return `unknown symbol ${value.toString()}`;
  }
  return `unknown ${typeof value}`;
}

export function main(): void {
  const red = Symbol('red');
  const green = Symbol('green');
  const tagged: Tagged = { name: 'apple', tag: red };
  console.log(String(tagged.tag === red));
  console.log(JSON.stringify(tagged));
  const list: symbol[] = [red, green, red];
  console.log(String(list.length));
  console.log(String(list.indexOf(green)));
  console.log(String(list.includes(red)));
  console.log(String(list[2] === red));
  console.log(list.map((s) => s.toString()).join(','));
  const mixed: (string | symbol)[] = ['a', green];
  console.log(mixed.map(describe).join('; '));
  console.log(describe(red));
  console.log(describe('plain'));

  const held: unknown = red;
  console.log(kind(held));
  console.log(kind(1));
  console.log(String(held));
  console.log(JSON.stringify({ a: 1, b: held, c: 'x' }));
  console.log(JSON.stringify([1, held, 'x']));
  console.log(held);
  console.log(String(JSON.stringify(red)));
  const maybe: unknown = { a: [held, 1] };
  console.log(String(JSON.stringify(maybe, null, 2)));

  const names = new Map<symbol, string>();
  names.set(red, 'red');
  names.set(green, 'green');
  names.set(Symbol('red'), 'other red');
  console.log(String(names.size));
  console.log(String(names.get(red)));
  console.log(String(names.has(Symbol('green'))));
  const seen = new Set<symbol>([red, red, green]);
  console.log(String(seen.size));
  console.log(String(seen.has(green)));

  const record: Record<string, symbol> = { first: red };
  record['second'] = green;
  console.log(JSON.stringify(record));
  console.log(String(record['second'] === green));
  console.log(String(Boolean(red)));
  console.log(String(!red));
}
