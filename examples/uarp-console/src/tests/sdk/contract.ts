// The loopback contract shared by the mock API (`../sdk-server/main.ts`) and the
// SDK client tests (`./main.ts`): canned UARP-shaped bodies, header names, timings.
// Nothing here is a credential: API_KEY is a fixed test value the mock compares.
import { SDK_VERSION } from "../../../node_modules/uarp-sdk/src/generated/meta.js";

/** The bearer key the mock requires; a test value, not a UARP key. */
export const API_KEY = "uarp_loopback_test-only";
/** What the real Transport sends as `User-Agent`. */
export const SDK_USER_AGENT = "uarp-sdk-typescript/" + SDK_VERSION;
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
