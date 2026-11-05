// The Head Agent's sessions for the session-stitching tests, shared by the
// loopback mock (sdk-server/stitch.ts) and the view tests; no SDK module is
// imported, so the mock compiles without the SDK's graph.
//
// Transcripts are the API lane's recorded `GET /sessions/{id}/messages`
// responses (fixtures/events-v1/*.messages.json, with their recorded status).
// No `GET /sessions` page was recorded: each page below is composed in the
// list's served shape (`{items, cursor, has_more}`), and every item is the
// recorded `create_session` body of plain.json with its id, status and times
// changed. Every recording names its run `run-1`, while the API's run ids are
// unique, so the transcript of each older session is served with its runs
// renamed after the session (`transcriptOf`).
import plainMessages from "../../fixtures/events-v1/plain.messages.json" with { type: "text" };
import toolCallMessages from "../../fixtures/events-v1/tool_call.messages.json" with { type: "text" };
import rejectMessages from "../../fixtures/events-v1/approval_reject.messages.json" with { type: "text" };
import sessionEndMessages from "../../fixtures/events-v1/session_end.messages.json" with { type: "text" };

/** The scenario whose list has a live session with history before it, over two pages. */
export const STITCH_SCENARIO = "stitch";
/** The scenario whose list has no live session: the first message creates one. */
export const STITCH_NEW_SCENARIO = "stitch-new";
/** The cursor of the stitch list's second page. */
export const SECOND_PAGE_CURSOR = "page-2";
/** The page size the console asks for (chat/stitch.ts SESSION_PAGE_SIZE). */
export const PAGE_LIMIT = 20;

/** The live session of STITCH_SCENARIO, created by the recorded plain scenario. */
export const LIVE_SESSION = "session-1";
export const EXPIRED_SESSION = "session-2";
export const CLOSED_SESSION = "session-3";
export const OLDEST_SESSION = "session-4";

/** One recorded transcript response, as `*.messages.json` hold it: its provenance, status and body. */
export interface RecordedTranscript {
  scenario: string;
  request: string;
  recorded: string;
  status: number;
  body: unknown;
}

interface SessionRow {
  id: string;
  status: string;
  createdAt: string;
  transcript: string;
}

/** Newest first, as the list pages; the first two are the first page. */
const ROWS: SessionRow[] = [
  { id: LIVE_SESSION, status: "active", createdAt: "2026-01-03T09:00:00.000Z", transcript: plainMessages },
  { id: EXPIRED_SESSION, status: "expired", createdAt: "2026-01-02T15:00:00.000Z", transcript: toolCallMessages },
  { id: CLOSED_SESSION, status: "closed", createdAt: "2026-01-02T09:00:00.000Z", transcript: sessionEndMessages },
  { id: OLDEST_SESSION, status: "expired", createdAt: "2026-01-01T09:00:00.000Z", transcript: rejectMessages },
];
/** Rows of the stitch list's first page; the rest are its second page. */
const FIRST_PAGE_ROWS = 3;
/** The recorded run id, as every transcript names it in `run_id` and `message_id`. */
const RECORDED_RUN = "\"run-1";
const RUN_PREFIX = "\"run-";
const SESSION_PREFIX = "session-";

/** The recorded `create_session` body of plain.json, with this row's id, status and times. */
function item(row: SessionRow): string {
  const id = JSON.stringify(row.id);
  const created = JSON.stringify(row.createdAt);
  return (
    "{\"session_id\":" + id + ",\"tenant_id\":\"tenant-1\",\"agent_id\":\"agent-head\",\"created_by\":\"id-1\",\"status\":" + JSON.stringify(row.status) +
    ",\"conversation_history\":[],\"metadata\":{\"canon_key\":\"event-fixture-plain\"},\"runs\":[],\"branches\":[],\"active_branch\":\"main\"" +
    ",\"created_at\":" + created + ",\"updated_at\":" + created + ",\"expires_at\":null,\"title\":null}"
  );
}

function page(rows: SessionRow[], cursor: string | null): string {
  const cursorText = cursor === null ? "null" : JSON.stringify(cursor);
  return "{\"items\":[" + rows.map(item).join(",") + "],\"cursor\":" + cursorText + ",\"has_more\":" + String(cursor !== null) + "}";
}

/** `GET /api/v1/sessions` of a scenario; `cursor` is the query's, empty for the first page; null for an unknown cursor. */
export function sessionPage(scenario: string, cursor: string): string | null {
  if (scenario === STITCH_SCENARIO) {
    if (cursor === "") return page(ROWS.slice(0, FIRST_PAGE_ROWS), SECOND_PAGE_CURSOR);
    if (cursor === SECOND_PAGE_CURSOR) return page(ROWS.slice(FIRST_PAGE_ROWS), null);
    return null;
  }
  // No live session: an expired and a closed one only.
  if (scenario === STITCH_NEW_SCENARIO && cursor === "") return page(ROWS.slice(1, FIRST_PAGE_ROWS), null);
  return null;
}

/** An empty first page: the console has no session yet. */
export const EMPTY_PAGE = "{\"items\":[],\"cursor\":null,\"has_more\":false}";
/** A first page with one live session `session-1` and no history. */
export function liveOnlyPage(): string {
  return page([{ id: LIVE_SESSION, status: "active", createdAt: "2026-01-01T00:00:00.000Z", transcript: "" }], null);
}
/** The transcript of a session with no message yet. */
export const EMPTY_TRANSCRIPT = "{\"messages\":[],\"items\":[],\"total\":0}";

/** The recorded transcript of a stitch session; the runs of every session but the live one renamed after it. */
export function transcriptOf(sessionId: string): RecordedTranscript | null {
  for (const row of ROWS) {
    if (row.id !== sessionId) continue;
    const text = row.id === LIVE_SESSION ? row.transcript : row.transcript.split(RECORDED_RUN).join(RUN_PREFIX + row.id.slice(SESSION_PREFIX.length));
    return JSON.parse(text) as RecordedTranscript;
  }
  return null;
}
