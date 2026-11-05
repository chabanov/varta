// The console end to end (`/e2e/<scenario>`): the requests the console makes,
// answered from the recorded scenarios (recordings.ts) or generated streams.
// The mock keeps no state between requests, so the session stream follows
// `Last-Event-ID` alone, as the API does: frames strictly after that id, from
// the recording. A recorded scenario's first connection ends without `[DONE]`
// after its drop point (contract DROP_AFTER), so the console must reconnect;
// later connections run to the end of the recording, which ends without
// `[DONE]` unless the recording has it (session_end). The client test labels
// each stream connection with `x-loopback-attempt`, so the mock can fail one.
import { END_SESSION_CLOSED, PLAIN_COMPLETED, PLAIN_CONTENT, PLAIN_RUN_ADDED, PLAIN_RUN_DONE, PLAIN_STARTED, WIRE_SESSION_NOT_FOUND } from "../frames.js";
import {
  ANSWER,
  ATTEMPT_HEADER,
  DROP_AFTER,
  E2E_PREFIX,
  EXPIRED_SCENARIO,
  HEAD_AGENT_ID,
  LONG_CHUNKS,
  LONG_DELTA,
  LONG_FIRST_CHUNK_SEQ,
  LONG_HEAD_DELAY_MS,
  LONG_SCENARIO,
  MISSING_SCENARIO,
  NO_HEAD_SCENARIO,
  RUN_ID,
  SESSION_ID,
  STREAM_DONE,
  TENANT,
  TENANT_WITHOUT_HEAD,
  UNAUTHORIZED_SCENARIO,
  UNAVAILABLE_CONNECTION,
  UNAVAILABLE_SCENARIO,
  UNAVAILABLE_PROBLEM,
} from "../sdk/contract.js";
import { recordingText, type RecordedFrame, type RecordedRequest, type Recording } from "./recordings.js";

/** What the routes need from the server's helpers (main.ts). */
export interface Responder {
  header(request: VartaHttpRequest, name: string): string;
  json(status: number, body: string): VartaHttpResponse;
  problem(status: number, title: string, detail: string): VartaHttpResponse;
  stream(body: string): VartaHttpResponse;
  unavailable(body: string): VartaHttpResponse;
  /** Resolves after `ms` (a timer of the mock's own `time.timer` grant). */
  wait(ms: number): Promise<void>;
}

const OK = 200;
const CREATED = 201;
const BAD_REQUEST = 400;
const UNAUTHORIZED = 401;
const NOT_FOUND = 404;
const NOT_ACCEPTABLE = 406;

const TENANT_PATH = "/api/v1/tenants/me";
const SESSIONS_PATH = "/api/v1/sessions";
const SESSION_PATH = SESSIONS_PATH + "/" + SESSION_ID;
const MESSAGES_PATH = SESSION_PATH + "/messages";
const EVENTS_PATH = SESSION_PATH + "/events";
const EVENTS_QUERY = "?event_schema=1";
const RUN_PATH = "/api/v1/runs/" + RUN_ID;
const APPROVE_PATH = RUN_PATH + "/approve";
const RESPOND_PATH = RUN_PATH + "/respond";

const EVENT_STREAM_TYPE = "text/event-stream";
const CONNECTED_EVENT = "connected";
const CONNECTED_ID = "0";
/** The `connected` frame of approval_approve.json. */
const CONNECTED = "{\"session_id\":\"" + SESSION_ID + "\",\"connected_at\":\"2026-01-01T00:00:00.098Z\",\"runs\":[]}";
const TERMINATOR_EVENT = "message";
const CREATE_STEP = "create_session";
const SEND_STEP = "send_message";
const APPROVE_STEP = "approve";
const RESPOND_STEP = "respond";
/** The session a scenario without a recording creates: the required fields of `Session`. */
const GENERATED_SESSION = "{\"session_id\":\"" + SESSION_ID + "\",\"tenant_id\":\"tenant-1\",\"agent_id\":\"" + HEAD_AGENT_ID + "\",\"status\":\"active\"}";
/** `requests.ts` approves with an empty body. */
const EMPTY_BODY = "{}";

/** The recorded seq every generated long-stream chunk replaces. */
const RECORDED_CHUNK_SEQ = "\"seq\":23";
const RECORDED_CHUNK_DELTA = "\"delta\":\"ok\"";
const RECORDED_COMPLETED_SEQ = "\"seq\":29";

