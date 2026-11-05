// The SDK's `fetch` for the console's end-to-end tests (ClientOptions.fetch, the
// SDK's documented hook for instrumentation and tests). It delegates every
// request to the engine's fetch unchanged and records it. Each session-stream
// connection is numbered in `x-loopback-attempt` for the stateless mock and,
// when holding, waits until the test releases it: the mock answers a stream at
// once and whole, so holding is what puts the person's actions between two
// connections in a fixed order, as a live server would interleave them.
import { ATTEMPT_HEADER } from "./sdk/contract.js";

const EVENT_STREAM_TYPE = "text/event-stream";
const NO_RESUME = "-";
const ABORT_ERROR = "AbortError";
const ABORTED = "the stream connection was aborted while held";

export class GatedFetch {
  /** Hold each stream connection until `release()`. */
  hold: boolean;
  /** Every request in order: method and path (stream connections included). */
  requests: string[] = [];
  /** `Last-Event-ID` of each stream connection, `-` when it had none. */
  resumes: string[] = [];
  /** Releases of connections not yet requested. */
  released: number = 0;
  /** Held connections in request order; the first `woken` have been let go. */
  private waiting: (() => void)[] = [];
  private woken: number = 0;
  private watchers: { count: number; resolve: () => void }[] = [];

  constructor(hold: boolean) {
    this.hold = hold;
  }

  fetch(input: string, init: RequestInit): Promise<Response> {
    const headers = new Headers(init.headers);
    const url = new URL(input);
    this.requests.push(String(init.method) + " " + url.pathname);
    if (headers.get("accept") !== EVENT_STREAM_TYPE) return this.send(input, init, headers);
    const resume = headers.get("last-event-id");
    this.resumes.push(resume === null ? NO_RESUME : resume);
    headers.set(ATTEMPT_HEADER, String(this.resumes.length));
    this.notify();
    if (!this.hold || this.released >= this.resumes.length) return this.send(input, init, headers);
    const signal = init.signal;
    return new Promise<void>((resolve, reject) => {
      this.waiting.push(resolve);
      if (signal !== null && signal !== undefined) {
        signal.addEventListener("abort", () => reject(new DOMException(ABORTED, ABORT_ERROR)), { once: true });
      }
    }).then((): Promise<Response> => this.send(input, init, headers));
  }

  /** Lets the next stream connection reach the network (now, or when it is requested). */
  release(): void {
    this.released += 1;
    if (this.woken < this.waiting.length) {
      const next = this.waiting[this.woken]!;
      this.woken += 1;
      next();
    }
  }

  /** Resolves once `count` stream connections have been requested. */
  connections(count: number): Promise<void> {
    return new Promise<void>((resolve) => {
      this.watchers.push({ count: count, resolve: resolve });
      this.notify();
    });
  }

  private notify(): void {
    const pending: { count: number; resolve: () => void }[] = [];
    for (const watcher of this.watchers) {
      if (this.resumes.length >= watcher.count) watcher.resolve();
      else pending.push(watcher);
    }
    this.watchers = pending;
  }

  private send(input: string, init: RequestInit, headers: Headers): Promise<Response> {
    return fetch(input, { method: init.method, headers: headers, body: init.body, signal: init.signal, redirect: init.redirect });
  }
}
