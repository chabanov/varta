// A v35 program over function values, closures, unions and optional
// parameters, whose v35 bytes `function_field_tests` freezes.
function make(tag: string): (n: number) => string {
  let count = 0;
  return (n: number): string => {
    count += n;
    return `${tag}${count}`;
  };
}
function apply(f: (n: number) => string, n: number, suffix?: string): string {
  return f(n) + (suffix ?? '');
}
export function main(): void {
  const a = make('a');
  const b = make('b');
  const same = a === a;
  const text: string | number = same ? apply(a, 1, '!') : 0;
  console.log(`${text} ${apply(b, 2)} ${apply(a, 3)} ${typeof a}`);
  const xs = [1, 2, 3].map((x: number): number => x * 2);
  console.log(JSON.stringify(xs));
}
