// Reactive access to the chat model for the view. The model and its handlers stay
// plain and deterministic; every change goes through `change`, which runs the
// handler and then bumps the version of what it touched in one batch: the
// segment's entry list when entries were added, and the version of the entry
// that changed. Views read versions inside holes and effects, so only what
// changed re-renders.
//
// Session stitching (design/chat-timeline.md): older sessions are prepended as
// history segments, and a message sent while no segment is live opens a local
// segment at once and queues a `start` intent, which creates the session
// (`sessions.create`) before sending; `bind` then gives the segment the
// session's real id. A segment keeps the key it was opened under for its whole
// life, so its entry list and its version survive the binding.
import { Runtime, signal, batch, untracked, type Accessor, type Signal } from "../l1/reactive.js";
import { timeline, openSegment, prependSegment, entryKey, ask, askFailed, addNote, decide, answer, type Timeline, type Segment, type Entry } from "./timeline.js";

const TURN_PREFIX = "turn:";
const ASK_PREFIX = "ask:";
const SEGMENT_PREFIX = "segment:";
const LOCAL_PREFIX = "local-";
/** The id a segment carries until its session exists. */
export const LOCAL_SESSION_PREFIX = "local-session-";

/** What the person asked for, in order; the request layer performs them. */
export type Intent =
  /** Create the session of the local segment `sessionId`, then send its first message. */
  | { kind: "start"; sessionId: string; localId: string; text: string }
  | { kind: "send"; sessionId: string; localId: string; text: string }
  | { kind: "decide"; runId: string; approved: boolean }
  | { kind: "answer"; runId: string; text: string };

export class ChatStore {
  rt: Runtime;
  timeline: Timeline;
  segments: Signal<Segment[]>;
  /** Entry lists by segment key (the id the segment was opened under). */
  lists: { [key: string]: Signal<Entry[]> } = {};
  versions: { [key: string]: Signal<number> } = {};
  /** The key of a segment whose session id changed when it was bound, by its current id. */
  keys: { [sessionId: string]: string } = {};
  /** The real session id of a bound local segment, by its local id. */
  aliases: { [localId: string]: string } = {};
  /** Monotonic source of version values. */
  clock: number = 0;
  intents: Signal<Intent[]>;
  nextLocal: number = 1;
  nextSession: number = 1;
  /**
   * Called after every change. The view sets it to commit the turn's view
   * operations: changes from timers and request completions are not inside a
   * view event turn, which is the only kind the view commits by itself.
   */
  afterChange: () => void = (): void => {};

  constructor(rt: Runtime) {
    this.rt = rt;
    this.timeline = timeline();
    this.segments = signal<Segment[]>(rt, []);
    this.intents = signal<Intent[]>(rt, []);
  }

  /** The segment new messages go to: the last one, when it is live. */
  live(): Segment | null {
    const segments = this.segments.get();
    const last = segments[segments.length - 1];
    return last !== undefined && last.state === "live" ? last : null;
  }

  /** Whether any segment shows an entry; reading it subscribes to the segments and their lists. */
  hasEntries(): boolean {
    let any = false;
    for (const s of this.segments.get()) {
      const list = this.list(s);
      if (list.get().length > 0) any = true;
    }
    return any;
  }

  // ------------------------------------------------------- the person's actions

  /**
   * The person sends a message: to the live segment, or, with none, to a new
   * local segment whose session the request layer creates first.
   */
  say(text: string): void {
    const s = untracked<Segment | null>(this.rt, () => this.live());
    if (s !== null) {
      this.send(s, text);
      return;
    }
    const local = this.open(LOCAL_SESSION_PREFIX + String(this.nextSession));
    this.nextSession += 1;
    const localId = this.nextLocalId();
    this.change(local, null, () => ask(local, localId, text));
    this.intend({ kind: "start", sessionId: local.sessionId, localId: localId, text: text });
  }

  /** The session of a local segment exists: the segment takes its id and day. */
  bind(s: Segment, sessionId: string, date: string): void {
    const key = this.keyOf(s);
    this.aliases[s.sessionId] = sessionId;
    this.keys[sessionId] = key;
    s.sessionId = sessionId;
    s.date = date;
    this.bump(SEGMENT_PREFIX + key);
    this.afterChange();
  }

  /** Creating a local segment's session failed: its messages are not sent, and the next message opens a new segment. */
  abandon(s: Segment, reason: string): void {
    this.change(s, null, () => {
      for (const e of s.entries) {
        if (e.kind === "ask" && e.state === "sending") askFailed(s, e.id);
      }
      addNote(s, reason);
      s.state = "closed";
    });
    for (const e of s.entries) {
      if (e.kind === "ask") this.bump(ASK_PREFIX + e.key);
    }
    this.afterChange();
  }

