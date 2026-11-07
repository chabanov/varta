const COUNT: unique symbol = Symbol('count');

class Money {
  readonly cents: number;
  constructor(cents: number) {
    this.cents = cents;
  }
  [COUNT] = 1;
  [Symbol.toPrimitive](hint: string): string | number {
    return hint === 'number' ? this.cents / 100 : `$${(this.cents / 100).toFixed(2)}`;
  }
}

export function main(): void {
  const price = new Money(1250);
  console.log(`${price} ${+price * 2} ${price[COUNT]}`); // $12.50 25 1
}
