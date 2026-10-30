// Engine prelude of checked-f64-utf16-v37 (`net_request`): WHATWG `fetch`
// with `Request`, `Response` and a `ReadableStream` subset over the exact
// `net.request` capability. The compiler adds it to a program only when a
// module names one of its exports; see docs/architecture.md (Network
// requests).
//
// The runtime does the I/O: `varta.net.start` checks the grant and starts the
// transfer, `varta.net.watch` suspends until the armed operation (the head,
// then one body chunk per `varta.net.read`) completes, `varta.net.take` and
// `varta.net.text` read the completion and `varta.net.abort` cancels the
// transfer. These intrinsics exist only in engine preludes. Bodies are text:
// the runtime decodes response bytes as UTF-8 per chunk and encodes request
// bodies as UTF-8 (bytes wait for `Uint8Array`).

// The request descriptor and completion codes of `varta_common::net`; a
// compiler test keeps them in step.
const DESCRIPTOR_VERSION = 'varta-net-request-1';
const SEPARATOR = '\n';
const NO_BODY = '0';
const WITH_BODY = '1';
const KIND_HEAD = 1;
const KIND_CHUNK = 2;
const KIND_END = 3;
const KIND_FAILED = 4;
const KIND_ABORTED = 5;
const KIND_DENIED = 6;

// Fetch "redirect status" codes, and the bound on followed redirects.
const REDIRECT_STATUSES = [301, 302, 303, 307, 308];
const SEE_OTHER = 303;
const MAX_REDIRECTS = 20;
// Methods `fetch` normalizes to uppercase, by their lowercase spelling.
const NORMALIZED_LOWER = ['delete', 'get', 'head', 'options', 'post', 'put'];
const NORMALIZED_UPPER = ['DELETE', 'GET', 'HEAD', 'OPTIONS', 'POST', 'PUT'];
const FORBIDDEN_LOWER = ['connect', 'trace', 'track'];
const TEXT_CONTENT_TYPE = 'text/plain;charset=UTF-8';
const FORM_CONTENT_TYPE = 'application/x-www-form-urlencoded;charset=UTF-8';
const BYTE_ORDER_MARK = '﻿';
const FETCH_FAILED = 'fetch failed';
const NOT_ALLOWED = 'NotAllowedError';

// The head record a `KIND_HEAD` completion carries (`net::HEAD_FIELDS`).
interface NetHead {
  status: number;
  statusText: string;
  url: string;
  location: string;
  headers: string[];
}

export type RequestRedirect = 'error' | 'follow' | 'manual';
export type BodyInit = string | URLSearchParams;

export interface RequestInit {
  method?: string;
  headers?: HeadersInit;
  body?: BodyInit | null;
  signal?: AbortSignal | null;
  redirect?: RequestRedirect;
}

export interface ResponseInit {
  status?: number;
  statusText?: string;
  headers?: HeadersInit;
}

export interface ReadableStreamReadResult {
  done: boolean;
  // The empty string once `done` (WHATWG: undefined), so a chunk read after
  // a `done` check is a string without a non-null assertion.
  value: string;
}

export interface ReadableStreamDefaultController {
  enqueue: (chunk: string) => void;
  close: () => void;
  error: (reason?: unknown) => void;
}

export interface UnderlyingSource {
  start?: (controller: ReadableStreamDefaultController) => void;
  pull?: (controller: ReadableStreamDefaultController) => Promise<void>;
  cancel?: (reason: unknown) => Promise<void>;
}

// Suspend until the armed operation of request `id` completes.
function waitFor(id: number): Promise<void> {
  return new Promise<void>((resolve) => {
    varta.net.watch(id, resolve);
  });
}

// Node's `TypeError: fetch failed` (or `terminated` for a body), with the
// runtime's reason as an `Error` cause.
class NetworkFailure extends TypeError {
  constructor(message: string, detail: string) {
    super(message, { cause: new Error(detail) });
    this.name = 'TypeError';
  }
}

function failure(detail: string): NetworkFailure {
  return new NetworkFailure(FETCH_FAILED, detail);
}

