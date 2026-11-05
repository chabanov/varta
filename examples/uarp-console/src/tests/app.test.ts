// The console end to end, executed by Varta: the product's own start
// (src/app/console.ts) with the real, unmodified UarpClient against the loopback
// mock (src/tests/sdk-server/e2e.ts), which replays the API lane's recorded
// scenarios. The person's actions go through the store as the screen sends
// them; the timeline the stream builds is compared with the reviewed
// expectation of the same recording (scenarios.test.ts), plus the person's
// message. `scripts/test.sh` starts the mock, writes the exact `net.request`
// grant for its port and runs these groups as application instances.
import type { SendSessionMessageRequest } from "uarp-sdk";
import { Runtime } from "../l1/reactive.js";
import { ChatStore } from "../chat/store.js";
import { show } from "../chat/show.js";
import type { FollowEnd, FollowOptions } from "../chat/follow.js";
import type { Turn } from "../chat/timeline.js";
import { ConsoleApp, endText, type ConsoleSession, type StartState } from "../app/console.js";
import { API_BASE_URL, KEY_DATABASE } from "../app/config.js";
import { API_KEY_ENTRY, writeApiKey } from "../app/key.js";
import { recordingText, type Recording } from "./sdk-server/recordings.js";
import { GatedFetch } from "./loopback.js";
import { EXPECTED } from "./scenarios.test.js";
import {
  ANSWER,
  API_KEY,
  BUSY_SCENARIO,
  CONNECTED_ONLY_SCENARIO,
  DROP_AFTER,
  E2E_PREFIX,
  EMPTY_SCENARIO,
  EXPIRED_SCENARIO,
  FORBIDDEN_DETAIL,
  FORBIDDEN_SCENARIO,
  INVALID_SCENARIO,
  LONG_CHUNKS,
  LONG_FIRST_CHUNK_SEQ,
  LONG_SCENARIO,
  MISSING_SCENARIO,
  NO_HEAD_SCENARIO,
  REOPEN_EMPTY_SCENARIO,
  RUN_ID,
  SESSION_ID,
  UNAUTHORIZED_SCENARIO,
} from "./sdk/contract.js";
import { equal, type Report } from "./check.js";
import ceilingText from "./app-ceiling.json" with { type: "text" };

/** Short waits, so a reconnect takes milliseconds; the product uses FOLLOW_DEFAULTS. */
const E2E_FOLLOW: FollowOptions = { retryMs: 20, maxRetryMs: 100, maxReconnects: 3 };
/**
 * The tests' own key database (granted by varta.test.json), never the
 * product's KEY_DATABASE, so a run never touches a stored key.
 */
const TEST_DATABASE = "/data/e2e.kv";
/** A database the test configuration grants, holding no key. */
const EMPTY_DATABASE = "/data/e2e-empty.kv";
/** A database the test configuration grants whose key entry holds a number, not a string. */
const MALFORMED_DATABASE = "/data/e2e-malformed.kv";
/** The number stored as MALFORMED_DATABASE's key. */
const MALFORMED_KEY = 1;
/** The engine's open-database limit (varta-common `kv::MAX_OPEN_DATABASES`). */
const KV_OPEN_LIMIT = 8;
/** A stream that fails to open makes two requests: the open and the SDK's one reconnect. */
const FAILED_OPEN_REQUESTS = 2;
const TENANT_PATH = "/api/v1/tenants/me";
const EVENTS_PATH = "/api/v1/sessions/" + SESSION_ID + "/events";
const EVENTS_QUERY = "?event_schema=1";
/** How follow.ts reports a connection that ended without `[DONE]`. */
const DROPPED_REASON = "the stream ended without [DONE]";
/** The 503 of BUSY_SCENARIO (UNAVAILABLE_PROBLEM) as the SDK reports it. */
const BUSY_REASON = "ServiceUnavailableError: 503 Service Unavailable — scheduler busy";
/** Strict parsing refuses INVALID_SCENARIO's run.started, which lacks its `seq`. */
const INVALID_REASON = "TypeError: type assertion to RunEventV1 failed";
/** How follow.ts reports a stream that reopened at the SDK's reconnect and ended silently without a frame. */
const SILENT_REASON = "the stream reopened and ended without a frame";
/** A database under the data root that no grant names. */
const UNGRANTED_DATABASE = "/data/e2e-ungranted.kv";
/** Loopback, but not the mock's origin: `net.request` grants only the bound port. */
const UNGRANTED_ORIGIN = "http://127.0.0.1:9";
/** The built product manifest (`varta build --config varta.json`, run by scripts/test.sh). */
const APP_MANIFEST = "/project/dist/app.manifest.json";
/** The headers the SDK reads from a response (Retry-After, x-should-retry, request and correlation ids). */
const RESPONSE_HEADERS: string[] = ["retry-after", "x-correlation-id", "x-request-id", "x-should-retry"];
/** The host ceiling budgets of the app groups (app-ceiling.json), which scripts/test.sh puts in their policy. */
interface AppCeiling {
  deadlinePerTurnMs: number;
}
const SEND_STEP = "send_message";
const SEGMENT_PREFIXES: string[] = ["(segment live ", "(segment closed "];

