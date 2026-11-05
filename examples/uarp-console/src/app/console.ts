// The console's start: the API key from the KV database, the real UarpClient,
// a new session with the tenant's Head Agent, the request layer performing the
// person's intents as the store queues them, and the session's live stream
// followed into the timeline. Every failure ends in a state or a note the
// screen shows; nothing here retries a request the SDK did not retry.
import type { UarpClient } from "uarp-sdk";
import { effect, signal, type Signal } from "../l1/reactive.js";
import type { ChatStore } from "../chat/store.js";
import { Requests } from "../chat/requests.js";
import { SessionFollower, type FollowEnd, type FollowOptions } from "../chat/follow.js";
import { describeFailure } from "../chat/failure.js";
import { addNote, type Segment } from "../chat/timeline.js";
import { NO_AGENT, NO_KEY, STARTING, STREAM_AUTHENTICATE, STREAM_FAILED, STREAM_GAVE_UP, STREAM_INVALID, STREAM_REFUSED, UNAVAILABLE } from "../ui/text.js";
import { ClientApi, createClient, type FetchFunction } from "./api.js";
import { readApiKey } from "./key.js";

export interface ConsoleOptions {
  /** The API base URL; `net.request` must grant it. */
  baseURL: string;
  /** The KV database with the key; `kv.read` must grant it. */
  keyDatabase: string;
  /** The SDK's fetch hook; null uses the engine's fetch (the product). */
  fetcher: FetchFunction | null;
  follow: FollowOptions;
}

/** Where the start stopped, or that a session is followed. */
export type StartState =
  | { kind: "starting" }
  | { kind: "no_key" }
  | { kind: "no_agent" }
  /** A start request failed: denied, unreachable or answered with an error. */
  | { kind: "unavailable"; reason: string }
  | { kind: "following"; sessionId: string };

/** A followed session: its segment, the request layer and the follower. */
export interface ConsoleSession {
  segment: Segment;
  requests: Requests;
  follower: SessionFollower;
  /** Resolves when following ends (never rejects). */
  ended: Promise<FollowEnd>;
}

/** The line the screen shows for a start state; empty while following. */
export function startText(state: StartState): string {
  switch (state.kind) {
    case "starting":
      return STARTING;
    case "no_key":
      return NO_KEY;
    case "no_agent":
      return NO_AGENT;
    case "unavailable":
      return UNAVAILABLE + state.reason;
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

export class ConsoleApp {
  store: ChatStore;
  options: ConsoleOptions;
  state: Signal<StartState>;
  session: ConsoleSession | null = null;

  constructor(store: ChatStore, options: ConsoleOptions) {
    this.store = store;
    this.options = options;
    this.state = signal<StartState>(store.rt, { kind: "starting" });
  }

  /** Starts the console; resolves with the state it reached (never rejects). */
  async start(): Promise<StartState> {
    const state = await this.open();
    this.state.set(state);
    return state;
  }

  async open(): Promise<StartState> {
    let key: string | null = null;
    try {
      const kv = await varta.kv.open(this.options.keyDatabase);
      try {
        key = await readApiKey(kv);
      } finally {
        kv.close();
      }
    } catch (error) {
      return { kind: "unavailable", reason: describeFailure(error) };
    }
    if (key === null) return { kind: "no_key" };
    const client = createClient(this.options.baseURL, key, this.options.fetcher);
    let sessionId = "";
    try {
      const agentId = await headAgent(client);
      if (agentId === null) return { kind: "no_agent" };
      const session = await client.sessions.create({ agent_id: agentId });
      sessionId = session.session_id;
    } catch (error) {
      return { kind: "unavailable", reason: describeFailure(error) };
    }
    this.session = this.follow(new ClientApi(client), sessionId);
    return { kind: "following", sessionId: sessionId };
  }

  follow(api: ClientApi, sessionId: string): ConsoleSession {
    const store = this.store;
    const segment = store.open(sessionId);
    const requests = new Requests(store, api);
    // Every intent the store queues starts its request; reading the queue
    // subscribes, and drain() itself reads it untracked.
    effect(store.rt, () => {
      store.intents.get();
      requests.drain();
    });
    const follower = new SessionFollower(store, segment, api, this.options.follow);
    const ended = follower.follow().then((end: FollowEnd): FollowEnd => {
      const text = endText(end);
      if (text !== "") store.change(segment, null, () => addNote(segment, text));
      return end;
    });
    return { segment: segment, requests: requests, follower: follower, ended: ended };
  }
}

/** The tenant's Head Agent, the one agent the Single Chat talks to. */
async function headAgent(client: UarpClient): Promise<string | null> {
  const tenant = await client.tenants.getCurrentTenant();
  const id = tenant.head_agent_id;
  return id === undefined ? null : id;
}
