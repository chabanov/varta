// AbortController/AbortSignal without timers: state, the default AbortError
// DOMException reason, custom reasons, throwIfAborted and AbortSignal.abort.
function describe(reason: unknown): string {
  if (reason instanceof DOMException) return `${reason.name}(${reason.code}): ${reason.message}`;
  if (typeof reason === 'string') return `string ${reason}`;
  return 'other';
}

export function main(): void {
  const controller = new AbortController();
  const signal = controller.signal;
  console.log(`${signal.aborted} ${signal.reason === undefined}`);
  controller.abort();
  controller.abort('ignored');
  console.log(`${signal.aborted} ${describe(signal.reason)}`);
  try {
    signal.throwIfAborted();
  } catch (error) {
    console.log(`thrown ${error === signal.reason}`);
  }
  console.log(describe(AbortSignal.abort('stop').reason));
}
