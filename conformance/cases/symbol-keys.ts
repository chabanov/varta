// ECMA-262 §10.1.11.1 OrdinaryOwnPropertyKeys: string-key forms skip
// symbol-keyed members, §20.1.2.11 Object.getOwnPropertySymbols lists them
// and §7.3.25 CopyDataProperties copies them.
const TAG = Symbol("tag");

interface Tagged {
  [TAG]: string;
  name: string;
}

export function main(): void {
  const tagged: Tagged = { [TAG]: "t", name: "n" };
  const copy: Tagged = { ...tagged };
  const symbols = Object.getOwnPropertySymbols(copy);
  console.log(`${Object.keys(tagged).join()} ${symbols.length} ${String(symbols[0] === TAG)} ${copy[TAG]}`);
}
