// A cancellable delay: the timer callback names the abort listener that is
// declared after it (a capture before initialization), and the listener
// clears the timer it captures.
function delay(ms: number, label: string, signal?: AbortSignal): Promise<string> {
  return new Promise((resolve, reject) => {
    if (signal !== undefined && signal.aborted) {
      reject(signal.reason);
      return;
    }
    const handle = setTimeout(() => {
      if (signal !== undefined) signal.removeEventListener("abort", stop);
      resolve(label);
    }, ms);
    const stop = () => {
      clearTimeout(handle);
      reject(signal?.reason);
    };
    signal?.addEventListener("abort", stop);
  });
}
export async function main(): Promise<void> {
  console.log(await delay(5, "first"));
  const controller = new AbortController();
  const pending = delay(1000, "never", controller.signal);
  controller.abort("cancelled");
  try {
    console.log(await pending);
  } catch (e) {
    console.log("rejected " + String(e));
  }
  const done = new AbortController();
  done.abort("already");
  try {
    await delay(1, "late", done.signal);
  } catch (e) {
    console.log("rejected " + String(e));
  }
  console.log(await delay(2, "after", new AbortController().signal));
}
