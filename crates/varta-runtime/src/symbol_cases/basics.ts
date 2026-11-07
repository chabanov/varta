// Symbol values (ECMA-262 §20.4): creation, descriptions, identity,
// conversions and the global registry.
export function main(): void {
  const a = Symbol('a');
  const b = Symbol('a');
  const none = Symbol();
  const empty = Symbol('');
  const fromNumber = Symbol(42);
  const optional: string | undefined = undefined;
  const fromOptional = Symbol(optional);
  console.log(typeof a);
  console.log(typeof Symbol);
  console.log(String(a === b));
  console.log(String(a === a));
  console.log(String(Object.is(a, a)));
  console.log(a.toString());
  console.log(String(none));
  console.log(String(empty));
  console.log(fromNumber.toString());
  console.log(String(a.description));
  console.log(String(none.description));
  console.log(String(empty.description === ''));
  console.log(String(fromOptional.description));
  console.log(String(a.valueOf() === a));

  const registered = Symbol.for('app.key');
  console.log(String(registered === Symbol.for('app.key')));
  console.log(String(Symbol.keyFor(registered)));
  console.log(String(Symbol.keyFor(a)));
  console.log(String(Symbol.for('a') === a));
  console.log(registered.toString());

  const iterator = Symbol.iterator;
  console.log(iterator.toString());
  console.log(String(iterator === Symbol.iterator));
  console.log(String(Symbol.keyFor(Symbol.iterator)));
  console.log(String(Symbol.asyncIterator.description));
  console.log(String(Symbol.dispose));
  console.log(String(Symbol.asyncDispose));
  console.log(String(Symbol.toPrimitive));
  console.log(String(Symbol.toStringTag));
  console.log(String(Symbol.hasInstance));
  console.log(String(Symbol.for('Symbol.iterator') === Symbol.iterator));

  try {
    console.log(String(Number(a)));
  } catch (error) {
    console.log(`${(error as TypeError).name}: ${(error as TypeError).message}`);
  }
}
