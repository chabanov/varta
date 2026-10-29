// Engine prelude of checked-f64-utf16-v36 (`timers`): `AbortSignal.timeout`.
// The compiler adds it to a program only when a module calls
// `AbortSignal.timeout` under a timers profile, and binds that call to
// `abortSignalTimeout`; programs that do not name it never arm a timer, so
// they need no `time.timer` grant. The signal's timer does not keep the event
// loop alive, as in Node. See docs/architecture.md (Timers).

const TIMEOUT_ERROR = 'TimeoutError';
const TIMEOUT_MESSAGE = 'The operation was aborted due to timeout';
// Node validates the delay as a uint32 (`validateUint32`).
const MAX_DELAY = 4294967295;
const RANGE_PREFIX = 'The value of "delay" is out of range. It must be ';

export function abortSignalTimeout(delay: number): AbortSignal {
  if (!Number.isInteger(delay)) {
    throw new RangeError(RANGE_PREFIX + 'an integer. Received ' + String(delay));
  }
  if (delay < 0 || delay > MAX_DELAY) {
    throw new RangeError(
      RANGE_PREFIX + '>= 0 && <= ' + String(MAX_DELAY) + '. Received ' + String(delay),
    );
  }
  const controller = new AbortController();
  varta.web.unrefTimeout(() => {
    controller.abort(new DOMException(TIMEOUT_MESSAGE, TIMEOUT_ERROR));
  }, delay);
  return controller.signal;
}