// -------------------------------------------------------------- the manifest

interface ManifestGrant {
  id: string;
  patterns?: string[];
  methods?: string[];
  responseHeaders?: string[];
  enabled?: boolean;
}

interface AppManifest {
  schemaVersion: number;
  abiVersion: number;
  entry: string;
  target: string;
  policy: string;
  capabilities: ManifestGrant[];
  fsRoots: string[];
}

function list(values: string[] | undefined): string {
  return values === undefined ? "" : " [" + values.join(" ") + "]";
}

function grantLine(grant: ManifestGrant): string {
  const enabled = grant.enabled === undefined ? "" : " " + String(grant.enabled);
  return grant.id + list(grant.patterns) + list(grant.methods) + list(grant.responseHeaders) + enabled;
}

/** The built product manifest requests exactly the console's grants, nothing else. */
export function appManifestTests(r: Report): void {
  const manifest = JSON.parse(varta.fs.read(APP_MANIFEST)) as AppManifest;
  equal(r, "manifest: the product entry", manifest.entry, "src/app/main.tsx");
  equal(r, "manifest: the data root only", manifest.fsRoots.join(" "), "data");
  const expected: string[] = [
    "kv.read [" + KEY_DATABASE + "]",
    "kv.write [" + KEY_DATABASE + "]",
    "net.request [" + API_BASE_URL + "/**] [GET POST]" + list(RESPONSE_HEADERS),
    "time.now true",
    "time.timer true",
  ];
  equal(r, "manifest: exactly the console's grants", manifest.capabilities.map(grantLine).join("; "), expected.join("; "));
}

// ------------------------------------------------------------------ helpers

function seed(): Promise<void> {
  return varta.kv.open(TEST_DATABASE).then(async (kv: Kv): Promise<void> => {
    await writeApiKey(kv, API_KEY);
    kv.close();
  });
}

function startName(state: StartState): string {
  switch (state.kind) {
    case "unavailable":
      return "unavailable " + state.reason;
    case "following":
      return "following " + state.sessionId;
    default:
      return state.kind;
  }
}

function endName(end: FollowEnd): string {
  switch (end.kind) {
    case "failed":
      return "failed " + end.code;
    case "gave_up":
    case "invalid":
    case "refused":
      return end.kind + " " + end.reason;
    default:
      return end.kind;
  }
}

function recording(scenario: string): Recording {
  const text = recordingText(scenario);
  if (text === null) throw new Error("no recording " + scenario);
  return JSON.parse(text) as Recording;
}

/** What the person sent in the recording. */
function sentText(scenario: string): string {
  for (const request of recording(scenario).requests) {
    if (request.step === SEND_STEP) return (request.request as SendSessionMessageRequest).content;
  }
  throw new Error("no message in " + scenario);
}

/** The reviewed timeline of a recording, with the person's message before its turn. */
function expectedWithAsk(scenario: string, text: string): string {
  const reviewed = EXPECTED[scenario]!;
  let timeline = reviewed.slice(0, reviewed.lastIndexOf(" "));
  for (const prefix of SEGMENT_PREFIXES) {
    timeline = timeline.replace(prefix, prefix + "(ask sent " + JSON.stringify(text) + ") ");
  }
  return timeline;
}

