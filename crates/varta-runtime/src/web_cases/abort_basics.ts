// AbortController/AbortSignal state, default and custom reasons,
// throwIfAborted, AbortSignal.abort and DOMException (v35 `abort_signals`).
function describe(reason: unknown): string {
  if (reason instanceof DOMException) return `DOMException ${reason.name} ${reason.message} ${reason.code}`;
  if (reason instanceof Error) return `Error ${reason.message}`;
  if (typeof reason === 'string') return `string ${reason}`;
  if (reason === undefined) return 'undefined';
  return 'other';
}

function guarded(signal: AbortSignal): string {
  try {
    signal.throwIfAborted();
    return 'running';
  } catch (error) {
    return `thrown ${describe(error)} ${error === signal.reason}`;
  }
}

export function main(): void {
  const controller = new AbortController();
  const signal = controller.signal;
  console.log(`${signal.aborted} ${describe(signal.reason)} ${guarded(signal)}`);
  controller.abort();
  console.log(`${signal.aborted} ${describe(signal.reason)} ${guarded(signal)}`);
  const first = signal.reason;
  controller.abort('again');
  console.log(`${signal.reason === first} ${signal instanceof AbortSignal}`);
  const custom = new AbortController();
  custom.abort('stop');
  console.log(`${describe(custom.signal.reason)} ${guarded(custom.signal)}`);
  const explicit = new AbortController();
  explicit.abort(undefined);
  console.log(describe(explicit.signal.reason));
  const failed = new AbortController();
  failed.abort(new Error('boom'));
  console.log(guarded(failed.signal));
  console.log(describe(AbortSignal.abort().reason));
  console.log(describe(AbortSignal.abort(42).reason));
  console.log(String(AbortSignal.abort().aborted));
  const reason = AbortSignal.abort().reason;
  if (reason instanceof Error) {
    console.log(`${reason.toString()} ${reason instanceof DOMException}`);
  }
  const e = new DOMException('late', 'TimeoutError');
  console.log(`${e.name} ${e.message} ${e.code} ${e instanceof Error}`);
  const plain = new DOMException();
  console.log(`[${plain.name}] [${plain.message}] ${plain.code}`);
}
