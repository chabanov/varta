// Chat timeline tests on recorded event_schema=1 frames. Each frame is parsed
// strictly into its closed uarp-sdk type, exactly as the client will parse the
// stream, and then applied by its handler.
import type {
  RunEventV1ChildRunCompleted,
  RunEventV1LLMChunk,
  RunEventV1RunAwaitingApproval,
  RunEventV1RunAwaitingInput,
  RunEventV1RunCancelled,
  RunEventV1RunCompleted,
  RunEventV1RunFailed,
  RunEventV1RunInputReceived,
  RunEventV1RunStarted,
  RunEventV1SessionContext,
  RunEventV1ToolCall,
  RunEventV1ToolResult,
  SessionStreamFrameV1RunAdded,
  SessionStreamFrameV1Error,
  SessionStreamFrameV1RunDone,
  SessionStreamFrameV1SessionClosed,
} from "../../node_modules/uarp-sdk/src/generated/models.js";
import {
  timeline,
  openSegment,
  ask,
  askSent,
  onRunAdded,
  onRunDone,
  onSessionClosed,
  onRunStarted,
  onChunk,
  onToolCall,
  onToolResult,
  onAwaitingApproval,
  onAwaitingInput,
  onInputReceived,
  onRunCompleted,
  onRunFailed,
  onRunCancelled,
  onChildRunCompleted,
  onSessionContext,
  type Timeline,
  type Segment,
} from "../chat/timeline.js";
import { show, showSegment } from "../chat/show.js";
import { afterDone, afterDrop, resumeId } from "../chat/stream.js";
import { applyFrame } from "../chat/dispatch.js";
import { ChatStore } from "../chat/store.js";
import { Runtime } from "../l1/reactive.js";
import {
  APPROVE_AWAITING,
  APPROVE_RESTARTED,
  APPROVE_RESUMED,
  APPROVE_STARTED,
  APPROVE_TOOL_CALL,
  APPROVE_TOOL_CHUNK,
  APPROVE_TOOL_RESULT,
  ASK_AWAITING,
  ASK_RECEIVED,
  DELEGATION_CHILD,
  END_SESSION_CLOSED,
  ERROR_CANCELLED,
  PLAIN_COMPLETED,
  PLAIN_CONTENT,
  PLAIN_CONTEXT,
  PLAIN_RUN_ADDED,
  PLAIN_RUN_DONE,
  PLAIN_STARTED,
  PLAIN_THINKING_1,
  PLAIN_THINKING_2,
  REJECT_AWAITING,
  REJECT_FAILED,
  WIRE_SESSION_NOT_FOUND,
} from "./frames.js";
import { equal, type Report } from "./check.js";

const RUN_ID = "run-1";
const SESSION_ID = "session-1";
const LOCAL_ID = "local-1";

function live(): Segment {
  return openSegment(timeline(), SESSION_ID);
}

function started(s: Segment, text: string): void {
  onRunStarted(s, JSON.parse(text) as RunEventV1RunStarted);
}

function chunk(s: Segment, text: string): void {
  onChunk(s, JSON.parse(text) as RunEventV1LLMChunk);
}

function plainTurn(t: Timeline, s: Segment): void {
  ask(s, LOCAL_ID, "Reply with the single word: ok");
  askSent(s, LOCAL_ID, RUN_ID);
  onRunAdded(s, JSON.parse(PLAIN_RUN_ADDED) as SessionStreamFrameV1RunAdded);
  started(s, PLAIN_STARTED);
  chunk(s, PLAIN_THINKING_1);
  chunk(s, PLAIN_THINKING_2);
  chunk(s, PLAIN_CONTENT);
  onSessionContext(t, s, JSON.parse(PLAIN_CONTEXT) as RunEventV1SessionContext);
  onRunCompleted(s, JSON.parse(PLAIN_COMPLETED) as RunEventV1RunCompleted);
  onRunDone(s, JSON.parse(PLAIN_RUN_DONE) as SessionStreamFrameV1RunDone);
}

