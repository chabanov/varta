// Interface method signatures (v37 `class_generics`) are function fields:
// object literal methods, arrows and async functions satisfy them, optional
// methods are called with `?.()`. `implements` checks class conformance at
// compile time only.
interface Body {
  text(): Promise<string>;
  size(scale: number): number;
  close?(): void;
  kind: string;
}
interface Named {
  name: string;
  greet(other: string): string;
  get?(): number;
}
class Person implements Named {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
  greet(other: string): string {
    return `${this.name} greets ${other}`;
  }
}
class Box implements Named, Body {
  readonly kind: string = 'box';
  get name(): string {
    return 'box';
  }
  greet(other: string): string {
    return `box ${other}`;
  }
  async text(): Promise<string> {
    return 'boxed';
  }
  size(scale: number): number {
    return scale * 2;
  }
}
function makeBody(data: string): Body {
  let closed = 0;
  return {
    kind: 'literal',
    async text(): Promise<string> {
      return data;
    },
    size(scale: number): number {
      return data.length * scale;
    },
    close(): void {
      closed++;
      console.log(`closed ${closed}`);
    },
  };
}
async function read(body: Body): Promise<string> {
  const text = await body.text();
  body.close?.();
  return `${body.kind} ${text} ${body.size(3)}`;
}
export async function main(): Promise<void> {
  const body = makeBody('abc');
  console.log(await read(body));
  console.log(await read(body));
  const plain: Body = { kind: 'arrow', text: async (): Promise<string> => 'x', size: (n: number): number => n };
  console.log(await read(plain));
  const p = new Person('ann');
  const b = new Box();
  console.log(`${p.greet('bo')} ${b.greet('cy')} ${b.name} ${await b.text()} ${b.size(4)}`);
  const n: Named = { name: 'lit', greet: (o: string): string => `hi ${o}` };
  console.log(`${n.greet('z')} ${n.get?.() ?? -1} ${typeof n.greet}`);
  console.log(JSON.stringify(makeBody('q')));
  const counter = {
    total: 0,
    add(x: number): number {
      return x + 1;
    },
  };
  console.log(`${counter.add(counter.total)} ${JSON.stringify(counter)}`);
}
