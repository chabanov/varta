// Session stitching (design/chat-timeline.md, "Session stitching"): the Head
// Agent's sessions are one conversation. At start the newest page of its
// sessions (`sessions.list`, newest first) gives the live session, the newest
// one that is neither closed nor expired; every other session of the page,
// and of each older page behind its cursor, loads as a history segment from
// its transcript when the person scrolls up, one session per request, newest
// first, each prepended before the segments already shown. A closed session's
// transcript is gone (404): its segment shows a note instead of entries.
import { NotFoundError, type GetSessionMessagesResponse, type ListSessionsParams, type ListSessionsResponse, type ListSessionsResponseItem } from "uarp-sdk";
import { signal, untracked, type Signal } from "../l1/reactive.js";
import { loadTranscript } from "./history.js";
import { addNote, dayOf } from "./timeline.js";
import { describeFailure } from "./failure.js";
import type { ChatStore } from "./store.js";

/** The part of the client's `sessions` stitching reads. */
export interface HistoryApi {
  list(params: ListSessionsParams): Promise<ListSessionsResponse>;
  getSessionMessages(sessionId: string): Promise<GetSessionMessagesResponse>;
}

/** Sessions per page of the list (the API clamps `limit` to 1–100). */
export const SESSION_PAGE_SIZE = 20;
/** The status of a session that is neither closed nor expired. */
const ACTIVE = "active";

/** Whether older history can be loaded: `more` until every page and session is loaded. */
export type HistoryState = "none" | "more" | "loading";

export class Stitcher {
  store: ChatStore;
  api: HistoryApi;
  agentId: string;
  /** Shown instead of the entries of a session whose transcript is gone. */
  goneNote: string;
  /** Reports a failed load; the session stays queued for the next attempt. */
  failed: (reason: string) => void;
  history: Signal<HistoryState>;
  /** Sessions not yet loaded as history, newest first; `next` is the first of them. */
  queue: ListSessionsResponseItem[] = [];
  next: number = 0;
  /** The cursor of the next page; null after the last one. */
  cursor: string | null = null;

  constructor(store: ChatStore, api: HistoryApi, agentId: string, goneNote: string, failed: (reason: string) => void) {
    this.store = store;
    this.api = api;
    this.agentId = agentId;
    this.goneNote = goneNote;
    this.failed = failed;
    this.history = signal<HistoryState>(store.rt, "none");
  }

  /** Reads the newest page; returns the live session, if any. Rejects with the request's error. */
  async begin(): Promise<ListSessionsResponseItem | null> {
    const page = await this.api.list({ agent_id: this.agentId, limit: SESSION_PAGE_SIZE });
    let live: ListSessionsResponseItem | null = null;
    for (const item of page.items) {
      if (live === null && item.status === ACTIVE) live = item;
      else this.queue.push(item);
    }
    this.take(page);
    this.history.set(this.remaining() ? "more" : "none");
    return live;
  }

  /** Whether a session or a page is still to load. */
  remaining(): boolean {
    return this.next < this.queue.length || this.cursor !== null;
  }

  /**
   * Loads the next older session as a history segment, reading the next page
   * first when the queue is empty. Does nothing while a load runs or when
   * nothing remains; a failure is reported and can be retried.
   */
  async older(): Promise<void> {
    if (untracked<HistoryState>(this.store.rt, this.history.get) !== "more") return;
    this.history.set("loading");
    try {
      const cursor = this.cursor;
      if (this.next >= this.queue.length && cursor !== null) {
        const page = await this.api.list({ agent_id: this.agentId, limit: SESSION_PAGE_SIZE, cursor: cursor });
        for (const item of page.items) this.queue.push(item);
        this.take(page);
      }
      if (this.next < this.queue.length) await this.load(this.queue[this.next]!);
    } catch (error) {
      this.failed(describeFailure(error));
    }
    this.history.set(this.remaining() ? "more" : "none");
  }

  /** One session as a history segment; it leaves the queue only once shown. */
  async load(item: ListSessionsResponseItem): Promise<void> {
    let entries: GetSessionMessagesResponse | null = null;
    try {
      entries = await this.api.getSessionMessages(item.session_id);
    } catch (error) {
      if (!(error instanceof NotFoundError)) throw error;
    }
    const transcript = entries;
    const note = this.goneNote;
    this.store.prepend(item.session_id, dayOf(item.created_at), (s) => {
      if (transcript === null) addNote(s, note);
      else loadTranscript(s, transcript.items);
    });
    this.next += 1;
  }

  private take(page: ListSessionsResponse): void {
    const cursor = page.cursor;
    this.cursor = page.has_more && cursor !== null ? cursor : null;
  }
}
