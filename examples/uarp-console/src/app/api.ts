// The console's view of the real uarp-sdk client: the tenant, the session list
// and transcripts stitching reads (chat/stitch.ts), session creation, the
// request layer's ChatApi (requests.ts) and the follower's StreamApi
// (follow.ts), each method passing straight through to the UarpClient's own
// resource method.
import {
  UarpClient,
  type ApproveRunResponse,
  type CreateSessionRequest,
  type EventStream,
  type GetSessionMessagesResponse,
  type ListSessionsParams,
  type ListSessionsResponse,
  type RejectRunRequest,
  type RejectRunResponse,
  type RequestOptions,
  type RespondToRunRequest,
  type RespondToRunResponse,
  type RunApproveRequest,
  type SendSessionMessageRequest,
  type SendSessionMessageResponse,
  type Session,
  type StreamSessionEventsParams,
  type Tenant,
} from "uarp-sdk";
import type { RunsApi, SessionsApi } from "../chat/requests.js";
import type { SessionStreamApi } from "../chat/follow.js";
import type { HistoryApi } from "../chat/stitch.js";

/** The SDK's `ClientOptions.fetch`: the product leaves it to the engine's fetch. */
export type FetchFunction = (input: string, init: RequestInit) => Promise<Response>;

export interface TenantsApi {
  getCurrentTenant(): Promise<Tenant>;
}

/** Everything the console does with `sessions`. */
export interface ConsoleSessionsApi extends SessionsApi, SessionStreamApi, HistoryApi {
  create(body: CreateSessionRequest): Promise<Session>;
}

/** The API the console runs on: the real client in the product, recorded answers in view tests. */
export interface ConsoleApi {
  tenants: TenantsApi;
  sessions: ConsoleSessionsApi;
  runs: RunsApi;
}

export function createClient(baseURL: string, apiKey: string, fetcher: FetchFunction | null): UarpClient {
  if (fetcher === null) return new UarpClient({ apiKey: apiKey, baseURL: baseURL });
  return new UarpClient({ apiKey: apiKey, baseURL: baseURL, fetch: fetcher });
}

export class ClientTenants implements TenantsApi {
  client: UarpClient;

  constructor(client: UarpClient) {
    this.client = client;
  }

  getCurrentTenant(): Promise<Tenant> {
    return this.client.tenants.getCurrentTenant();
  }
}

export class ClientSessions implements ConsoleSessionsApi {
  client: UarpClient;

  constructor(client: UarpClient) {
    this.client = client;
  }

  create(body: CreateSessionRequest): Promise<Session> {
    return this.client.sessions.create(body);
  }

  list(params: ListSessionsParams): Promise<ListSessionsResponse> {
    return this.client.sessions.list(params);
  }

  getSessionMessages(sessionId: string): Promise<GetSessionMessagesResponse> {
    return this.client.sessions.getSessionMessages(sessionId);
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

export class ClientApi implements ConsoleApi {
  tenants: ClientTenants;
  sessions: ClientSessions;
  runs: ClientRuns;

  constructor(client: UarpClient) {
    this.tenants = new ClientTenants(client);
    this.sessions = new ClientSessions(client);
    this.runs = new ClientRuns(client);
  }
}

/** The product's and the end-to-end tests' API: the real client for a base URL and key. */
export function clientApi(fetcher: FetchFunction | null): (baseURL: string, apiKey: string) => ConsoleApi {
  return (baseURL: string, apiKey: string): ConsoleApi => new ClientApi(createClient(baseURL, apiKey, fetcher));
}
