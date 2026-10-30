// Chat timeline model and its reducer (design/chat-timeline.md). Each handler
// takes one closed event_schema=1 type from uarp-sdk, so every field it reads is
// checked against the served contract. The handlers are pure and deterministic:
// the same frames in the same order always give the same timeline.
//
// The dispatcher over the whole RunEventV1 union is not here yet: the union has
// 24 record members and waits for Varta v38 wave 2 (structural unions above 16).
import type {
  RunEventV1ChildRunCompleted,
  RunEventV1ChildRunFailed,
  RunEventV1LLMChunk,
  RunEventV1RunAwaitingApproval,
  RunEventV1RunAwaitingInput,
  RunEventV1RunCancelled,
  RunEventV1RunCompleted,
  RunEventV1RunFailed,
  RunEventV1RunGuardrailBlocked,
  RunEventV1RunInputReceived,
  RunEventV1RunPaused,
  RunEventV1RunStarted,
  RunEventV1SessionContext,
  RunEventV1SessionContextCompacted,
  RunEventV1ToolCall,
  RunEventV1ToolError,
  RunEventV1ToolResult,
  SessionStreamFrameV1RunAdded,
  SessionStreamFrameV1RunDone,
  SessionStreamFrameV1SessionClosed,
} from "../../node_modules/uarp-sdk/src/generated/models.js";

export type TurnState =
  | "queued"
  | "running"
  | "awaiting_approval"
  | "awaiting_input"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled"
  | "blocked";

export interface ToolCard {
  callId: string;
  name: string;
  /** Compact JSON text of the arguments, for display only. */
  arguments: string;
  state: "running" | "done" | "error";
  /** Result or error text. */
  result: string;
  latencyMs: number | null;
}

export interface Approval {
  kind: "approval";
  calls: ToolCard[];
  decision: "open" | "sending" | "approved" | "rejected";
}

export interface Question {
  kind: "question";
  callId: string;
  text: string;
  options: string[];
  answer: "open" | "sending" | "sent";
}

export type Wait = Approval | Question;

export interface ChildCard {
  childRunId: string;
  agentName: string;
  state: "completed" | "failed";
  preview: string;
}

export interface Failure {
  reason: "error" | "rejected" | "cancelled" | "blocked";
  message: string;
  /** Refines the reason; never decides it. */
  code: string | null;
}

/** The person's message. `id` is the run id once the POST answered; `key` never changes. */
export interface Ask {
  kind: "ask";
  key: string;
  id: string;
  text: string;
  state: "sending" | "sent" | "failed";
}

/** One run of the Head Agent. */
export interface Turn {
  kind: "turn";
  runId: string;
  state: TurnState;
  /** Highest seq applied; anything at or below it is a replay and is dropped. */
  lastSeq: number;
  thinking: string;
  answer: string;
  tools: ToolCard[];
  children: ChildCard[];
  wait: Wait | null;
  failure: Failure | null;
  /** Loaded from a finished transcript: the stream's replay of this run is dropped. */
  fromHistory: boolean;
}

export interface Note {
  kind: "note";
  id: string;
  text: string;
}

export type Entry = Ask | Turn | Note;

export interface Segment {
  sessionId: string;
  state: "live" | "closed" | "history";
  entries: Entry[];
  /** `<runId>:<seq>` of the last new run event, for Last-Event-ID; null before any. */
  lastEventId: string | null;
}

export interface ContextMeter {
  usedTokens: number;
  windowTokens: number;
}

export interface Timeline {
  /** Oldest first; the last one may be live. */
  segments: Segment[];
  context: ContextMeter | null;
}

const RESPONSE_KEY = "response";

export function timeline(): Timeline {
  return { segments: [], context: null };
}

export function openSegment(t: Timeline, sessionId: string): Segment {
  const segment: Segment = { sessionId: sessionId, state: "live", entries: [], lastEventId: null };
  t.segments.push(segment);
  return segment;
}

/** Key of an entry in its segment: stable for the entry's whole life. */
export function entryKey(e: Entry): string {
  switch (e.kind) {
    case "ask":
      return "ask:" + e.key;
    case "turn":
      return "turn:" + e.runId;
    case "note":
      return "note:" + e.id;
  }
}

/** Notes are append-only, so their position in the segment names them. */
export function addNote(s: Segment, text: string): void {
  s.entries.push({ kind: "note", id: String(s.entries.length), text: text });
}

// ------------------------------------------------------------------ the person

/** The person sends a message; `localId` names it until the POST answers. */
export function ask(s: Segment, localId: string, text: string): void {
  s.entries.push({ kind: "ask", key: localId, id: localId, text: text, state: "sending" });
}

/** The POST answered with the run it started. */
export function askSent(s: Segment, localId: string, runId: string): void {
  const a = findAsk(s, localId);
  if (a === undefined) return;
  a.id = runId;
  a.state = "sent";
}

