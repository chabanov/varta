// Loopback mock of the UARP API for the SDK client tests (`varta.sdk-server.json`).
// It is itself a Varta HTTP/1 application: `varta dev` binds 127.0.0.1 on an
// OS-assigned port (exact `net.listen` grant `127.0.0.1:0`) and reports the bound
// address on stderr, which `scripts/sdk-loopback.sh` reads.
//
// Every route answers a canned, UARP-shaped response. The client test points each
// scenario at its own prefix through the SDK's per-call `baseURL` option, so the
// SDK's own paths (`/health`, `/api/v1/sessions/{id}/events`) follow the prefix.
// Each request runs in a fresh store, so the mock keeps no state between requests:
// the retry scenario reads the attempt number the client test's counting `fetch`
// sends in `x-loopback-attempt`.
import {
  PLAIN_RUN_ADDED,
  PLAIN_STARTED,
  PLAIN_THINKING_1,
  PLAIN_CONTENT,
  PLAIN_COMPLETED,
  PLAIN_RUN_DONE,
} from "../frames.js";
import {
  API_KEY,
  ATTEMPT_HEADER,
  BARE_ERROR,
  BARE_CORRELATION_ID,
  HEALTH,
  NOT_FOUND_PROBLEM,
  PROBLEM_REQUEST_ID,
  RETRY_AFTER_SECONDS,
  SDK_USER_AGENT,
  SESSION_ID,
  SLOW_RESPONSE_MS,
  STREAM_DONE,
  STREAM_RETRY_MS,
  UNARY_REQUEST_ID,
  UNAVAILABLE_PROBLEM,
} from "../sdk/contract.js";

const OK = 200;
const BAD_REQUEST = 400;
const UNAUTHORIZED = 401;
const FORBIDDEN = 403;
const NOT_FOUND = 404;
const NOT_ACCEPTABLE = 406;
const SERVICE_UNAVAILABLE = 503;

/** The value of `x-loopback-attempt` on the retry route's first request. */
const FIRST_ATTEMPT = "1";

const JSON_TYPE = "application/json";
const PROBLEM_TYPE = "application/problem+json";
const EVENT_STREAM_TYPE = "text/event-stream";

/** One recorded `event_schema=1` frame and the SSE `event`/`id` fields it travelled with. */
interface Frame {
  event: string;
  id: string;
  data: string;
}

/** The plain scenario's frames, in the order the session stream sent them. */
const FRAMES: Frame[] = [
  { event: "run_added", id: "run-1:added", data: PLAIN_RUN_ADDED },
  { event: "run.started", id: "run-1:1", data: PLAIN_STARTED },
  { event: "llm.chunk", id: "run-1:7", data: PLAIN_THINKING_1 },
  { event: "llm.chunk", id: "run-1:23", data: PLAIN_CONTENT },
  { event: "run.completed", id: "run-1:29", data: PLAIN_COMPLETED },
  { event: "run_done", id: "run-1:done", data: PLAIN_RUN_DONE },
];

function header(request: VartaHttpRequest, name: string): string {
  for (const field of request.headers) {
    if (field.name.toLowerCase() === name) return field.value;
  }
  return "";
}

function respond(status: number, type: string, body: string, extra: VartaHttpHeader[]): VartaHttpResponse {
  const headers: VartaHttpHeader[] = [{ name: "content-type", value: type }];
  for (const field of extra) headers.push(field);
  return { status: status, body: body, headers: headers };
}

function problem(status: number, title: string, detail: string): VartaHttpResponse {
  return respond(status, PROBLEM_TYPE, JSON.stringify({ type: "about:blank", title: title, status: status, detail: detail }), []);
}

function health(requestId: string): VartaHttpResponse {
  return respond(OK, JSON_TYPE, HEALTH, [{ name: "x-request-id", value: requestId }]);
}

/** A finite session stream: a keep-alive comment, a `retry:` field, the frames and `[DONE]`. */
function eventStream(): string {
  let body = ": keep-alive\n\nretry: " + String(STREAM_RETRY_MS) + "\n\n";
  for (const frame of FRAMES) {
    body += "event: " + frame.event + "\nid: " + frame.id + "\ndata: " + frame.data + "\n\n";
  }
  return body + "event: message\ndata: " + STREAM_DONE + "\n\n";
}

function delay(ms: number): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(() => resolve(), ms);
  });
}

function path(target: string): string {
  const query = target.indexOf("?");
  return query < 0 ? target : target.slice(0, query);
}

async function handle(request: VartaHttpRequest): Promise<VartaHttpResponse> {
  if (header(request, "authorization") !== "Bearer " + API_KEY) {
    return problem(UNAUTHORIZED, "Unauthorized", "missing or wrong bearer key");
  }
  if (header(request, "user-agent") !== SDK_USER_AGENT) {
    return problem(BAD_REQUEST, "Bad Request", "unexpected user agent " + header(request, "user-agent"));
  }
  const route = path(request.target);
  if (route === "/unary/health") return health(UNARY_REQUEST_ID);
  if (route === "/problem/health") {
    return respond(NOT_FOUND, PROBLEM_TYPE, NOT_FOUND_PROBLEM, [{ name: "x-request-id", value: PROBLEM_REQUEST_ID }]);
  }
  if (route === "/bare/health") {
    return respond(FORBIDDEN, JSON_TYPE, BARE_ERROR, [{ name: "x-correlation-id", value: BARE_CORRELATION_ID }]);
  }
  if (route === "/retry/health") {
    if (header(request, ATTEMPT_HEADER) === FIRST_ATTEMPT) {
      return respond(SERVICE_UNAVAILABLE, PROBLEM_TYPE, UNAVAILABLE_PROBLEM, [{ name: "retry-after", value: String(RETRY_AFTER_SECONDS) }]);
    }
    return health(UNARY_REQUEST_ID);
  }
  if (route === "/no-retry/health") {
    return respond(SERVICE_UNAVAILABLE, PROBLEM_TYPE, UNAVAILABLE_PROBLEM, [{ name: "x-should-retry", value: "false" }]);
  }
  if (route === "/slow/health") {
    await delay(SLOW_RESPONSE_MS);
    return health(UNARY_REQUEST_ID);
  }
  if (route === "/stream/api/v1/sessions/" + SESSION_ID + "/events") {
    if (request.target !== route + "?event_schema=1") return problem(BAD_REQUEST, "Bad Request", "expected event_schema=1 only");
    if (header(request, "accept") !== EVENT_STREAM_TYPE) return problem(NOT_ACCEPTABLE, "Not Acceptable", "expected an event stream request");
    return respond(OK, EVENT_STREAM_TYPE, eventStream(), [{ name: "cache-control", value: "no-cache" }]);
  }
  return problem(NOT_FOUND, "Not Found", "no loopback route " + route);
}

export function main(): void {}

varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
