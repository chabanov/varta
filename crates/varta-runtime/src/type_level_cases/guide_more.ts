// The language guide's step-2 type-level example.
type UserId = string & { readonly __brand: 'UserId' };
type Parse = { (text: string): number; (value: number): string };
type Identity = <T>(value: T) => T;
class Shape { area(): number { return 0; } }
class Square extends Shape { side = 2; area(): number { return this.side ** 2; } }

export function main(): void {
  const id = 'u1' as UserId;
  const parse = ((v: string | number) => (typeof v === 'string' ? v.length : String(v))) as Parse;
  const same: Identity = (value) => value;
  const squares: Square[] = [new Square()];
  const shapes: Shape[] = squares;          // a copy: Varta never aliases a wider view
  const point = { x: 1, y: 2 };
  const next: typeof point = { x: 3, y: 4 };
  console.log(`${id} ${parse('abc') + 1} ${same(5)} ${shapes[0].area()} ${next.x}`);
}
