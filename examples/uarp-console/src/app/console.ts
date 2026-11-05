// The console's start: the settings (API key and base URL) from their store,
// the API for them, the tenant's Head Agent, the newest page of its sessions
// (chat/stitch.ts) and, when one of them is live, its transcript and its live
// stream followed into the timeline. Without a live session the console is
// ready: the first message creates one (`sessions.create`) and follows it.
// Older sessions load as history when the person scrolls up. Every failure
// ends in a state or a note the screen shows; nothing here retries a request
// the SDK did not retry.
//
// The settings screen saves through `saveSettings`. The key goes to the
// settings store and to the client constructed for it, nowhere else. The key is
// a bearer credential, so a base URL the person enters must use https: a
// plaintext one is refused when saved and, when the store holds one anyway, at
// start before any client is constructed. The default base URL is the
// product's configuration (config.ts), not the person's input. A start
// that did not get as far as the session list can run again with the new
// settings; after that, new settings apply at the next start of the console,
// because the timeline, its view and the request layer already belong to the
// running session.
import { effect, signal, type Signal } from "../l1/reactive.js";
import type { ChatStore } from "../chat/store.js";
import { Requests, type SessionStarter } from "../chat/requests.js";
import { SessionFollower, type FollowEnd, type FollowOptions } from "../chat/follow.js";
import { describeFailure } from "../chat/failure.js";
import { loadTranscript } from "../chat/history.js";
import { Stitcher, type HistoryState } from "../chat/stitch.js";
import { addNote, dayOf, type Segment } from "../chat/timeline.js";
import type { Toasts } from "../ui/notices.js";
import {
  FORGET_FAILED,
  FORGOTTEN,
  NO_AGENT,
  NO_KEY,
  OLDER_FAILED,
  STORED_URL_INVALID,
  SAVE_FAILED,
  SAVED,
  SAVED_NEXT_START,
  STARTING,
  STREAM_AUTHENTICATE,
  STREAM_FAILED,
  STREAM_GAVE_UP,
  STREAM_INVALID,
  STREAM_REFUSED,
  TRANSCRIPT_GONE,
  UNAVAILABLE,
} from "../ui/text.js";
import type { ConsoleApi } from "./api.js";
import type { SettingsStore } from "./key.js";

export interface ConsoleOptions {
  /** The base URL when the settings hold none; `net.request` must grant the one used. */
  defaultBaseURL: string;
  /**
   * The one plaintext base URL a local build may use besides https: the
   * loopback API proxy (`varta.local.json`); null in the product.
   */
  localBaseURL: string | null;
  /** Where the key and base URL live (the product: the KV database). */
  settings: SettingsStore;
  /** The API for a base URL and key (the product: the real UarpClient). */
  connect: (baseURL: string, apiKey: string) => ConsoleApi;
  follow: FollowOptions;
  /** Notices for the person (a failed history load, saved settings). */
  toasts: Toasts;
}

/** Where the start stopped, or what the console does now. */
export type StartState =
  | { kind: "starting" }
  | { kind: "no_key" }
  | { kind: "no_agent" }
  /** The stored base URL does not use https; nothing was requested. */
  | { kind: "url_invalid" }
  /** A start request failed: denied, unreachable or answered with an error. */
  | { kind: "unavailable"; reason: string }
  /** No live session: the next message creates one. */
  | { kind: "ready" }
  | { kind: "following"; sessionId: string };

/** A followed session: its segment and the follower. */
export interface ConsoleSession {
  segment: Segment;
  follower: SessionFollower;
  /** Resolves when following ends (never rejects). */
  ended: Promise<FollowEnd>;
}

/** Why settings were not saved, or what saving them did. */
export type SaveOutcome =
  | { kind: "key_missing" }
  | { kind: "url_invalid" }
  | { kind: "failed"; reason: string }
  /** Saved and the console started again with them. */
  | { kind: "restarted"; state: StartState }
  /** Saved; they apply at the next start. */
  | { kind: "saved" };

/**
 * The one scheme a base URL may use: the key goes out as a bearer credential,
 * so it is never offered to a plaintext origin. The manifest's `net.request`
 * grant still decides which origin is reachable.
 */
const URL_SCHEME = "https:";

