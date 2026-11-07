// ECMA-262 §20.1.2.6 Object.freeze, §20.1.2.20 Object.seal,
// §20.1.2.18 Object.preventExtensions and their predicates; a failed
// [[Set]] or [[Delete]] throws a TypeError in strict code (§7.3.4 Set,
// §7.3.9 DeletePropertyOrThrow).
interface Point {
  x: number;
  y?: number;
}

export function main(): void {
  const point: Point = { x: 1 };
  Object.seal(point);
  point.x = 2;
  try {
    point.y = 3;
  } catch (error) {
    console.log(String(error));
  }
  const frozen = Object.freeze({ a: 1 });
  try {
    frozen.a = 2;
  } catch (error) {
    console.log(String(error));
  }
  console.log(`${Object.isSealed(point)} ${Object.isFrozen(point)} ${Object.isFrozen(frozen)} ${Object.isExtensible({})}`);
  console.log(JSON.stringify(point) + JSON.stringify(frozen));
}