// The state shared by a stream, its reader and its body transfer. A stream
// backed by a request reads chunks from the runtime; any other stream reads
// its queue and, when the queue is empty, its source's `pull`.
class StreamState {
  // The request id while the transfer holds a runtime slot, else 0.
  id: number;
  // Whether an operation of `id` is armed (its completion not taken yet).
  armed: boolean;
  signal: AbortSignal | null;
  listener: (() => void) | null;
  queue: string[];
  // The index of the next queued chunk.
  head: number;
  closed: boolean;
  errored: boolean;
  error: unknown;
  cancelled: boolean;
  disturbed: boolean;
  locked: boolean;
  // The latest read and the reads not settled yet: reads run one at a
  // time, in call order.
  reading: Promise<ReadableStreamReadResult> | null;
  reads: number;
  source: UnderlyingSource | null;
  controller: ReadableStreamDefaultController | null;
  constructor(id: number, signal: AbortSignal | null) {
    this.id = id;
    this.armed = id > 0;
    this.signal = signal;
    this.listener = null;
    this.queue = [];
    this.head = 0;
    this.closed = false;
    this.errored = false;
    this.error = undefined;
    this.cancelled = false;
    this.disturbed = false;
    this.locked = false;
    this.reading = null;
    this.reads = 0;
    this.source = null;
    this.controller = null;
  }
}

// Stop following the request's signal.
function detach(state: StreamState): void {
  const listener = state.listener;
  const signal = state.signal;
  state.listener = null;
  if (listener !== null && signal !== null) {
    signal.removeEventListener('abort', listener);
  }
}

// Abort the transfer of `state` with `reason`: an armed operation completes as
// aborted (the waiting read sees it); an idle request is released at once.
function abortTransfer(state: StreamState, reason: unknown): void {
  const id = state.id;
  if (id === 0) {
    return;
  }
  if (!state.errored && !state.closed) {
    state.errored = true;
    state.error = reason;
  }
  if (!state.armed) {
    state.id = 0;
    detach(state);
  }
  varta.net.abort(id);
}

function follow(state: StreamState): void {
  const signal = state.signal;
  if (signal === null) {
    return;
  }
  const listener = (): void => {
    abortTransfer(state, signal.reason);
  };
  state.listener = listener;
  signal.addEventListener('abort', listener, { once: true });
}

// The next queued chunk, if any; an emptied queue is reset.
function dequeue(state: StreamState): string | undefined {
  const next = state.queue[state.head];
  if (next === undefined) {
    return undefined;
  }
  state.head += 1;
  if (state.head === state.queue.length) {
    state.queue = [];
    state.head = 0;
  }
  return next;
}

function pending(state: StreamState): number {
  return state.queue.length - state.head;
}

function done(): ReadableStreamReadResult {
  return { done: true, value: '' };
}

function chunk(value: string): ReadableStreamReadResult {
  return { done: false, value: value };
}

function controllerOf(state: StreamState): ReadableStreamDefaultController {
  return {
    enqueue: (value: string): void => {
      if (state.closed || state.errored) {
        throw new TypeError('Invalid state: Controller is already closed');
      }
      state.queue.push(value);
    },
    close: (): void => {
      if (state.closed || state.errored) {
        throw new TypeError('Invalid state: Controller is already closed');
      }
      state.closed = true;
    },
    error: (reason?: unknown): void => {
      if (!state.closed && !state.errored) {
        state.errored = true;
        state.error = reason;
        state.queue = [];
        state.head = 0;
      }
    },
  };
}

// One chunk from the queue, pulling the source while it is empty.
async function pullQueue(state: StreamState): Promise<ReadableStreamReadResult> {
  for (;;) {
    const next = dequeue(state);
    if (next !== undefined) {
      return chunk(next);
    }
    if (state.errored) {
      throw state.error;
    }
    if (state.closed || state.cancelled) {
      return done();
    }
    const source = state.source;
    const controller = state.controller;
    const pull = source === null ? undefined : source.pull;
    if (pull === undefined || controller === null) {
      // Nothing can ever enqueue: a source without `pull` closes when empty.
      state.closed = true;
      return done();
    }
    await pull(controller);
    if (pending(state) === 0 && !state.closed && !state.errored) {
      throw new TypeError('ReadableStream source pull() enqueued nothing');
    }
  }
  // Unreachable: the loop only exits through a return or a throw.
  return done();
}

