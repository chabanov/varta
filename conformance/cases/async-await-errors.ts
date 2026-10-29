// An async function runs synchronously until its first await, try/catch/
// finally hold across awaits, a rejection is thrown at the await and an
// uncaught exception rejects the function's promise.
async function fail(text: string): Promise<number> {
  await null;
  throw new Error(text);
}
async function guarded(): Promise<string> {
  console.log("guarded start");
  try {
    const value = await fail("inner");
    return "unreachable " + String(value);
  } catch (error) {
    if (error instanceof Error) {
      console.log("caught " + error.message);
    }
    return "recovered";
  } finally {
    await 1;
    console.log("finally after await");
  }
}
export async function main(): Promise<void> {
  const pending = guarded();
  console.log("main continues");
  const text = await pending;
  console.log(text);
  try {
    await fail("outer");
  } catch (error) {
    if (error instanceof Error) {
      console.log("main caught " + error.message);
    }
  }
  const total = (await Promise.resolve(2)) + (await 3);
  console.log("total " + String(total));
}
