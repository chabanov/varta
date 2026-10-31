// Engine prelude of checked-f64-utf16-v38 (`web_bytes`): WHATWG `fetch` with
// `Request`, `Response` and `ReadableStream<Uint8Array>` over the exact
// `net.request` capability. It replaces the v37 text-body prelude under v38;
// the compiler adds it to a program only when a module names one of its
// exports; see docs/architecture.md (Web bytes).
//
// The runtime does the I/O: `varta.net.start` checks the grant and starts the
// transfer (descriptor version 2: text or byte bodies), `varta.net.watch`
// suspends until the armed operation completes, `varta.net.read` arms the
// next chunk as UTF-8 text (the `text()` and `json()` fast path) and
// `varta.net.readBytes` as bytes, one code unit per byte. A byte string is
// turned into a `Uint8Array` only when a reader asks for one; `blob()` keeps
// the byte strings as they are.

// The request descriptor and completion codes of `varta_common::net`; a
// compiler test keeps them in step.
const DESCRIPTOR_VERSION = 'varta-net-request-2';
const SEPARATOR = '\n';
const NO_BODY = '0';
const WITH_BODY = '1';
const WITH_BYTES = '2';
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
const MULTIPART_CONTENT_TYPE = 'multipart/form-data; boundary=';
const BYTE_ORDER_MARK = '﻿';
const FETCH_FAILED = 'fetch failed';
const NOT_ALLOWED = 'NotAllowedError';

// The head record a `KIND_HEAD` completion carries (`net::HEAD_FIELDS`).
// Under v38 its `headers` are canonical (each name once, in byte order,
// repeats combined with ", ") and hold only the names the `net.request`
// grant shows (`responseHeaders`, else the CORS-safelisted names), never
// set-cookie; `location` is only used to follow a redirect.
interface NetHead {
  status: number;
  statusText: string;
  url: string;
  location: string;
  headers: string[];
}

export type RequestRedirect = 'error' | 'follow' | 'manual';
export type BodyInit = string | URLSearchParams | Blob | ArrayBuffer | Uint8Array | FormData;

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

// One read. `value` is an empty `Uint8Array` once `done` (WHATWG:
// `undefined`), so a chunk read after a `done` check needs no assertion.
// Streams carry `Uint8Array` chunks only: the type parameter must be it.
export interface ReadableStreamReadResult<R extends Uint8Array = Uint8Array> {
  done: boolean;
  value: Uint8Array;
}

export interface ReadableStreamDefaultController<R extends Uint8Array = Uint8Array> {
  enqueue: (chunk: Uint8Array) => void;
  close: () => void;
  error: (reason?: unknown) => void;
}

export interface UnderlyingSource<R extends Uint8Array = Uint8Array> {
  start?: (controller: ReadableStreamDefaultController<R>) => void;
  pull?: (controller: ReadableStreamDefaultController<R>) => Promise<void>;
  cancel?: (reason: unknown) => Promise<void>;
}

// A body as it is sent: UTF-8 text or a byte string (one code unit per
// byte), and its default content type ('' for none).
class BodySource {
  readonly content: string;
  readonly bytes: boolean;
  readonly type: string;
  constructor(content: string, bytes: boolean, type: string) {
    this.content = content;
    this.bytes = bytes;
    this.type = type;
  }
}

