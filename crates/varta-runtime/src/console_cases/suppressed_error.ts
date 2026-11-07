class Disposal {
  name: string;
  constructor(name: string) { this.name = name; }
}
export function main(): void {
  const e = new SuppressedError(new Error("a"), "b", "msg");
  console.log(e);
  console.log(e.name, e.message, e.error, e.suppressed);
  console.log(e instanceof SuppressedError, e instanceof Error, e.message === "msg");
  const f = new SuppressedError(1, new Disposal("d"));
  console.log(f, JSON.stringify(f), f.toString(), f.message === "");
  try {
    throw new SuppressedError(new TypeError("body"), new RangeError("dispose"), "both failed");
  } catch (caught) {
    if (caught instanceof SuppressedError) {
      const inner = caught.error;
      console.log(caught.message, inner instanceof TypeError, caught.suppressed instanceof RangeError);
    }
  }
  console.log({ wrapped: new SuppressedError(null, undefined, "x") }, typeof SuppressedError);
}
