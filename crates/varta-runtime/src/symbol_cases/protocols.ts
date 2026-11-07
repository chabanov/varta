// The protocols keyed by well-known symbols (ECMA-262 §7.1.1 ToPrimitive,
// §20.1.3.6 Object.prototype.toString, §13.10.2 InstanceofOperator).
class Money {
  readonly cents: number;
  constructor(cents: number) {
    this.cents = cents;
  }
  [Symbol.toPrimitive](hint: string): string | number {
    return hint === 'number' ? this.cents / 100 : `$${(this.cents / 100).toFixed(2)}`;
  }
}

class Tagged {
  get [Symbol.toStringTag](): string {
    return 'Tagged';
  }
}

class Even {
  static [Symbol.hasInstance](value: unknown): boolean {
    return typeof value === 'number' && value % 2 === 0;
  }
}

export function main(): void {
  const price = new Money(1250);
  console.log(String(price));
  console.log(`${price}`);
  console.log(String(Number(price)));
  console.log(String(+price * 2));
  const tagged = new Tagged();
  console.log(String(tagged));
  console.log(`${tagged}`);
  const four: unknown = 4;
  const three: unknown = 3;
  console.log(String(four instanceof Even));
  console.log(String(three instanceof Even));
}
