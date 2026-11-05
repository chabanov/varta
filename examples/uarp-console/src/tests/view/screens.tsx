// The console screens for the headless view tests (TEST_VIEW): the product's
// ConsoleView and ConsoleApp over an in-process API that answers from the
// recorded tenant, the composed session list and the recorded transcripts
// (../sessions.ts), and over settings held in memory in place of the KV
// database. Nothing here reaches the network, so a script is deterministic;
// the network path and the KV database are covered by the app groups
// (app.test.ts). A request the screen does not need rejects.
import {
  AuthenticationError,
  NotFoundError,
  type ApproveRunResponse,
  type CreateSessionRequest,
  type EventStream,
  type GetSessionMessagesResponse,
  type ListSessionsParams,
  type ListSessionsResponse,
  type ProblemDocument,
  type RejectRunRequest,
  type RejectRunResponse,
  type RequestOptions,
  type RespondToRunRequest,
  type RespondToRunResponse,
  type RunApproveRequest,
  type RunEventV1ChildRunCompleted,
  type RunEventV1LLMChunk,
  type RunEventV1RunAwaitingApproval,
  type RunEventV1RunCompleted,
  type RunEventV1RunStarted,
  type RunEventV1SessionContext,
  type RunEventV1ToolCall,
  type RunEventV1ToolResult,
  type SendSessionMessageRequest,
  type SendSessionMessageResponse,
  type Session,
  type StreamSessionEventsParams,
  type Tenant,
} from "uarp-sdk";
import { Runtime } from "../../l1/reactive.js";
import { ChatStore } from "../../chat/store.js";
import { FOLLOW_DEFAULTS } from "../../chat/follow.js";
import type { RunsApi } from "../../chat/requests.js";
import {
  ask,
  askSent,
  onAwaitingApproval,
  onChildRunCompleted,
  onChunk,
  onRunCompleted,
  onRunStarted,
  onSessionContext,
  onToolCall,
  onToolResult,
} from "../../chat/timeline.js";
import { ConsoleApp } from "../../app/console.js";
import type { ConsoleApi, ConsoleSessionsApi, TenantsApi } from "../../app/api.js";
import type { Settings, SettingsStore } from "../../app/key.js";
import { ConsoleView } from "../../ui/console.js";
import { Confirmation, TOAST_MS, Toasts } from "../../ui/notices.js";
import { commitSoon } from "../../ui/commit.js";
import { EMPTY_PAGE, sessionPage, transcriptOf, STITCH_NEW_SCENARIO } from "../sessions.js";
import { TENANT } from "../sdk/contract.js";
import {
  APPROVE_AWAITING,
  APPROVE_STARTED,
  APPROVE_TOOL_CALL,
  APPROVE_TOOL_RESULT,
  DELEGATION_CHILD,
  PLAIN_COMPLETED,
  PLAIN_CONTENT,
  PLAIN_CONTEXT,
  PLAIN_STARTED,
  PLAIN_THINKING_1,
} from "../frames.js";

/** The base URL the screens show; nothing is requested from it. */
const VIEW_BASE_URL = "https://api.snaga.ai";
/** The key the settings in memory hold when a screen starts with one; never shown. */
const STORED_KEY = "uarp_view_stored-only";
const NOT_FOUND = 404;
const UNAUTHORIZED = 401;
/** The problem document of a revoked key, in the shape the API answers. */
const REVOKED = "{\"type\":\"about:blank\",\"title\":\"Unauthorized\",\"status\":401,\"detail\":\"the key was revoked\",\"code\":\"UNAUTHORIZED\"}";
const NOT_IN_VIEW = "not requested by this screen";
const RECORDED_RUN = '"run_id":"run-1"';

/** Settings in memory, standing in for the KV database (KvSettings) in view tests. */
class MemorySettings implements SettingsStore {
  baseURL: string | null = null;
  apiKey: string | null;

  constructor(apiKey: string | null) {
    this.apiKey = apiKey;
  }

  async read(): Promise<Settings> {
    return { baseURL: this.baseURL, apiKey: this.apiKey };
  }

  async write(baseURL: string, apiKey: string): Promise<void> {
    this.baseURL = baseURL;
    this.apiKey = apiKey;
  }

  async forget(): Promise<void> {
    this.apiKey = null;
  }
}

/** How the tenant read ends: answered, never answered (loading), or refused with a revoked key. */
type TenantAnswer = "tenant" | "pending" | "revoked";

class RecordedTenants implements TenantsApi {
  answer: TenantAnswer;

  constructor(answer: TenantAnswer) {
    this.answer = answer;
  }

  getCurrentTenant(): Promise<Tenant> {
    if (this.answer === "pending") return new Promise<Tenant>((resolve) => {});
    if (this.answer === "revoked") return Promise.reject(new AuthenticationError(UNAUTHORIZED, JSON.parse(REVOKED) as ProblemDocument, new Headers()));
    return Promise.resolve(JSON.parse(TENANT) as Tenant);
  }
}

class RecordedSessions implements ConsoleSessionsApi {
  /** The session list: a stitch scenario of sessions.ts, or empty. */
  scenario: string | null;

  constructor(scenario: string | null) {
    this.scenario = scenario;
  }

  async list(params: ListSessionsParams): Promise<ListSessionsResponse> {
    const scenario = this.scenario;
    if (scenario === null) return JSON.parse(EMPTY_PAGE) as ListSessionsResponse;
    const page = sessionPage(scenario, params.cursor === undefined ? "" : params.cursor);
    if (page === null) throw new Error("no page");
    return JSON.parse(page) as ListSessionsResponse;
  }