export function chatTurnTests(r: Report): void {
  const t = timeline();
  const s = openSegment(t, SESSION_ID);
  plainTurn(t, s);
  equal(
    r,
    "chat: plain answer",
    show(t),
    '(timeline (context 42343/524288) (segment live (ask sent "Reply with the single word: ok") (turn completed (thinking 28) (answer "ok"))))',
  );

  const replay = live();
  onRunAdded(replay, JSON.parse(PLAIN_RUN_ADDED) as SessionStreamFrameV1RunAdded);
  started(replay, PLAIN_STARTED);
  chunk(replay, PLAIN_THINKING_1);
  started(replay, PLAIN_STARTED);
  chunk(replay, PLAIN_THINKING_1);
  chunk(replay, PLAIN_THINKING_2);
  chunk(replay, PLAIN_THINKING_1);
  equal(r, "chat: replayed frames are dropped by seq", showSegment(replay), "(segment live (turn running (thinking 28)))");

  const skipped = live();
  started(skipped, PLAIN_STARTED);
  chunk(skipped, PLAIN_THINKING_1);
  onRunCompleted(skipped, JSON.parse(PLAIN_COMPLETED) as RunEventV1RunCompleted);
  equal(r, "chat: completed output replaces skipped chunks", showSegment(skipped), '(segment live (turn completed (thinking 3) (answer "ok")))');

  const cancelled = live();
  onRunAdded(cancelled, JSON.parse(PLAIN_RUN_ADDED) as SessionStreamFrameV1RunAdded);
  onRunCancelled(cancelled, JSON.parse(ERROR_CANCELLED) as RunEventV1RunCancelled);
  equal(r, "chat: cancelled run", showSegment(cancelled), '(segment live (turn cancelled (failure cancelled "Stopped by a person." RUN_CANCELLED)))');

  onSessionClosed(s, JSON.parse(END_SESSION_CLOSED) as SessionStreamFrameV1SessionClosed);
  equal(r, "chat: session closed", s.state, "closed");
}

export function chatWaitTests(r: Report): void {
  const approved = live();
  started(approved, APPROVE_STARTED);
  chunk(approved, APPROVE_TOOL_CHUNK);
  onAwaitingApproval(approved, JSON.parse(APPROVE_AWAITING) as RunEventV1RunAwaitingApproval);
  equal(
    r,
    "chat: approval requested",
    showSegment(approved),
    '(segment live (turn awaiting_approval (approval open (call manage_permissions {"action":"get","agent_id":"agent-probe"}))))',
  );
  started(approved, APPROVE_RESUMED);
  started(approved, APPROVE_RESTARTED);
  onToolCall(approved, JSON.parse(APPROVE_TOOL_CALL) as RunEventV1ToolCall);
  onToolResult(approved, JSON.parse(APPROVE_TOOL_RESULT) as RunEventV1ToolResult);
  equal(
    r,
    "chat: approved tool runs",
    showSegment(approved),
    '(segment live (turn running (tool manage_permissions done {"action":"get","agent_id":"agent-probe"} 12ms "{\\"agent_id\\":\\"agent-probe\\",\\"exists\\":false,\\"permissions\\":{}}") (approval approved (call manage_permissions {"action":"get","agent_id":"agent-probe"}))))',
  );

  const rejected = live();
  onAwaitingApproval(rejected, JSON.parse(REJECT_AWAITING) as RunEventV1RunAwaitingApproval);
  onRunFailed(rejected, JSON.parse(REJECT_FAILED) as RunEventV1RunFailed);
  equal(
    r,
    "chat: rejected by the person",
    showSegment(rejected),
    '(segment live (turn failed (approval rejected (call manage_permissions {"action":"get","agent_id":"agent-probe"})) (failure rejected "Not now." approval_rejected)))',
  );

  const question = live();
  onAwaitingInput(question, JSON.parse(ASK_AWAITING) as RunEventV1RunAwaitingInput);
  equal(r, "chat: question asked", showSegment(question), '(segment live (turn awaiting_input (question open "A or B?" "A" "B")))');
  onInputReceived(question, JSON.parse(ASK_RECEIVED) as RunEventV1RunInputReceived);
  equal(r, "chat: question answered", showSegment(question), '(segment live (turn running (question sent "A or B?" "A" "B")))');

  const delegated = live();
  onChildRunCompleted(delegated, JSON.parse(DELEGATION_CHILD) as RunEventV1ChildRunCompleted);
  equal(r, "chat: delegated child run", showSegment(delegated), '(segment live (turn queued (child "Canon Research Agent" completed "ok")))');
}

