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
//
// A connection that fails to open is the exception. The SDK announces it before
// `connected` and keeps the error to itself, so the follower lets the SDK make
// its one reconnect (the same request: nothing was delivered, so the same
// Last-Event-ID) and the SDK throws the error of the second failure. A 401, a
// status outside the SDK's retryable set and a capability denial end following
// with that error; any other failure counts as a connection without progress.
//
// Only run and session frames are progress. The API sends `connected` at the
// start of every connection, and an `error` frame before `[DONE]` asks for a
// resume, so a connection that delivers nothing else makes no progress, as one
// that delivers nothing at all; `maxReconnects` of them in a row end following.
import { APIError, type EventStream, type RequestOptions, type StreamSessionEventsParams, type StreamState } from "uarp-sdk";
import { applyFrame, type FrameOutcome } from "./dispatch.js";
import { describeFailure } from "./failure.js";
import { afterDone, afterDrop, resumeId, type BeforeDone } from "./stream.js";
import type { ChatStore } from "./store.js";
import type { Segment } from "./timeline.js";

/** The `event_schema` the dispatcher parses strictly. */
export const EVENT_SCHEMA = 1;
const UNAUTHORIZED = 401;
/**
 * The statuses the SDK's Transport retries (its `RETRYABLE_STATUS`, which it
 * does not export): a stream that fails to open with one of them may open
 * later. Any other status repeats, as a capability denial does.
 */
const RETRYABLE_STATUSES: number[] = [408, 429, 500, 502, 503, 504];
/** The name of the DOMException the engine rejects an ungranted request with. */
const CAPABILITY_DENIED = "NotAllowedError";
/**
 * Reconnects the SDK may make per stream. One: the follower stops the stream
 * at the announcement of a drop, and with at least one allowed the SDK
 * announces every drop instead of ending as if `[DONE]` had arrived; a failed
 * open is reconnected once, and the second failure surfaces its error.
 */
const SDK_RECONNECTS = 1;
/** The SDK announced a reconnect after the stream opened. */
const DROPPED = "the stream ended without [DONE]";
/**
 * The stream opened only at the SDK's reconnect and ended without a frame and
 * without an announcement. The SDK then ends silently both after `[DONE]` and
 * after a plain end (its reconnect budget is spent), so this is not taken for
 * `[DONE]`.
 */
const SILENT_AFTER_REOPEN = "the stream reopened and ended without a frame";
const DONE_AFTER = "[DONE] after ";
const ERROR_FRAME = "error ";

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
  /** Connections in a row without a run or session frame before the follower gives up. */
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
  /** `maxReconnects` connections in a row made no progress; `reason` is how the last one ended. */
  | { kind: "gave_up"; reason: string }
  /** The stream did not open, and the same request would fail again (a status, a capability denial). */
  | { kind: "refused"; reason: string }
  /** A frame the dispatcher refused (strict parsing); never silently dropped. */
  | { kind: "invalid"; reason: string }
  /** `stop()` was called. */
  | { kind: "stopped" };

/** How one connection ended. */
type Ending =
  | { kind: "done"; before: BeforeDone }
  | { kind: "dropped"; reason: string }
  | { kind: "authenticate" }
  | { kind: "refused"; reason: string }
  | { kind: "invalid"; reason: string }
  | { kind: "stopped" };

/** Run and session frames; `connected` and `error` are not progress. */
function progresses(outcome: FrameOutcome): boolean {
  return outcome.kind === "applied" || outcome.kind === "closed";
}

/** A failed open that the same request would repeat. */
function permanent(error: unknown): boolean {
  if (error instanceof APIError) return !RETRYABLE_STATUSES.includes(error.status);
  return error instanceof Error && error.name === CAPABILITY_DENIED;
}

