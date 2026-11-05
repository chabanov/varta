// The Head Agent's session list and transcripts for the console end to end:
// every scenario's first page (`GET /api/v1/sessions`, its query checked),
// and for the stitch scenarios their pages behind the cursor and each
// session's recorded transcript (404 for the closed one, as recorded). The
// stitch scenarios replay the recorded plain scenario for the live stream, the
// creation and the message (e2e.ts). The data is in ../sessions.ts.
import { sessionPage, transcriptOf, PAGE_LIMIT, STITCH_NEW_SCENARIO, STITCH_SCENARIO } from "../sessions.js";
import { HEAD_AGENT_ID } from "../sdk/contract.js";
import type { Responder } from "./responder.js";

const OK = 200;
const BAD_REQUEST = 400;
const NOT_FOUND = 404;

const SESSIONS_PATH = "/api/v1/sessions";
const SESSION_PREFIX = SESSIONS_PATH + "/";
const MESSAGES_SUFFIX = "/messages";
const CURSOR_PARAMETER = "&cursor=";

/** The first page's query, in the SDK's parameter order (`agent_id`, `limit`, `cursor`). */
function firstPageQuery(): string {
  return "?agent_id=" + HEAD_AGENT_ID + "&limit=" + String(PAGE_LIMIT);
}

/** Whether a scenario is served here. */
export function stitched(scenario: string): boolean {
  return scenario === STITCH_SCENARIO || scenario === STITCH_NEW_SCENARIO;
}

/** `GET /api/v1/sessions` of a scenario: a stitch scenario's page, or `firstPage` for any other; the query is checked. */
export function listSessions(r: Responder, scenario: string, query: string, firstPage: string): VartaHttpResponse {
  const first = firstPageQuery();
  if (!query.startsWith(first)) return r.problem(BAD_REQUEST, "Bad Request", "expected " + first + ", got " + query);
  const rest = query.slice(first.length);
  if (rest !== "" && !rest.startsWith(CURSOR_PARAMETER)) return r.problem(BAD_REQUEST, "Bad Request", "unexpected query " + query);
  const cursor = rest.slice(CURSOR_PARAMETER.length);
  if (!stitched(scenario)) return rest === "" ? r.json(OK, firstPage) : r.problem(BAD_REQUEST, "Bad Request", "no page after the first");
  const page = sessionPage(scenario, cursor);
  return page === null ? r.problem(BAD_REQUEST, "Bad Request", "unknown cursor " + cursor) : r.json(OK, page);
}

/** `GET /api/v1/sessions/{id}/messages` of a stitch scenario, as recorded. */
export function transcript(r: Responder, path: string): VartaHttpResponse | null {
  if (!path.startsWith(SESSION_PREFIX) || !path.endsWith(MESSAGES_SUFFIX)) return null;
  const sessionId = path.slice(SESSION_PREFIX.length, path.length - MESSAGES_SUFFIX.length);
  const recorded = transcriptOf(sessionId);
  if (recorded === null) return r.problem(NOT_FOUND, "Not Found", "no session " + sessionId);
  return r.json(recorded.status, JSON.stringify(recorded.body));
}
