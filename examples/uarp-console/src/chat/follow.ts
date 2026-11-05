// Follows one session's live stream: opens it with the uarp-sdk client
// (`sessions.streamSessionEvents(sessionId, { event_schema: 1 })`), hands every
// frame to the dispatcher (dispatch.ts) and, when a connection ends, decides
// what comes next with the rules of stream.ts: `[DONE]` is judged by the frame
// before it (afterDone), an end without it is a drop (afterDrop), and a resume
// reconnects with the model's Last-Event-ID (resumeId).
//
// The SDK's EventStream reconnects by itself with the last SSE `id` it saw,
// which may name a frame that is not a run event (`run-1:added`, `run-1:done`).
// The follower keeps that decision: it lets the stream announce a reconnect
// (`onState` "reconnecting", which the SDK reports for a drop and for a failed
// connection, never after `[DONE]`), stops it there and opens a new stream with
// `Last-Event-ID` of the last run event the model applied. A stream that ends
// without announcing a reconnect therefore ended with `[DONE]`.
import { APIError, type EventStream, type RequestOptions, type StreamSessionEventsParams, type StreamState } from "uarp-sdk";
import { applyFrame } from "./dispatch.js";
import { afterDone, afterDrop, resumeId, type BeforeDone } from "./stream.js";
import type { ChatStore } from "./store.js";
import type { Segment } from "./timeline.js";

/** The `event_schema` the dispatcher parses strictly. */
export const EVENT_SCHEMA = 1;
const UNAUTHORIZED = 401;
/**
 * Reconnects the SDK may announce per stream. One is enough: the follower
 * stops the stream at the first announcement, and with at least one allowed
 * the SDK announces every drop instead of ending as if `[DONE]` had arrived.
 */
const SDK_RECONNECTS = 1;
const UNKNOWN_FAILURE = "stream failed";
/** The SDK announced a reconnect: the stream ended without `[DONE]` or did not open. */
const DROPPED = "the stream ended without [DONE] or did not open";

/**
 * Applying frames is the application's turn budget (architecture, long-lived
 * transfers): the engine's byte path and the SDK's TextDecoder take about two
 * thirds of the turn that receives a read of up to `net::MAX_READ_DELIVERY_BYTES`,
 * and that read holds dozens of frames. So the follower applies one frame per
 * event turn and lets the SDK split the rest of the read in later turns.
 */
function nextTurn(): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(() => resolve(), 0);
  });
}

/** The part of the client's `sessions` the follower uses. */
export interface SessionStreamApi {
  streamSessionEvents(sessionId: string, params: StreamSessionEventsParams, options: RequestOptions): EventStream;
}

export interface StreamApi {
  sessions: SessionStreamApi;
}

/** When the follower reconnects after a drop. */
export interface FollowOptions {
  /** Wait before the first reconnect in a row; doubled for each further one. */
  retryMs: number;
  /** Cap of the doubled wait. */
  maxRetryMs: number;
  /** Connections in a row that deliver no frame before the follower gives up. */
  maxReconnects: number;
}

/** The SDK's own EventStream defaults (base retry, backoff cap, reconnect budget). */
export const FOLLOW_DEFAULTS: FollowOptions = { retryMs: 2000, maxRetryMs: 8000, maxReconnects: 5 };

/** Why following ended. A resume never ends it. */
export type FollowEnd =
  /** `session_closed` and `[DONE]`: the session is over. */
  | { kind: "over" }
  /** A final `error` frame (the session does not exist). */
  | { kind: "failed"; code: string }
  /** `[DONE]` with nothing before it, or a 401: the credential is no longer valid. */
  | { kind: "authenticate" }
  /** `maxReconnects` connections in a row delivered nothing; `reason` is the last failure. */
  | { kind: "gave_up"; reason: string }
  /** A frame the dispatcher refused (strict parsing); never silently dropped. */
  | { kind: "invalid"; reason: string }
  /** `stop()` was called. */
  | { kind: "stopped" };

/** How one connection ended. */
type Ending =
  | { kind: "done"; before: BeforeDone }
  | { kind: "dropped"; reason: string }
  | { kind: "authenticate" }
  | { kind: "invalid"; reason: string }
  | { kind: "stopped" };

function describe(error: unknown): string {
  if (error instanceof Error) return error.name + ": " + error.message;
  return UNKNOWN_FAILURE;
}

export class SessionFollower {
  store: ChatStore;
  segment: Segment;
  api: StreamApi;
  options: FollowOptions;
  /** Connections opened so far, the first included. */
  connections: number = 0;
  stopped: boolean = false;
  /** Aborts the open connection or the wait before the next one. */
  control: AbortController = new AbortController();

