// AbortSignal.timeout: Node's uint32 delay validation, the TimeoutError
// reason after the delay, and a timer that does not keep the loop alive.
function sleep(ms: number): Promise<void> {
  return new Promise<void>((resolve) => setTimeout(resolve, ms));
}
function probe(delay: number): void {
  try {
    AbortSignal.timeout(delay);
  } catch (e) {
    console.log((e as Error).name + ": " + (e as Error).message);
  }
}
export async function main(): Promise<void> {
  probe(-1);
  probe(1.5);
  probe(4294967296);
  const signal = AbortSignal.timeout(20);
  console.log("aborted " + String(signal.aborted));
  await sleep(10);
  console.log("at 10 " + String(signal.aborted));
  await sleep(20);
  console.log("at 30 " + String(signal.aborted));
  const reason = signal.reason as DOMException;
  console.log(reason.name + " " + reason.message + " " + String(reason.code) + " " + String(reason instanceof DOMException));
  try {
    signal.throwIfAborted();
  } catch (e) {
    console.log("thrown " + (e as Error).name);
  }
  // An unreferenced timeout does not keep the loop alive.
  const idle = AbortSignal.timeout(4000000);
  console.log("idle " + String(idle.aborted));
}
