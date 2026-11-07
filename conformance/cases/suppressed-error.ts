// SuppressedError (ECMA-262 2026 §20.5.8, explicit resource management),
// checked profile v38: constructor, fields, prototype chain and catch
// narrowing.
export function main(): void {
  try {
    throw new SuppressedError(new TypeError("body"), new RangeError("dispose"), "both failed");
  } catch (caught) {
    if (caught instanceof SuppressedError) {
      console.log(`${caught.name}: ${caught.message}`);
      console.log(String(caught instanceof Error), String(caught.error instanceof TypeError));
      console.log(String(caught.suppressed instanceof RangeError));
    }
  }
  const plain = new SuppressedError(1, 2);
  console.log(JSON.stringify(plain), plain.message === "", typeof SuppressedError);
}
