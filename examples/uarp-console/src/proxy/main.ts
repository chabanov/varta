// The console's local API proxy (`varta.proxy.json`): a Varta HTTP/1
// application that lets the console's browser build, served by
// `varta dev --target browser` from the page origin, reach the UARP API.
// A browser refuses the API's answers to that origin (the API sends no CORS
// headers); the proxy runs natively, where no CORS applies, forwards each
// request and answers the browser with the CORS headers of that one origin.
//
// It forwards only what the console needs and nothing else:
// - only from the page origin (`Origin` must be it exactly);
// - only `/api/v1/…` paths, only GET and POST (plus the CORS preflight);
// - only the request headers the SDK sends (`authorization` carries the key,
//   which the proxy passes on and never logs or stores);
// - only the response headers the console reads.
// It binds loopback only (`net.listen`), and its `net.request` grant names
// the API origin alone, so it can reach nothing else.
//
// The handler returns a Web `Response` whose body is the API's own body
// stream, so an event stream (the live session) reaches the browser chunk by
// chunk as the API sends it; ordinary answers stream the same way.

/** The API every request goes to, unless `PROXY_UPSTREAM` names a granted test origin. */
const UPSTREAM = "https://api.snaga.ai";
/** The page origin of `varta dev --target browser --listen 127.0.0.1:8770`. */
const PAGE_ORIGIN = "http://127.0.0.1:8770";
const API_PREFIX = "/api/v1/";

const NO_CONTENT = 204;
const FORBIDDEN = 403;
const NOT_FOUND = 404;
const METHOD_NOT_ALLOWED = 405;
const BAD_GATEWAY = 502;
/** How long a browser may cache a preflight answer, in seconds. */
const PREFLIGHT_MAX_AGE = "600";

const FORWARDED_METHODS = ["GET", "POST"];
const PREFLIGHT_METHOD = "OPTIONS";
/** Request headers passed to the API (lower case); the host owns user-agent. */
const FORWARDED_REQUEST_HEADERS = ["accept", "authorization", "content-type", "idempotency-key", "last-event-id"];
/** Request headers the browser may send (the SDK also sets user-agent). */
const ALLOWED_REQUEST_HEADERS = "accept, authorization, content-type, idempotency-key, last-event-id, user-agent";
/** Response headers passed back to the console and exposed to it. */
const FORWARDED_RESPONSE_HEADERS = ["content-type", "retry-after", "x-correlation-id", "x-request-id", "x-should-retry"];
const PROBLEM_TYPE = "application/problem+json";

function header(request: VartaHttpRequest, name: string): string {
  for (const field of request.headers) {
    if (field.name.toLowerCase() === name) return field.value;
  }
  return "";
}

/** A non-empty test override, else the product value. */
function setting(value: string | undefined, fallback: string): string {
  return value === undefined || value === "" ? fallback : value;
}

/** The CORS headers of an answer to the page origin. */
function cors(origin: string): Headers {
  const headers = new Headers();
  headers.set("access-control-allow-origin", origin);
  headers.set("access-control-expose-headers", FORWARDED_RESPONSE_HEADERS.join(", "));
  headers.set("vary", "origin");
  return headers;
}

/** An RFC 9457 problem the proxy itself answers. */
function problem(status: number, title: string, detail: string, origin: string): Response {
  const headers = cors(origin);
  headers.set("content-type", PROBLEM_TYPE);
  const body = JSON.stringify({ type: "about:blank", title: title, status: status, detail: detail });
  return new Response(body, { status: status, headers: headers });
}

async function forward(request: VartaHttpRequest, upstream: string, origin: string): Promise<Response> {
  const headers = new Headers();
  for (const name of FORWARDED_REQUEST_HEADERS) {
    const value = header(request, name);
    if (value !== "") headers.set(name, value);
  }
  const init: RequestInit = { method: request.method, headers: headers };
  if (request.method === "POST") init.body = request.body;
  try {
    const response = await fetch(upstream + request.target, init);
    const answer = cors(origin);
    for (const name of FORWARDED_RESPONSE_HEADERS) {
      const value = response.headers.get(name);
      if (value !== null) answer.set(name, value);
    }
    // The API's body streams through as it arrives.
    return new Response(response.body, { status: response.status, headers: answer });
  } catch (error) {
    // The cause names the request, never its headers: the key stays out.
    const reason = error instanceof Error ? error.name + ": " + error.message : "request failed";
    return problem(BAD_GATEWAY, "Bad Gateway", "the API could not be reached: " + reason, origin);
  }
}

async function handle(request: VartaHttpRequest): Promise<Response> {
  const origin = setting(globalThis.process.env.PROXY_PAGE_ORIGIN, PAGE_ORIGIN);
  if (header(request, "origin") !== origin) {
    return problem(FORBIDDEN, "Forbidden", "only the console page origin may use this proxy", origin);
  }
  if (!request.target.startsWith(API_PREFIX)) {
    return problem(NOT_FOUND, "Not Found", "the proxy forwards " + API_PREFIX + " paths only", origin);
  }
  if (request.method === PREFLIGHT_METHOD) {
    const wanted = header(request, "access-control-request-method");
    if (!FORWARDED_METHODS.includes(wanted)) {
      return problem(METHOD_NOT_ALLOWED, "Method Not Allowed", "the proxy forwards GET and POST only", origin);
    }
    const headers = cors(origin);
    headers.set("access-control-allow-methods", FORWARDED_METHODS.join(", "));
    headers.set("access-control-allow-headers", ALLOWED_REQUEST_HEADERS);
    headers.set("access-control-max-age", PREFLIGHT_MAX_AGE);
    return new Response(null, { status: NO_CONTENT, headers: headers });
  }
  if (!FORWARDED_METHODS.includes(request.method)) {
    return problem(METHOD_NOT_ALLOWED, "Method Not Allowed", "the proxy forwards GET and POST only", origin);
  }
  return forward(request, setting(globalThis.process.env.PROXY_UPSTREAM, UPSTREAM), origin);
}

export function main(): void {}

varta.http.serve({ hostname: "127.0.0.1", port: 8771 }, handle);
