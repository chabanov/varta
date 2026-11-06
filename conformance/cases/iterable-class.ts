class Countdown implements Iterable<number> {
  constructor(private from: number) {}
  *[Symbol.iterator](): Generator<number> {
    for (let i = this.from; i > 0; i--) {
      yield i;
    }
  }
}
class EvenCountdown extends Countdown {
  *[Symbol.iterator](): Generator<number> {
    for (const n of new Countdown(6)) {
      if (n % 2 === 0) {
        yield n;
      }
    }
  }
}
function sum(values: Countdown): number {
  let total = 0;
  for (const n of values) {
    total += n;
  }
  return total;
}
export function main(): void {
  console.log(String(sum(new Countdown(4))));
  console.log(String(sum(new EvenCountdown(0))));
  const it = new Countdown(2)[Symbol.iterator]();
  console.log(String(it.next().value));
  console.log(String(it.next().value));
  console.log(String(it.next().done));
}