// One read: a queued chunk, the next chunk from the runtime, or a pull of
// the source. It counts itself in `reads` until it settles.
async function readOnce(state: StreamState): Promise<ReadableStreamReadResult> {
  state.disturbed = true;
  try {
    if (state.source !== null) {
      return await pullQueue(state);
    }
    const queued = dequeue(state);
    if (queued !== undefined) {
      return chunk(queued);
    }
    const id = state.id;
    if (id === 0) {
      if (state.errored) {
        throw state.error;
      }
      return done();
    }
    varta.net.read(id);
    state.armed = true;
    await waitFor(id);
    state.armed = false;
    const kind = varta.net.take(id);
    if (kind === KIND_CHUNK) {
      return chunk(varta.net.text(id));
    }
    state.id = 0;
    detach(state);
    if (kind === KIND_END) {
      state.closed = true;
      return done();
    }
    if (kind === KIND_ABORTED) {
      if (state.cancelled) {
        return done();
      }
      throw state.error;
    }
    const reason = new NetworkFailure('terminated', varta.net.text(id));
    state.errored = true;
    state.error = reason;
    throw reason;
  } finally {
    state.reads -= 1;
  }
}

async function readAfter(
  previous: Promise<ReadableStreamReadResult>,
  state: StreamState,
): Promise<ReadableStreamReadResult> {
  try {
    await previous;
  } catch (error) {
    // The earlier read's failure belongs to its own caller.
  }
  return readOnce(state);
}

// Reads run one at a time, in call order, as WHATWG read requests do; a read
// with none in progress starts at once.
function startRead(state: StreamState): Promise<ReadableStreamReadResult> {
  const previous = state.reads > 0 ? state.reading : null;
  state.reads += 1;
  const next = previous === null ? readOnce(state) : readAfter(previous, state);
  state.reading = next;
  return next;
}

async function cancelState(state: StreamState, reason: unknown): Promise<void> {
  state.disturbed = true;
  if (state.closed || state.errored) {
    if (state.errored) {
      throw state.error;
    }
    return;
  }
  state.cancelled = true;
  state.queue = [];
  state.head = 0;
  if (state.id > 0) {
    const id = state.id;
    if (!state.armed) {
      state.id = 0;
    }
    detach(state);
    varta.net.abort(id);
    return;
  }
  const source = state.source;
  const cancel = source === null ? undefined : source.cancel;
  if (cancel !== undefined) {
    await cancel(reason);
  }
}

// Hands a transfer's state to the next `ReadableStream` the prelude builds;
// program code can never fill it, so its streams always own a fresh state.
const HANDOFF: { state: StreamState | null; headers: Headers | null } = {
  state: null,
  headers: null,
};

export class ReadableStream {
  private readonly state: StreamState;

  constructor(source?: UnderlyingSource) {
    const handed = HANDOFF.state;
    HANDOFF.state = null;
    const state = handed ?? new StreamState(0, null);
    this.state = state;
    if (handed === null && source !== undefined) {
      const controller = controllerOf(state);
      state.source = source;
      state.controller = controller;
      const start = source.start;
      if (start !== undefined) {
        start(controller);
      }
    }
  }

  get locked(): boolean {
    return this.state.locked;
  }

  getReader(): ReadableStreamDefaultReader {
    return new ReadableStreamDefaultReader(this);
  }

  cancel(reason?: unknown): Promise<void> {
    if (this.state.locked) {
      return Promise.reject(new TypeError('Invalid state: ReadableStream is locked'));
    }
    return cancelState(this.state, reason);
  }

  // Async iteration (`for await`): reads every chunk and releases the lock;
  // leaving the loop early cancels the stream.
  async *[Symbol.asyncIterator](): AsyncGenerator<string, void, undefined> {
    const reader = this.getReader();
    let finished = false;
    try {
      for (;;) {
        const result = await reader.read();
        if (result.done) {
          finished = true;
          return;
        }
        yield result.value;
      }
    } finally {
      if (!finished) {
        await reader.cancel();
      }
      reader.releaseLock();
    }
  }

  // WHATWG `values()`: the same iteration (a computed member call cannot
  // delegate to the method above).
  async *values(): AsyncGenerator<string, void, undefined> {
    const reader = this.getReader();
    let finished = false;
    try {
      for (;;) {
        const result = await reader.read();
        if (result.done) {
          finished = true;
          return;
        }
        yield result.value;
      }
    } finally {
      if (!finished) {
        await reader.cancel();
      }
      reader.releaseLock();
    }
  }

  // Engine-internal: the shared state, for readers and bodies. `ReadableStream`
  // has no such member in WHATWG; programs cannot name the result's type.
  stateOf(): StreamState {
    return this.state;
  }
}

export class ReadableStreamDefaultReader {
  private state: StreamState | null;

  constructor(stream: ReadableStream) {
    const state = stream.stateOf();
    if (state.locked) {
      throw new TypeError('Invalid state: ReadableStream is locked');
    }
    state.locked = true;
    this.state = state;
  }