/** Whether a base URL parses and uses https, or is exactly the local build's loopback proxy. */
export function secureBaseURL(baseURL: string, local: string | null): boolean {
  const trimmed = baseURL.trim();
  if (local !== null && trimmed === local) return true;
  try {
    return new URL(trimmed).protocol === URL_SCHEME;
  } catch (error) {
    return false;
  }
}

/** Whether settings can be saved: a key, and a base URL that `secureBaseURL` admits. */
export function checkSettings(baseURL: string, apiKey: string, local: string | null): SaveOutcome | null {
  if (apiKey.trim() === "") return { kind: "key_missing" };
  if (!secureBaseURL(baseURL, local)) return { kind: "url_invalid" };
  return null;
}

/** The line the screen shows for a start state; empty while ready or following. */
export function startText(state: StartState): string {
  switch (state.kind) {
    case "starting":
      return STARTING;
    case "no_key":
      return NO_KEY;
    case "no_agent":
      return NO_AGENT;
    case "url_invalid":
      return STORED_URL_INVALID;
    case "unavailable":
      return UNAVAILABLE + state.reason;
    case "ready":
    case "following":
      return "";
  }
}

/** The note a follow end leaves in the timeline; empty when the timeline already shows it. */
export function endText(end: FollowEnd): string {
  switch (end.kind) {
    case "over":
    case "stopped":
      return "";
    case "failed":
      return STREAM_FAILED + end.code;
    case "authenticate":
      return STREAM_AUTHENTICATE;
    case "gave_up":
      return STREAM_GAVE_UP + end.reason;
    case "refused":
      return STREAM_REFUSED + end.reason;
    case "invalid":
      return STREAM_INVALID + end.reason;
  }
}

/** Whether a state lets the console start again: nothing of a session exists yet. */
function restartable(state: StartState): boolean {
  return state.kind === "no_key" || state.kind === "no_agent" || state.kind === "url_invalid" || state.kind === "unavailable";
}

/** What the start reached before the console wires itself to the API. */
interface Opened {
  state: StartState;
  api: ConsoleApi | null;
  stitcher: Stitcher | null;
  agentId: string;
}

export class ConsoleApp {
  store: ChatStore;
  options: ConsoleOptions;
  state: Signal<StartState>;
  /** The base URL in use or offered by the settings screen; never the key. */
  baseURL: Signal<string>;
  /** The tenant's name once read; empty before. */
  tenantName: Signal<string>;
  /** Whether older sessions can be loaded (stitch.ts); `none` until the start read the list. */
  history: Signal<HistoryState>;
  /** The live session followed now, if any. */
  session: ConsoleSession | null = null;
  requests: Requests | null = null;
  stitcher: Stitcher | null = null;

  constructor(store: ChatStore, options: ConsoleOptions) {
    this.store = store;
    this.options = options;
    this.state = signal<StartState>(store.rt, { kind: "starting" });
    this.baseURL = signal<string>(store.rt, options.defaultBaseURL);
    this.tenantName = signal<string>(store.rt, "");
    this.history = signal<HistoryState>(store.rt, "none");
  }

  /** Starts the console; resolves with the state it reached (never rejects). */
  async start(): Promise<StartState> {
    this.state.set({ kind: "starting" });
    const opened = await this.open();
    const api = opened.api;
    const stitcher = opened.stitcher;
    if (api !== null && stitcher !== null) this.wire(api, stitcher, opened.agentId);
    this.state.set(opened.state);
    this.store.afterChange();
    return opened.state;
  }

