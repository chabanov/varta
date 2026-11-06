function* naturals(): Generator<number, void, undefined> {
  let n = 1;
  for (;;) {
    yield n;
    n++;
  }
}
export function main(): void {
  console.log(naturals().filter((n) => n % 2 === 1).map((n) => n * n).take(4).toArray().join(","));
  console.log(naturals().drop(3).take(2).toArray().join(","));
  console.log(naturals().take(5).reduce((sum, n) => sum + n, 0));
  console.log(String(naturals().some((n) => n > 3)));
  console.log(String(naturals().take(3).every((n) => n < 3)));
  console.log(String(naturals().find((n) => n * n > 50)));
  console.log(["x", "y"].values().map((s, i) => `${i}${s}`).toArray().join(","));
  try {
    naturals().take(-1);
  } catch (error) {
    console.log(`${(error as RangeError).name}: ${(error as RangeError).message}`);
  }
}