  read(): Promise<ReadableStreamReadResult> {
    const state = this.state;
    if (state === null) {
      return Promise.reject(new TypeError('Invalid state: The reader is not attached to a stream'));
    }
    return startRead(state);
  }

  cancel(reason?: unknown): Promise<void> {
    const state = this.state;
    if (state === null) {
      return Promise.reject(new TypeError('Invalid state: The reader is not attached to a stream'));
    }
    return cancelState(state, reason);
  }

  releaseLock(): void {
    const state = this.state;
    if (state !== null) {
      state.locked = false;
      this.state = null;
    }
  }
}

function normalizeMethod(method: string): string {
  if (method.length === 0 || varta.web.indexNonToken(method) >= 0) {
    throw new TypeError(`'${method}' is not a valid HTTP method.`);
  }
  const lower = varta.web.asciiLowercase(method);
  if (FORBIDDEN_LOWER.indexOf(lower) >= 0) {
    throw new TypeError(`'${method}' HTTP method is unsupported.`);
  }
  const index = NORMALIZED_LOWER.indexOf(lower);
  const normalized = index < 0 ? undefined : NORMALIZED_UPPER[index];
  return normalized === undefined ? method : normalized;
}

function textStream(text: string): ReadableStream {
  const stream = new ReadableStream();
  const state = stream.stateOf();
  if (text.length > 0) {
    state.queue.push(text);
  }
  state.closed = true;
  return stream;
}

export class Request {
  readonly method: string;
  readonly url: string;
  readonly headers: Headers;
  readonly redirect: RequestRedirect;
  private readonly content: string | null;
  // The signal the request follows; a request without one gets a signal
  // that never aborts, created when first read.
  private readonly given: AbortSignal | null;
  private fallback: AbortSignal | null;

  constructor(input: string | Request, init?: RequestInit) {
    const base = input instanceof Request ? input : null;
    this.url = input instanceof Request ? input.url : input;
    this.method = normalizeMethod(init?.method ?? (base === null ? 'GET' : base.method));
    this.headers = new Headers(init?.headers ?? (base === null ? undefined : base.headers));
    this.given = init?.signal ?? (base === null ? null : base.signalOf());
    this.fallback = null;
    let redirect: RequestRedirect = base === null ? 'error' : base.redirect;
    if (init !== undefined) {
      const requested = init.redirect;
      if (requested !== undefined) {
        redirect = requested;
      }
    }
    this.redirect = redirect;
    const given = init?.body ?? null;
    let body = base === null ? null : base.bodyText();
    let type = TEXT_CONTENT_TYPE;
    if (given instanceof URLSearchParams) {
      body = given.toString();
      type = FORM_CONTENT_TYPE;
    } else if (given !== null) {
      body = given;
    }
    if (body !== null && (this.method === 'GET' || this.method === 'HEAD')) {
      throw new TypeError('Request with GET/HEAD method cannot have body.');
    }
    if (body !== null && !this.headers.has('content-type')) {
      this.headers.set('content-type', type);
    }
    this.content = body;
  }

  get signal(): AbortSignal {
    const given = this.given ?? this.fallback;
    if (given !== null) {
      return given;
    }
    const created = new AbortController().signal;
    this.fallback = created;
    return created;
  }

  // Engine-internal: the request body text, or null.
  bodyText(): string | null {
    return this.content;
  }

  // Engine-internal: the signal the request follows, or null.
  signalOf(): AbortSignal | null {
    return this.given;
  }
}

function descriptor(method: string, url: string, headers: Headers, body: string | null): string {
  if (url.indexOf(SEPARATOR) >= 0 || url.indexOf('\r') >= 0) {
    throw new TypeError(`Failed to parse URL from ${url}`);
  }
  const entries = headers.entries();
  let text = DESCRIPTOR_VERSION + SEPARATOR + method + SEPARATOR + url + SEPARATOR;
  text += String(entries.length) + SEPARATOR;
  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i];
    if (entry !== undefined) {
      text += (entry[0] ?? '') + SEPARATOR + (entry[1] ?? '') + SEPARATOR;
    }
  }
  return body === null ? text + NO_BODY + SEPARATOR : text + WITH_BODY + SEPARATOR + body;
}

export class Response {
  readonly status: number;
  readonly statusText: string;
  readonly headers: Headers;
  // Set by `fetch` for network responses (WHATWG: readonly).
  url: string;
  redirected: boolean;
  private readonly stream: ReadableStream | null;

