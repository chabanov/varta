// SDK client tests executed by Varta against the loopback mock API:
//   TEST_GROUP=<group> SDK_BASE_URL=http://127.0.0.1:<port> varta dev --config <config>
// `scripts/sdk-loopback.sh` starts the mock (`varta.sdk-server.json`), writes the
// exact `net.request` grant for its port and runs each group as one execution.
// Every group constructs the real, unmodified UarpClient and drives its Transport
// and EventStream through the engine's fetch; nothing here mocks the SDK.
import {
  APIConnectionError,
  APIError,
  APITimeoutError,
  InternalServerError,
  NotFoundError,
  PermissionDeniedError,
  ServiceUnavailableError,
  UarpClient,
  UarpError,
  type StreamState,
  type UarpEvent,
} from "../../../node_modules/uarp-sdk/src/index.js";
import { PLAIN_RUN_ADDED, PLAIN_STARTED, PLAIN_THINKING_1, PLAIN_CONTENT, PLAIN_COMPLETED, PLAIN_RUN_DONE } from "../frames.js";
import { equal, report, summary, type Report } from "../check.js";
import {
  ABORT_AFTER_MS,
  API_KEY,
  ATTEMPT_HEADER,
  BARE_CORRELATION_ID,
  PROBLEM_REQUEST_ID,
  RETRY_AFTER_SECONDS,
  SESSION_ID,
  SLOW_RESPONSE_MS,
} from "./contract.js";

const MS_PER_SECOND = 1000;
/** `backoffDelay(0)` never waits longer than this; a Retry-After wait is at least a second. */
const FIRST_BACKOFF_MAX_MS = 500;
/** Loopback, but not the mock's origin: `net.request` grants only the bound port. */
const UNGRANTED_ORIGIN = "http://127.0.0.1:9";
const GROUPS: string[] = ["sdk-unary", "sdk-problem", "sdk-retry", "sdk-abort", "sdk-timeout", "sdk-sse"];

function client(base: string): UarpClient {
  return new UarpClient({ apiKey: API_KEY, baseURL: base });
}

function text(value: string | undefined): string {
  return value === undefined ? "undefined" : value;
}

async function unaryTests(r: Report, base: string): Promise<void> {
  const uarp = client(base + "/");
  equal(r, "unary: the base URL drops its trailing slash", uarp.baseURL, base);
  const health = await uarp.health.get({ baseURL: base + "/unary" });
  equal(r, "unary: status", text(health.status), "healthy");
  equal(r, "unary: version", text(health.version), "2026-09-10");
  equal(r, "unary: build_sha", text(health.build_sha), "loopback");
  equal(r, "unary: kv_connected", String(health.kv_connected), "true");
  equal(r, "unary: uptime_seconds", String(health.uptime_seconds), "42");

  // Deny by default: an origin outside the exact grant never reaches the network;
  // the SDK reports the engine's NotAllowedError as a connection error with that cause.
  try {
    await uarp.health.get({ baseURL: UNGRANTED_ORIGIN, maxRetries: 0 });
    equal(r, "denied: an ungranted origin rejects", "resolved", "rejected");
  } catch (error) {
    equal(r, "denied: APIConnectionError", String(error instanceof APIConnectionError), "true");
    const cause = error instanceof APIConnectionError ? error.cause : undefined;
    equal(r, "denied: caused by the engine's NotAllowedError", String(cause instanceof DOMException && cause.name === "NotAllowedError"), "true");
  }
}