  async getSessionMessages(sessionId: string): Promise<GetSessionMessagesResponse> {
    const recorded = transcriptOf(sessionId);
    if (recorded === null) throw new Error("no session " + sessionId);
    if (recorded.status === NOT_FOUND) throw new NotFoundError(NOT_FOUND, recorded.body as ProblemDocument, new Headers());
    return recorded.body as GetSessionMessagesResponse;
  }

  async create(body: CreateSessionRequest): Promise<Session> {
    throw new Error(NOT_IN_VIEW);
  }

  async sendSessionMessage(sessionId: string, body: SendSessionMessageRequest): Promise<SendSessionMessageResponse> {
    throw new Error(NOT_IN_VIEW);
  }

  streamSessionEvents(sessionId: string, params: StreamSessionEventsParams, options: RequestOptions): EventStream {
    throw new Error(NOT_IN_VIEW);
  }
}

class RecordedRuns implements RunsApi {
  async approveRun(runId: string, body?: RunApproveRequest): Promise<ApproveRunResponse> {
    throw new Error(NOT_IN_VIEW);
  }

  async rejectRun(runId: string, body?: RejectRunRequest): Promise<RejectRunResponse> {
    throw new Error(NOT_IN_VIEW);
  }

  async respondToRun(runId: string, body: RespondToRunRequest): Promise<RespondToRunResponse> {
    throw new Error(NOT_IN_VIEW);
  }
}

class RecordedConsoleApi implements ConsoleApi {
  tenants: RecordedTenants;
  sessions: RecordedSessions;
  runs: RecordedRuns;

  constructor(tenant: TenantAnswer, scenario: string | null) {
    this.tenants = new RecordedTenants(tenant);
    this.sessions = new RecordedSessions(scenario);
    this.runs = new RecordedRuns();
  }
}

/** One console screen: whether a key is stored, how the tenant answers, which session list. */
interface Screen {
  key: string | null;
  tenant: TenantAnswer;
  scenario: string | null;
}

function screenOf(name: string): Screen {
  if (name === "settings" || name === "settings-saved") return { key: null, tenant: "tenant", scenario: null };
  if (name === "history") return { key: STORED_KEY, tenant: "tenant", scenario: STITCH_NEW_SCENARIO };
  if (name === "loading") return { key: STORED_KEY, tenant: "pending", scenario: null };
  if (name === "error") return { key: STORED_KEY, tenant: "revoked", scenario: null };
  // empty, context, confirm
  return { key: STORED_KEY, tenant: "tenant", scenario: null };
}

function asRun(frame: string, runId: string): string {
  return frame.replace(RECORDED_RUN, '"run_id":"' + runId + '"');
}

/** A conversation that mentions runs and a delegated agent, from recorded frames (the context panel's data). */
function mentionRuns(store: ChatStore): void {
  const s = store.openAt("session-1", "2026-01-01");
  store.change(s, null, () => {
    ask(s, "recorded-1", "Reply with the single word: ok");
    askSent(s, "recorded-1", "run-1");
    onRunStarted(s, JSON.parse(PLAIN_STARTED) as RunEventV1RunStarted);
    onChunk(s, JSON.parse(PLAIN_THINKING_1) as RunEventV1LLMChunk);
    onChunk(s, JSON.parse(PLAIN_CONTENT) as RunEventV1LLMChunk);
    onSessionContext(store.timeline, s, JSON.parse(PLAIN_CONTEXT) as RunEventV1SessionContext);
    onRunCompleted(s, JSON.parse(PLAIN_COMPLETED) as RunEventV1RunCompleted);
    ask(s, "recorded-2", "Delegate it.");
    askSent(s, "recorded-2", "run-2");
    onRunStarted(s, JSON.parse(asRun(APPROVE_STARTED, "run-2")) as RunEventV1RunStarted);
    onToolCall(s, JSON.parse(asRun(APPROVE_TOOL_CALL, "run-2")) as RunEventV1ToolCall);
    onToolResult(s, JSON.parse(asRun(APPROVE_TOOL_RESULT, "run-2")) as RunEventV1ToolResult);
    onChildRunCompleted(s, JSON.parse(asRun(DELEGATION_CHILD, "run-2")) as RunEventV1ChildRunCompleted);
    ask(s, "recorded-3", 'Call manage_permissions with action "get".');
    askSent(s, "recorded-3", "run-3");
    onAwaitingApproval(s, JSON.parse(asRun(APPROVE_AWAITING, "run-3")) as RunEventV1RunAwaitingApproval);
  });
}

/** Mounts the console screen `name` and starts the console in a turn of its own, as the product does. */
export function consoleScreen(rt: Runtime, name: string): void {
  const store = new ChatStore(rt);
  store.afterChange = commitSoon;
  const screen = screenOf(name);
  const toasts = new Toasts(rt, TOAST_MS, commitSoon);
  const confirmation = new Confirmation(rt);
  const api = new RecordedConsoleApi(screen.tenant, screen.scenario);
  const app = new ConsoleApp(store, {
    defaultBaseURL: VIEW_BASE_URL,
    localBaseURL: null,
    settings: new MemorySettings(screen.key),
    connect: (baseURL: string, apiKey: string): ConsoleApi => api,
    follow: FOLLOW_DEFAULTS,
    toasts: toasts,
  });
  View.mount(<ConsoleView store={store} app={app} toasts={toasts} confirmation={confirmation} />);
  setTimeout((): void => {
    app.start().then((): void => {
      if (name === "context") mentionRuns(store);
      commitSoon();
    });
  }, 0);
}