interface Run {
  store: ChatStore;
  app: ConsoleApp;
  gate: GatedFetch;
  state: StartState;
}

async function start(base: string, scenario: string, hold: boolean, keyDatabase: string): Promise<Run> {
  return startWith(base, scenario, new GatedFetch(hold, null), keyDatabase);
}

async function startWith(base: string, scenario: string, gate: GatedFetch, keyDatabase: string): Promise<Run> {
  const store = new ChatStore(new Runtime());
  const app = new ConsoleApp(store, {
    baseURL: base + E2E_PREFIX + scenario,
    keyDatabase: keyDatabase,
    fetcher: (input: string, init: RequestInit): Promise<Response> => gate.fetch(input, init),
    follow: E2E_FOLLOW,
  });
  const state = await app.start();
  return { store: store, app: app, gate: gate, state: state };
}

function sessionOf(run: Run): ConsoleSession {
  const session = run.app.session;
  if (session === null) throw new Error("not following: " + startName(run.state));
  return session;
}

function turnOf(session: ConsoleSession): Turn | null {
  for (const e of session.segment.entries) {
    if (e.kind === "turn" && e.runId === RUN_ID) return e;
  }
  return null;
}

// ------------------------------------------------------------- the start

/** Start without a key, with an ungranted database, an ungranted origin or no Head Agent. */
export async function appStartTests(r: Report, base: string): Promise<void> {
  await seed();
  const empty = await start(base, NO_HEAD_SCENARIO, false, EMPTY_DATABASE);
  equal(r, "start: no stored key", startName(empty.state), "no_key");
  equal(r, "start: no key, no request", String(empty.gate.requests.length), "0");

  const denied = await start(base, NO_HEAD_SCENARIO, false, UNGRANTED_DATABASE);
  equal(r, "start: an ungranted database is denied before any effect", startName(denied.state), "unavailable NotAllowedError: <capability denied: kv.read> (" + UNGRANTED_DATABASE + ")");

  // The SDK wraps the engine's rejection in its connection error; the reason
  // shows the capability denial it wraps, which a refused TCP connection is not.
  const store = new ChatStore(new Runtime());
  const outside = new ConsoleApp(store, { baseURL: UNGRANTED_ORIGIN, keyDatabase: TEST_DATABASE, fetcher: null, follow: E2E_FOLLOW });
  const state = await outside.start();
  equal(
    r,
    "start: an ungranted origin is denied by net.request",
    startName(state),
    "unavailable APIConnectionError: Connection error (" + netDenial("GET", UNGRANTED_ORIGIN + TENANT_PATH) + ")",
  );
  equal(r, "start: denied, nothing followed", String(outside.session === null), "true");

  const headless = await start(base, NO_HEAD_SCENARIO, false, TEST_DATABASE);
  equal(r, "start: a tenant without a Head Agent", startName(headless.state), "no_agent");
  equal(r, "start: the tenant is read, no session created", headless.gate.requests.join("; "), "GET " + E2E_PREFIX + NO_HEAD_SCENARIO + TENANT_PATH);

  // A key that does not read as a string fails the start, and the database is
  // closed all the same: more starts than the engine keeps databases open all
  // fail with the read's own error, never with the open limit.
  const malformed = await varta.kv.open(MALFORMED_DATABASE);
  await malformed.set<number>(API_KEY_ENTRY, MALFORMED_KEY);
  malformed.close();
  const failures: string[] = [];
  for (let i = 0; i <= KV_OPEN_LIMIT; i++) {
    const failed = await start(base, NO_HEAD_SCENARIO, false, MALFORMED_DATABASE);
    failures.push(startName(failed.state));
  }
  equal(r, "start: a malformed key is unavailable", failures[0]!.startsWith("unavailable TypeError: ") ? "unavailable TypeError" : failures[0]!, "unavailable TypeError");
  equal(r, "start: the database is closed after a failed read", failures[KV_OPEN_LIMIT]!, failures[0]!);
}

/** The engine's denial of a request no `net.request` grant covers. */
function netDenial(method: string, url: string): string {
  return "NotAllowedError: <capability denied: net.request> (" + method + " " + url + ")";
}

// ------------------------------------------------ send, stream, approve, answer

