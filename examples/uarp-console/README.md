# UARP Console: Varta product example

A cross-platform client console for the UARP platform, written in strictly typed
TypeScript on top of [`uarp-sdk`](https://www.npmjs.com/package/uarp-sdk) and
compiled by Varta into one portable `app.wasm`. The same artifact is the
acceptance target for every Varta platform: the browser, desktop macOS, Linux
aarch64 (Raspberry Pi 5) and the headless conformance renderer.

## UX: Single Chat

The console is one continuous conversation with the tenant's **Head Agent**, the
main agent of the whole space (`Tenant.head_agent_id`). There is no separate
navigation for agents or runs:

- messages go through `sessions` (`sendSessionMessage` returns a `run_id`); the
  reply streams over the session's SSE stream with `event_schema=1` (`llm.chunk`
  with content and thinking, `tool.*`, `child_run.*`, `run.completed`/`run.failed`);
- approvals (`run.awaiting_approval`) and questions (`run.awaiting_input`) appear
  as interactive cards inside the conversation;
- sessions that close or expire are stitched into one timeline with separators;
- agents and runs mentioned in the conversation open in an optional context panel.

Administration (creating or suspending agents) is not designed yet.

## Boundaries

- This folder is an application, not part of the engine. Nothing here is imported
  by `crates/`, and engine crates never reference UARP types, routes or logic.
- `uarp-sdk` (0.13.2, the v1 schema of build bd5dc091) is an exact-pinned npm dependency (`package.json`, `package-lock.json`;
  install with `npm ci --ignore-scripts`). It lives in the ignored `node_modules/`
  and is never vendored into the repository. The console imports it by name,
  `from "uarp-sdk"` (v38 package specifiers: Varta compiles the TypeScript source
  the package ships); no source names a path inside `node_modules/`.
- The application uses Varta only through its public tooling and contracts
  (`varta check`, `varta build`, the embedding contract and the L0 view protocol).
  Where Varta cannot yet compile or run something, the gap is reported to the
  core, not worked around here.

## Layout

| Path | Content |
| --- | --- |
| `src/markdown/` | Deterministic Markdown subset for chat replies, parsed into a closed tree of blocks and inline nodes that maps onto L0; incremental parsing for streaming answers |
| `src/l1/` | L1 reactive core: signals, lazy computeds, effects, batches and keyed lists over an explicit runtime; accessors are functions, as in SolidJS |
| `src/app/` | The product entry (`varta.json`, `main.tsx`): the API key from the KV database, the real `UarpClient`, a new session with the tenant's Head Agent, the request layer wired to the store and the followed session stream; `api.ts` adapts the client to the chat's `ChatApi` and `StreamApi` |
| `src/chat/` | Chat timeline model: the dispatcher of session-stream frames as the wire delivers them and one handler per closed v1 event type, stream resume and end rules, transcripts as history, the reactive store with the person's actions as queued intents, the request layer that performs them through the uarp-sdk client, the follower of the live session stream (`follow.ts`), and a compact text form for tests |
| `src/ui/` | Single Chat screen in TSX (L0 view protocol, v38): the live timeline, the composer, Markdown as L0 nodes, interface strings; the stylesheet is `mockups/console.css`, imported as text |
| `src/tests/` | Tests executed by Varta itself (`varta.test.json`, one execution per group); `frames.ts`, `transcripts.ts` and `responses.ts` hold recorded v1 frames, transcripts and API responses |
| `fixtures/events-v1/` | The API lane's nine recorded `event_schema=1` session streams and their transcripts (anonymised; provenance in its README), read by the scenario tests through the virtual root `/project` |
| `src/tests/sdk/` | SDK client tests executed by Varta (`varta.sdk-test.json`): the real, unmodified `UarpClient` against the loopback mock, one execution per group; `contract.ts` holds the canned bodies, headers and timings both sides share |
| `src/tests/sdk-server/` | The loopback mock of the UARP API, itself a Varta HTTP/1 application (`varta.sdk-server.json`); `e2e.ts` replays the recorded scenarios for the console end to end |
| `src/tests/app.test.ts` | The console end to end: the product's start, request layer and follower with the real `UarpClient` against the mock, and the product manifest; `loopback.ts` is the instrumented `fetch` those tests pass to the client, `app-ceiling.json` the host ceiling budgets of the app groups |
| `src/tests/view/` | View tests: a timeline from recorded frames rendered headlessly by `varta dev --view-script` (`varta.view.json`), compared with the reviewed tree and accessibility snapshot |
| `design/agent-studio.md` | Draft of Agent Studio (building a personal agent with Varta tools); built after the rest of the console works |
| `design/chat-timeline.md` | Draft model of the chat timeline: sources, entries, reducer, transport, session stitching |
| `scripts/test.sh` | Builds the product and the test entry once, starts the loopback mock and runs the Varta-executed tests group by group, then the view tests |
| `scripts/mock.sh` | Starts and stops the loopback mock for `test.sh` and `sdk-loopback.sh` |
| `scripts/check-sdk.sh` | `varta check` of every pinned SDK source file, one file at a time, then of the instantiated SDK generics |
| `scripts/check-sdk-generics.ts` | The entry that instantiates every generic function and method of the pinned SDK, used by `check-sdk.sh` |
| `scripts/sdk-loopback.sh` | Runs the loopback mock and the SDK client tests as two `varta dev` processes |
| `mockups/` | Static Single Chat screens in plain HTML/CSS restricted to the L0 v1 subset. `console.css` is the guest stylesheet, `frame.css` the embedding page, `lint.html` checks every screen against the common registry and official host model |
| `mockups/conformance/` | Renders every screen through the official browser host and compares structure, live control state and boxes with the mockup |

