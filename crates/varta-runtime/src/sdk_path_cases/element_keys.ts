// A string key of an array is no element read: `xs['length']` is the
// length, so a switch over it is not exhaustive over the element type and
// falls through to the trailing return. A number key `xs[i]` is an element
// read whose cases cover the element type.
function lengthName(xs: (1 | 2)[]): string {
  switch (xs['length']) {
    case 1:
      return 'one';
    case 2:
      return 'two';
  }
  return 'other';
}
function elementName(xs: (1 | 2)[], at: number): string {
  switch (xs[at]) {
    case 1:
      return 'one';
    case 2:
      return 'two';
  }
}
export function main(): void {
  console.log(lengthName([1, 1, 1]));
  console.log(lengthName([1]));
  console.log(lengthName([2, 2]));
  console.log(elementName([2, 1], 0) + ' ' + elementName([2, 1], 1));
}