/**
 * One recorded scenario with a wait: the person sends, the first connection
 * ends without `[DONE]` at the wait, the person acts, the console reconnects
 * with the model's Last-Event-ID and the run completes; the test then stops
 * following at the next reconnect.
 */
async function waitScenario(r: Report, base: string, scenario: string, act: (store: ChatStore, session: ConsoleSession) => void, waitShown: string, lastId: string, actionPath: string): Promise<void> {
  const run = await start(base, scenario, true, TEST_DATABASE);
  equal(r, scenario + ": following", startName(run.state), "following session-1");
  const session = sessionOf(run);
  const text = sentText(scenario);
  run.store.send(session.segment, text);
  await session.requests.drain();
  run.gate.release();
  await run.gate.connections(2);
  equal(r, scenario + ": the stream shows the wait", String(show(run.store.timeline).includes(waitShown)), "true");
  act(run.store, session);
  await session.requests.drain();
  run.gate.release();
  await run.gate.connections(3);
  equal(r, scenario + ": the timeline equals the reviewed recording", show(run.store.timeline), expectedWithAsk(scenario, text));
  session.follower.stop();
  equal(r, scenario + ": stopped by the person", endName(await session.ended), "stopped");
  equal(r, scenario + ": reconnects with the model's Last-Event-ID", run.gate.resumes.join(" "), "- " + DROP_AFTER[scenario]! + " " + lastId);
  const prefix = E2E_PREFIX + scenario;
  const events = "GET " + prefix + "/api/v1/sessions/session-1/events";
  equal(
    r,
    scenario + ": requests in order",
    run.gate.requests.join("; "),
    ["GET " + prefix + TENANT_PATH, "POST " + prefix + "/api/v1/sessions", events, "POST " + prefix + "/api/v1/sessions/session-1/messages", events, "POST " + prefix + actionPath, events].join("; "),
  );
}

/** The SSE id of the recording's last run event (dotted event names are run events). */
function lastRunEventId(scenario: string): string {
  let last = "";
  for (const frame of recording(scenario).frames) {
    const id = frame.id;
    if (frame.event.includes(".") && id !== undefined) last = id;
  }
  return last;
}

