// ECMA-262 §20.4.1.1 Symbol(description), §20.4.3 Symbol.prototype and
// §25.5.2.2 SerializeJSONProperty (symbol values are skipped).
export function main(): void {
  const a = Symbol("a");
  const held: unknown = a;
  console.log(`${typeof a} ${String(a === Symbol("a"))} ${a.toString()} ${String(Symbol().description)}`);
  console.log(JSON.stringify({ x: 1, tag: held }) + JSON.stringify([held]));
  if (typeof held === "symbol") {
    console.log(String(held.description));
  }
}