  async open(): Promise<Opened> {
    const none = (state: StartState): Opened => ({ state: state, api: null, stitcher: null, agentId: "" });
    let apiKey: string | null = null;
    let stored: string | null = null;
    try {
      const settings = await this.options.settings.read();
      apiKey = settings.apiKey;
      stored = settings.baseURL;
      if (stored !== null) this.baseURL.set(stored);
    } catch (error) {
      return none({ kind: "unavailable", reason: describeFailure(error) });
    }
    if (apiKey === null) return none({ kind: "no_key" });
    if (stored !== null && !secureBaseURL(stored, this.options.localBaseURL)) return none({ kind: "url_invalid" });
    const api = this.options.connect(this.baseURL.get(), apiKey);
    try {
      const tenant = await api.tenants.getCurrentTenant();
      this.tenantName.set(tenant.name);
      const agentId = tenant.head_agent_id;
      if (agentId === undefined) return none({ kind: "no_agent" });
      const toasts = this.options.toasts;
      const stitcher = new Stitcher(this.store, api.sessions, agentId, TRANSCRIPT_GONE, (reason: string): void => toasts.show(OLDER_FAILED + reason));
      const live = await stitcher.begin();
      if (live === null) return { state: { kind: "ready" }, api: api, stitcher: stitcher, agentId: agentId };
      // The live session's transcript alone has the person's messages; the
      // stream's replay of the runs it holds is dropped (history.ts).
      const transcript = await api.sessions.getSessionMessages(live.session_id);
      const segment = this.store.openAt(live.session_id, dayOf(live.created_at));
      this.store.change(segment, null, () => loadTranscript(segment, transcript.items));
      this.session = this.follow(api, segment);
      return { state: { kind: "following", sessionId: live.session_id }, api: api, stitcher: stitcher, agentId: agentId };
    } catch (error) {
      return none({ kind: "unavailable", reason: describeFailure(error) });
    }
  }

  /** The request layer and history loading, once per console: a start that reached the session list. */
  wire(api: ConsoleApi, stitcher: Stitcher, agentId: string): void {
    const store = this.store;
    this.stitcher = stitcher;
    effect(store.rt, () => {
      this.history.set(stitcher.history.get());
    });
    const starter: SessionStarter = {
      create: () => api.sessions.create({ agent_id: agentId }),
      started: (s: Segment): void => {
        this.session = this.follow(api, s);
        this.state.set({ kind: "following", sessionId: s.sessionId });
      },
    };
    const requests = new Requests(store, api, starter);
    this.requests = requests;
    // Every intent the store queues starts its request; reading the queue
    // subscribes, and drain() itself reads it untracked.
    effect(store.rt, () => {
      store.intents.get();
      requests.drain();
    });
  }

  follow(api: ConsoleApi, segment: Segment): ConsoleSession {
    const store = this.store;
    const follower = new SessionFollower(store, segment, api, this.options.follow);
    const ended = follower.follow().then((end: FollowEnd): FollowEnd => {
      const text = endText(end);
      if (text !== "") store.change(segment, null, () => addNote(segment, text));
      return end;
    });
    return { segment: segment, follower: follower, ended: ended };
  }

  /** Loads the next older session as history (the person scrolled up); does nothing while one loads or when none remain. */
  older(): Promise<void> {
    const stitcher = this.stitcher;
    if (stitcher === null) return Promise.resolve();
    return stitcher.older();
  }

  /**
   * Saves the settings the person entered. The key goes to the settings store
   * only; a console that never got as far as the session list starts again
   * with them.
   */
  /** Whether the settings the person entered can be saved by this build. */
  check(baseURL: string, apiKey: string): SaveOutcome | null {
    return checkSettings(baseURL, apiKey, this.options.localBaseURL);
  }

  async saveSettings(baseURL: string, apiKey: string): Promise<SaveOutcome> {
    const problem = this.check(baseURL, apiKey);
    if (problem !== null) return problem;
    const url = baseURL.trim();
    try {
      await this.options.settings.write(url, apiKey.trim());
    } catch (error) {
      const reason = describeFailure(error);
      this.options.toasts.show(SAVE_FAILED + reason);
      return { kind: "failed", reason: reason };
    }
    this.baseURL.set(url);
    if (!restartable(this.state.get())) {
      this.options.toasts.show(SAVED_NEXT_START);
      return { kind: "saved" };
    }
    this.options.toasts.show(SAVED);
    const state = await this.start();
    return { kind: "restarted", state: state };
  }

  /** Removes the stored key; a running session keeps its client until the next start. */
  async forgetKey(): Promise<boolean> {
    try {
      await this.options.settings.forget();
    } catch (error) {
      this.options.toasts.show(FORGET_FAILED + describeFailure(error));
      return false;
    }
    this.options.toasts.show(FORGOTTEN);
    if (restartable(this.state.get())) this.state.set({ kind: "no_key" });
    return true;
  }
}