export function askFailed(s: Segment, localId: string): void {
  const a = findAsk(s, localId);
  if (a !== undefined) a.state = "failed";
}

function findAsk(s: Segment, id: string): Ask | undefined {
  for (const e of s.entries) {
    if (e.kind === "ask" && e.id === id) return e;
  }
  return undefined;
}

// ------------------------------------------------------------------- turns

function findTurn(s: Segment, runId: string): Turn | undefined {
  for (const e of s.entries) {
    if (e.kind === "turn" && e.runId === runId) return e;
  }
  return undefined;
}

function turnFor(s: Segment, runId: string): Turn {
  const found = findTurn(s, runId);
  if (found !== undefined) return found;
  const created: Turn = {
    kind: "turn",
    runId: runId,
    state: "queued",
    lastSeq: 0,
    thinking: "",
    answer: "",
    tools: [],
    children: [],
    wait: null,
    failure: null,
    fromHistory: false,
  };
  s.entries.push(created);
  return created;
}

/** The turn of a run event, or undefined when the event is a replay. */
function accept(s: Segment, runId: string, seq: number): Turn | undefined {
  const turn = turnFor(s, runId);
  if (turn.fromHistory || seq <= turn.lastSeq) return undefined;
  turn.lastSeq = seq;
  s.lastEventId = runId + ":" + String(seq);
  return turn;
}

function isTerminal(state: TurnState): boolean {
  return state === "completed" || state === "failed" || state === "cancelled" || state === "blocked";
}

function findTool(turn: Turn, callId: string): ToolCard | undefined {
  for (const card of turn.tools) {
    if (card.callId === callId) return card;
  }
  return undefined;
}

/** The person decided an approval; the decision is sent, then the stream confirms it. */
export function decide(s: Segment, runId: string): boolean {
  const turn = findTurn(s, runId);
  if (turn === undefined || turn.wait === null || turn.wait.kind !== "approval" || turn.wait.decision !== "open") return false;
  turn.wait.decision = "sending";
  return true;
}

/** The person answered a question; the stream confirms it with run.input_received. */
export function answer(s: Segment, runId: string): boolean {
  const turn = findTurn(s, runId);
  if (turn === undefined || turn.wait === null || turn.wait.kind !== "question" || turn.wait.answer !== "open") return false;
  turn.wait.answer = "sending";
  return true;
}

/** Sending a decision failed: the card is open again. */
export function decideFailed(s: Segment, runId: string): void {
  const turn = findTurn(s, runId);
  if (turn !== undefined && turn.wait !== null && turn.wait.kind === "approval" && turn.wait.decision === "sending") turn.wait.decision = "open";
}

/** Sending an answer failed: the question is open again. */
export function answerFailed(s: Segment, runId: string): void {
  const turn = findTurn(s, runId);
  if (turn !== undefined && turn.wait !== null && turn.wait.kind === "question" && turn.wait.answer === "sending") turn.wait.answer = "open";
}

// ---------------------------------------------------------- session frames

export function onRunAdded(s: Segment, f: SessionStreamFrameV1RunAdded): void {
  turnFor(s, f.run_id);
}

/** Confirms the terminal state; the run's own terminal event already set it. */
export function onRunDone(s: Segment, f: SessionStreamFrameV1RunDone): void {
  const turn = findTurn(s, f.run_id);
  if (turn !== undefined && !isTerminal(turn.state) && f.status === "completed") turn.state = "completed";
}

export function onSessionClosed(s: Segment, f: SessionStreamFrameV1SessionClosed): void {
  if (f.session_id === s.sessionId) s.state = "closed";
}

// -------------------------------------------------------------- run events

/** Idempotent: a resume after approval sends it twice in a row. */
export function onRunStarted(s: Segment, e: RunEventV1RunStarted): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined || isTerminal(turn.state)) return;
  turn.state = "running";
  if (turn.wait !== null && turn.wait.kind === "approval" && e.payload.approved === true) turn.wait.decision = "approved";
}

/** Tool-call argument chunks are not shown: tool.call carries them whole. */
export function onChunk(s: Segment, e: RunEventV1LLMChunk): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  if (e.payload.chunk_type === "thinking") turn.thinking += e.payload.delta;
  else if (e.payload.chunk_type === "content") turn.answer += e.payload.delta;
}

export function onToolCall(s: Segment, e: RunEventV1ToolCall): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  const call = e.payload.tool_call;
  const existing = findTool(turn, call.id);
  if (existing !== undefined) {
    existing.state = "running";
    return;
  }
  turn.tools.push({ callId: call.id, name: call.name, arguments: JSON.stringify(call.arguments), state: "running", result: "", latencyMs: null });
}