/** Send, stream, approve, answer, a drop, a failed reconnect, and the ends of a stream. */
export async function appStreamTests(r: Report, base: string): Promise<void> {
  await seed();
  await waitScenario(
    r,
    base,
    "approval_approve",
    (store: ChatStore, session: ConsoleSession): void => store.decide(session.segment, RUN_ID, true),
    "(approval open",
    lastRunEventId("approval_approve"),
    "/api/v1/runs/" + RUN_ID + "/approve",
  );
  await waitScenario(
    r,
    base,
    "ask_user",
    (store: ChatStore, session: ConsoleSession): void => store.answer(session.segment, RUN_ID, ANSWER),
    "(question open",
    lastRunEventId("ask_user"),
    "/api/v1/runs/" + RUN_ID + "/respond",
  );

  // A drop mid-run, then a reconnect the mock answers with a 503: the console
  // resumes again from the same event, and `session_closed` + `[DONE]` end it.
  const closing = await start(base, "session_end", true, TEST_DATABASE);
  const session = sessionOf(closing);
  const text = sentText("session_end");
  closing.store.send(session.segment, text);
  await session.requests.drain();
  closing.gate.release();
  closing.gate.release();
  closing.gate.release();
  equal(r, "session_end: over after session_closed and [DONE]", endName(await session.ended), "over");
  equal(r, "session_end: the timeline equals the reviewed recording", show(closing.store.timeline), expectedWithAsk("session_end", text));
  const dropped = DROP_AFTER["session_end"]!;
  equal(r, "session_end: a failed reconnect resumes from the same event", closing.gate.resumes.join(" "), "- " + dropped + " " + dropped);

  const giveUp = E2E_FOLLOW.maxReconnects;
  const ends: EndCase[] = [
    { scenario: MISSING_SCENARIO, end: "failed SESSION_NOT_FOUND", connections: 1, requests: 1 },
    { scenario: EXPIRED_SCENARIO, end: "authenticate", connections: 1, requests: 1 },
    { scenario: UNAUTHORIZED_SCENARIO, end: "authenticate", connections: 1, requests: 1 },
    // `connected` alone is no progress: the follower gives up as on empty connections.
    { scenario: CONNECTED_ONLY_SCENARIO, end: "gave_up " + DROPPED_REASON, connections: giveUp, requests: giveUp },
    { scenario: EMPTY_SCENARIO, end: "gave_up " + DROPPED_REASON, connections: giveUp, requests: giveUp },
    { scenario: INVALID_SCENARIO, end: "invalid " + INVALID_REASON, connections: 1, requests: 1 },
    // A failed open is retried once by the SDK, then its error decides: a 403
    // ends following, a 503 counts as a connection without progress.
    { scenario: FORBIDDEN_SCENARIO, end: "refused PermissionDeniedError: 403 Forbidden — " + FORBIDDEN_DETAIL, connections: 1, requests: FAILED_OPEN_REQUESTS },
    { scenario: BUSY_SCENARIO, end: "gave_up " + BUSY_REASON, connections: giveUp, requests: giveUp * FAILED_OPEN_REQUESTS },
    // Reopened by the SDK, then ended without a frame: the SDK ends that stream as
    // silently as after `[DONE]`, and the follower takes it for a drop, not `[DONE]`.
    { scenario: REOPEN_EMPTY_SCENARIO, end: "gave_up " + SILENT_REASON, connections: giveUp, requests: giveUp * FAILED_OPEN_REQUESTS },
  ];
  for (const c of ends) {
    const run = await start(base, c.scenario, false, TEST_DATABASE);
    await checkEnd(r, c.scenario, run, c);
  }
  // Only the stream's origin is ungranted: the capability denial ends following.
  const denied = await startWith(base, FORBIDDEN_SCENARIO, new GatedFetch(false, UNGRANTED_ORIGIN), TEST_DATABASE);
  const streamPath = E2E_PREFIX + FORBIDDEN_SCENARIO + EVENTS_PATH + EVENTS_QUERY;
  await checkEnd(r, "stream origin denied", denied, { scenario: FORBIDDEN_SCENARIO, end: "refused " + netDenial("GET", UNGRANTED_ORIGIN + streamPath), connections: 1, requests: FAILED_OPEN_REQUESTS });
}

/** How following one generated stream must end. */
interface EndCase {
  scenario: string;
  end: string;
  /** Streams the follower opened. */
  connections: number;
  /** Stream requests made, the SDK's reconnects after a failed open included. */
  requests: number;
}

async function checkEnd(r: Report, name: string, run: Run, c: EndCase): Promise<void> {
  const followed = sessionOf(run);
  const end = await followed.ended;
  equal(r, name + ": the stream ends with", endName(end), c.end);
  equal(r, name + ": streams opened", String(followed.follower.connections), String(c.connections));
  equal(r, name + ": stream requests", String(run.gate.resumes.length), String(c.requests));
  const note = "(note " + JSON.stringify(endText(end)) + ")";
  equal(r, name + ": the note shows why", show(run.store.timeline).includes(note) ? "noted" : show(run.store.timeline), "noted");
}

/** A stream whose frames take longer than one per-turn deadline to arrive and apply. */
export async function appLongTests(r: Report, base: string): Promise<void> {
  await seed();
  const deadline = (JSON.parse(ceilingText) as AppCeiling).deadlinePerTurnMs;
  const run = await start(base, LONG_SCENARIO, false, TEST_DATABASE);
  const session = sessionOf(run);
  const started = Date.now();
  const end = await session.ended;
  const elapsed = Date.now() - started;
  console.log("long: the stream lasted " + String(elapsed) + " ms; per-turn deadline " + String(deadline) + " ms");
  equal(r, "long: over after session_closed and [DONE]", endName(end), "over");
  equal(r, "long: one connection", String(session.follower.connections), "1");
  const turn = turnOf(session);
  equal(r, "long: every chunk applied", turn === null ? "no turn" : String(turn.lastSeq), String(LONG_FIRST_CHUNK_SEQ + LONG_CHUNKS));
  equal(r, "long: completed with the authoritative answer", turn === null ? "no turn" : turn.state + " " + turn.answer, "completed ok");
  equal(r, "long: the stream outlived one per-turn deadline", elapsed > deadline ? "longer" : "only " + String(elapsed) + " ms", "longer");
}