function frameText(event: string, id: string | undefined, data: string): string {
  const idLine = id === undefined ? "" : "id: " + id + "\n";
  return "event: " + event + "\n" + idLine + "data: " + data + "\n\n";
}

/** The data text the wire carried: the terminator as is, every other frame as compact JSON. */
function wireData(frame: RecordedFrame): string {
  if (frame.event === TERMINATOR_EVENT) return STREAM_DONE;
  return JSON.stringify(frame.data);
}

function recorded(recording: Recording, step: string): RecordedRequest | null {
  for (const request of recording.requests) {
    if (request.step === step) return request;
  }
  return null;
}

/** The recorded answer to one request, after checking the body the console sent. */
function replay(r: Responder, request: VartaHttpRequest, recording: Recording, step: string, body: string | null): VartaHttpResponse {
  const entry = recorded(recording, step);
  if (entry === null) return r.problem(NOT_FOUND, "Not Found", "no recorded " + step + " in " + recording.scenario);
  const expected = body !== null ? body : entry.request === undefined ? EMPTY_BODY : JSON.stringify(entry.request);
  if (request.body !== expected) return r.problem(BAD_REQUEST, "Bad Request", step + ": expected body " + expected + ", got " + request.body);
  return r.json(entry.status, JSON.stringify(entry.body));
}

/** Frames strictly after `lastId` (from the start when empty), up to the drop point or the end. */
function recordedStream(r: Responder, recording: Recording, lastId: string): VartaHttpResponse {
  const frames = recording.frames;
  let start = 0;
  if (lastId !== "") {
    start = -1;
    for (let i = 0; i < frames.length; i++) {
      if (frames[i]!.id === lastId) start = i + 1;
    }
    if (start < 0) return r.problem(BAD_REQUEST, "Bad Request", "unknown Last-Event-ID " + lastId);
  }
  const dropAfter = lastId === "" ? DROP_AFTER[recording.scenario] : undefined;
  const parts: string[] = [];
  for (let i = 0; i < frames.length; i++) {
    const frame = frames[i]!;
    // Every connection opens with the session's `connected` frame.
    if (frame.event === CONNECTED_EVENT) {
      parts.push(frameText(frame.event, frame.id, wireData(frame)));
      continue;
    }
    if (i < start) continue;
    parts.push(frameText(frame.event, frame.id, wireData(frame)));
    if (dropAfter !== undefined && frame.id === dropAfter) break;
  }
  return r.stream(parts.join(""));
}

/** One run of `LONG_CHUNKS` content chunks, then the session closes. */
function longStream(): string {
  const parts: string[] = [connected(), frameText("run_added", RUN_ID + ":added", PLAIN_RUN_ADDED), frameText("run.started", RUN_ID + ":1", PLAIN_STARTED)];
  // The recorded chunk cut once around its seq; each chunk only joins the pieces
  // (a replace per chunk does not fit one request's fuel).
  const seqAt = PLAIN_CONTENT.indexOf(RECORDED_CHUNK_SEQ);
  const deltaAt = PLAIN_CONTENT.indexOf(RECORDED_CHUNK_DELTA);
  const head = PLAIN_CONTENT.slice(0, seqAt) + "\"seq\":";
  const middle = PLAIN_CONTENT.slice(seqAt + RECORDED_CHUNK_SEQ.length, deltaAt) + "\"delta\":\"" + LONG_DELTA + "\"";
  const tail = PLAIN_CONTENT.slice(deltaAt + RECORDED_CHUNK_DELTA.length) + "\n\n";
  for (let i = 0; i < LONG_CHUNKS; i++) {
    const seq = String(LONG_FIRST_CHUNK_SEQ + i);
    parts.push("event: llm.chunk\nid: " + RUN_ID + ":" + seq + "\ndata: " + head + seq + middle + tail);
  }
  const completedSeq = LONG_FIRST_CHUNK_SEQ + LONG_CHUNKS;
  parts.push(frameText("run.completed", RUN_ID + ":" + String(completedSeq), PLAIN_COMPLETED.replace(RECORDED_COMPLETED_SEQ, "\"seq\":" + String(completedSeq))));
  parts.push(frameText("run_done", RUN_ID + ":done", PLAIN_RUN_DONE));
  parts.push(frameText("session_closed", "session_closed", END_SESSION_CLOSED));
  parts.push(frameText(TERMINATOR_EVENT, undefined, STREAM_DONE));
  return parts.join("");
}

