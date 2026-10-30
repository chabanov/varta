// The language guide's abortable sleep with a deadline (docs/language-guide.md,
// Timers (opt-in v36)).
function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    if (signal?.aborted) {
      reject(signal.reason);
      return;
    }
    let timer = 0;
    const onAbort = (): void => {
      clearTimeout(timer);
      reject(signal?.reason);
    };
    timer = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort);
      resolve();
    }, ms);
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}

export async function main(): Promise<void> {
  const user = new AbortController();
  const deadline = AbortSignal.any([user.signal, AbortSignal.timeout(30)]);
  await sleep(10, deadline);                     // resolves
  try {
    await sleep(100, deadline);
  } catch (reason) {
    if (reason instanceof DOMException) console.log(reason.name); // TimeoutError
  }
}
