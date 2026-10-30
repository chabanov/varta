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
} from "../../node_modules/uarp-sdk/src/generated/models.js";
import { NotFoundError, type ProblemDocument } from "../../node_modules/uarp-sdk/src/core/errors.js";
import { Runtime } from "../l1/reactive.js";
import { onAwaitingApproval, onAwaitingInput } from "../chat/timeline.js";
import { ChatStore } from "../chat/store.js";
import { Requests, type ChatApi, type SessionsApi, type RunsApi } from "../chat/requests.js";
import { showSegment } from "../chat/show.js";
import { APPROVE_AWAITING, ASK_AWAITING } from "./frames.js";
import { SEND_ACCEPTED, APPROVED, REJECTED, RESPONDED, SESSION_NOT_FOUND, SESSION_NOT_FOUND_STATUS } from "./responses.js";
import { equal, type Report } from "./check.js";

const RUN_ID = "run-1";

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
  const requests = new Requests(store, api);
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
}
