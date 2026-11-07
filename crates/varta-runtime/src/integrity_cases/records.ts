// Integrity levels of records and class instances (ECMA-262 §7.3.15,
// §7.3.16, §20.1.2): freeze, seal and preventExtensions, their predicates,
// and the TypeError of each failed write, addition and deletion.
function attempt(label: string, action: () => void): void {
  try {
    action();
    console.log(label + ': ok');
  } catch (error) {
    console.log(label + ': ' + String(error));
  }
}

interface Point {
  x: number;
  y?: number;
}

class Base {
  value = 1;
  label: string;
  #secret = 2;
  constructor(label: string) {
    this.label = label;
  }
  bump(): number {
    this.#secret++;
    return this.#secret;
  }
}

class Derived extends Base {
  extra = 3;
}

export function main(): void {
  const plain = { a: 1, b: 'two' };
  console.log(String(Object.isFrozen(plain)) + ' ' + String(Object.isSealed(plain)) + ' ' + String(Object.isExtensible(plain)));
  const frozen = Object.freeze(plain);
  console.log(String(frozen === plain));
  console.log(String(Object.isFrozen(plain)) + ' ' + String(Object.isSealed(plain)) + ' ' + String(Object.isExtensible(plain)));
  attempt('assign frozen', () => {
    plain.a = 5;
  });
  attempt('update frozen', () => {
    plain.a++;
  });
  attempt('compound frozen', () => {
    plain.b += '!';
  });
  console.log(JSON.stringify(plain));

  const point: Point = { x: 1 };
  Object.seal(point);
  console.log(String(Object.isSealed(point)) + ' ' + String(Object.isFrozen(point)));
  attempt('assign sealed', () => {
    point.x = 2;
  });
  attempt('add sealed', () => {
    point.y = 3;
  });
  attempt('delete sealed', () => {
    delete point.y;
  });
  console.log(JSON.stringify(point));

  const open: Point = { x: 1, y: 2 };
  Object.preventExtensions(open);
  console.log(String(Object.isSealed(open)) + ' ' + String(Object.isFrozen(open)) + ' ' + String(Object.isExtensible(open)));
  attempt('delete non-extensible', () => {
    delete open.y;
  });
  attempt('re-add non-extensible', () => {
    open.y = 4;
  });
  attempt('assign non-extensible', () => {
    open.x = 9;
  });
  console.log(JSON.stringify(open));

  const lone: Point = { x: 1, y: 2 };
  Object.preventExtensions(lone);
  delete lone.y;
  console.log(String(Object.isSealed(lone)) + ' ' + String(Object.isFrozen(lone)));

  const empty = {};
  Object.preventExtensions(empty);
  console.log(String(Object.isSealed(empty)) + ' ' + String(Object.isFrozen(empty)));

  const instance = new Derived('d');
  Object.freeze(instance);
  attempt('assign instance', () => {
    instance.extra = 4;
  });
  attempt('assign inherited field', () => {
    instance.value = 4;
  });
  attempt('private field', () => {
    console.log(String(instance.bump()));
  });
  const base: Base = instance;
  attempt('assign through base', () => {
    base.label = 'x';
  });
  const other = Object.seal(new Base('b'));
  attempt('assign sealed instance', () => {
    other.label = 'c';
  });
  console.log(other.label + ' ' + String(Object.isSealed(other)) + ' ' + String(Object.isFrozen(other)));

  console.log(String(Object.isFrozen(1)) + ' ' + String(Object.isSealed('s')) + ' ' + String(Object.isExtensible(true)));
  console.log(String(Object.freeze(7)) + ' ' + Object.seal('text'));

  const nested = Object.freeze({ inner: { n: 1 } });
  nested.inner.n = 2;
  console.log(String(nested.inner.n) + ' ' + String(Object.isFrozen(nested.inner)));
}
