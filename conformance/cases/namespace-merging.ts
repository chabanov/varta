namespace Shapes {
  export const unit = 'cm';
  export interface Box { width: number; height: number }
  export function area(box: Box): string { return String(box.width * box.height) + unit; }
  export namespace Presets { export const square: Box = { width: 2, height: 2 }; }
}
namespace Shapes {
  export function double(box: Box): Box { return { width: box.width * 2, height: box.height * 2 }; }
}
function tag(text: string): string { return tag.open + text + tag.close; }
namespace tag { export const open = '<'; export const close = '>'; }
export function main(): void {
  const box: Shapes.Box = Shapes.double(Shapes.Presets.square);
  console.log(Shapes.area(box) + ' ' + tag('b'));
}
