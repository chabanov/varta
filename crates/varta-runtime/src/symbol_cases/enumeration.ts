// String-key enumeration of records with symbol-keyed members skips them
// (ECMA-262 §20.1.2.18 Object.keys, §14.7.5.9 for-in, §10.1.11.1
// OrdinaryOwnPropertyKeys); Object.getOwnPropertySymbols lists them in
// insertion order; spreads and Object.assign copy them (§7.3.25
// CopyDataProperties); JSON skips them.
const FIRST = Symbol('first');
const SECOND = Symbol('second');

interface Tagged {
  [SECOND]?: number;
  [FIRST]: string;
  name: string;
  size?: number;
}

export function main(): void {
  const tagged: Tagged = { name: 'a', [FIRST]: 'one', size: 2 };
  tagged[SECOND] = 2;
  console.log(Object.keys(tagged).join() + ' ' + Object.getOwnPropertyNames(tagged).join());
  console.log(Object.values(tagged).join());
  console.log(Object.entries(tagged).map(([key, value]) => key + '=' + String(value)).join());
  const keys: string[] = [];
  for (const key in tagged) {
    keys.push(key);
  }
  console.log(keys.join());
  const symbols = Object.getOwnPropertySymbols(tagged);
  console.log(String(symbols.length) + ' ' + symbols.map((symbol) => String(symbol.description)).join());
  delete tagged[SECOND];
  console.log(String(Object.getOwnPropertySymbols(tagged).length));
  const copy: Tagged = { ...tagged };
  console.log(copy[FIRST] + ' ' + JSON.stringify(copy));
  const target: Tagged = { name: 'b', [FIRST]: 'none' };
  Object.assign(target, tagged);
  console.log(target[FIRST] + ' ' + target.name);
  const none = Object.getOwnPropertySymbols({ plain: 1 });
  console.log(String(none.length) + ' ' + String(Object.getOwnPropertySymbols([1, 2]).length));
}
