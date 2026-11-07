const RED: unique symbol = Symbol('red');

function describe(value: string | symbol): string {
  return typeof value === 'symbol' ? `symbol ${String(value.description)}` : value;
}

export function main(): void {
  const names = new Map<symbol, string>([[RED, 'red']]);
  console.log(describe(RED)); // symbol red
  console.log(String(names.get(RED))); // red
  console.log(String(Symbol.for('app') === Symbol.for('app'))); // true
  console.log(Symbol.iterator.toString()); // Symbol(Symbol.iterator)
  console.log(JSON.stringify({ tag: RED, name: 'x' })); // {"name":"x"}
}
