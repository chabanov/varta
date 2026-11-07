// Integrity levels of arrays (ECMA-262 §10.4.2 array exotic objects,
// §23.1.3): element and length writes, the mutating methods step by step,
// and holes of sparse arrays.
function attempt(label: string, action: () => void): void {
  try {
    action();
    console.log(label + ': ok');
  } catch (error) {
    console.log(label + ': ' + String(error));
  }
}

function show(values: number[]): string {
  const parts: string[] = [];
  for (let index = 0; index < values.length; index++) {
    parts.push(Object.hasOwn(values, index) ? String(values[index]) : '_');
  }
  return '[' + parts.join(',') + '] ' + String(values.length);
}

export function main(): void {
  const frozen = Object.freeze([1, 2, 3]);
  console.log(String(Object.isFrozen(frozen)) + ' ' + String(Object.isSealed(frozen)) + ' ' + String(Object.isExtensible(frozen)));
  attempt('element', () => {
    frozen[0] = 5;
  });
  attempt('append', () => {
    frozen[5] = 5;
  });
  attempt('push', () => {
    frozen.push(4);
  });
  attempt('push none', () => {
    frozen.push();
  });
  attempt('pop', () => {
    frozen.pop();
  });
  attempt('length', () => {
    frozen.length = 0;
  });
  attempt('reverse', () => {
    frozen.reverse();
  });
  attempt('fill', () => {
    frozen.fill(0);
  });
  attempt('fill empty range', () => {
    frozen.fill(0, 3);
  });
  attempt('sort', () => {
    frozen.sort((a, b) => b - a);
  });
  attempt('splice', () => {
    frozen.splice(0, 1);
  });
  attempt('shift', () => {
    frozen.shift();
  });
  attempt('unshift', () => {
    frozen.unshift(0);
  });
  attempt('copyWithin', () => {
    frozen.copyWithin(0, 1);
  });
  attempt('delete', () => {
    delete frozen[0];
  });
  console.log(show(frozen));

  const empty = Object.freeze([] as number[]);
  attempt('pop empty', () => {
    empty.pop();
  });
  attempt('reverse empty', () => {
    empty.reverse();
  });

  const sealed = Object.seal([1, 2, 3]);
  console.log(String(Object.isFrozen(sealed)) + ' ' + String(Object.isSealed(sealed)));
  attempt('sealed element', () => {
    sealed[0] = 5;
  });
  attempt('sealed push', () => {
    sealed.push(4);
  });
  attempt('sealed pop', () => {
    sealed.pop();
  });
  attempt('sealed shrink', () => {
    sealed.length = 1;
  });
  console.log(show(sealed));
  attempt('sealed grow', () => {
    sealed.length = 5;
  });
  console.log(show(sealed));
  attempt('sealed reverse', () => {
    sealed.reverse();
  });
  console.log(show(sealed));
  attempt('sealed shift', () => {
    sealed.shift();
  });
  console.log(show(sealed));

  const bounded = Object.preventExtensions([1, 2, 3]);
  console.log(String(Object.isFrozen(bounded)) + ' ' + String(Object.isSealed(bounded)) + ' ' + String(Object.isExtensible(bounded)));
  attempt('bounded push', () => {
    bounded.push(4);
  });
  attempt('bounded pop', () => {
    console.log(String(bounded.pop()));
  });
  attempt('bounded unshift', () => {
    bounded.unshift(0);
  });
  console.log(show(bounded));
  attempt('bounded shrink', () => {
    bounded.length = 0;
  });
  console.log(show(bounded));
  console.log(String(Object.isSealed(bounded)));

  const sparse = [1, , 3];
  Object.preventExtensions(sparse);
  attempt('fill hole', () => {
    sparse[1] = 2;
  });
  attempt('sparse reverse', () => {
    sparse.reverse();
  });
  console.log(show(sparse as number[]));
  const holes = [, , 3];
  Object.seal(holes);
  attempt('sealed sparse reverse', () => {
    holes.reverse();
  });
  console.log(show(holes as number[]));

  const tuple: [number, string] = [1, 'a'];
  console.log(String(Object.isFrozen(tuple)));
  Object.freeze(tuple);
  console.log(String(Object.isFrozen(tuple)) + ' ' + tuple[1]);
}
