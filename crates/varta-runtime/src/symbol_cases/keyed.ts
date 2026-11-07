// Symbol-keyed members (ECMA-262 §6.1.7: a property key is a String or a
// Symbol): records, interfaces, classes and member access keyed by a
// `unique symbol` constant. Symbol keys are no string keys: JSON
// skips them.
const BRAND: unique symbol = Symbol('brand');
const COUNT = Symbol('count');

interface Branded {
  name: string;
  [BRAND]: string;
}

class Counter {
  [COUNT] = 0;
  label = 'c';
  [BRAND](): string {
    return `counter ${this[COUNT]}`;
  }
  bump(): number {
    this[COUNT] += 1;
    return this[COUNT];
  }
}

export function main(): void {
  const item: Branded = { name: 'a', [BRAND]: 'yes' };
  console.log(item[BRAND]);
  item[BRAND] = 'changed';
  console.log(item[BRAND]);
  console.log(JSON.stringify(item));

  const counter = new Counter();
  counter.bump();
  console.log(String(counter.bump()));
  console.log(counter[BRAND]());
  console.log(JSON.stringify(counter));
  const literal = { [COUNT]: 5, plain: true };
  console.log(String(literal[COUNT] + 1));
  console.log(JSON.stringify(literal));
}
