function* squares(limit: number): Generator<number> {
  for (let i = 1; i <= limit; i++) {
    yield i * i;
  }
}
class Pair implements Iterable<string> {
  *[Symbol.iterator](): Generator<string> {
    yield "x";
    yield "y";
  }
}
export function main(): void {
  const head: number[] = [1, 2];
  const all = [0, ...head, ...squares(3), ...new Set<number>([7, 7, 8])];
  console.log(all.join(","));
  const mixed = [...new Pair(), ...head];
  console.log(mixed.join(""));
  console.log(typeof mixed[0] + " " + typeof mixed[2]);
  const copy = Array.from(head);
  copy.push(3);
  console.log(String(head.length) + " " + String(copy.length));
  console.log(Array.from(squares(2)).join("|"));
  const maybe: (number | undefined)[] = [1, undefined];
  console.log(String([...maybe][1]));
}
