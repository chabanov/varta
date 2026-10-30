// Reactive access to the chat model for the view. The model and its handlers stay
// plain and deterministic; every change goes through `change`, which runs the
// handler and then bumps the version of what it touched in one batch: the
// segment's entry list when entries were added, and the version of the entry
// that changed. Views read versions inside holes and effects, so only what
// changed re-renders.
import { Runtime, signal, batch, untracked, type Accessor, type Signal } from "../l1/reactive.js";
import { timeline, openSegment, entryKey, ask, decide, answer, type Timeline, type Segment, type Entry } from "./timeline.js";

const TURN_PREFIX = "turn:";
const ASK_PREFIX = "ask:";
const LOCAL_PREFIX = "local-";

/** What the person asked for, in order; the request layer performs them. */
export type Intent =
  | { kind: "send"; sessionId: string; localId: string; text: string }
  | { kind: "decide"; runId: string; approved: boolean }
  | { kind: "answer"; runId: string; text: string };

export class ChatStore {
  rt: Runtime;
  timeline: Timeline;
  segments: Signal<Segment[]>;
  lists: { [sessionId: string]: Signal<Entry[]> } = {};
  versions: { [key: string]: Signal<number> } = {};
  /** Monotonic source of version values. */
  clock: number = 0;
  intents: Signal<Intent[]>;
  nextLocal: number = 1;
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

  // ------------------------------------------------------- the person's actions

  /** The person sends a message in the live segment. */
  send(s: Segment, text: string): void {
    const localId = LOCAL_PREFIX + String(this.nextLocal);
    this.nextLocal += 1;
    this.change(s, null, () => ask(s, localId, text));
    this.intend({ kind: "send", sessionId: s.sessionId, localId: localId, text: text });
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

  /** The segment of a session, if it is open. */
  segment(sessionId: string): Segment | null {
    for (const s of this.timeline.segments) {
      if (s.sessionId === sessionId) return s;
    }
    return null;
  }

  /** The segment holding a run's turn, if any. */
  segmentOfRun(runId: string): Segment | null {
    for (const s of this.timeline.segments) {
      for (const e of s.entries) {
        if (e.kind === "turn" && e.runId === runId) return s;
      }
    }
    return null;
  }

  open(sessionId: string): Segment {
    const s = openSegment(this.timeline, sessionId);
    this.lists[sessionId] = signal<Entry[]>(this.rt, []);
    this.segments.set(this.timeline.segments.slice(0));
    return s;
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
    const existing = this.lists[s.sessionId];
    if (existing !== undefined) return existing;
    const created = signal<Entry[]>(this.rt, s.entries.slice(0));
    this.lists[s.sessionId] = created;
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