function bodySource(body: BodyInit): BodySource {
  if (typeof body === 'string') {
    return new BodySource(body, false, TEXT_CONTENT_TYPE);
  }
  if (body instanceof URLSearchParams) {
    return new BodySource(body.toString(), false, FORM_CONTENT_TYPE);
  }
  if (body instanceof Blob) {
    return new BodySource(body.byteString(), true, body.type);
  }
  if (body instanceof FormData) {
    const multipart = body.multipart();
    return new BodySource(multipart.bytes, true, MULTIPART_CONTENT_TYPE + multipart.boundary);
  }
  if (body instanceof ArrayBuffer) {
    return new BodySource(varta.text.fromUnits(body.storageOf()), true, '');
  }
  return new BodySource(varta.text.fromUnits(varta.bytes.view(body)), true, '');
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
// backed by a request reads chunks from the runtime; a stream made from a
// body reads its unread content; any other stream reads its queue and, when
// the queue is empty, its source's `pull`.
class StreamState {
  // The request id while the transfer holds a runtime slot, else 0.
  id: number;
  // Whether an operation of `id` is armed (its completion not taken yet).
  armed: boolean;
  signal: AbortSignal | null;
  listener: (() => void) | null;
  queue: Uint8Array[];
  // The index of the next queued chunk.
  head: number;
  // The unread content of a body-born stream (one chunk), as UTF-8 text or
  // as a byte string.
  content: string | null;
  bytes: boolean;
  closed: boolean;
  errored: boolean;
  error: unknown;
  cancelled: boolean;
  disturbed: boolean;
  locked: boolean;
  // The latest read and the reads not settled yet: reads run one at a
  // time, in call order.
  reading: Promise<ReadableStreamReadResult<Uint8Array>> | null;
  reads: number;
  source: UnderlyingSource<Uint8Array> | null;
  controller: ReadableStreamDefaultController<Uint8Array> | null;
  constructor(id: number, signal: AbortSignal | null) {
    this.id = id;
    this.armed = id > 0;
    this.signal = signal;
    this.listener = null;
    this.queue = [];
    this.head = 0;
    this.content = null;
    this.bytes = false;
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
function dequeue(state: StreamState): Uint8Array | undefined {
  const next: Uint8Array | undefined = state.queue[state.head];
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

function bytesOf(byteString: string): Uint8Array {
  return varta.bytes.adopt(varta.text.units(byteString));
}

function byteStringOf(bytes: Uint8Array): string {
  return varta.text.fromUnits(varta.bytes.view(bytes));
}

function done(): ReadableStreamReadResult<Uint8Array> {
  return { done: true, value: new Uint8Array(0) };
}

function chunk(value: Uint8Array): ReadableStreamReadResult<Uint8Array> {
  return { done: false, value: value };
}

function controllerOf(state: StreamState): ReadableStreamDefaultController<Uint8Array> {
  return {
    enqueue: (value: Uint8Array): void => {
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
async function pullQueue(state: StreamState): Promise<ReadableStreamReadResult<Uint8Array>> {
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

// The next chunk from the runtime as a byte string (or as UTF-8 text when
// `text`), or null at the end of the body.
async function networkChunk(state: StreamState, text: boolean): Promise<string | null> {
  const id = state.id;
  if (id === 0) {
    if (state.errored) {
      throw state.error;
    }
    return null;
  }
  if (text) {
    varta.net.read(id);
  } else {
    varta.net.readBytes(id);
  }
  state.armed = true;
  await waitFor(id);
  state.armed = false;
  const kind = varta.net.take(id);
  if (kind === KIND_CHUNK) {
    return varta.net.text(id);
  }
  state.id = 0;
  detach(state);
  if (kind === KIND_END) {
    state.closed = true;
    return null;
  }
  if (kind === KIND_ABORTED) {
    if (state.cancelled) {
      return null;
    }
    throw state.error;
  }
  const reason = new NetworkFailure('terminated', varta.net.text(id));
  state.errored = true;
  state.error = reason;
  throw reason;
}

// The unread content of a body-born stream as one byte string, or null.
function takeContent(state: StreamState): string | null {
  const content = state.content;
  if (content === null) {
    return null;
  }
  state.content = null;
  state.closed = true;
  return state.bytes ? content : new Blob().encode(content);
}

// One read: a queued chunk, the body's content, the next chunk from the
// runtime, or a pull of the source. It counts itself in `reads` until it
// settles.
async function readOnce(state: StreamState): Promise<ReadableStreamReadResult<Uint8Array>> {
  state.disturbed = true;
  try {
    if (state.source !== null) {
      return await pullQueue(state);
    }
    const queued = dequeue(state);
    if (queued !== undefined) {
      return chunk(queued);
    }
    const content = takeContent(state);
    if (content !== null) {
      return content.length === 0 ? done() : chunk(bytesOf(content));
    }
    if (state.cancelled || (state.closed && state.id === 0)) {
      return done();
    }
    const next = await networkChunk(state, false);
    return next === null ? done() : chunk(bytesOf(next));
  } finally {
    state.reads -= 1;
  }
}

async function readAfter(
  previous: Promise<ReadableStreamReadResult<Uint8Array>>,
  state: StreamState,
): Promise<ReadableStreamReadResult<Uint8Array>> {
  try {
    await previous;
  } catch (error) {
    // The earlier read's failure belongs to its own caller.
  }
  return readOnce(state);
}

// Reads run one at a time, in call order, as WHATWG read requests do; a read
// with none in progress starts at once.
function startRead(state: StreamState): Promise<ReadableStreamReadResult<Uint8Array>> {
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
  state.content = null;
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

export class ReadableStream<R extends Uint8Array = Uint8Array> {
  private readonly state: StreamState;

  constructor(source?: UnderlyingSource<R>) {
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

  getReader(): ReadableStreamDefaultReader<R> {
    return new ReadableStreamDefaultReader<R>(this);
  }

  cancel(reason?: unknown): Promise<void> {
    if (this.state.locked) {
      return Promise.reject(new TypeError('Invalid state: ReadableStream is locked'));
    }
    return cancelState(this.state, reason);
  }

  // Async iteration (`for await`): reads every chunk and releases the lock;
  // leaving the loop early cancels the stream.
  async *[Symbol.asyncIterator](): AsyncGenerator<Uint8Array, void, undefined> {
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
  async *values(): AsyncGenerator<Uint8Array, void, undefined> {
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

export class ReadableStreamDefaultReader<R extends Uint8Array = Uint8Array> {
  private state: StreamState | null;

  constructor(stream: ReadableStream<R>) {
    const state = stream.stateOf();
    if (state.locked) {
      throw new TypeError('Invalid state: ReadableStream is locked');
    }
    state.locked = true;
    this.state = state;
  }

  read(): Promise<ReadableStreamReadResult<R>> {
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

// A closed stream over a body's whole content.
function contentStream(source: BodySource): ReadableStream<Uint8Array> {
  const stream = new ReadableStream<Uint8Array>();
  const state = stream.stateOf();
  state.content = source.content;
  state.bytes = source.bytes;
  return stream;
}

export class Request {
  readonly method: string;
  readonly url: string;
  readonly headers: Headers;
  readonly redirect: RequestRedirect;
  private readonly content: BodySource | null;
  // The signal the request follows; a request without one gets a signal
  // that never aborts, created when first read.
  private readonly given: AbortSignal | null;
  private fallback: AbortSignal | null;

  constructor(input: string | URL | Request, init?: RequestInit) {
    const base = input instanceof Request ? input : null;
    this.url = input instanceof Request ? input.url : input instanceof URL ? input.href : input;
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
    const body = given === null ? (base === null ? null : base.bodySource()) : bodySource(given);
    if (body !== null && (this.method === 'GET' || this.method === 'HEAD')) {
      throw new TypeError('Request with GET/HEAD method cannot have body.');
    }
    if (body !== null && body.type !== '' && !this.headers.has('content-type')) {
      this.headers.set('content-type', body.type);
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

  // Engine-internal: the request body, or null.
  bodySource(): BodySource | null {
    return this.content;
  }

  // Engine-internal: the signal the request follows, or null.
  signalOf(): AbortSignal | null {
    return this.given;
  }
}

function descriptor(method: string, url: string, headers: Headers, body: BodySource | null): string {
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
  if (body === null) {
    return text + NO_BODY + SEPARATOR;
  }
  return text + (body.bytes ? WITH_BYTES : WITH_BODY) + SEPARATOR + body.content;
}

export class Response {
  readonly status: number;
  readonly statusText: string;
  readonly headers: Headers;
  // Set by `fetch` for network responses (WHATWG: readonly).
  url: string;
  redirected: boolean;
  private readonly stream: ReadableStream<Uint8Array> | null;

  constructor(body?: BodyInit | ReadableStream<Uint8Array> | null, init?: ResponseInit) {
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
    let stream: ReadableStream<Uint8Array> | null = null;
    if (body instanceof ReadableStream) {
      stream = body;
    } else if (body !== undefined && body !== null) {
      const source = bodySource(body);
      stream = contentStream(source);
      if (source.type !== '' && !this.headers.has('content-type')) {
        this.headers.set('content-type', source.type);
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

  get body(): ReadableStream<Uint8Array> | null {
    return this.stream;
  }

  get bodyUsed(): boolean {
    const stream = this.stream;
    return stream !== null && stream.stateOf().disturbed;
  }

  // The body's state for one consumption, or null without a body.
  private consume(): StreamState | null {
    const stream = this.stream;
    if (stream === null) {
      return null;
    }
    const state = stream.stateOf();
    if (state.disturbed || state.locked) {
      throw new TypeError('Body is unusable: Body has already been read');
    }
    state.disturbed = true;
    state.locked = true;
    return state;
  }

  // The whole body as one byte string.
  private async byteString(): Promise<string> {
    const state = this.consume();
    if (state === null) {
      return '';
    }
    try {
      if (state.source !== null) {
        let bytes = '';
        for (;;) {
          const result = await pullQueue(state);
          if (result.done) {
            return bytes;
          }
          bytes += byteStringOf(result.value);
        }
      }
      let bytes = takeContent(state) ?? '';
      for (;;) {
        const queued = dequeue(state);
        if (queued === undefined) {
          break;
        }
        bytes += byteStringOf(queued);
      }
      for (;;) {
        const next = await networkChunk(state, false);
        if (next === null) {
          return bytes;
        }
        bytes += next;
      }
    } finally {
      state.locked = false;
    }
    // Unreachable: the loops only exit through a return or a throw.
    return '';
  }

  async text(): Promise<string> {
    const stream = this.stream;
    if (stream === null) {
      return '';
    }
    const state = stream.stateOf();
    // A network body, or text content, decodes without a byte step.
    if (state.source === null && pending(state) === 0 && (state.content === null || !state.bytes)) {
      this.consume();
      let text = state.content ?? '';
      state.content = null;
      try {
        for (;;) {
          const next = await networkChunk(state, true);
          if (next === null) {
            break;
          }
          text += next;
        }
      } finally {
        state.locked = false;
      }
      return text.startsWith(BYTE_ORDER_MARK) ? text.slice(1) : text;
    }
    const bytes = await this.byteString();
    return new TextDecoder().decode(bytesOf(bytes));
  }

  async json(): Promise<unknown> {
    return JSON.parse(await this.text());
  }

  async arrayBuffer(): Promise<ArrayBuffer> {
    const bytes = await this.byteString();
    return new ArrayBuffer(0).adopt(varta.text.units(bytes));
  }

  async bytes(): Promise<Uint8Array> {
    const bytes = await this.byteString();
    return bytesOf(bytes);
  }

  async blob(): Promise<Blob> {
    const type = this.headers.get('content-type');
    const bytes = await this.byteString();
    return new Blob().adopt(bytes, type === null ? '' : varta.web.asciiLowercase(type));
  }
}

// One network round trip; redirects are followed (each hop checked against
// the grant again), returned (`manual`) or refused (`error`, the default).
async function send(
  request: Request,
  url: string,
  method: string,
  body: BodySource | null,
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
  const stream = new ReadableStream<Uint8Array>();
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

export async function fetch(input: string | URL | Request, init?: RequestInit): Promise<Response> {
  const request = new Request(input, init);
  return send(request, request.url, request.method, request.bodySource(), 0);
}
