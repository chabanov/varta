// Namespace objects as values: their exported functions, constants, enum
// objects and nested namespace objects, in declaration order.
namespace Geo {
  export const scale = 2;
  export function double(x: number): number { return x * scale; }
  export interface Point { x: number }
  export enum Axis { X = 'x', Y = 'y' }
  export namespace Inner { export const depth = 1; }
  const hidden = 0;
  export const unit: string = 'cm' + String(hidden);
}
function apply(tools: typeof Geo, value: number): string {
  return String(tools.double(value)) + tools.unit;
}
export function main(): void {
  console.log(JSON.stringify(Object.keys(Geo)));
  console.log(apply(Geo, 4) + ' ' + String(Geo.Inner.depth));
  const inner = Geo.Inner;
  console.log(JSON.stringify(inner) + ' ' + JSON.stringify(Geo.Axis));
}
