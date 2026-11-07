interface Loop {
  a: number;
  readonly self: Loop;
}
interface Holder {
  z: Wrapper | null;
}
interface Wrapper {
  x: { y: Holder[] };
  readonly q: number;
}
interface Deep {
  readonly a: { b: { c: { d: { e: { f: Deep } } } } };
}
interface Shared {
  readonly n: number;
}
function report(error: unknown): void {
  if (error instanceof Error) {
    console.log(error.name + ": " + error.message);
  }
}
export function main(): void {
  const loop: Loop = {
    a: 1,
    get self(): Loop {
      return loop;
    },
  };
  try {
    console.log(JSON.stringify(loop));
  } catch (error) {
    report(error);
  }
  const holder: Holder = { z: null };
  const wrapper: Wrapper = {
    x: { y: [holder] },
    get q(): number {
      return 1;
    },
  };
  holder.z = wrapper;
  try {
    console.log(JSON.stringify(wrapper));
  } catch (error) {
    report(error);
  }
  const deep: Deep = {
    get a(): { b: { c: { d: { e: { f: Deep } } } } } {
      return { b: { c: { d: { e: { f: deep } } } } };
    },
  };
  try {
    console.log(JSON.stringify(deep));
  } catch (error) {
    report(error);
  }
  // A repeated object that is not an ancestor is no cycle.
  const shared: Shared = {
    get n(): number {
      return 7;
    },
  };
  console.log(JSON.stringify({ a: shared, b: { inner: shared }, c: [shared] }));
}