export function onToolResult(s: Segment, e: RunEventV1ToolResult): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined || e.payload.tool_call_id === undefined) return;
  const card = findTool(turn, e.payload.tool_call_id);
  if (card === undefined) return;
  card.state = "done";
  card.result = e.payload.tool_result.content;
  card.latencyMs = e.payload.latency_ms === undefined ? null : e.payload.latency_ms;
}

export function onToolError(s: Segment, e: RunEventV1ToolError): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined || e.payload.tool_call_id === undefined) return;
  const card = findTool(turn, e.payload.tool_call_id);
  if (card === undefined) return;
  card.state = "error";
  card.result = e.payload.error;
}

export function onAwaitingApproval(s: Segment, e: RunEventV1RunAwaitingApproval): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  turn.state = "awaiting_approval";
  const calls: ToolCard[] = [];
  for (const call of e.payload.tool_calls) {
    calls.push({ callId: call.id, name: call.name, arguments: JSON.stringify(call.arguments), state: "running", result: "", latencyMs: null });
  }
  turn.wait = { kind: "approval", calls: calls, decision: "open" };
}

export function onAwaitingInput(s: Segment, e: RunEventV1RunAwaitingInput): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  turn.state = "awaiting_input";
  turn.wait = {
    kind: "question",
    callId: e.payload.tool_call_id,
    text: e.payload.question === undefined ? "" : e.payload.question,
    options: e.payload.options === undefined ? [] : e.payload.options,
    answer: "open",
  };
}

export function onInputReceived(s: Segment, e: RunEventV1RunInputReceived): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  if (turn.wait !== null && turn.wait.kind === "question") turn.wait.answer = "sent";
  turn.state = "running";
}

export function onRunPaused(s: Segment, e: RunEventV1RunPaused): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn !== undefined) turn.state = "paused";
}

/** The completed output is authoritative; it also covers chunks the stream skipped. */
export function onRunCompleted(s: Segment, e: RunEventV1RunCompleted): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  turn.state = "completed";
  const response = e.payload.output[RESPONSE_KEY];
  if (typeof response === "string") turn.answer = response;
}

/** A run that fails with `rejected` was rejected by the person; any other failure is an error. */
export function onRunFailed(s: Segment, e: RunEventV1RunFailed): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  const rejected = e.payload.rejected === true;
  turn.state = "failed";
  turn.failure = { reason: rejected ? "rejected" : "error", message: e.payload.error, code: codeOf(e.payload.error_code) };
  if (rejected && turn.wait !== null && turn.wait.kind === "approval") turn.wait.decision = "rejected";
}

export function onRunCancelled(s: Segment, e: RunEventV1RunCancelled): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  turn.state = "cancelled";
  turn.failure = { reason: "cancelled", message: e.payload.error === undefined ? "" : e.payload.error, code: codeOf(e.payload.error_code) };
}

export function onGuardrailBlocked(s: Segment, e: RunEventV1RunGuardrailBlocked): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  turn.state = "blocked";
  turn.failure = { reason: "blocked", message: e.payload.error, code: codeOf(e.payload.error_code) };
}

function codeOf(code: string | undefined): string | null {
  return code === undefined ? null : code;
}

export function onChildRunCompleted(s: Segment, e: RunEventV1ChildRunCompleted): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  const p = e.payload;
  turn.children.push({
    childRunId: p.child_run_id,
    agentName: p.agent_name === undefined ? p.agent_id : p.agent_name,
    state: "completed",
    preview: p.output_preview === undefined ? "" : p.output_preview,
  });
}

export function onChildRunFailed(s: Segment, e: RunEventV1ChildRunFailed): void {
  const turn = accept(s, e.run_id, e.seq);
  if (turn === undefined) return;
  const p = e.payload;
  turn.children.push({
    childRunId: p.child_run_id,
    agentName: p.agent_name === undefined ? p.agent_id : p.agent_name,
    state: "failed",
    preview: p.error === undefined ? "" : p.error,
  });
}

export function onContextCompacted(s: Segment, e: RunEventV1SessionContextCompacted): void {
  if (accept(s, e.run_id, e.seq) !== undefined) addNote(s, "context compacted");
}

/** The context meter lives in the header, not in the stream. */
export function onSessionContext(t: Timeline, s: Segment, e: RunEventV1SessionContext): void {
  if (accept(s, e.run_id, e.seq) === undefined) return;
  const used = e.payload.used_tokens;
  const window = e.payload.window_tokens;
  if (typeof used === "number" && typeof window === "number") t.context = { usedTokens: used, windowTokens: window };
}

/** Events with no entry in v1 (step.*, run.step_*, llm.error, team.delegate) still advance the turn's seq. */
export function onUnshown(s: Segment, runId: string, seq: number): void {
  accept(s, runId, seq);
}
