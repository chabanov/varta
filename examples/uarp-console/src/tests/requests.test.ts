// Request layer tests: intents are performed in order through an API that
// answers with recorded responses, parsed strictly into their uarp-sdk types;
// a recorded problem document becomes the SDK's own APIError.
import type {
  ApproveRunResponse,
  RejectRunRequest,
  RejectRunResponse,
  RespondToRunRequest,
  RespondToRunResponse,
  RunApproveRequest,
  RunEventV1RunAwaitingApproval,
  RunEventV1RunAwaitingInput,
  SendSessionMessageRequest,
  SendSessionMessageResponse,
  Session,
} from "uarp-sdk";
import { NotFoundError, type ProblemDocument } from "uarp-sdk";
import { Runtime } from "../l1/reactive.js";
import { onAwaitingApproval, onAwaitingInput, type Segment } from "../chat/timeline.js";
import { ChatStore } from "../chat/store.js";
import { Requests, type ChatApi, type SessionsApi, type RunsApi, type SessionStarter } from "../chat/requests.js";
import { show, showSegment } from "../chat/show.js";
import { applyFrame } from "../chat/dispatch.js";
import { APPROVE_AWAITING, ASK_AWAITING, END_SESSION_CLOSED } from "./frames.js";
import { SEND_ACCEPTED, APPROVED, REJECTED, RESPONDED, SESSION_CREATED, SESSION_NOT_FOUND, SESSION_NOT_FOUND_STATUS } from "./responses.js";
import { equal, type Report } from "./check.js";

const RUN_ID = "run-1";
/** The session the recordings create and close (SESSION_CREATED, END_SESSION_CLOSED), and its day. */
const RECORDED_SESSION = "session-1";
const RECORDED_DAY = "2026-01-01";
/** The session created after it: the API's session ids are unique. */
const NEXT_SESSION = "session-2";
const SESSION_CLOSED_EVENT = "session_closed";

function notFound(): NotFoundError {
  return new NotFoundError(SESSION_NOT_FOUND_STATUS, JSON.parse(SESSION_NOT_FOUND) as ProblemDocument, new Headers());
}

/** Answers with the recorded responses and logs every call; `failing` answers with the recorded 404. */
class RecordedApi implements ChatApi {
  calls: string[] = [];
  failing: boolean = false;
  sessions: SessionsApi;
  runs: RunsApi;

  constructor() {
    const calls = this.calls;
    const fails = (): boolean => this.failing;
    this.sessions = {
      sendSessionMessage: async (sessionId: string, body: SendSessionMessageRequest): Promise<SendSessionMessageResponse> => {
        calls.push("sendSessionMessage " + sessionId + " " + JSON.stringify(body.content));
        if (fails()) throw notFound();
        return JSON.parse(SEND_ACCEPTED) as SendSessionMessageResponse;
      },
    };
    this.runs = {
      approveRun: async (runId: string, body?: RunApproveRequest): Promise<ApproveRunResponse> => {
        calls.push("approveRun " + runId);
        if (fails()) throw notFound();
        return JSON.parse(APPROVED) as ApproveRunResponse;
      },
      rejectRun: async (runId: string, body?: RejectRunRequest): Promise<RejectRunResponse> => {
        calls.push("rejectRun " + runId);
        if (fails()) throw notFound();
        return JSON.parse(REJECTED) as RejectRunResponse;
      },
      respondToRun: async (runId: string, body: RespondToRunRequest): Promise<RespondToRunResponse> => {
        calls.push("respondToRun " + runId + " " + JSON.stringify(body.response));
        if (fails()) throw notFound();
        return JSON.parse(RESPONDED) as RespondToRunResponse;
      },
    };
  }
}

export async function requestTests(r: Report): Promise<void> {
  const store = new ChatStore(new Runtime());
  const api = new RecordedApi();
  const requests = new Requests(store, api, null);
  const s = store.open("session-1");

  store.send(s, "Привіт");
  store.send(s, "Ще одне");
  await requests.drain();
  equal(r, "requests: sends run in order", api.calls.join("; "), 'sendSessionMessage session-1 "Привіт"; sendSessionMessage session-1 "Ще одне"');
  equal(r, "requests: a sent message is bound to its run", showSegment(s), '(segment live (ask sent "Привіт") (ask sent "Ще одне"))');

  store.change(s, RUN_ID, () => onAwaitingApproval(s, JSON.parse(APPROVE_AWAITING) as RunEventV1RunAwaitingApproval));
  store.decide(s, RUN_ID, true);
  await requests.drain();
  equal(r, "requests: an approval is sent", api.calls[api.calls.length - 1]!, "approveRun run-1");
  equal(
    r,
    "requests: the stream, not the response, confirms the decision",
    showSegment(s).includes("(approval sending") ? "still sending" : "changed",
    "still sending",
  );

  api.failing = true;
  store.send(s, "Не дійде");
  await requests.drain();
  equal(r, "requests: a failed send is marked and explained", showSegment(s).includes('(ask failed "Не дійде") (note "NOT_FOUND")') ? "failed with code" : showSegment(s), "failed with code");

  const q = store.open("session-2");
  store.change(q, "run-2", () => onAwaitingInput(q, JSON.parse(ASK_AWAITING.replace('"run_id":"run-1"', '"run_id":"run-2"')) as RunEventV1RunAwaitingInput));
  store.answer(q, "run-2", "A");
  await requests.drain();
  equal(r, "requests: an answer is sent", api.calls[api.calls.length - 1]!, 'respondToRun run-2 "A"');
  equal(r, "requests: a failed answer reopens the question", showSegment(q), '(segment live (turn awaiting_input (question open "A or B?" "A" "B")) (note "NOT_FOUND"))');
  await startTests(r);
}

