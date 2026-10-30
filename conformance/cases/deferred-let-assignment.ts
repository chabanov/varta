// A `let` declared without an initializer is definitely assigned on every
// path before its reads: both branches of an `if`, every clause of a `switch`
// with a default, and a `try` whose `catch` leaves the function.
function classify(n: number): string {
  let size: string;
  if (n > 10) size = "big";
  else size = "small";
  let parity: string;
  switch (n % 2) {
    case 0:
      parity = "even";
      break;
    default:
      parity = "odd";
  }
  let root: number;
  try {
    if (n < 0) throw new RangeError("negative");
    root = Math.sqrt(n);
  } catch (e) {
    return "error";
  }
  return size + " " + parity + " " + String(root);
}
export function main(): void {
  console.log(classify(16) + "|" + classify(9) + "|" + classify(-1));
}
