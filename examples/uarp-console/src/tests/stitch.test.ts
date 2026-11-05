// Session stitching tests on the recorded transcripts and the composed session
// list (sessions.ts), through an in-process HistoryApi: the live session comes
// from the newest page (its newest session, only when active, so the timeline
// stays in creation order), older sessions load one per request, newest first,
// each prepended with its date, the closed one as a note, the second page only
// once the first is used up; a failed load is reported and can be retried.
import { NotFoundError, ServiceUnavailableError, type GetSessionMessagesResponse, type ListSessionsParams, type ListSessionsResponse, type ProblemDocument } from "uarp-sdk";
import { Runtime, untracked } from "../l1/reactive.js";
import { ChatStore } from "../chat/store.js";
import { Stitcher, type HistoryApi, type HistoryState } from "../chat/stitch.js";
import { show } from "../chat/show.js";
import type { Segment } from "../chat/timeline.js";
import { sessionPage, transcriptOf, STITCH_NEW_SCENARIO, STITCH_SCENARIO, STITCH_SUPERSEDED_SCENARIO } from "./sessions.js";
import { UNAVAILABLE_PROBLEM } from "./sdk/contract.js";
import { equal, type Report } from "./check.js";

const AGENT_ID = "agent-head";
const GONE = "gone";
const NOT_FOUND = 404;
const UNAVAILABLE = 503;

/** Answers from the recorded transcripts and the composed list; `failNext` answers the next transcript with a 503. */
class RecordedHistory implements HistoryApi {
  scenario: string;
  calls: string[] = [];
  failNext: boolean = false;

  constructor(scenario: string) {
    this.scenario = scenario;
  }

  async list(params: ListSessionsParams): Promise<ListSessionsResponse> {
    const cursor = params.cursor === undefined ? "" : params.cursor;
    this.calls.push("list " + String(params.agent_id) + " " + String(params.limit) + (cursor === "" ? "" : " " + cursor));
    const page = sessionPage(this.scenario, cursor);
    if (page === null) throw new Error("no page " + cursor);
    return JSON.parse(page) as ListSessionsResponse;
  }

  async getSessionMessages(sessionId: string): Promise<GetSessionMessagesResponse> {
    this.calls.push("messages " + sessionId);
    if (this.failNext) {
      this.failNext = false;
      throw new ServiceUnavailableError(UNAVAILABLE, JSON.parse(UNAVAILABLE_PROBLEM) as ProblemDocument, new Headers());
    }
    const recorded = transcriptOf(sessionId);
    if (recorded === null) throw new Error("no session " + sessionId);
    if (recorded.status === NOT_FOUND) throw new NotFoundError(NOT_FOUND, recorded.body as ProblemDocument, new Headers());
    return recorded.body as GetSessionMessagesResponse;
  }
}

function state(store: ChatStore, stitcher: Stitcher): string {
  return untracked<HistoryState>(store.rt, stitcher.history.get);
}

export async function stitchTests(r: Report): Promise<void> {
  const store = new ChatStore(new Runtime());
  const api = new RecordedHistory(STITCH_SCENARIO);
  const failures: string[] = [];
  const stitcher = new Stitcher(store, api, AGENT_ID, GONE, (reason: string): void => {
    failures.push(reason);
  });
  const live = await stitcher.begin();
  equal(r, "stitch: the newest open session is live", live === null ? "none" : live.session_id + " " + live.status, "session-1 active");
  equal(r, "stitch: the rest of the page waits as history", state(store, stitcher) + " " + String(stitcher.queue.length), "more 2");

  await stitcher.older();
  equal(
    r,
    "stitch: the next older session is prepended with its date",
    show(store.timeline),
    '(timeline (segment history 2026-01-02 (ask sent "Call the `now` tool and tell me the current UTC time in one line.") (turn completed (thinking 148) (answer "2026-01-01T00:00:02.838Z UTC") (tool now done {} "2026-01-01T00:00:03.537Z"))))',
  );
  await stitcher.older();
  equal(r, "stitch: a closed session's transcript is gone", show(store.timeline).startsWith('(timeline (segment history 2026-01-02 (note "gone")) (segment history 2026-01-02 (ask') ? "noted" : show(store.timeline), "noted");

  api.failNext = true;
  await stitcher.older();
  equal(r, "stitch: a failed load is reported", failures.join("; "), "ServiceUnavailableError: 503 Service Unavailable — scheduler busy");
  equal(r, "stitch: a failed load can be retried", state(store, stitcher) + " " + String(store.timeline.segments.length), "more 2");

  await stitcher.older();
  equal(
    r,
    "stitch: the second page loads once the first is used up",
    show(store.timeline).startsWith('(timeline (segment history 2026-01-01 (ask sent "Call manage_permissions with action \\"get\\" and agent_id \\"agent-probe\\". Do not call anything else.")) (segment history 2026-01-02 (note "gone"))')
      ? "loaded"
      : show(store.timeline),
    "loaded",
  );
  equal(r, "stitch: nothing remains", state(store, stitcher), "none");
  await stitcher.older();
  equal(
    r,
    "stitch: requests in order, none once nothing remains",
    api.calls.join("; "),
    "list agent-head 20; messages session-2; messages session-3; list agent-head 20 page-2; messages session-4; messages session-4",
  );

  const fresh = new ChatStore(new Runtime());
  const none = new Stitcher(fresh, new RecordedHistory(STITCH_NEW_SCENARIO), AGENT_ID, GONE, (reason: string): void => {});
  const noLive = await none.begin();
  equal(r, "stitch: closed and expired sessions only, no live one", (noLive === null ? "none" : noLive.session_id) + " " + state(fresh, none), "none more");

  // The newest session is closed and an older one still active: no session is
  // live, both load as history oldest at the top, and the next message opens
  // its session below them, so the timeline stays in creation order.
  const superseded = new ChatStore(new Runtime());
  const supersededApi = new RecordedHistory(STITCH_SUPERSEDED_SCENARIO);
  const second = new Stitcher(superseded, supersededApi, AGENT_ID, GONE, (reason: string): void => {
    failures.push(reason);
  });
  const behind = await second.begin();
  equal(r, "stitch: an active session older than a closed one is not live", (behind === null ? "none" : behind.session_id) + " " + String(second.queue.length), "none 2");
  await second.older();
  await second.older();
  superseded.say("Нова розмова");
  equal(r, "stitch: segments in creation order around the new live one", order(superseded), "history 2026-01-03, history 2026-01-04, live");
  equal(r, "stitch: the older active session loads as history", supersededApi.calls.join("; "), "list agent-head 20; messages session-3; messages session-1");
}

/** Each segment's state and day, top to bottom. */
function order(store: ChatStore): string {
  return store.timeline.segments.map((s: Segment): string => (s.date === "" ? s.state : s.state + " " + s.date)).join(", ");
}
