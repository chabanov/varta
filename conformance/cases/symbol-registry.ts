// ECMA-262 §20.4.2.2 Symbol.for, §20.4.2.6 Symbol.keyFor and the
// well-known symbols (§6.1.5.1).
export function main(): void {
  const key = Symbol.for("app");
  console.log(`${String(key === Symbol.for("app"))} ${String(Symbol.keyFor(key))}`);
  console.log(`${Symbol.iterator.toString()} ${String(Symbol.keyFor(Symbol.iterator))}`);
  const names = new Map<symbol, string>([[key, "app"]]);
  console.log(String(names.get(Symbol.for("app"))));
}
