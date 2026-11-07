// Generic construct signatures (K9 A step 4): a class value of a generic
// constructor type constructs every instantiation the program asks for.
class Box<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
  describe(): string {
    return `Box(${String(this.value)})`;
  }
}
class LabeledBox<T> extends Box<T> {
  label: string;
  constructor(value: T, label: string = "item") {
    super(value);
    this.label = label;
  }
  tagged(): string {
    return `${this.label}:${String(this.value)}`;
  }
}
class Pair<A, B> {
  first: A;
  second: B;
  constructor(first: A, second: B) {
    this.first = first;
    this.second = second;
  }
}
interface BoxConstructor {
  new <T>(value: T): Box<T>;
}
type PairConstructor = new <A, B>(first: A, second: B) => Pair<A, B>;

function fill(make: BoxConstructor): string {
  const count = new make(3);
  const name = new make("ada");
  const flag = new make<boolean>(true);
  const doubled: number = count.value * 2;
  return `${count.describe()} ${name.describe()} ${flag.value} ${doubled}`;
}

function swap<A, B>(make: PairConstructor, pair: Pair<A, B>): Pair<B, A> {
  return new make(pair.second, pair.first);
}

function both(first: BoxConstructor, second: BoxConstructor): boolean {
  return first === second;
}

export function main(): void {
  console.log(fill(Box));
  console.log(fill(LabeledBox));
  const pair = swap<number, string>(Pair, new Pair(1, "one"));
  console.log(`${pair.first} ${pair.second}`);
  const again = swap<string, number>(Pair, pair);
  console.log(`${again.first + 1} ${again.second.length}`);
  const make: BoxConstructor = Box;
  const makePair: PairConstructor = Pair;
  const list = new makePair([1, 2, 3], new make("x"));
  console.log(`${list.first.length} ${list.second.value} ${both(Box, Box)} ${both(Box, LabeledBox)} ${typeof make}`);
}
