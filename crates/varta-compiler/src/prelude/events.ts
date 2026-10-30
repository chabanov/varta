// Engine prelude of checked-f64-utf16-v38 (`turn_budgets`): host event
// sources. The compiler adds it to a module that names `varta.events.listen`
// or `varta.events.close` and binds them under those dotted names, which no
// source declaration can spell. See docs/architecture.md (Long-lived
// application instances).
//
// `varta.events.open` opens a named source in the runtime; every record the
// embedder delivers to it is one host task, run as its own event turn: it
// wakes the listening frame (`varta.events.watch`), which takes the record
// (`varta.events.take`, `varta.events.text`) and calls the listener. A
// listener that throws ends the pump with a rejection nobody handles, so the
// instance fails after that turn. These intrinsics exist only in engine
// preludes.

// The codes of `varta_common::events`; a compiler test keeps them in step.
const INVALID_SOURCE = 0;
const KIND_EVENT = 1;
const KIND_CLOSED = 2;

// Suspend until the source `id` has a record.
function waitFor(id: number): Promise<void> {
  return new Promise<void>((resolve) => {
    varta.events.watch(id, resolve);
  });
}

async function pump(id: number, listener: (record: string) => void): Promise<void> {
  while (true) {
    await waitFor(id);
    const kind = varta.events.take(id);
    if (kind === KIND_CLOSED) {
      return;
    }
    if (kind !== KIND_EVENT) {
      throw new TypeError('invalid host event');
    }
    listener(varta.events.text(id));
  }
}

export function eventsListen(name: string, listener: (record: string) => void): number {
  const id = varta.events.open(name);
  if (id === INVALID_SOURCE) {
    throw new TypeError(`'${name}' is not a valid host event source name, or it is already open`);
  }
  pump(id, listener);
  return id;
}

export function eventsClose(id: number): void {
  varta.events.release(id);
}