/** The session's `connected` frame, as every recording opens with it. */
function connected(): string {
  return frameText(CONNECTED_EVENT, CONNECTED_ID, CONNECTED);
}

function generatedStream(r: Responder, scenario: string): VartaHttpResponse | null {
  if (scenario === LONG_SCENARIO) return r.stream(longStream());
  if (scenario === MISSING_SCENARIO) {
    return r.stream(connected() + frameText("error", "error-0", WIRE_SESSION_NOT_FOUND) + frameText(TERMINATOR_EVENT, undefined, STREAM_DONE));
  }
  // Nothing before `[DONE]`, not even `connected`: the credential is no longer valid.
  if (scenario === EXPIRED_SCENARIO) return r.stream(frameText(TERMINATOR_EVENT, undefined, STREAM_DONE));
  if (scenario === UNAUTHORIZED_SCENARIO) return r.problem(UNAUTHORIZED, "Unauthorized", "the key was revoked");
  return null;
}

async function events(r: Responder, request: VartaHttpRequest, scenario: string, recording: Recording | null, query: string): Promise<VartaHttpResponse> {
  if (query !== EVENTS_QUERY) return r.problem(BAD_REQUEST, "Bad Request", "expected event_schema=1 only");
  if (r.header(request, "accept") !== EVENT_STREAM_TYPE) return r.problem(NOT_ACCEPTABLE, "Not Acceptable", "expected an event stream request");
  if (recording === null) {
    // The long stream's head comes late, though within one window of the
    // client's per-turn deadline: with its many frames the stream then lasts
    // longer than one deadline while no single wait does.
    if (scenario === LONG_SCENARIO) await r.wait(LONG_HEAD_DELAY_MS);
    const generated = generatedStream(r, scenario);
    return generated === null ? r.problem(NOT_FOUND, "Not Found", "no stream for " + scenario) : generated;
  }
  if (recording.scenario === UNAVAILABLE_SCENARIO && r.header(request, ATTEMPT_HEADER) === UNAVAILABLE_CONNECTION) {
    return r.unavailable(UNAVAILABLE_PROBLEM);
  }
  return recordedStream(r, recording, r.header(request, "last-event-id"));
}

/** Answers a request under `/e2e/`; null for any other route. */
export async function e2e(r: Responder, request: VartaHttpRequest, route: string, query: string): Promise<VartaHttpResponse | null> {
  if (!route.startsWith(E2E_PREFIX)) return null;
  const rest = route.slice(E2E_PREFIX.length);
  const slash = rest.indexOf("/");
  if (slash < 0) return r.problem(NOT_FOUND, "Not Found", "no scenario path " + route);
  const scenario = rest.slice(0, slash);
  const path = rest.slice(slash);
  const text = recordingText(scenario);
  const recording = text === null ? null : (JSON.parse(text) as Recording);
  if (path === TENANT_PATH && request.method === "GET") return r.json(OK, scenario === NO_HEAD_SCENARIO ? TENANT_WITHOUT_HEAD : TENANT);
  if (path === SESSIONS_PATH && request.method === "POST") {
    const expected = JSON.stringify({ agent_id: HEAD_AGENT_ID });
    if (recording !== null) return replay(r, request, recording, CREATE_STEP, expected);
    if (request.body !== expected) return r.problem(BAD_REQUEST, "Bad Request", "expected body " + expected);
    return r.json(CREATED, GENERATED_SESSION);
  }
  if (path === EVENTS_PATH && request.method === "GET") return await events(r, request, scenario, recording, query);
  if (recording !== null && request.method === "POST") {
    if (path === MESSAGES_PATH) return replay(r, request, recording, SEND_STEP, null);
    if (path === APPROVE_PATH) return replay(r, request, recording, APPROVE_STEP, EMPTY_BODY);
    if (path === RESPOND_PATH) return replay(r, request, recording, RESPOND_STEP, JSON.stringify({ response: ANSWER }));
  }
  return r.problem(NOT_FOUND, "Not Found", "no loopback route " + route);
}