async function problemTests(r: Report, base: string): Promise<void> {
  const uarp = client(base);
  try {
    await uarp.health.get({ baseURL: base + "/problem" });
    equal(r, "problem: a 404 rejects", "resolved", "rejected");
  } catch (error) {
    equal(r, "problem: NotFoundError", String(error instanceof NotFoundError), "true");
    equal(r, "problem: an APIError, a UarpError and an Error", String(error instanceof APIError && error instanceof UarpError && error instanceof Error), "true");
    if (error instanceof APIError) {
      equal(r, "problem: status", String(error.status), "404");
      equal(r, "problem: name", error.name, "NotFoundError");
      equal(r, "problem: message", error.message, "404 Not Found — session \"missing\" not found (correlationId: corr-404)");
      equal(r, "problem: code", text(error.code), "NOT_FOUND");
      equal(r, "problem: reason", text(error.reason), "unknown_session");
      equal(r, "problem: correlationId from the document", text(error.correlationId), "corr-404");
      equal(r, "problem: requestId from x-request-id", text(error.requestId), PROBLEM_REQUEST_ID);
      equal(r, "problem: title", text(error.problem.title), "Not Found");
      const fields = error.validationErrors.map((entry) => text(entry.field) + ": " + text(entry.message));
      equal(r, "problem: validation errors", fields.join("; "), "session_id: unknown");
    }
  }
  try {
    await uarp.health.get({ baseURL: base + "/bare" });
    equal(r, "bare: a 403 rejects", "resolved", "rejected");
  } catch (error) {
    equal(r, "bare: PermissionDeniedError", String(error instanceof PermissionDeniedError), "true");
    if (error instanceof APIError) {
      equal(r, "bare: message keeps the server's reason", error.message, "403 HTTP 403 — Insufficient role: viewer");
      equal(r, "bare: detail", text(error.problem.detail), "Insufficient role: viewer");
      equal(r, "bare: correlationId from x-correlation-id", text(error.correlationId), BARE_CORRELATION_ID);
      equal(r, "bare: no requestId", text(error.requestId), "undefined");
      equal(r, "bare: no code", text(error.code), "undefined");
    }
  }
}

/** Counts the transport's attempts and labels each one for the stateless mock. */
class CountingFetch {
  attempts: number = 0;

  fetch(input: string, init: RequestInit): Promise<Response> {
    this.attempts += 1;
    const headers = new Headers(init.headers);
    headers.set(ATTEMPT_HEADER, String(this.attempts));
    return fetch(input, { method: init.method, headers: headers, body: init.body, signal: init.signal, redirect: init.redirect });
  }
}

function delayClass(elapsed: number): string {
  if (elapsed >= RETRY_AFTER_SECONDS * MS_PER_SECOND) return "retry-after";
  if (elapsed <= FIRST_BACKOFF_MAX_MS) return "backoff";
  return "between " + String(elapsed);
}

async function retryTests(r: Report, base: string): Promise<void> {
  const counter = new CountingFetch();
  const uarp = new UarpClient({
    apiKey: API_KEY,
    baseURL: base,
    fetch: (input: string, init: RequestInit): Promise<Response> => counter.fetch(input, init),
  });
  const started = Date.now();
  const health = await uarp.health.get({ baseURL: base + "/retry" });
  const elapsed = Date.now() - started;
  equal(r, "retry: the second attempt succeeds", text(health.status), "healthy");
  equal(r, "retry: one retry after the 503", String(counter.attempts), "2");
  equal(r, "retry: the wait follows Retry-After", delayClass(elapsed), "retry-after");

  counter.attempts = 0;
  try {
    await uarp.health.get({ baseURL: base + "/no-retry" });
    equal(r, "no-retry: a 503 with x-should-retry: false rejects", "resolved", "rejected");
  } catch (error) {
    equal(r, "no-retry: ServiceUnavailableError, an InternalServerError", String(error instanceof ServiceUnavailableError && error instanceof InternalServerError), "true");
    if (error instanceof APIError) equal(r, "no-retry: code", text(error.code), "SERVICE_UNAVAILABLE");
    equal(r, "no-retry: a single attempt", String(counter.attempts), "1");
  }
}