/** Creates the session from the recorded `create_session`, logging into the API's call log; `failing` answers with the recorded 404. */
class RecordedStarter implements SessionStarter {
  calls: string[];
  failing: boolean = false;
  /** The id the created session gets instead of the recorded one; null keeps it. */
  sessionId: string | null = null;
  /** Sessions handed over to be followed, in order. */
  followed: string[] = [];

  constructor(calls: string[]) {
    this.calls = calls;
  }

  async create(): Promise<Session> {
    this.calls.push("create");
    if (this.failing) throw notFound();
    const created = JSON.parse(SESSION_CREATED) as Session;
    const id = this.sessionId;
    if (id !== null) created.session_id = id;
    return created;
  }

  started(s: Segment): void {
    this.followed.push(s.sessionId);
  }
}

/** Messages without a live session: the session is created first, then the messages go to it in order. */
async function startTests(r: Report): Promise<void> {
  const store = new ChatStore(new Runtime());
  const api = new RecordedApi();
  const starter = new RecordedStarter(api.calls);
  const requests = new Requests(store, api, starter);
  store.say("Привіт");
  store.say("Ще одне");
  await requests.drain();
  equal(r, "requests: create, then the messages in order", api.calls.join("; "), 'create; sendSessionMessage session-1 "Привіт"; sendSessionMessage session-1 "Ще одне"');
  equal(r, "requests: the new session's segment, dated by its creation", show(store.timeline), '(timeline (segment live 2026-01-01 (ask sent "Привіт") (ask sent "Ще одне")))');
  equal(r, "requests: the new session is followed once", starter.followed.join(" "), "session-1");

  const failing = new ChatStore(new Runtime());
  const failingApi = new RecordedApi();
  const refused = new RecordedStarter(failingApi.calls);
  refused.failing = true;
  const failingRequests = new Requests(failing, failingApi, refused);
  failing.say("Не дійде");
  failing.say("І це");
  await failingRequests.drain();
  equal(r, "requests: a failed creation sends nothing", failingApi.calls.join("; "), "create");
  equal(r, "requests: a failed creation fails the messages with the code", show(failing.timeline), '(timeline (segment closed (ask failed "Не дійде") (ask failed "І це") (note "NOT_FOUND")))');
  equal(r, "requests: a failed creation follows nothing", String(refused.followed.length), "0");

  const unstarted = new ChatStore(new Runtime());
  const nothing = new Requests(unstarted, new RecordedApi(), null);
  unstarted.say("Немає кому");
  await nothing.drain();
  equal(r, "requests: without a starter the message fails", show(unstarted.timeline), '(timeline (segment closed (ask failed "Немає кому") (note "no session can be created")))');

  // `session_closed` (the recorded frame, through the dispatcher) closes the
  // live session; the next message creates a new session the same way.
  const closing = new ChatStore(new Runtime());
  const closingApi = new RecordedApi();
  const reopened = new RecordedStarter(closingApi.calls);
  reopened.sessionId = NEXT_SESSION;
  const closingRequests = new Requests(closing, closingApi, reopened);
  const ended = closing.openAt(RECORDED_SESSION, RECORDED_DAY);
  const outcome = applyFrame(closing, ended, SESSION_CLOSED_EVENT, END_SESSION_CLOSED);
  closing.say("Знову");
  await closingRequests.drain();
  equal(r, "requests: session_closed closes the live segment", outcome.kind + " " + ended.state, "closed closed");
  equal(r, "requests: after session_closed the next message creates a new session", closingApi.calls.join("; "), 'create; sendSessionMessage session-2 "Знову"');
  equal(r, "requests: the new session follows the closed one", show(closing.timeline), '(timeline (segment closed 2026-01-01) (segment live 2026-01-01 (ask sent "Знову")))');
  equal(r, "requests: the new session is followed", reopened.followed.join(" "), NEXT_SESSION);
}
