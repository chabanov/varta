function* counter(limit: number, trace: string[]): Generator<number, string, number> {
  let total = 0;
  try {
    for (let i = 0; i < limit; i++) {
      const step = yield total;
      total += step;
    }
  } finally {
    trace.push("finally " + String(total));
  }
  return "total " + String(total);
}
function* letters(trace: string[]): Generator<string> {
  try {
    yield "a";
    yield "b";
    yield "c";
  } finally {
    trace.push("closed");
  }
}
export function main(): void {
  const trace: string[] = [];
  const g = counter(3, trace);
  console.log(g.next(100).value);
  console.log(g.next(2).value);
  console.log(g.next(3).value);
  const last = g.next(4);
  console.log(String(last.done));
  console.log(last.value);
  console.log(String(g.next(5).done));
  let seen = "";
  for (const letter of letters(trace)) {
    if (letter === "c") {
      break;
    }
    seen += letter;
  }
  console.log(seen);
  const early = counter(5, trace);
  early.next(0);
  console.log(early.return("stopped").value);
  try {
    counter(1, trace).throw("not started");
  } catch (e) {
    console.log(typeof e === "string" ? e : "?");
  }
  console.log(trace.length);
  console.log(trace[0]);
  console.log(trace[1]);
  console.log(trace[2]);
}
