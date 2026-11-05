// The chat's view of the real uarp-sdk client: the request layer's ChatApi
// (requests.ts) and the follower's StreamApi (follow.ts), each method passing
// straight through to the UarpClient's own resource method.
import {
  UarpClient,
  type ApproveRunResponse,
  type EventStream,
  type RejectRunRequest,
  type RejectRunResponse,
  type RequestOptions,
  type RespondToRunRequest,
  type RespondToRunResponse,
  type RunApproveRequest,
  type SendSessionMessageRequest,
  type SendSessionMessageResponse,
  type StreamSessionEventsParams,
} from "uarp-sdk";
import type { ChatApi, RunsApi, SessionsApi } from "../chat/requests.js";
import type { SessionStreamApi, StreamApi } from "../chat/follow.js";

/** The SDK's `ClientOptions.fetch`: the product leaves it to the engine's fetch. */
export type FetchFunction = (input: string, init: RequestInit) => Promise<Response>;

export function createClient(baseURL: string, apiKey: string, fetcher: FetchFunction | null): UarpClient {
  if (fetcher === null) return new UarpClient({ apiKey: apiKey, baseURL: baseURL });
  return new UarpClient({ apiKey: apiKey, baseURL: baseURL, fetch: fetcher });
}

export class ClientSessions implements SessionsApi, SessionStreamApi {
  client: UarpClient;

  constructor(client: UarpClient) {
    this.client = client;
  }

  sendSessionMessage(sessionId: string, body: SendSessionMessageRequest): Promise<SendSessionMessageResponse> {
    return this.client.sessions.sendSessionMessage(sessionId, body);
  }

  streamSessionEvents(sessionId: string, params: StreamSessionEventsParams, options: RequestOptions): EventStream {
    return this.client.sessions.streamSessionEvents(sessionId, params, options);
  }
}

export class ClientRuns implements RunsApi {
  client: UarpClient;

  constructor(client: UarpClient) {
    this.client = client;
  }

  approveRun(runId: string, body?: RunApproveRequest): Promise<ApproveRunResponse> {
    return this.client.runs.approveRun(runId, body);
  }

  rejectRun(runId: string, body?: RejectRunRequest): Promise<RejectRunResponse> {
    return this.client.runs.rejectRun(runId, body);
  }

  respondToRun(runId: string, body: RespondToRunRequest): Promise<RespondToRunResponse> {
    return this.client.runs.respondToRun(runId, body);
  }
}

export class ClientApi implements ChatApi, StreamApi {
  sessions: ClientSessions;
  runs: ClientRuns;

  constructor(client: UarpClient) {
    this.sessions = new ClientSessions(client);
    this.runs = new ClientRuns(client);
  }
}