Live browser execution uses the official host generated by `varta dev --target browser`.
Mockup tooling imports the same host modules from `crates/varta-project/src/browser/`
and the common registry through `mockups/official-host.js`; it does not depend on
the historical `varta-web-host` prototype. The application owns the HTML encoder,
comparison and loopback-only development server. L1 and the Markdown parser stay
in `src/` until Varta projects can depend on packages outside their own folder.

Secrets such as the UARP API key live in the ignored `.env.local`, never in the
repository.

## Status

Checkpoint 2026-11-05 (debug `varta` of `4fce278` with this example's working
tree, after the review fixes): `scripts/test.sh` passes **179 tests in 32 groups
and two headless view checks**: the 107 tests in 28 groups below plus the four
`app-*` groups (72 tests) of the product entry against the loopback mock
([product entry](#product-entry-and-the-console-end-to-end)).
The product entry `varta.json` builds into one `dist/app.wasm` and
`dist/app.manifest.json`. Headless, without a stored key, its screen shows the
"no key" banner (`varta dev --config varta.json --view-script`, checked by hand,
not by a test).

Checkpoint 2026-11-01 (`abea3ac`): the native Varta test workflow passes
**107 tests in 28 groups and two headless view checks on the normal CLI stack**.
An additional local Chrome probe, `node examples/uarp-console/scripts/test-browser-chat.mjs`,
performs the chat view's thinking, approval, option, input and Enter actions with
trusted browser events and checks selected final text against the native view
expectation. It uses an isolated browser profile and explicit test-only decisions.
The same focused interaction probe also runs in Firefox, WebKit and Chrome with
`node examples/uarp-console/scripts/test-browser-chat-matrix.mjs <varta-executable> <playwright-module-path>`.
Append `chrome`, `firefox` or `webkit` to run one engine during development;
omitting the selector still attempts the full three-engine sequence. A selected
engine pass does not establish a full matrix.
First build the projection tool with
`cargo build -p varta-view --example project_flex_snapshot --locked`.
The probe locates that tool relative to the repository; set `VARTA_FLEX_PROJECTION`
to its executable path when using a custom Cargo target directory. It defaults to
the installed Chrome channel; set `VARTA_CHROMIUM_CHANNEL=chromium` to use
Playwright's bundled Chromium, or name another installed Playwright channel.
The probe compares the full ordered element/text tree and non-style attributes,
plus 195 declarations across 14 style properties against the native flex-projected
snapshot. It also compares the selected region, button, heading and textbox roles,
names and recorded control states through Playwright accessibility selectors. The
test retains a reference to the closed shadow root for those selectors. Full style,
layout and accessibility parity remain open; the ordinary native snapshot retains
its cascaded display values.
The SDK stays pinned at 0.13.2. This checkpoint does not verify live-service
credentials, SDK-own-test completeness, a real browser or native display targets.
Earlier fuel/row figures below are dated development measurements, not current
performance limits or production capacity.

| Piece | State | Waits for |
| --- | --- | --- |
| L0 conformance | Every mockup rendered by the applier matches it in structure and boxes (±1 px) at three sizes | The headless and native renderers for the cross-renderer criterion |
| Static mockups (Single Chat) | Five screens in `mockups/`; 0 subset violations per `mockups/lint.html` | Owner review |
| Markdown subset | Compiled and tested by Varta (v38), including incremental parsing while streaming; a 99-character reply parses in about 59k fuel | — |
| L1 reactive core and keyed lists | Compiled and tested by Varta (v38); a 1M-fuel turn fits 28 rows mounted, 16 updated | Final target performance measurements |
| Chat model | Every frame of the session stream dispatched and parsed strictly into the closed v1 types; the nine recorded scenarios replayed from their files match an independently derived timeline; resume and end rules, history and its merge with the live stream, the store, intents and the request layer | — |
| Single Chat screen | Live timeline over the store with the stylesheet, checked headlessly and deterministically (tree with cascaded styles, accessibility snapshot): thinking toggle, tool lines, approval, question and failure cards with their actions, streaming Markdown, the composer | Full cross-target acceptance beyond the focused official-browser interaction probes |
| SDK source admission | `scripts/check-sdk.sh` checks 65/65 pinned 0.13.2 files unmodified under v38, plus the bodies of all six generic functions and methods through an instantiation probe (2026-11-04, debug build of `978b793`) | SDK-own tests (M7); generic bodies only for the probed type arguments |
| Real requests to UARP | The real `UarpClient` runs natively under Varta against the loopback mock: unary JSON, problem documents and bare error bodies, retry after a 503 with `Retry-After` (and none with `x-should-retry: false`), `AbortSignal`, timeout, and `sessions.streamSessionEvents` to `[DONE]`; 43 cases in 6 groups pass (2026-11-04) | Live-service validation, streams with gaps between frames or of real size (finding F4), cross-target acceptance; findings below |
| Product entry | `src/app/` with `varta.json`: key from KV, the real client, a new session with the Head Agent, the request layer and the followed session stream with the stream.ts reconnect rules; send, approve, answer, drop, reconnect (also after a 503) and the ends after `session_closed`, a final `error`, `[DONE]` alone and a 401 pass end to end against the recorded scenarios (2026-11-04); giving up after connections without progress, a frame strict parsing refuses and a refused stream open (403, capability denial) with the real reason in the note pass end to end (2026-11-05) | The settings screen (the key is only read), the session list and history stitching, live-service validation, the view under scripted input with the live stream, cross-target acceptance |

## Running the checks

```bash
VARTA_BIN=/path/to/varta scripts/test.sh [group…]
/path/to/varta build --config varta.json
VARTA_BIN=/path/to/varta scripts/check-sdk.sh
VARTA_BIN=/path/to/varta scripts/sdk-loopback.sh [group…]
node scripts/serve-mockups.mjs 8766
```

Then open `http://127.0.0.1:8766/uarp-console/mockups/lint.html` and
`http://127.0.0.1:8766/uarp-console/mockups/conformance/conformance.html`.

From the repository root, check the server allowlist and run the mockup browser
matrix with a local Playwright installation (Chrome, Firefox and WebKit binaries
must be installed):

```sh
node --test examples/uarp-console/scripts/serve-mockups.test.mjs
node examples/uarp-console/scripts/test-mockups.mjs /path/to/playwright/index.mjs
```

Append `chrome`, `firefox` or `webkit` to select one engine. Each engine checks five
screens at three sizes, lint rejection cases, live control state, disabled select
selection and work-budget rollback. Dialog and fixed-position subtrees have
structure/state checks but no box-parity claim. These static projections do not
prove compiled application execution or cross-renderer acceptance.

`check-sdk.sh` copies `node_modules/uarp-sdk/src` (it refuses any version but the
pinned one) unmodified into a temporary project. For each of its files it writes a
probe entry that re-exports the file with `export *`, and default-imports it when it
has a default export. It runs `varta check` under `checked-f64-utf16-v38`, prints
one line per file and `N/N passed`, and exits nonzero on any failure.

The per-file pass alone does not cover generic bodies: `varta check` compiles the
body of a generic function or method only where it is instantiated, so an exported
`f<U>` or a method of `Box<T>` whose body holds a refused construct still gives
`ok`. The script therefore also checks `scripts/check-sdk-generics.ts` as the entry.
It instantiates `autoPaginate<T>` and `collect<T>` (`core/pagination.ts`),
`Transport.request<T>` (`core/transport.ts`), `UarpClient.request<T = unknown>`
(`index.ts`), `UarpEvent.json<T = JsonValue>` as `makeEvent` implements it
(`core/sse.ts`) and `pick<T, K>` (`core/util.ts`) with concrete SDK types, and with
the declared default where there is one. Two guards keep that probe honest:

- **Inventory.** A lexical scan of the SDK's declaration lines must find exactly the
  generic sites the script lists; a new or moved generic fails until the probe and
  the list are updated.
- **Control.** For each listed body, a separate copy of the SDK gets `eval("0");`
  (never admitted) as the body's first statement, and the probe must be refused at
  that line. The script prints `generic bodies reached: N/N`.

Remaining caveat: the inventory is a scan of declaration lines, not a parse, so a
generic written in a form it does not recognise (for example a line break before
the type parameters) would be neither listed nor probed. Each generic is checked for
one concrete type argument and its default, so a construct refused only for another
type argument is not covered.

`sdk-loopback.sh` starts the mock with `varta dev --config varta.sdk-server.json`.
The mock's only grants are the exact `net.listen` grant `127.0.0.1:0` and
`time.timer`, used by the slow route and the late head of the long console stream. The script reads the bound origin from
`varta listening: …`. `net.request` ports are never wildcards, so the committed
`varta.sdk-test.json` names the placeholder origin `http://127.0.0.1:1`. The
script writes a copy with the bound origin next to it,
`varta.sdk-test.loopback-<pid>.json` (git-ignored), and removes the copy on exit. A
run killed without its exit trap (`SIGKILL`, a crashed shell) leaves that file and
possibly the mock's `varta dev` behind; delete the file and stop the process by
hand. The client's grants are `GET` on that origin only, with the declared response
headers `content-type`, `retry-after`, `x-correlation-id`, `x-request-id` and
`x-should-retry`. It also needs `time.now`, which the SDK's `EventStream` and the
retry timing read, `time.timer`, and `env.get` of `TEST_GROUP` and `SDK_BASE_URL`.
Each group runs as one execution with production budgets. The script prints `PASS`
or `FAIL` per group and exits nonzero on any failure.

| Group | What the real client does |
| --- | --- |
| `sdk-unary` | `health.get()` decoded strictly into `GetHealthResponse`; base-URL normalisation; an ungranted origin is denied before any effect and surfaces as `APIConnectionError` caused by the engine's `NotAllowedError` |
| `sdk-problem` | A 404 problem document becomes `NotFoundError`, with `status`, `code`, `reason`, `correlationId`, `requestId` (from `x-request-id`), `message` and `validationErrors`. A bare `{"error": …}` 403 becomes `PermissionDeniedError` with its detail and `correlationId` from `x-correlation-id` |
| `sdk-retry` | A 503 with `Retry-After: 1` is retried once and succeeds, with a wait of at least one second rather than the first backoff of at most 500 ms. A 503 with `x-should-retry: false` is not retried. Attempts are counted by a `fetch` passed in `ClientOptions.fetch` that delegates to the engine's `fetch`. Each mock request runs in a fresh store, so the mock keeps no state and reads the attempt number from `x-loopback-attempt` |
| `sdk-abort` | Aborting the caller's `AbortSignal` rejects with its `AbortError` before the slow answer; an already aborted signal rejects at once |
| `sdk-timeout` | `timeout: 200` turns into `APITimeoutError` (`Request timed out after 200 ms`). This group runs as its own execution because of finding F2 |
| `sdk-sse` | `sessions.streamSessionEvents(…, { event_schema: 1 })` parses a finite stream of recorded v1 frames. It checks names, ids and data byte for byte. A keep-alive comment and a `retry:`-only frame deliver nothing, `json()` decodes a frame, and `[DONE]` ends the stream without a reconnect (`connecting connected disconnected`) |

Checkpoint 2026-11-04 (debug `varta` of `978b793`): `check-sdk.sh` printed
`65/65 passed`, the generic inventory (7 declaration sites), the instantiation probe
and `generic bodies reached: 6/6`. `sdk-loopback.sh` passed all six groups with 43
cases.

## Product entry and the console end to end

`varta build --config varta.json` builds the product entry `src/app/main.tsx` into
one `dist/app.wasm` and its manifest `dist/app.manifest.json`. The manifest
requests exactly:

| Grant | Scope | Why |
| --- | --- | --- |
| `net.request` | `https://api.snaga.ai/**` (`API_BASE_URL`), `GET` and `POST`, response headers `retry-after`, `x-correlation-id`, `x-request-id`, `x-should-retry` | The SDK's requests and the session stream; the four headers are the ones the SDK reads (retry timing, `APIError` ids) |
| `kv.read`, `kv.write` | `/data/console.kv` (`KEY_DATABASE`, root `data`) | The API key (`src/app/key.ts`); nothing writes it yet, the settings screen will |
| `time.timer` | — | The SDK's `Transport.request` arms a `setTimeout` for its timeout on every request, so without it every request fails before the network (`NotAllowedError: <capability denied: time.timer> (setTimeout)`, observed) |
| `time.now` | — | The SDK's `EventStream` reads `Date.now` (its default clock) when each connection opens (`core/sse.ts`, `connectedAt`) |

**Owner decision (2026-11-05): the two `time` grants are accepted.** The brief
asked for `net.request` and `kv` only; the unmodified SDK arms `setTimeout` on every
request and reads `Date.now` on every stream connection, so the product manifest
requests `time.timer` and `time.now` as well, and nothing wider: the `app-manifest`
group fails on any other grant. `varta dev --config varta.json` maps
the `data` root to `data/` here (git-ignored except its `.gitignore`).

At start the console reads the key from KV, constructs `UarpClient` with it and the
base URL, reads the tenant (`tenants.getCurrentTenant`), creates a session with its
Head Agent (`sessions.create`), wires the request layer to the store's intents and
follows the session (`src/chat/follow.ts`):

- `sessions.streamSessionEvents(sessionId, { event_schema: 1 })`; every frame goes to
  `applyFrame` (dispatch.ts), one frame per event turn, because a read of the
  engine's per-read delivery cap holds dozens of frames and applying them all in
  the turn that receives the read exhausted the turn's fuel (observed: `turn 12
  (network) exhausted its fuel budget`).
- When a connection ends, stream.ts decides: `[DONE]` is judged by the frame
  before it (`afterDone`), an end without it is a drop (`afterDrop`), and a resume
  reconnects with `Last-Event-ID` of the last run event the model applied
  (`resumeId`), not the SDK's last SSE id (which can be `run-1:done`). The SDK
  reports a drop or a failed connection as `onState` `reconnecting` and never after
  `[DONE]`; after `connected` the follower stops the SDK's stream there and
  reconnects itself.
- A connection that fails to open is announced before `connected`, and the SDK
  keeps its error. The follower lets the SDK make its one reconnect (same request,
  same `Last-Event-ID`), so the second failure surfaces the error itself: a 401
  ends following with "authenticate"; a status outside the SDK's retryable set
  (403, 404, …) or a capability denial (`NotAllowedError`) ends it as `refused`
  with that error in the note; a retryable status (408, 429, 5xx) or a connection
  error counts as a connection without progress, and its error is the reason if
  the follower gives up. A failed open therefore costs two requests.
- Only run and session frames are progress: the `connected` frame every connection
  opens with, and an `error` frame before `[DONE]`, are not. Waits between
  reconnects start at `retryMs` and double per connection in a row without
  progress, up to `maxRetryMs`; after `maxReconnects` such connections the
  follower gives up (`gave_up`, with how the last one ended). The product uses the
  SDK's own defaults (2 s, 8 s, 5). `[DONE]` with nothing before it also ends
  following with "authenticate"; a frame strict parsing refuses ends it as
  `invalid`. The screen shows a note for every end but `over` and `stopped`.

`scripts/test.sh` runs the end-to-end groups as `varta run --app` application
instances of the one test build. Every group runs under a host policy derived from
the built `dist/tests.manifest.json` (its ABI version and grants, the roots mapped);
the app groups add the budgets of `src/tests/app-ceiling.json`, which lower the
per-turn deadline to 250 ms and which `app.test.ts` reads too. The client is the real, unmodified
`UarpClient`, given an instrumented `fetch` through `ClientOptions.fetch`
(`src/tests/loopback.ts`): it delegates to the engine's `fetch`, records every
request, numbers each stream connection in `x-loopback-attempt` and, where a
scenario needs it, holds a stream connection until the test releases it. The mock
answers a stream at once and whole (finding F4), so holding is what places the
person's action between two connections. The mock (`src/tests/sdk-server/e2e.ts`)
serves the recorded scenarios `approval_approve`, `ask_user` and `session_end`
(built in as text imports), checks every request body against the recording, and
serves frames strictly after the `Last-Event-ID` it receives. A recording's first
connection ends without `[DONE]` at its drop point (`DROP_AFTER`).

| Group | What runs |
| --- | --- |
| `app-manifest` | The built `dist/app.manifest.json` requests exactly the grants above, with the `data` root only |
| `app-start` | No stored key: no request. An ungranted database: `NotAllowedError` from `kv.read` before any effect. An ungranted origin (the product's own `fetch`): the SDK's `APIConnectionError` caused by the engine's `NotAllowedError` for `net.request`, which the reason shows, nothing followed. A tenant without a Head Agent: the tenant is read and no session is created. A key entry that does not decode as a string: the start reports the read's `TypeError`, and more such starts than the engine keeps databases open all report it, so the database is closed on that path too |
| `app-e2e` | `approval_approve` and `ask_user`: send, the stream to the wait, the drop, approve or answer, the reconnect with the model's `Last-Event-ID`, the run to its end; the timeline equals the reviewed expectation of that recording (scenarios.test.ts) plus the person's message, and the requests come in the expected order. `session_end`: a drop mid-run, a reconnect the mock answers with a 503, the SDK's reconnect from the same event, then `session_closed` and `[DONE]` end following with `over`. A final `error` frame (`SESSION_NOT_FOUND`), `[DONE]` alone and a 401 end following without a reconnect. Connections that deliver only `connected`, and connections that deliver only a keep-alive comment, end with `gave_up` after `maxReconnects`. A run event missing a required field ends with `invalid` and the strict parser's `TypeError`. A stream open answered 403, or sent to an ungranted origin, ends with `refused` and the `PermissionDeniedError` or `NotAllowedError` after two requests; one answered 503 every time ends with `gave_up` and the `ServiceUnavailableError`. Each end checks the streams opened, the stream requests and the note with the real reason |
| `app-e2e-long` | One connection whose head the mock delays 200 ms (within the 250 ms window) and whose 100 content chunks (about 24 KB, several reads) are applied one per turn: the stream lasts longer than one per-turn deadline although no single wait does, every chunk is applied and `session_closed` + `[DONE]` end it |

## Pilot findings

Engine refusals and wrong behaviour met while running the SDK; reported to the
core, not worked around in the SDK. The tests avoid each one only in their own
code, never by changing the SDK.

- **F1 (fixed by the wave-2 package `w2-v38-sdk-path-gaps`: v38 admits a
  generic method call on an array element receiver). A generic method call on
  an element-access receiver was refused.** The original report follows. The
  call `events[0].json<RunAdded>()` stops with `unsupported generic call: outside
  the supported TypeScript subset`. The same call on a local binding compiles and
  runs. The method is `UarpEvent.json<T = JsonValue>()` (`core/sse.ts:16`, built by
  `makeEvent` at `core/sse.ts:365-368`). Minimal reproducer:
  ```ts
  interface Ev { data: string; json<T>(): T }
  function make(data: string): Ev { return { data: data, json<T>(): T { return JSON.parse(data) as T; } }; }
  interface RunAdded { run_id: string }
  export function main(): void {
    const events: Ev[] = [make("{\"run_id\":\"run-1\"}")];
    console.log(events[0].json<RunAdded>().run_id); // refused; `const e = events[0]; e.json<RunAdded>()` runs
  }
  ```
  `sdk-sse` binds the event to a local first.
- **F2 (fixed by the wave-2 package `w2-net-abort`: an abort, or a transfer
  that ends on its own, closes its connection at once, and a v2 server waits
  for its worker permit). An aborted request's connection stayed open until
  the client's event loop next woke.** Both sides were at fault: the client's
  abort only marked the transfer cancelled until the event loop next drove
  the request runtime, and the server accepted the next connection while the
  cancelled worker still held its permit. Now the abort closes the socket
  before it returns, a transfer that ends on its own closes its socket before
  the guest sees the end, and a v2 server serves the next connection once the
  permit is free (`net_tests` regressions
  `aborted_requests_close_their_connection_before_the_guest_wakes`,
  `transfers_that_fail_on_their_own_close_their_connection_before_the_guest_wakes`
  and `aborted_requests_leave_the_server_worker_free_for_the_next_request`).
  The reproducer below, run against a scratch copy of the mock's
  `/slow/health` and `/unary/health` routes without its `user-agent` check,
  now logs the cancelled request after about 200 ms of wall time and gets
  `200` for the next request, with or without the 1 s wait. `sdk-timeout`
  still runs as its own execution; nothing requires that any more. The
  original report follows. The architecture's `net_request` cancellation rule
  says aborting while the head is pending closes the connection at once. In `sdk-abort`, the
  SDK's abort path (`core/transport.ts:139-146`) aborts a request to the slow
  route after 200 ms. In a run where the client then waited 2 s, `lsof` (sampled
  about every 0.4 s) still showed the client's socket `ESTABLISHED` 1.4 s after the
  abort. The mock logs `connection closed before message completed` and `request
  1: Cancelled` only when the client wakes. That request's wall time was 1223 ms
  when the client waited 1 s after the abort, and 2205 ms when it waited 2 s.
  Until then the mock's single worker stays busy. A request sent right after the
  wake-up gets the runtime's own `503` with body `request failed`, because the
  worker permit is still held (`varta-runtime` `server/transport.rs`,
  `try_acquire_owned`). The SDK received that 503 as `ServiceUnavailableError`
  instead of the expected timeout. Not established: whether the client defers the
  close or the server reads the end of stream late. The client-side socket sample
  points at the client. Minimal reproducer, without the SDK, against the running
  mock:
  ```ts
  const controller = new AbortController();
  setTimeout(() => controller.abort(), 200);
  try { await fetch(origin + "/slow/health", { headers: HEADERS, signal: controller.signal }); } catch (error) {}
  await new Promise<void>((resolve) => { setTimeout(() => resolve(), 1000); });
  const next = await fetch(origin + "/unary/health", { headers: HEADERS }); // 503 "request failed\n"
  ```
  `sdk-timeout` therefore runs as its own execution.
- **F3 (fixed by the wave-2 package `w2-v38-sdk-path-gaps`: v38 reads `cause`
  after `instanceof Error` and admits `new Error(message, options)` for the
  native error families). `cause` could not be read through `instanceof
  Error`.** The original report follows. The SDK puts the
  transport error in `cause` (`core/transport.ts:154`,
  `new APIConnectionError('Connection error', { cause: error })`). Reading it as
  `error instanceof Error ? error.cause : undefined` stops with `unsupported property
  other than name/message of a native error or Error subclass union; narrow with
  instanceof`, although the value is narrowed with `instanceof`. Narrowing to the
  SDK class (`error instanceof APIConnectionError`) reads it. `new Error(message, {
  cause })` is refused as well (`unsupported error constructor accepts zero or one
  string/undefined argument`), while `super(message, options)` in an `Error`
  subclass is admitted. `sdk-unary` narrows to `APIConnectionError`.
- **F4. The loopback mock cannot send a stream over time, and one request fits
  only a small body.** A Varta HTTP/1 handler answers with a whole body
  (`VartaHttpResponse.body: string`), so the mock cannot send frames with gaps
  between them, and each request runs on the HTTP entry point's fixed fuel (a
  fresh store "with existing fuel" per request, architecture). A generated stream
  of 200 content chunks exhausted it (`wasm trap: all fuel consumed` in the mock,
  which answered with a 500); 100 chunks (24,470 bytes) are served. The long-stream group
  therefore lasts longer than one per-turn deadline through a late head and many
  read turns, not through gaps inside the body.
- **F5. Compiling an entry that imports `"uarp-sdk"` by name takes about 25 s with
  a debug `varta`.** `varta dev` of the unit-test entry took about 4 s per group
  while it imported only `generated/models.ts` by path, and 27–30 s per group
  after the switch to the package root (`index.ts` and every resource module).
  `scripts/test.sh` therefore builds the test entry once and runs each group with
  `varta run`; the view checks still compile per view.
- **F6. Refused constructs met while writing the console's own code**, each
  rewritten in the console without changing the SDK:
  - a module constant whose initializer names a function (`const R: Responder = {
    header: header, … }`): `unsupported reference to header in a module constant
    initializer; only module constants are admitted`;
  - a conditional expression whose arms are two members of a record union, assigned
    to a variable of that union: `assignment type mismatch: expected reference
    union, found reference union` (an `if`/`else` with one assignment per arm
    compiles);
  - `reject(signal.reason)` inside `new Promise<void>(…)`: `value unknown is not a
    member of the expected union`;
  - `shift()` on an array of functions (`(() => void)[]`): `unsupported call`;
  - a property narrowed by `frame.id !== undefined`, then assigned to a `string`:
    `assignment type mismatch: expected string, found primitive union` (a local
    copy narrows);
  - returning a promise from an `async` function whose result is a union
    (`return events(…)` with `Promise<VartaHttpResponse>` inside
    `Promise<VartaHttpResponse | null>`): `value promise is not a member of the
    expected union` (`return await` compiles).
- **F7. `sdk-loopback.sh` failed at `4fce278` before this change.** The mock
  refused every request whose `user-agent` was not the SDK's, but v38 makes
  `user-agent` host-owned and the native host sends none. In that run
  `sdk-problem`, `sdk-retry`, `sdk-abort` and `sdk-timeout` failed with `400 Bad
  Request — unexpected user agent`, and `sdk-sse` ended with `execution deadline
  exceeded` while reconnecting. The mock now refuses a request that does carry the
  SDK's product token, which checks the host-owned rule instead.