/** How a connection that leads to a reconnect ended, for the give-up note. */
function endReason(ending: Ending): string {
  switch (ending.kind) {
    case "dropped":
      return ending.reason;
    case "done":
      if (ending.before.kind === "error") return DONE_AFTER + ERROR_FRAME + ending.before.frame.code;
      if (ending.before.kind === "frame") return DONE_AFTER + ending.before.event;
      return DONE_AFTER;
    default:
      return ending.kind;
  }
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
      const progress = { frames: 0 };
      const ending = await this.connect(progress);
      switch (ending.kind) {
        case "stopped":
          return { kind: "stopped" };
        case "authenticate":
          return { kind: "authenticate" };
        case "refused":
          return { kind: "refused", reason: ending.reason };
        case "invalid":
          return { kind: "invalid", reason: ending.reason };
        case "done": {
          const action = afterDone(ending.before);
          if (action === "over") return { kind: "over" };
          if (action === "authenticate") return { kind: "authenticate" };
          if (action === "failed") return { kind: "failed", code: ending.before.kind === "error" ? ending.before.frame.code : endReason(ending) };
          break;
        }
        case "dropped":
          if (afterDrop() !== "resume") return { kind: "gave_up", reason: ending.reason };
          break;
      }
      empty = progress.frames > 0 ? 0 : empty + 1;
      if (empty >= this.options.maxReconnects) return { kind: "gave_up", reason: endReason(ending) };
      if (!(await this.wait(this.delay(empty)))) return { kind: "stopped" };
    }
  }

  /** The wait before the next connection: `retryMs` after a connection that made progress, doubled per one that did not. */
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
  async connect(progress: { frames: number }): Promise<Ending> {
    if (this.stopped) return { kind: "stopped" };
    const connection = new AbortController();
    const parent = this.control.signal;
    const stopped = (): void => connection.abort();
    parent.addEventListener("abort", stopped, { once: true });
    const ending = await this.stream(connection, progress);
    parent.removeEventListener("abort", stopped);
    return ending;
  }

  /** One stream: `progress.frames` counts its run and session frames. */
  async stream(connection: AbortController, progress: { frames: number }): Promise<Ending> {
    // What the SDK announced: `connected`, and a reconnect before it (a failed
    // open, left to the SDK) or after it (a drop, which stops the stream here).
    let opened = false;
    let reopened = false;
    let dropped = false;
    const announced = (state: StreamState): void => {
      if (state.type === "connected") {
        opened = true;
        return;
      }
      if (state.type !== "reconnecting") return;
      if (!opened) {
        reopened = true;
        return;
      }
      dropped = true;
      connection.abort();
    };
    const resume = resumeId(this.segment);
    const params: StreamSessionEventsParams =
      resume === null ? { event_schema: EVENT_SCHEMA } : { event_schema: EVENT_SCHEMA, "Last-Event-ID": resume };
    this.connections += 1;
    const stream = this.api.sessions.streamSessionEvents(this.segment.sessionId, params, {
      stream: {
        signal: connection.signal,
        reconnect: true,
        maxReconnects: SDK_RECONNECTS,
        baseRetryMillis: this.options.retryMs,
        maxBackoffMillis: this.options.maxRetryMs,
        onState: announced,
      },
    });
    let before: BeforeDone = { kind: "nothing" };
    let frames = 0;
    let refused: string | null = null;
    try {
      for await (const event of stream) {
        frames += 1;
        try {
          const outcome = applyFrame(this.store, this.segment, event.event, event.data);
          if (progresses(outcome)) progress.frames += 1;
          if (outcome.kind === "error") before = { kind: "error", frame: outcome.frame };
          else before = { kind: "frame", event: event.event };
        } catch (error) {
          refused = describeFailure(error);
          break;
        }
        await nextTurn();
      }
    } catch (error) {
      if (this.stopped) return { kind: "stopped" };
      // The abort at the announcement of a drop surfaces here.
      if (dropped) return { kind: "dropped", reason: DROPPED };
      if (error instanceof APIError && error.status === UNAUTHORIZED) return { kind: "authenticate" };
      // The second failed open: a failure the same request repeats ends following.
      if (!opened && permanent(error)) return { kind: "refused", reason: describeFailure(error) };
      // A transient status, a connection error, or a read that failed (a reset,
      // the engine's inactivity window): an end without `[DONE]`, as a drop is.
      return { kind: "dropped", reason: describeFailure(error) };
    }
    if (this.stopped) return { kind: "stopped" };
    if (refused !== null) return { kind: "invalid", reason: refused };
    if (dropped) return { kind: "dropped", reason: DROPPED };
    if (reopened && frames === 0) return { kind: "dropped", reason: SILENT_AFTER_REOPEN };
    return { kind: "done", before: before };
  }
}