  // A `URLSearchParams` body would make a union of two classes, which the
  // profile does not admit; pass `params.toString()`.
  constructor(body?: string | ReadableStream | null, init?: ResponseInit) {
    const status = init?.status ?? 200;
    if (!Number.isInteger(status) || status < 200 || status > 599) {
      throw new RangeError(
        `init["status"] must be in the range of 200 to 599, inclusive.`,
      );
    }
    this.status = status;
    this.statusText = init?.statusText ?? '';
    const handed = HANDOFF.headers;
    HANDOFF.headers = null;
    this.headers = handed ?? new Headers(init?.headers);
    this.url = '';
    this.redirected = false;
    let stream: ReadableStream | null = null;
    if (body instanceof ReadableStream) {
      stream = body;
    } else if (body !== undefined && body !== null) {
      stream = textStream(body);
      if (!this.headers.has('content-type')) {
        this.headers.set('content-type', TEXT_CONTENT_TYPE);
      }
    }
    this.stream = stream;
  }

  get ok(): boolean {
    return this.status >= 200 && this.status <= 299;
  }

  get type(): string {
    return 'basic';
  }

  get body(): ReadableStream | null {
    return this.stream;
  }

  get bodyUsed(): boolean {
    const stream = this.stream;
    return stream !== null && stream.stateOf().disturbed;
  }

  async text(): Promise<string> {
    const stream = this.stream;
    if (stream === null) {
      return '';
    }
    if (stream.stateOf().disturbed || stream.locked) {
      throw new TypeError('Body is unusable: Body has already been read');
    }
    const reader = stream.getReader();
    let text = '';
    for (;;) {
      const result = await reader.read();
      if (result.done) {
        break;
      }
      text += result.value;
    }
    reader.releaseLock();
    return text.startsWith(BYTE_ORDER_MARK) ? text.slice(1) : text;
  }

  async json(): Promise<unknown> {
    return JSON.parse(await this.text());
  }
}

// One network round trip; redirects are followed (each hop checked against
// the grant again), returned (`manual`) or refused (`error`, the default).
async function send(
  request: Request,
  url: string,
  method: string,
  body: string | null,
  hops: number,
): Promise<Response> {
  const signal = request.signalOf();
  if (signal !== null && signal.aborted) {
    throw signal.reason;
  }
  const id = varta.net.start(descriptor(method, url, request.headers, body));
  const state = new StreamState(id, signal);
  follow(state);
  await waitFor(id);
  state.armed = false;
  const kind = varta.net.take(id);
  if (kind !== KIND_HEAD) {
    state.id = 0;
    detach(state);
    if (kind === KIND_ABORTED) {
      throw state.error;
    }
    if (kind === KIND_DENIED) {
      // The permission broker refused the request before any effect; the
      // rejection is catchable, as a browser reports a refused permission.
      throw new DOMException(varta.net.text(id), NOT_ALLOWED);
    }
    throw failure(kind === KIND_END ? 'empty response' : varta.net.text(id));
  }
  const head = varta.json.parse<NetHead>(varta.net.text(id));
  if (REDIRECT_STATUSES.indexOf(head.status) >= 0 && head.location !== '' && request.redirect !== 'manual') {
    abortTransfer(state, undefined);
    if (request.redirect === 'error') {
      throw failure('unexpected redirect');
    }
    if (hops >= MAX_REDIRECTS) {
      throw failure('redirect count exceeded');
    }
    const get =
      (head.status === SEE_OTHER && method !== 'HEAD') ||
      ((head.status === 301 || head.status === 302) && method === 'POST');
    return send(request, head.location, get ? 'GET' : method, get ? null : body, hops + 1);
  }
  const headers = new Headers();
  for (let i = 0; i + 1 < head.headers.length; i += 2) {
    headers.append(head.headers[i] ?? '', head.headers[i + 1] ?? '');
  }
  HANDOFF.state = state;
  const stream = new ReadableStream();
  // The response takes the header list as built, without a copy.
  HANDOFF.headers = headers;
  const response = new Response(stream, {
    status: head.status < 200 ? 200 : head.status,
    statusText: head.statusText,
  });
  response.url = head.url;
  response.redirected = hops > 0;
  return response;
}

export async function fetch(input: string | Request, init?: RequestInit): Promise<Response> {
  const request = new Request(input, init);
  return send(request, request.url, request.method, request.bodyText(), 0);
}
