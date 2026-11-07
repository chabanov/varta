// Numeric, string, heterogeneous and const enums; reverse mapping; enum
// types in unions, narrowing and switch exhaustiveness.
enum Color { Red, Green, Blue = 10, Cyan }
enum Direction { Up = 'UP', Down = 'DOWN' }
enum Mixed { No = 0, Yes = 'YES', Later = 5, After }
const enum Flags { None = 0, A = 1 << 0, B = 1 << 1, AB = A | B, Neg = -1, Big = 2 ** 4, Mask = ~0 >>> 28 }
enum Computed { Base = 3, Twice = Base * 2, Text = 'ab'.length, Ref = Color.Blue + 1 }
enum Quoted { 'with space' = 1, plain = 2 }

function name(c: Color): string {
  switch (c) {
    case Color.Red: return 'red';
    case Color.Green: return 'green';
    case Color.Blue: return 'blue';
    case Color.Cyan: return 'cyan';
  }
}
function flip(d: Direction): Direction {
  return d === Direction.Up ? Direction.Down : Direction.Up;
}
function describe(value: Color | Direction): string {
  if (typeof value === 'string') return 'direction ' + value;
  return 'color ' + String(value);
}
class Pixel {
  constructor(public color: Color, public label: Direction = Direction.Up) {}
  show(): string { return name(this.color) + '/' + this.label; }
}
class Light {
  color = Color.Red;
  next(step = Color.Green): string { this.color = step; return name(this.color); }
}
export function main(): void {
  console.log(String(Color.Red) + ' ' + String(Color.Green) + ' ' + String(Color.Blue) + ' ' + String(Color.Cyan));
  console.log(Color[0] + ' ' + Color[Color.Cyan] + ' ' + Color[11]);
  for (let i = -1; i < 3; i++) {
    const found = Color[i];
    console.log(String(i) + ' -> ' + (found === undefined ? 'undefined' : found));
  }
  console.log(name(Color.Blue) + ' ' + flip(Direction.Down) + ' ' + describe(Direction.Up) + ' ' + describe(Color.Cyan));
  console.log(String(Mixed.No) + ' ' + Mixed.Yes + ' ' + String(Mixed.After) + ' ' + Mixed[6]);
  console.log(String(Flags.AB) + ' ' + String(Flags.Neg) + ' ' + String(Flags.Big) + ' ' + String(Flags.Mask));
  console.log(String(Computed.Twice) + ' ' + String(Computed.Text) + ' ' + String(Computed.Ref));
  console.log(String(Quoted['with space']) + ' ' + Quoted[2]);
  const colors: Color[] = [Color.Cyan, Color.Red];
  colors.push(Color.Green);
  console.log(colors.map((c: Color): string => name(c)).join(','));
  const pixel = new Pixel(Color.Green);
  pixel.color = Color.Blue;
  console.log(pixel.show());
  let current: Direction = Direction.Up;
  current = flip(current);
  console.log(current.toLowerCase() + ' ' + String(current === Direction.Down));
  let widened = Color.Red;
  widened = Color.Cyan;
  const light = new Light();
  console.log(name(widened) + ' ' + light.next() + ' ' + light.next(Color.Blue));
  const Color2 = 'shadowed';
  {
    const Color = { Red: 'local' };
    console.log(Color.Red + ' ' + Color2);
  }
}
