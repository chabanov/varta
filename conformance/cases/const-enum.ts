const enum Axis { X = 'x', Y = 'y' }
const enum Size { Small = 2, Large = Small * 8, Mask = ~Large & 0xff }
interface Point { axis: Axis; length: number }
function flip(point: Point): Point {
  return { axis: point.axis === Axis.X ? Axis.Y : Axis.X, length: point.length };
}
export function main(): void {
  const point: Point = { axis: Axis.X, length: Size.Large };
  console.log(flip(point).axis + ' ' + String(point.length) + ' ' + String(Size.Mask));
}
