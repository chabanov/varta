// Performs the person's intents with the UARP API, one at a time and in order,
// and applies each result to the store. `ChatApi` is the part of the uarp-sdk
// client the chat uses, so the client itself is passed in; nothing here knows
// about transport. A send that succeeds binds the message to its run; the
// stream then delivers the run. A decision or an answer that succeeds needs no
// change: the stream confirms it (run.started, run.input_received). A failed
// request puts the message, card or question back and adds a note with the
// API's code, never a silently dropped action.
import type {
  ApproveRunResponse,
  RejectRunRequest,
  RejectRunResponse,
  RespondToRunRequest,
  RespondToRunResponse,
  RunApproveRequest,
  SendSessionMessageRequest,
  SendSessionMessageResponse,
} from "../../node_modules/uarp-sdk/src/generated/models.js";
import { APIError } from "../../node_modules/uarp-sdk/src/core/errors.js";
import { untracked } from "../l1/reactive.js";
import { askSent, askFailed, decideFailed, answerFailed, addNote } from "./timeline.js";
import { ChatStore, type Intent } from "./store.js";

export interface SessionsApi {
  sendSessionMessage(sessionId: string, body: SendSessionMessageRequest): Promise<SendSessionMessageResponse>;
}

export interface RunsApi {
  approveRun(runId: string, body?: RunApproveRequest): Promise<ApproveRunResponse>;
  rejectRun(runId: string, body?: RejectRunRequest): Promise<RejectRunResponse>;
  respondToRun(runId: string, body: RespondToRunRequest): Promise<RespondToRunResponse>;
}

export interface ChatApi {
  sessions: SessionsApi;
  runs: RunsApi;
}

const UNKNOWN_FAILURE = "request failed";

/** What a failed request shows: the API's code when it has one. */
function reason(error: unknown): string {
  if (error instanceof APIError) return error.code === undefined ? String(error.status) : error.code;
  if (error instanceof Error) return error.message;
  return UNKNOWN_FAILURE;
}

export class Requests {
  store: ChatStore;
  api: ChatApi;
  /** Intents already started, in queue order. */
  started: number = 0;
  /** The chain that keeps requests in order. */
  tail: Promise<void> = Promise.resolve();

  constructor(store: ChatStore, api: ChatApi) {
    this.store = store;
    this.api = api;
  }

  /** Starts every queued intent not yet started; returns when they are all done. */
  drain(): Promise<void> {
    const queued = untracked<Intent[]>(this.store.rt, this.store.intents.get);
    while (this.started < queued.length) {
      const intent = queued[this.started]!;
      this.started += 1;
      this.tail = this.tail.then((): Promise<void> => this.perform(intent));
    }
    return this.tail;
  }

  async perform(intent: Intent): Promise<void> {
    switch (intent.kind) {
      case "send":
        return this.send(intent.sessionId, intent.localId, intent.text);
      case "decide":
        return this.decide(intent.runId, intent.approved);
      case "answer":
        return this.answer(intent.runId, intent.text);
    }
  }

  async send(sessionId: string, localId: string, text: string): Promise<void> {
    const s = this.store.segment(sessionId);
    if (s === null) return;
    try {
      const sent = await this.api.sessions.sendSessionMessage(sessionId, { content: text });
      this.store.changeAsk(s, localId, () => askSent(s, localId, sent.run_id));
    } catch (error) {
      const why = reason(error);
      this.store.changeAsk(s, localId, () => askFailed(s, localId));
      this.store.change(s, null, () => addNote(s, why));
    }
  }

  async decide(runId: string, approved: boolean): Promise<void> {
    const s = this.store.segmentOfRun(runId);
    if (s === null) return;
    try {
      if (approved) await this.api.runs.approveRun(runId, {});
      else await this.api.runs.rejectRun(runId, {});
    } catch (error) {
      const why = reason(error);
      this.store.change(s, runId, () => decideFailed(s, runId));
      this.store.change(s, null, () => addNote(s, why));
    }
  }

  async answer(runId: string, text: string): Promise<void> {
    const s = this.store.segmentOfRun(runId);
    if (s === null) return;
    try {
      await this.api.runs.respondToRun(runId, { response: text });
    } catch (error) {
      const why = reason(error);
      this.store.change(s, runId, () => answerFailed(s, runId));
      this.store.change(s, null, () => addNote(s, why));
    }
  }
}
