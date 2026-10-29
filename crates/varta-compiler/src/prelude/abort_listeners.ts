// Engine prelude of checked-f64-utf16-v36 (`abort_signals` with
// `function_fields`): the v35 `AbortController`/`AbortSignal` data model and
// `DOMException` plus abort listeners (`addEventListener`,
// `removeEventListener`, `onabort`), implemented in typed guest code with no
// host import, no capability and no timers. V35 programs keep the v35 prelude
// (prelude/abort.ts). The compiler adds it to a program only when a module
// names `AbortController`, `AbortSignal` or `DOMException`; see
// docs/architecture.md (Web types, abort listeners).

const ABORT_ERROR = 'AbortError';
const ABORT_MESSAGE = 'This operation was aborted';

// DOMException names with a legacy `code`, indexed by that code; codes 0, 2,
// 6 and 16 have no name.
const LEGACY_NAMES = [
  '',
  'IndexSizeError',
  '',
  'HierarchyRequestError',
  'WrongDocumentError',
  'InvalidCharacterError',
  '',
  'NoModificationAllowedError',
  'NotFoundError',
  'NotSupportedError',
  'InUseAttributeError',
  'InvalidStateError',
  'SyntaxError',
  'InvalidModificationError',
  'NamespaceError',
  'InvalidAccessError',
  '',
  'TypeMismatchError',
  'SecurityError',
  'NetworkError',
  'AbortError',
  'URLMismatchError',
  'QuotaExceededError',
  'TimeoutError',
  'InvalidNodeTypeError',
  'DataCloneError',
];

function legacyCode(name: string): number {
  const code = LEGACY_NAMES.indexOf(name);
  return code < 0 ? 0 : code;
}

export class DOMException extends Error {
  constructor(message?: string, name?: string) {
    super(message ?? '');
    this.name = name ?? 'Error';
  }

  get code(): number {
    return legacyCode(this.name);
  }
}

// An abort listener. Listeners receive no `Event` object: Varta does not
// model DOM events, and `this` is not bound to the signal.
type AbortListener = () => void;

// The options of `addEventListener` and `removeEventListener` that affect an
// abort listener (`removeEventListener` reads only `capture`); `passive` and
// `signal` are not modelled.
interface ListenerOptions {
  once?: boolean;
  capture?: boolean;
}

// One registered listener. `removed` marks an entry removed while a dispatch
// that snapshotted it is still running, so it is skipped.
class ListenerEntry {
  type: string;
  callback: AbortListener;
  capture: boolean;
  once: boolean;
  removed: boolean;
  constructor(type: string, callback: AbortListener, capture: boolean, once: boolean) {
    this.type = type;
    this.callback = callback;
    this.capture = capture;
    this.once = once;
    this.removed = false;
  }
}

function captureOption(options?: ListenerOptions | boolean): boolean {
  if (typeof options === 'boolean') return options;
  return options?.capture ?? false;
}

// Shared state of one signal. It is not exported, so a program can neither
// construct a signal directly nor change its state except through its
// controller.
class AbortState {
  aborted: boolean;
  reason: unknown;
  listeners: ListenerEntry[];
  constructor(aborted: boolean, reason: unknown) {
    this.aborted = aborted;
    this.reason = reason;
    this.listeners = [];
  }
}

function findListener(state: AbortState, type: string, callback: AbortListener, capture: boolean): number {
  for (let i = 0; i < state.listeners.length; i++) {
    const entry = state.listeners[i];
    if (entry !== undefined && entry.type === type && entry.callback === callback && entry.capture === capture) {
      return i;
    }
  }
  return -1;
}

function removeListener(state: AbortState, index: number): void {
  const entry = state.listeners[index];
  if (entry !== undefined) {
    entry.removed = true;
    state.listeners = state.listeners.filter((other: ListenerEntry): boolean => other !== entry);
  }
}

function abortReason(reason?: unknown): unknown {
  return reason === undefined ? new DOMException(ABORT_MESSAGE, ABORT_ERROR) : reason;
}

export class AbortSignal {
  private readonly state: AbortState;
  // Runs before the `abort` listeners (DOM: at the position of its first
  // assignment among them).
  onabort: AbortListener | null;

  constructor(state: AbortState) {
    this.state = state;
    this.onabort = null;
  }

  static abort(reason?: unknown): AbortSignal {
    return new AbortSignal(new AbortState(true, abortReason(reason)));
  }

  get aborted(): boolean {
    return this.state.aborted;
  }

  get reason(): unknown {
    return this.state.reason;
  }

  throwIfAborted(): void {
    if (this.state.aborted) {
      throw this.state.reason;
    }
  }

  addEventListener(type: string, listener: AbortListener, options?: ListenerOptions | boolean): void {
    const capture = captureOption(options);
    if (findListener(this.state, type, listener, capture) >= 0) {
      return;
    }
    const once = typeof options === 'boolean' ? false : (options?.once ?? false);
    this.state.listeners.push(new ListenerEntry(type, listener, capture, once));
  }

  removeEventListener(type: string, listener: AbortListener, options?: ListenerOptions | boolean): void {
    const index = findListener(this.state, type, listener, captureOption(options));
    if (index >= 0) {
      removeListener(this.state, index);
    }
  }
}

// Fire `abort` at a signal that has just been aborted: `onabort`, then the
// `abort` listeners registered when the dispatch starts, in registration
// order; a `once` listener is removed before it runs, and a listener removed
// by an earlier one is skipped. An exception from a listener propagates out
// of `abort()` (Node reports it as uncaught after the dispatch instead).
function dispatchAbort(signal: AbortSignal, state: AbortState): void {
  const handler = signal.onabort;
  if (handler !== null) {
    handler();
  }
  const snapshot = state.listeners.slice();
  for (let i = 0; i < snapshot.length; i++) {
    const entry = snapshot[i];
    if (entry === undefined || entry.removed || entry.type !== 'abort') {
      continue;
    }
    if (entry.once) {
      const index = state.listeners.indexOf(entry);
      if (index >= 0) {
        removeListener(state, index);
      }
    }
    entry.callback();
  }
}

export class AbortController {
  private readonly state: AbortState;
  readonly signal: AbortSignal;

  constructor() {
    const state = new AbortState(false, undefined);
    this.state = state;
    this.signal = new AbortSignal(state);
  }

  abort(reason?: unknown): void {
    if (!this.state.aborted) {
      this.state.reason = abortReason(reason);
      this.state.aborted = true;
      dispatchAbort(this.signal, this.state);
    }
  }
}