  /** The person sends a message in the live segment. */
  send(s: Segment, text: string): void {
    const localId = this.nextLocalId();
    this.change(s, null, () => ask(s, localId, text));
    this.intend({ kind: "send", sessionId: s.sessionId, localId: localId, text: text });
  }

  private nextLocalId(): string {
    const localId = LOCAL_PREFIX + String(this.nextLocal);
    this.nextLocal += 1;
    return localId;
  }

  decide(s: Segment, runId: string, approved: boolean): void {
    let changed = false;
    this.change(s, runId, () => {
      changed = decide(s, runId);
    });
    if (changed) this.intend({ kind: "decide", runId: runId, approved: approved });
  }

  answer(s: Segment, runId: string, text: string): void {
    let changed = false;
    this.change(s, runId, () => {
      changed = answer(s, runId);
    });
    if (changed) this.intend({ kind: "answer", runId: runId, text: text });
  }

  intend(intent: Intent): void {
    const next = untracked<Intent[]>(this.rt, this.intents.get).slice(0);
    next.push(intent);
    this.intents.set(next);
  }

  /** The segment of a session, if it is open; a bound local segment is found by its local id too. */
  segment(sessionId: string): Segment | null {
    const alias = this.aliases[sessionId];
    const id = alias === undefined ? sessionId : alias;
    for (const s of this.timeline.segments) {
      if (s.sessionId === id) return s;
    }
    return null;
  }

  /** The newest segment that is not history holding a run's turn, if any: history turns take no action. */
  segmentOfRun(runId: string): Segment | null {
    const segments = this.timeline.segments;
    for (let i = segments.length - 1; i >= 0; i--) {
      const s = segments[i]!;
      if (s.state === "history") continue;
      for (const e of s.entries) {
        if (e.kind === "turn" && e.runId === runId) return s;
      }
    }
    return null;
  }

  open(sessionId: string): Segment {
    return this.openAt(sessionId, "");
  }

  /** Opens a live segment for a session created on `date` (its `YYYY-MM-DD`, or empty). */
  openAt(sessionId: string, date: string): Segment {
    const s = openSegment(this.timeline, sessionId);
    s.date = date;
    this.lists[sessionId] = signal<Entry[]>(this.rt, []);
    this.segments.set(this.timeline.segments.slice(0));
    return s;
  }

  /** Prepends an older session as a history segment; `fill` adds its entries before the view sees it. */
  prepend(sessionId: string, date: string, fill: (s: Segment) => void): Segment {
    const s = prependSegment(this.timeline, sessionId, date);
    fill(s);
    this.lists[sessionId] = signal<Entry[]>(this.rt, s.entries.slice(0));
    this.segments.set(this.timeline.segments.slice(0));
    this.afterChange();
    return s;
  }

  /** The version of a segment's own fields (its date); reading it subscribes. */
  segmentVersion(s: Segment): Accessor<number> {
    return this.version(SEGMENT_PREFIX + this.keyOf(s));
  }

  /** The key a segment was opened under. */
  keyOf(s: Segment): string {
    const key = this.keys[s.sessionId];
    return key === undefined ? s.sessionId : key;
  }

  entries(s: Segment): Accessor<Entry[]> {
    const list = this.list(s);
    return list.get;
  }

  /** The version of one entry, by its key; reading it subscribes. */
  version(key: string): Accessor<number> {
    const v = this.versionSignal(key);
    return v.get;
  }

  /** Runs a change to the segment; `runId` names the turn it touched, if any. */
  change(s: Segment, runId: string | null, body: () => void): void {
    batch(this.rt, () => {
      const before = s.entries.length;
      body();
      if (s.entries.length !== before) {
        const list = this.list(s);
        list.set(s.entries.slice(0));
      }
      if (runId !== null) this.bump(TURN_PREFIX + runId);
    });
    this.afterChange();
  }

  /** Bumps the person's message with this local key (sent or failed). */
  changeAsk(s: Segment, key: string, body: () => void): void {
    batch(this.rt, () => {
      body();
      this.bump(ASK_PREFIX + key);
    });
    this.afterChange();
  }

  /** Bumps an entry by its key (state changed from the view, such as a decision). */
  touch(e: Entry): void {
    this.bump(entryKey(e));
  }

  private list(s: Segment): Signal<Entry[]> {
    const key = this.keyOf(s);
    const existing = this.lists[key];
    if (existing !== undefined) return existing;
    const created = signal<Entry[]>(this.rt, s.entries.slice(0));
    this.lists[key] = created;
    return created;
  }

  private versionSignal(key: string): Signal<number> {
    const existing = this.versions[key];
    if (existing !== undefined) return existing;
    const created = signal<number>(this.rt, 0);
    this.versions[key] = created;
    return created;
  }

  private bump(key: string): void {
    this.clock += 1;
    const v = this.versionSignal(key);
    v.set(this.clock);
  }
}
