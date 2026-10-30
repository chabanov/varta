// Engine prelude of checked-f64-utf16-v35 (`abort_signals`): the DOM
// `AbortController`/`AbortSignal` data model and `DOMException`, implemented in
// typed guest code with no host import, no capability and no timers. The
// compiler adds it to a program only when a module names `AbortController`,
// `AbortSignal` or `DOMException`; see docs/architecture.md (Web types).

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

// Shared state of one signal. It is not exported, so a program can neither
// construct a signal directly nor change its state except through its
// controller.
class AbortState {
  aborted: boolean;
  reason: unknown;
  constructor(aborted: boolean, reason: unknown) {
    this.aborted = aborted;
    this.reason = reason;
  }
}

function abortReason(reason?: unknown): unknown {
  return reason === undefined ? new DOMException(ABORT_MESSAGE, ABORT_ERROR) : reason;
}

export class AbortSignal {
  private readonly state: AbortState;

  constructor(state: AbortState) {
    this.state = state;
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
    }
  }
}
