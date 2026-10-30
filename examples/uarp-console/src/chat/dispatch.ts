// One frame of the session stream (GET /api/v1/sessions/{sessionId}/events?
// event_schema=1), as the wire delivers it: the SSE event name and the data
// text. Every frame is parsed strictly into its closed uarp-sdk type; a run
// event must carry the type its event name announces. Anything else (an unknown
// event name, an unknown field, a wrong discriminant) throws: a frame is never
// silently dropped.
import type {
  RunEventV1,
  SessionStreamFrameV1Connected,
  SessionStreamFrameV1Error,
  SessionStreamFrameV1RunAdded,
  SessionStreamFrameV1RunDone,
  SessionStreamFrameV1SessionClosed,
} from "../../node_modules/uarp-sdk/src/generated/models.js";
import {
  onRunAdded,
  onRunDone,
  onSessionClosed,
  onRunStarted,
  onChunk,
  onToolCall,
  onToolResult,
  onToolError,
  onAwaitingApproval,
  onAwaitingInput,
  onInputReceived,
  onRunPaused,
  onRunCompleted,
  onRunFailed,
  onRunCancelled,
  onGuardrailBlocked,
  onChildRunCompleted,
  onChildRunFailed,
  onContextCompacted,
  onSessionContext,
  onUnshown,
  type Segment,
} from "./timeline.js";
import type { ChatStore } from "./store.js";

const TERMINATOR_EVENT = "message";
const TERMINATOR_DATA = "[DONE]";

/** What a frame meant for the stream; `done` and `error` end it (stream.ts decides what next). */
export type FrameOutcome =
  | { kind: "applied" }
  | { kind: "connected" }
  | { kind: "closed" }
  | { kind: "error"; frame: SessionStreamFrameV1Error }
  | { kind: "done" };

export class FrameError extends Error {}

function applyRunEvent(store: ChatStore, s: Segment, e: RunEventV1): void {
  store.change(s, e.run_id, () => {
    switch (e.type) {
      case "run.started":
        onRunStarted(s, e);
        break;
      case "llm.chunk":
        onChunk(s, e);
        break;
      case "tool.call":
        onToolCall(s, e);
        break;
      case "tool.result":
        onToolResult(s, e);
        break;
      case "tool.error":
        onToolError(s, e);
        break;
      case "run.awaiting_approval":
        onAwaitingApproval(s, e);
        break;
      case "run.awaiting_input":
        onAwaitingInput(s, e);
        break;
      case "run.input_received":
        onInputReceived(s, e);
        break;
      case "run.paused":
        onRunPaused(s, e);
        break;
      case "run.completed":
        onRunCompleted(s, e);
        break;
      case "run.failed":
        onRunFailed(s, e);
        break;
      case "run.cancelled":
        onRunCancelled(s, e);
        break;
      case "run.guardrail_blocked":
        onGuardrailBlocked(s, e);
        break;
      case "child_run.completed":
        onChildRunCompleted(s, e);
        break;
      case "child_run.failed":
        onChildRunFailed(s, e);
        break;
      case "session.context_compacted":
        onContextCompacted(s, e);
        break;
      case "session.context":
        onSessionContext(store.timeline, s, e);
        break;
      case "llm.error":
      case "run.step_started":
      case "run.step_completed":
      case "step.plan":
      case "step.evaluate":
      case "step.act":
      case "team.delegate":
        onUnshown(s, e.run_id, e.seq);
        break;
    }
  });
}

/** Applies one session-stream frame to the segment it belongs to. */
export function applyFrame(store: ChatStore, s: Segment, event: string, data: string): FrameOutcome {
  switch (event) {
    case TERMINATOR_EVENT:
      if (data !== TERMINATOR_DATA) throw new FrameError("unexpected message frame");
      return { kind: "done" };
    case "connected":
      JSON.parse(data) as SessionStreamFrameV1Connected;
      return { kind: "connected" };
    case "run_added": {
      const f = JSON.parse(data) as SessionStreamFrameV1RunAdded;
      store.change(s, f.run_id, () => onRunAdded(s, f));
      return { kind: "applied" };
    }
    case "run_done": {
      const f = JSON.parse(data) as SessionStreamFrameV1RunDone;
      store.change(s, f.run_id, () => onRunDone(s, f));
      return { kind: "applied" };
    }
    case "session_closed": {
      const f = JSON.parse(data) as SessionStreamFrameV1SessionClosed;
      store.change(s, null, () => onSessionClosed(s, f));
      return { kind: "closed" };
    }
    case "error":
      return { kind: "error", frame: JSON.parse(data) as SessionStreamFrameV1Error };
  }
  const e = JSON.parse(data) as RunEventV1;
  if (e.type !== event) throw new FrameError("event " + event + " carries a " + e.type + " run event");
  applyRunEvent(store, s, e);
  return { kind: "applied" };
}
