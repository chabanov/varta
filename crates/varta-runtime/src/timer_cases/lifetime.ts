// Values reachable only from pending timers: callbacks, their captured
// strings and arrays, and the extra arguments, all built at run time after
// main's frame is gone.
function label(n: number): string {
  return "item-" + String(n);
}
function arm(prefix: string): void {
  const parts: string[] = [];
  for (let i = 0; i < 3; i++) {
    parts.push(label(i));
  }
  const joined = prefix + parts[0] + "/" + parts[2];
  setTimeout(
    (suffix: string, tail: string[]) => {
      console.log(joined + suffix + String(parts.length) + tail[0]);
    },
    10,
    "!" + label(9),
    [label(7) + "?"],
  );
  let id = 0;
  let n = 0;
  id = setInterval(() => {
    n = n + 1;
    console.log(label(n) + ":" + joined);
    if (n === 2) {
      clearInterval(id);
    }
  }, 3);
}
export function main(): void {
  arm("a:");
  arm("b:");
}
