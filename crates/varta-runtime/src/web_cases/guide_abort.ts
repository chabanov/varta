// The AbortController example of docs/language-guide.md (v35).
async function step(n: number, signal: AbortSignal): Promise<number> {
  signal.throwIfAborted();                          // throws signal.reason
  await null;
  return n * 2;
}

export async function main(): Promise<void> {
  const controller = new AbortController();
  const first = await step(1, controller.signal);
  console.log(String(first));                             // 2
  controller.abort();
  console.log(String(controller.signal.aborted));         // true
  try {
    await step(2, controller.signal);
  } catch (error) {
    if (error instanceof DOMException) {
      console.log(`${error.name} ${error.code}`);         // AbortError 20
    }
  }
  const stopped = AbortSignal.abort('shutdown');
  console.log(String(stopped.reason));                    // shutdown
}
