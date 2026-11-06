// The language guide's type-level example.
interface User { readonly id: number; name: string; email?: string }
type Getters<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };
type ElementOf<T> = T extends readonly (infer E)[] ? E : never;
type Reverse<T extends unknown[]> = T extends [infer H, ...infer R] ? [...Reverse<R>, H] : [];
const defaults = { width: 640, height: 480 };

function isString(value: string | number): value is string {
  return typeof value === 'string';
}
function assertDefined(value: number | undefined): asserts value is number {
  if (value === undefined) throw new TypeError('missing');
}
class Builder {
  parts: string[] = [];
  add(part: string): this { this.parts.push(part); return this; }
}
class Html extends Builder {
  render(): string { return this.parts.join(''); }
}

export function main(): void {
  const user: User = { id: 1, name: 'Ada' };
  const get: Getters<User> = { getId: () => user.id, getName: () => user.name };
  const first: ElementOf<string[]> = 'x';
  const reversed: Reverse<[1, 2, 3]> = [3, 2, 1];
  const size: typeof defaults = { width: 800, height: 600 };
  const picked: Pick<User, 'name'> = { name: get.getName() };
  const mode = 'dark';                       // type 'dark'
  let copy = mode;                           // widened to string
  copy = 'light';
  const config = { retries: 3 } satisfies Record<string, number>;
  const value: string | number = 'v';
  if (isString(value)) console.log(value.toUpperCase());
  const maybe: number | undefined = config.retries;
  assertDefined(maybe);
  console.log(`${first} ${reversed.join()} ${size.width} ${picked.name} ${copy} ${maybe + 1}`);
  console.log(new Html().add('<b>').add('</b>').render());
}
