// Enum objects as values: own keys in JavaScript order, values, entries,
// passing the object and its `typeof` type.
enum Color { Red, Green, Blue = 10 }
enum Direction { Up = 'UP', Down = 'DOWN' }
enum Mixed { No = 0, Yes = 'YES' }
function names(e: typeof Direction): string { return e.Up + '/' + e.Down; }
export function main(): void {
  console.log(JSON.stringify(Object.keys(Color)));
  console.log(JSON.stringify(Object.values(Direction)));
  console.log(JSON.stringify(Object.entries(Mixed)));
  console.log(names(Direction));
  const palette = Color;
  console.log(String(palette.Blue) + ' ' + palette[0]);
  console.log(JSON.stringify(Color));
  const key: keyof typeof Direction = 'Down';
  console.log(Direction[key]);
}