function rejectsAsStarted(text: string): string {
  try {
    started(live(), text);
    return "accepted";
  } catch (error) {
    return "rejected";
  }
}

function rejectsAsChunk(text: string): string {
  try {
    chunk(live(), text);
    return "accepted";
  } catch (error) {
    return "rejected";
  }
}

export function chatStrictTests(r: Report): void {
  equal(r, "chat: recorded frame parses", rejectsAsStarted(PLAIN_STARTED), "accepted");
  equal(r, "chat: unknown payload field is rejected", rejectsAsStarted(PLAIN_STARTED.replace('"payload":{}', '"payload":{"surprise":1}')), "rejected");
  equal(r, "chat: wrong event type is rejected", rejectsAsStarted(PLAIN_CONTENT), "rejected");
  equal(r, "chat: unknown chunk type is rejected", rejectsAsChunk(PLAIN_CONTENT.replace('"chunk_type":"content"', '"chunk_type":"image"')), "rejected");
  equal(r, "chat: missing required field is rejected", rejectsAsChunk(PLAIN_CONTENT.replace('"delta":"ok",', "")), "rejected");
}

function sessionError(text: string): SessionStreamFrameV1Error {
  return JSON.parse(text) as SessionStreamFrameV1Error;
}

function rejectsAsError(text: string): string {
  try {
    sessionError(text);
    return "accepted";
  } catch (error) {
    return "rejected";
  }
}

export function chatStreamTests(r: Report): void {
  const fresh = live();
  const before = resumeId(fresh);
  equal(r, "stream: no resume id before any event", before === null ? "null" : before, "null");

  const t = timeline();
  const s = openSegment(t, SESSION_ID);
  plainTurn(t, s);
  const after = resumeId(s);
  equal(r, "stream: resume id is the last new run event", after === null ? "null" : after, "run-1:29");
  started(s, PLAIN_STARTED);
  const replayed = resumeId(s);
  equal(r, "stream: a replayed event does not move it", replayed === null ? "null" : replayed, "run-1:29");

  equal(r, "stream: [DONE] with nothing before it", afterDone({ kind: "nothing" }), "authenticate");
  equal(r, "stream: [DONE] after session_closed", afterDone({ kind: "frame", event: "session_closed" }), "over");
  equal(r, "stream: [DONE] after another frame", afterDone({ kind: "frame", event: "run_done" }), "resume");
  equal(r, "stream: [DONE] after a missing session", afterDone({ kind: "error", frame: sessionError(WIRE_SESSION_NOT_FOUND) }), "failed");
  equal(r, "stream: [DONE] after a stream error", afterDone({ kind: "error", frame: sessionError(WIRE_SESSION_NOT_FOUND.replace("SESSION_NOT_FOUND", "STREAM_ERROR")) }), "resume");
  equal(r, "stream: ended without [DONE]", afterDrop(), "resume");
  equal(r, "stream: unknown error code is rejected", rejectsAsError(WIRE_SESSION_NOT_FOUND.replace("SESSION_NOT_FOUND", "TEAPOT")), "rejected");
  equal(r, "stream: error without code is rejected", rejectsAsError("{\"error\":\"Session not found\"}"), "rejected");
}

function dispatched(event: string, data: string): string {
  const store = new ChatStore(new Runtime());
  const s = store.open(SESSION_ID);
  try {
    return applyFrame(store, s, event, data).kind;
  } catch (error) {
    return "rejected";
  }
}

export function chatDispatchTests(r: Report): void {
  equal(r, "dispatch: a run event under its own name", dispatched("run.started", PLAIN_STARTED), "applied");
  equal(r, "dispatch: a run event under another name is rejected", dispatched("llm.chunk", PLAIN_STARTED), "rejected");
  equal(r, "dispatch: an unknown event name is rejected", dispatched("run.affect", PLAIN_STARTED), "rejected");
  equal(r, "dispatch: the terminator ends the stream", dispatched("message", "[DONE]"), "done");
  equal(r, "dispatch: any other message frame is rejected", dispatched("message", "bye"), "rejected");
  equal(r, "dispatch: a session error is reported", dispatched("error", WIRE_SESSION_NOT_FOUND), "error");
}
