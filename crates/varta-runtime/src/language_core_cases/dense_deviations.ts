// Deviations: holes and out-of-range indices throw TypeError under v37.
function attempt(label: string, f: () => string): void {
  try {
    console.log(label + " " + f());
  } catch (e) {
    console.log(label + " " + (e instanceof TypeError ? "TypeError " + e.message : "other"));
  }
}
export function main(): void {
  const xs: number[] = [1, 2];
  attempt("oob", () => String(xs[5]));
  const holes: number[] = [1, , 3];
  attempt("hole", () => String(holes[1]));
  attempt("forof", () => {
    let s = "";
    for (const h of holes) s += String(h);
    return s;
  });
  attempt("string", () => "abc"[7]);
  attempt("at", () => String(xs.at(5)));
  const first = xs[0];
  console.log(String(first !== undefined));
  const controller = new AbortController();
  controller.signal.addEventListener("abort", () => console.log("listener"));
  controller.abort();
  console.log(String(controller.signal.aborted));
}
