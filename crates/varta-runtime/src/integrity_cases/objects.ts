// Integrity levels of dictionaries, engine collections (whose internal
// slots stay writable), errors, promises, values held as `unknown`, and of
// the targets of `Object.assign`.
function attempt(label: string, action: () => void): void {
  try {
    action();
    console.log(label + ': ok');
  } catch (error) {
    console.log(label + ': ' + String(error));
  }
}

export function main(): void {
  const scores: { [name: string]: number } = {};
  scores['ann'] = 1;
  scores['bob'] = 2;
  console.log(String(Object.isFrozen(scores)) + ' ' + String(Object.isSealed(scores)));
  Object.seal(scores);
  console.log(String(Object.isFrozen(scores)) + ' ' + String(Object.isSealed(scores)));
  attempt('dictionary replace', () => {
    scores['ann'] = 3;
  });
  attempt('dictionary add', () => {
    scores['cy'] = 4;
  });
  attempt('dictionary delete', () => {
    delete scores['bob'];
  });
  attempt('dictionary delete absent', () => {
    delete scores['zed'];
  });
  Object.freeze(scores);
  attempt('dictionary frozen', () => {
    scores['ann'] = 5;
  });
  attempt('dictionary frozen add', () => {
    scores['5'] = 5;
  });
  console.log(JSON.stringify(scores) + ' ' + Object.keys(scores).join());
  const none: { [name: string]: string } = {};
  Object.preventExtensions(none);
  console.log(String(Object.isFrozen(none)) + ' ' + String(Object.isSealed(none)));

  const map = new Map<string, number>();
  Object.freeze(map);
  map.set('a', 1);
  map.delete('a');
  map.set('b', 2);
  const set = Object.freeze(new Set<number>([1]));
  set.add(2);
  console.log(String(map.size) + ' ' + String(set.size) + ' ' + String(Object.isFrozen(map)) + ' ' + String(Object.isFrozen(new Map<string, number>())));

  const error = new TypeError('bad');
  console.log(String(Object.isSealed(error)) + ' ' + String(Object.isFrozen(error)));
  Object.preventExtensions(error);
  console.log(String(Object.isSealed(error)) + ' ' + String(Object.isFrozen(error)) + ' ' + String(Object.isExtensible(error)));
  Object.seal(error);
  console.log(String(Object.isSealed(error)) + ' ' + String(Object.isFrozen(error)));
  const bare = Object.seal(new Error());
  console.log(String(Object.isFrozen(bare)) + ' ' + error.message);

  const promise = Promise.resolve(1);
  console.log(String(Object.isFrozen(promise)));
  Object.preventExtensions(promise);
  console.log(String(Object.isFrozen(promise)) + ' ' + String(Object.isSealed(promise)));

  const held: unknown = { n: 1 };
  console.log(String(Object.isExtensible(held)));
  Object.freeze(held);
  console.log(String(Object.isExtensible(held)));
  const primitive: unknown = 'text';
  console.log(String(Object.isExtensible(primitive)) + ' ' + String(Object.freeze(primitive)));
  const record = held as { n: number };
  attempt('narrowed', () => {
    record.n = 2;
  });
  const list: unknown = [1, 2];
  Object.seal(list);
  if (Array.isArray(list)) {
    attempt('narrowed array', () => {
      list.push(3);
    });
  }

  const target = Object.freeze({ a: 1, b: 2 });
  attempt('assign frozen target', () => {
    Object.assign(target, { a: 1 });
  });
  const partial = Object.preventExtensions({ a: 1, b: 2 });
  attempt('assign non-extensible target', () => {
    Object.assign(partial, { b: 3 });
  });
  console.log(JSON.stringify(partial));
  const copy = { ...target, a: 9 };
  copy.b = 8;
  console.log(JSON.stringify(copy) + ' ' + String(Object.isFrozen(copy)));
}
