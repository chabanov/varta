// Abort listeners of the v36 Web types prelude: registration order, `once`,
// duplicates, capture identity, removal during dispatch, `onabort` and
// `Promise.withResolvers` cancelled by a signal.
function sleepUntil(signal: AbortSignal, wake: (resolve: () => void) => void): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    if (signal?.aborted) {
      reject(signal.reason);
      return;
    }
    const onAbort = (): void => {
      reject(signal?.reason);
    };
    signal?.addEventListener('abort', onAbort, { once: true });
    wake((): void => {
      signal?.removeEventListener('abort', onAbort);
      resolve();
    });
  });
}
async function waitFor(signal: AbortSignal): Promise<string> {
  const { promise, resolve, reject } = Promise.withResolvers<string>();
  signal.addEventListener(
    'abort',
    (): void => {
      reject(signal.reason);
    },
    { once: true },
  );
  pending.push((): void => resolve('finished'));
  return promise;
}
const pending: (() => void)[] = [];
export function main(): void {
  const controller = new AbortController();
  const signal = controller.signal;
  const order: string[] = [];
  const first = (): void => {
    order.push('first');
  };
  const removed = (): void => {
    order.push('removed');
  };
  const last = (): void => {
    order.push('last');
  };
  signal.onabort = (): void => {
    order.push('onabort');
  };
  signal.addEventListener('abort', first);
  signal.addEventListener('abort', first);
  signal.addEventListener(
    'abort',
    (): void => {
      order.push('once');
    },
    { once: true },
  );
  signal.addEventListener('abort', removed);
  signal.addEventListener('other', (): void => {
    order.push('other');
  });
  signal.removeEventListener('abort', removed);
  signal.addEventListener('abort', (): void => {
    order.push(`reason ${String(signal.reason instanceof DOMException)} ${String(signal.aborted)}`);
    signal.removeEventListener('abort', last);
  });
  signal.addEventListener('abort', last);
  controller.abort();
  controller.abort();
  console.log(JSON.stringify(order));
  signal.addEventListener('abort', (): void => {
    order.push('late');
  });
  AbortSignal.abort('x').addEventListener('abort', (): void => {
    order.push('never');
  });
  console.log(order.length);
  const c2 = new AbortController();
  let count = 0;
  const tick = (): void => {
    count++;
  };
  c2.signal.addEventListener('abort', tick, true);
  c2.signal.removeEventListener('abort', tick);
  c2.signal.addEventListener('abort', tick, { capture: false, once: true });
  c2.abort('why');
  console.log(`${count} ${String(c2.signal.reason)}`);
  const sleeper = new AbortController();
  let wakeUp: (() => void) | undefined = undefined;
  sleepUntil(sleeper.signal, (resolve: () => void): void => {
    wakeUp = resolve;
  }).then((): void => {
    console.log('woke');
  });
  const cancelled = new AbortController();
  sleepUntil(cancelled.signal, (resolve: () => void): void => {}).then(
    (): void => {
      console.log('not cancelled');
    },
    (reason): void => {
      console.log(`cancelled ${typeof reason === 'string' ? reason : 'other'}`);
    },
  );
  const r = Promise.withResolvers<number>();
  console.log(`${JSON.stringify(Object.keys(r))} ${typeof r.resolve}`);
  r.promise.then((n: number): void => {
    console.log(`resolved ${n}`);
  });
  r.resolve(5);
  r.resolve(6);
  r.reject(new Error('late'));
  const c3 = new AbortController();
  waitFor(c3.signal).then(
    (s: string): void => {
      console.log(s);
    },
    (e): void => {
      console.log(`rejected ${String(e instanceof DOMException)}`);
    },
  );
  waitFor(new AbortController().signal).then((s: string): void => {
    console.log(s);
  });
  pending[1]?.();
  c3.abort();
  cancelled.abort('stop');
  if (wakeUp !== undefined) {
    wakeUp();
  }
  console.log('sync end');
}
