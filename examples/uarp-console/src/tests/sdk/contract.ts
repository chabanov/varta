// The loopback contract shared by the mock API (`../sdk-server/main.ts`) and the
// SDK client tests (`./main.ts`): canned UARP-shaped bodies, header names, timings.
// Nothing here is a credential: API_KEY is a fixed test value the mock compares.
// The mock imports no SDK module, so it compiles without the SDK's graph.

/** The bearer key the mock requires; a test value, not a UARP key. */
export const API_KEY = "uarp_loopback_test-only";
/**
 * The product token of the `User-Agent` the real Transport sets
 * (`uarp-sdk-typescript/<version>`). Under v38 `user-agent` is host-owned: the
 * engine checks the guest's value and never sends it (the native host sends
 * none, a browser its own), so the mock refuses a request whose user agent
 * starts with this token.
 */
export const SDK_USER_AGENT_PRODUCT = "uarp-sdk-typescript/";
/** Sent by the retry test's counting `fetch`, so the stateless mock can tell attempts apart. */
export const ATTEMPT_HEADER = "x-loopback-attempt";

/** `GET /health`: every field of `GetHealthResponse` is optional; these are the common ones. */
export const HEALTH = "{\"status\":\"healthy\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"kv_connected\":true,\"uptime_seconds\":42,\"version\":\"2026-09-10\",\"build_sha\":\"loopback\"}";
export const UNARY_REQUEST_ID = "req-unary";

/** An RFC 9457 problem document with every member `APIError` reads. */
export const NOT_FOUND_PROBLEM = "{\"type\":\"about:blank\",\"title\":\"Not Found\",\"status\":404,\"detail\":\"session \\\"missing\\\" not found\",\"code\":\"NOT_FOUND\",\"correlation_id\":\"corr-404\",\"reason\":\"unknown_session\",\"errors\":[{\"field\":\"session_id\",\"message\":\"unknown\"}]}";
export const PROBLEM_REQUEST_ID = "req-404";
/** A bare `{"error": …}` body, which the API still sends in places instead of a problem document. */
export const BARE_ERROR = "{\"error\":\"Insufficient role: viewer\"}";
export const BARE_CORRELATION_ID = "corr-403";
/** The 503 of the retry scenarios. */
export const UNAVAILABLE_PROBLEM = "{\"type\":\"about:blank\",\"title\":\"Service Unavailable\",\"status\":503,\"detail\":\"scheduler busy\",\"code\":\"SERVICE_UNAVAILABLE\"}";
/** `Retry-After` of the first 503, in seconds. */
export const RETRY_AFTER_SECONDS = 1;

/** How long the slow route waits before answering; the abort tests stop well before. */
export const SLOW_RESPONSE_MS = 3000;
/** When the abort tests abort (or time out) the slow request. */
export const ABORT_AFTER_MS = 200;

/** The session whose event stream the mock serves. */
export const SESSION_ID = "session-1";
/** The `retry:` field the stream sends before its frames. */
export const STREAM_RETRY_MS = 1500;
/** The data line that ends a session stream. */
export const STREAM_DONE = "[DONE]";

// ------------------------------------------------- the console end to end

/** Each end-to-end scenario is served under `/e2e/<scenario>`, the console's base URL. */
export const E2E_PREFIX = "/e2e/";
/** The tenant's Head Agent, as the recordings name it. */
export const HEAD_AGENT_ID = "agent-head";
/** `GET /api/v1/tenants/me`: the required fields of `Tenant` and the Head Agent. */
export const TENANT = "{\"tenant_id\":\"tenant-1\",\"name\":\"Loopback\",\"slug\":\"loopback\",\"status\":\"active\",\"head_agent_id\":\"agent-head\",\"created_at\":\"2026-01-01T00:00:00.000Z\",\"updated_at\":\"2026-01-01T00:00:00.000Z\"}";
/** `GET /api/v1/tenants/me` of a tenant without a Head Agent. */
export const TENANT_WITHOUT_HEAD = "{\"tenant_id\":\"tenant-1\",\"name\":\"Loopback\",\"slug\":\"loopback\",\"status\":\"active\",\"created_at\":\"2026-01-01T00:00:00.000Z\",\"updated_at\":\"2026-01-01T00:00:00.000Z\"}";
/** The scenario whose tenant has no Head Agent. */
export const NO_HEAD_SCENARIO = "no-head";
/** The run every recording starts. */
export const RUN_ID = "run-1";
/** The person's answer in `ask_user` (the transcript's choice). */
export const ANSWER = "A";
/** Where each recorded scenario's first connection ends without `[DONE]` (inclusive). */
export const DROP_AFTER: { [scenario: string]: string } = {
  approval_approve: "run-1:31",
  ask_user: "run-1:28",
  session_end: "run-1:16",
};
/** The scenario one of whose stream connections the mock answers with a 503, and that connection (by `ATTEMPT_HEADER`). */
export const UNAVAILABLE_SCENARIO = "session_end";
export const UNAVAILABLE_CONNECTION = "2";
/** Scenarios without a recording: their stream is generated. */
export const LONG_SCENARIO = "long";
export const MISSING_SCENARIO = "missing";
export const EXPIRED_SCENARIO = "expired";
export const UNAUTHORIZED_SCENARIO = "unauthorized";
/** Content chunks of the long stream, one frame each, after its run.started (seq 1). */
export const LONG_CHUNKS = 100;
/**
 * How late the long stream's head comes: below the per-turn deadline of the
 * app groups (scripts/test.sh `app_deadline_ms`, app.test.ts TEST_DEADLINE_MS),
 * so the head arrives within one window.
 */
export const LONG_HEAD_DELAY_MS = 200;
/** The seq of the long stream's first chunk; run.completed follows the last one. */
export const LONG_FIRST_CHUNK_SEQ = 2;
/** The delta every long-stream chunk carries. */
export const LONG_DELTA = "word ";