  constructor(store: ChatStore, segment: Segment, api: StreamApi, options: FollowOptions) {
    this.store = store;
    this.segment = segment;
    this.api = api;
    this.options = options;
  }

  /** Stops following: the open stream is closed and no reconnect follows. */
  stop(): void {
    this.stopped = true;
    this.control.abort();
  }

  /** Follows the session until it is over, fails, needs a new credential or is stopped. */
  async follow(): Promise<FollowEnd> {
    let empty = 0;
    for (;;) {
      const delivered = { frames: 0 };
      const ending = await this.connect(delivered);
      switch (ending.kind) {
        case "stopped":
          return { kind: "stopped" };
        case "authenticate":
          return { kind: "authenticate" };
        case "invalid":
          return { kind: "invalid", reason: ending.reason };
        case "done": {
          const action = afterDone(ending.before);
          if (action === "over") return { kind: "over" };
          if (action === "authenticate") return { kind: "authenticate" };
          if (action === "failed") return { kind: "failed", code: ending.before.kind === "error" ? ending.before.frame.code : UNKNOWN_FAILURE };
          break;
        }
        case "dropped":
          if (afterDrop() !== "resume") return { kind: "gave_up", reason: ending.reason };
          break;
      }
      empty = delivered.frames > 0 ? 0 : empty + 1;
      if (empty >= this.options.maxReconnects) {
        return { kind: "gave_up", reason: ending.kind === "dropped" ? ending.reason : UNKNOWN_FAILURE };
      }
      if (!(await this.wait(this.delay(empty)))) return { kind: "stopped" };
    }
  }

  /** The wait before the next connection: `retryMs` after a connection that delivered, doubled per empty one. */
  delay(empty: number): number {
    let ms = this.options.retryMs;
    for (let i = 0; i < empty && ms < this.options.maxRetryMs; i++) ms = ms * 2;
    return Math.min(ms, this.options.maxRetryMs);
  }

  /** Waits `ms`; false when stopped meanwhile. */
  wait(ms: number): Promise<boolean> {
    const signal = this.control.signal;
    return new Promise<boolean>((resolve) => {
      if (signal.aborted) {
        resolve(false);
        return;
      }
      const stopped = (): void => {
        clearTimeout(timer);
        resolve(false);
      };
      const timer = setTimeout(() => {
        signal.removeEventListener("abort", stopped);
        resolve(true);
      }, ms);
      signal.addEventListener("abort", stopped, { once: true });
    });
  }

  /** One connection: applies its frames in order and reports how it ended. */
  async connect(delivered: { frames: number }): Promise<Ending> {
    if (this.stopped) return { kind: "stopped" };
    const connection = new AbortController();
    const parent = this.control.signal;
    const stopped = (): void => connection.abort();
    parent.addEventListener("abort", stopped, { once: true });
    const ending = await this.stream(connection, delivered);
    parent.removeEventListener("abort", stopped);
    return ending;
  }

  async stream(connection: AbortController, delivered: { frames: number }): Promise<Ending> {
    let dropped = false;
    const announced = (state: StreamState): void => {
      if (state.type !== "reconnecting") return;
      dropped = true;
      connection.abort();
    };
    const resume = resumeId(this.segment);
    const params: StreamSessionEventsParams =
      resume === null ? { event_schema: EVENT_SCHEMA } : { event_schema: EVENT_SCHEMA, "Last-Event-ID": resume };
    this.connections += 1;
    const stream = this.api.sessions.streamSessionEvents(this.segment.sessionId, params, {
      stream: { signal: connection.signal, reconnect: true, maxReconnects: SDK_RECONNECTS, onState: announced },
    });
    let before: BeforeDone = { kind: "nothing" };
    let refused: string | null = null;
    try {
      for await (const event of stream) {
        try {
          const outcome = applyFrame(this.store, this.segment, event.event, event.data);
          delivered.frames += 1;
          if (outcome.kind === "error") before = { kind: "error", frame: outcome.frame };
          else before = { kind: "frame", event: event.event };
        } catch (error) {
          refused = describe(error);
          break;
        }
        await nextTurn();
      }
    } catch (error) {
      if (this.stopped) return { kind: "stopped" };
      if (error instanceof APIError && error.status === UNAUTHORIZED) return { kind: "authenticate" };
      // The abort above surfaces here; a read that failed (a reset, the engine's
      // inactivity window) also ends the connection without `[DONE]`, as a drop does.
      return { kind: "dropped", reason: dropped ? DROPPED : describe(error) };
    }
    if (this.stopped) return { kind: "stopped" };
    if (refused !== null) return { kind: "invalid", reason: refused };
    if (dropped) return { kind: "dropped", reason: DROPPED };
    return { kind: "done", before: before };
  }
}
