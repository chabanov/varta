// What the client does when the session stream ends, from the served
// `x-event-stream["1"]` of GET /api/v1/sessions/{sessionId}/events. `[DONE]`
// means the server closed the stream on purpose and the frame before it says
// why; a stream that ends without it is resumed with Last-Event-ID.
import type { SessionStreamFrameV1Error } from "../../node_modules/uarp-sdk/src/generated/models.js";
import type { Segment } from "./timeline.js";

export type EndAction =
  /** The session is over: do not reconnect. */
  | "over"
  /** A final error (the session does not exist): do not reconnect. */
  | "failed"
  /** Reconnect with `resumeId`. */
  | "resume"
  /** The credential is no longer valid: authenticate, then reconnect. */
  | "authenticate";

const SESSION_CLOSED = "session_closed";

/** What came before `[DONE]`. */
export type BeforeDone =
  | { kind: "nothing" }
  | { kind: "error"; frame: SessionStreamFrameV1Error }
  | { kind: "frame"; event: string };

export function afterDone(before: BeforeDone): EndAction {
  switch (before.kind) {
    case "nothing":
      return "authenticate";
    case "error":
      return before.frame.code === "SESSION_NOT_FOUND" ? "failed" : "resume";
    case "frame":
      return before.event === SESSION_CLOSED ? "over" : "resume";
  }
}

/** The stream ended without `[DONE]` (the two-hour cap, a dropped connection). */
export function afterDrop(): EndAction {
  return "resume";
}

/** The Last-Event-ID to reconnect with; null replays the whole session. */
export function resumeId(s: Segment): string | null {
  return s.lastEventId;
}
