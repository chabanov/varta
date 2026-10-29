// V36 timers with function fields: AbortSignal.any over controller and
// AbortSignal.timeout signals, abort listeners and onabort, and the pilot's
// abortable sleep (listener removed when the timer fires, timer cleared when
// the signal aborts).
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

function describe(reason: unknown): string {
  if (reason instanceof DOMException) return reason.name;
  return String(reason);
}

function composition(events: string[]): void {
  const a = new AbortController();
  const b = new AbortController();
  a.signal.addEventListener('abort', (): void => {
    events.push('a first');
  });
  const any = AbortSignal.any([a.signal, b.signal]);
  const nested = AbortSignal.any([any]);
  a.signal.addEventListener('abort', (): void => {
    events.push('a second ' + String(any.aborted));
  });
  any.addEventListener('abort', (): void => {
    events.push('any ' + String(any.reason) + ' ' + String(nested.aborted));
  });
  nested.onabort = (): void => {
    events.push('nested onabort');
  };
  nested.addEventListener('abort', (): void => {
    events.push('nested ' + String(nested.reason));
  });
  events.push('before ' + String(any.aborted));
  a.abort('stop');
  b.abort('later');
  events.push('after ' + String(any.reason) + ' ' + String(nested.reason));
  const done = AbortSignal.any([b.signal, a.signal]);
  events.push('pre-aborted ' + String(done.aborted) + ' ' + String(done.reason));
  events.push('empty ' + String(AbortSignal.any([]).aborted));
}

export async function main(): Promise<void> {
  const journal: string[] = [];
  composition(journal);
  console.log(JSON.stringify(journal));

  const user = new AbortController();
  const deadline = AbortSignal.any([user.signal, AbortSignal.timeout(30)]);
  deadline.addEventListener(
    'abort',
    (): void => {
      console.log('deadline ' + describe(deadline.reason));
    },
    { once: true },
  );
  await sleep(10, deadline);
  console.log('slept 10 ' + String(deadline.aborted));
  try {
    await sleep(100, deadline);
    console.log('slept 100');
  } catch (e) {
    console.log('sleep rejected ' + describe(e));
  }
  user.abort('late');
  console.log('user abort ignored ' + describe(deadline.reason));

  const cancel = new AbortController();
  const second = AbortSignal.any([AbortSignal.timeout(1000), cancel.signal]);
  setTimeout(() => cancel.abort('cancelled'), 5);
  try {
    await sleep(200, second);
  } catch (e) {
    console.log('second rejected ' + describe(e) + ' ' + String(second.reason));
  }
  try {
    await sleep(1, second);
  } catch (e) {
    console.log('already aborted ' + describe(e));
  }
}
