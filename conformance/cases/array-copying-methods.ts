export function main(): void {
  const base = [1, 2, 3, 4, 5];
  console.log(base.toReversed().join(","));
  console.log(base.with(-1, 9).join(","));
  console.log(base.toSpliced(1, 2, 7).join(","));
  console.log(base.toSpliced(2).join(","));
  console.log(base.join(","));
  try {
    base.with(5, 0);
  } catch (error) {
    console.log((error as RangeError).message);
  }
  console.log(Array.of(7, 8).join(","));
  console.log(Array.from({ length: 3 }, (_, index) => index * 2).join(","));
  console.log(Array.from([1, 2], (value) => value + 0.5).join(","));
}
