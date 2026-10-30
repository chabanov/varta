// `switch` selects by strict equality over strings, evaluating case
// expressions in order until one matches; clauses fall through until `break`
// and `default` is chosen only when no case matches, wherever it appears.
function code(name: string): number {
  let result = 0;
  switch (name) {
    default:
      result = -1;
      break;
    case "alpha":
      result = 1;
    case "beta":
      result += 10;
      break;
    case "gamma":
      result = 3;
  }
  return result;
}
function kind(x: number | string): string {
  switch (typeof x) {
    case "number":
      return "n";
    case "string":
      return "s";
  }
  return "?";
}
export function main(): void {
  console.log(String(code("alpha")) + " " + String(code("beta")) + " " + String(code("gamma")) + " " + String(code("delta")));
  console.log(kind(1) + kind("a"));
}