async function abortTests(r: Report, base: string): Promise<void> {
  const uarp = client(base);
  const controller = new AbortController();
  setTimeout(() => controller.abort(), ABORT_AFTER_MS);
  const started = Date.now();
  try {
    await uarp.health.get({ baseURL: base + "/slow", signal: controller.signal });
    equal(r, "abort: an aborted request rejects", "resolved", "rejected");
  } catch (error) {
    equal(r, "abort: rejects with the signal's AbortError", String(error instanceof DOMException && error.name === "AbortError"), "true");
    equal(r, "abort: before the slow answer", String(Date.now() - started < SLOW_RESPONSE_MS), "true");
  }

  try {
    await uarp.health.get({ baseURL: base + "/slow", signal: controller.signal });
    equal(r, "abort: an already aborted signal rejects", "resolved", "rejected");
  } catch (error) {
    equal(r, "abort: an already aborted signal rejects with its reason", String(error instanceof DOMException && error.name === "AbortError"), "true");
  }
}

/**
 * Its own execution: an aborted request's connection stays open until the client's
 * event loop next wakes, so a request right after an abort meets the mock's busy
 * worker (README, pilot finding F2).
 */
async function timeoutTests(r: Report, base: string): Promise<void> {
  const uarp = client(base);
  const timed = Date.now();
  try {
    await uarp.health.get({ baseURL: base + "/slow", timeout: ABORT_AFTER_MS, maxRetries: 0 });
    equal(r, "timeout: a slow request times out", "resolved", "rejected");
  } catch (error) {
    equal(r, "timeout: APITimeoutError", String(error instanceof APITimeoutError), "true");
    if (error instanceof Error) equal(r, "timeout: message", error.message, "Request timed out after " + String(ABORT_AFTER_MS) + " ms");
    equal(r, "timeout: before the slow answer", String(Date.now() - timed < SLOW_RESPONSE_MS), "true");
  }
}

interface RunAdded {
  run_id: string;
}

async function sseTests(r: Report, base: string): Promise<void> {
  const uarp = client(base);
  const states: string[] = [];
  const stream = uarp.sessions.streamSessionEvents(
    SESSION_ID,
    { event_schema: 1 },
    { baseURL: base + "/stream", stream: { onState: (state: StreamState) => { states.push(state.type); } } },
  );
  const events: UarpEvent[] = [];
  for await (const event of stream) events.push(event);
  equal(r, "sse: event names and ids", events.map((event) => event.event + " " + text(event.id)).join("; "),
    "run_added run-1:added; run.started run-1:1; llm.chunk run-1:7; llm.chunk run-1:23; run.completed run-1:29; run_done run-1:done");
  const expected: string[] = [PLAIN_RUN_ADDED, PLAIN_STARTED, PLAIN_THINKING_1, PLAIN_CONTENT, PLAIN_COMPLETED, PLAIN_RUN_DONE];
  let same = events.length === expected.length;
  for (let i = 0; same && i < expected.length; i++) same = events[i].data === expected[i];
  equal(r, "sse: data arrives byte for byte", String(same), "true");
  equal(r, "sse: retry-only and comment frames deliver nothing", events.map((event) => event.retry === undefined ? "-" : String(event.retry)).join(""), "------");
  if (events.length > 0) {
    // Bound to a local first: `events[0].json<RunAdded>()` is refused (README, pilot finding F1).
    const first = events[0];
    equal(r, "sse: json() decodes a frame", first.json<RunAdded>().run_id, "run-1");
  }
  equal(r, "sse: [DONE] ends without a reconnect", states.join(" "), "connecting connected disconnected");
  equal(r, "sse: the stream is not closed by its end", String(stream.closed), "false");
}

export async function main(): Promise<void> {
  const group = varta.env.get("TEST_GROUP");
  const base = varta.env.get("SDK_BASE_URL");
  const r = report();
  if (group === "sdk-unary") await unaryTests(r, base);
  else if (group === "sdk-problem") await problemTests(r, base);
  else if (group === "sdk-retry") await retryTests(r, base);
  else if (group === "sdk-abort") await abortTests(r, base);
  else if (group === "sdk-timeout") await timeoutTests(r, base);
  else if (group === "sdk-sse") await sseTests(r, base);
  else {
    console.log(`UNKNOWN GROUP ${group}; groups: ${GROUPS.join(" ")}`);
    return;
  }
  summary(r);
}
