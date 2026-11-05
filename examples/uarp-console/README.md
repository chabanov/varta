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
| `src/app/` | The product entry (`varta.json`, `main.tsx`): the settings (API key and base URL) from the KV database (`key.ts`), the real `UarpClient`, the tenant's Head Agent, its stitched sessions, the request layer wired to the store and the followed session stream (`console.ts`); `api.ts` adapts the client to the console's `ConsoleApi` |
| `src/chat/` | Chat timeline model: the dispatcher of session-stream frames as the wire delivers them and one handler per closed v1 event type, stream resume and end rules, transcripts as history, session stitching (`stitch.ts`), the reactive store with the person's actions as queued intents, the request layer that performs them through the uarp-sdk client (creating the session before the first message when none is live), the follower of the live session stream (`follow.ts`), and a compact text form for tests |
| `src/ui/` | The console screen in TSX (L0 view protocol, v38, `console.tsx`): top bar, start banner with its loading and error states, the stitched timeline with date lines and history on scroll, the empty chat, the composer, the context panel, the settings and confirm dialogs and the toasts (`notices.ts`); Markdown as L0 nodes, interface strings; the stylesheet is `mockups/console.css`, imported as text |
| `src/tests/` | Tests executed by Varta itself (`varta.test.json`, one execution per group); `frames.ts`, `transcripts.ts` and `responses.ts` hold recorded v1 frames, transcripts and API responses; `sessions.ts` the session list of the stitching tests (composed from recorded bodies) and the recorded transcripts they serve |
| `fixtures/events-v1/` | The API lane's nine recorded `event_schema=1` session streams and their transcripts (anonymised; provenance in its README), read by the scenario tests through the virtual root `/project` |
| `src/tests/sdk/` | SDK client tests executed by Varta (`varta.sdk-test.json`): the real, unmodified `UarpClient` against the loopback mock, one execution per group; `contract.ts` holds the canned bodies, headers and timings both sides share |
| `src/tests/sdk-server/` | The loopback mock of the UARP API, itself a Varta HTTP/1 application (`varta.sdk-server.json`); `e2e.ts` replays the recorded scenarios for the console end to end, `stitch.ts` serves the session list and transcripts |
| `src/tests/app.test.ts` | The console end to end: the product's start, settings, stitching, request layer and follower with the real `UarpClient` against the mock, and the product manifest; `loopback.ts` is the instrumented `fetch` those tests pass to the client, `app-ceiling.json` the host ceiling budgets of the app groups |
| `src/tests/view/` | View tests rendered headlessly by `varta dev --view-script` (`varta.view.json`) and compared with the reviewed tree and accessibility snapshot: a timeline from recorded frames (`main.tsx`), and every console screen over recorded answers and settings in memory (`screens.tsx`) |
| `design/agent-studio.md` | Draft of Agent Studio (building a personal agent with Varta tools); built after the rest of the console works |
| `design/chat-timeline.md` | Draft model of the chat timeline: sources, entries, reducer, transport, session stitching |
| `scripts/test.sh` | Builds the product and the test entry once, starts the loopback mock and runs the Varta-executed tests group by group, then the view tests |
| `scripts/mock.sh` | Starts and stops the loopback mock for `test.sh` and `sdk-loopback.sh` |
| `scripts/check-sdk.sh` | `varta check` of every pinned SDK source file, one file at a time, then of the instantiated SDK generics |
| `scripts/check-sdk-generics.ts` | The entry that instantiates every generic function and method of the pinned SDK, used by `check-sdk.sh` |
| `scripts/sdk-loopback.sh` | Runs the loopback mock and the SDK client tests as two `varta dev` processes |
| `scripts/test-browser-sdk.mjs` | Records every SDK group natively against the loopback mock, replays it natively and in local Chrome from the program's own browser bundle, and requires identical lines and usage |
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

Checkpoint 2026-11-05, wave 3 review fixes (debug `varta` built from this
branch, whose crates equal `49186cf`): `scripts/test.sh` passes **249 tests in 36
groups and ten headless view checks**. New: the base URL must use https (saved and
stored; `app-settings` +3), the live session is the newest one only when active,
so segments stay in creation order (`chat-stitch` +3), a message after
`session_closed` creates a new session (`chat-requests` +4), and the product build
is searched for any string of the API key form. The view snapshots changed only
by the base URL help text ("Лише https://.").

Checkpoint 2026-11-05, wave 3 (debug `varta` of `49186cf` with this example's
working tree): `scripts/test.sh` passes **239 tests in 36 groups and ten headless
view checks**. New since the checkpoint below: `chat-stitch`, `ui-notices`,
`app-stitch` and `app-settings`, more cases in `chat-store` and `chat-requests`, the
`app-*` flows starting from the session list, and the eight console screens
`settings`, `settings-saved`, `empty`, `history`, `context`, `loading`, `error` and
`confirm` ([product entry](#product-entry-and-the-console-end-to-end)). The run
also found the test key in no output of `app-settings`, and the key the
`settings-saved` script types in neither its snapshot nor its log. (The run's check
of `dist/app.wasm` and `dist/app.manifest.json` for the test key could not fire:
the module stores string literals as UTF-16 code units. After the review it looks
for any string of the API key form with NUL bytes dropped, and a key-shaped
literal planted in the product makes it fail.) The product manifest's grants
are unchanged. `scripts/sdk-loopback.sh` still passes its six groups (43 cases) against the extended mock.

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
| Product entry | `src/app/` with `varta.json`: settings from KV, the real client, the Head Agent's stitched sessions, the request layer and the followed session stream with the stream.ts reconnect rules; send, approve, answer, drop, reconnect (also after a 503) and the ends after `session_closed`, a final `error`, `[DONE]` alone and a 401 pass end to end against the recorded scenarios (2026-11-04); giving up after connections without progress, a frame strict parsing refuses and a refused stream open (403, capability denial) with the real reason in the note pass end to end (2026-11-05) | Live-service validation, the view under scripted input with the live stream, cross-target acceptance |
| Session stitching | The newest session of the Head Agent, when active, as the live session, loaded from its transcript and followed without duplicates; older sessions loaded on scroll (or the button) one per request across pages, each prepended with its date line, a closed one as a note, all in creation order; the first message without a live session (also after `session_closed`) creates one; end to end against the mock (`app-stitch`), as a model (`chat-stitch`, `chat-store`, `chat-requests`) and headlessly (`history` view) (wave 3) | A recorded `GET /sessions` page (the pages are composed from recorded bodies); live-service validation; a window of about 30 entries (design) is not implemented, every loaded segment stays rendered |
| Settings and the other screens | Settings dialog (base URL, API key) saving through KV only, with validation (a key, and an https base URL: the key is never offered to a plaintext origin; a stored base URL without https stops the start) and "forget the key" behind the confirm dialog; the empty chat with suggestions; the context panel of runs and delegated agents mentioned in the conversation, with the context meter; loading and error banners; toasts. End to end for the settings (`app-settings`), as state (`ui-notices`) and in eight headless view checks (`settings`, `settings-saved`, `empty`, `history`, `context`, `loading`, `error`, `confirm`) (wave 3) | New settings apply at the next start once a session was reached (no in-place restart); the panel shows only what the timeline holds (no agent or run requests); copy buttons (`clipboard.write` is not granted); native rendering of these screens |

## Running the checks

```bash
VARTA_BIN=/path/to/varta scripts/test.sh [group…]
/path/to/varta build --config varta.json
VARTA_BIN=/path/to/varta scripts/check-sdk.sh
VARTA_BIN=/path/to/varta scripts/sdk-loopback.sh [group…]
node scripts/test-browser-sdk.mjs /path/to/varta [group…]
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

### The SDK in the browser host

`scripts/test-browser-sdk.mjs` runs every group of the table above three
times. First natively against the loopback mock as an application instance
that records its network completions (`varta dev --app --network-record`);
then natively again with the mock stopped, replaying that log
(`--network-replay`, no socket opens); then in local Chrome from the
program's own `varta build --target browser` bundle, mounted with the same
log as the embedder's `networkReplay`, so no browser-side mock exists. The
browser run must print the native replay's lines and use exactly its fuel,
host calls, microtasks, output bytes, turns and idle maintenance.

Checkpoint 2026-11-05 (debug `varta`, local Chrome 156): all six groups pass
(`sdk-unary` 8, `sdk-problem` 17, `sdk-retry` 6, `sdk-abort` 3, `sdk-timeout` 3,
`sdk-sse` 6; 43 cases). The native recording and its native replay agree on
every turn; their idle-maintenance fuel can differ (the recording idles while
the network answers), so Chrome is compared with the native replay. Firefox
and WebKit were not run.

## SDK own test sources

The npm tarball of `uarp-sdk` does not ship the package's own tests (its `files` are
`dist`, `src`, `README.md` and `LICENSE`; its test script is
`npm run build && node --test test/*.test.ts`). `pins/uarp-sdk-tests.json` pins them
at commit `9fc7776e7d78b17bf69baf5400dcfcb73123066b` of
`github.com/Snaga-AI/uarp-sdks` (tag `v0.13.2`) with the sha256 of each pinned file:
the three `packages/typescript/test/*.test.ts` files and the two
`contract/sse-fixtures/` files that `sse-parity.test.ts` reads from the repository
root. They are fetched into the git-ignored `pins/uarp-sdk-tests/` with their
repository paths, so the tests' relative fixture URL keeps working, and are never
vendored.

```sh
python3 scripts/fetch-sdk-tests.py           # fetch at the pinned commit, verify, install
python3 scripts/fetch-sdk-tests.py --check   # offline: verify the installed copy
python3 scripts/fetch-sdk-tests.test.py      # offline tests of the script
```

Fetching contacts only `https://github.com/Snaga-AI/uarp-sdks.git` (HTTPS only, no
redirects, credentials or caller git configuration) and fetches the commit by its
hash. Before the ignored directory is replaced, every pinned file must match its
sha256, and every file of the installed `node_modules/uarp-sdk` except `dist/` must
equal the same path under the commit's `packages/typescript`. That comparison is how
the commit was identified: all 68 such files of the 0.13.2 tarball equal `v0.13.2`,
while `v0.13.1` and `v0.13.3` differ in 3 and 9 files. A mismatch, a missing or extra
file, or a symlink fails closed and keeps the previous copy; `--check` applies the
same file rules offline.

Checkpoint 2026-11-05 (the main checkout's debug `varta`, built after `a842166`;
`49186cf` changes only `docs/`):
`varta check` of each test file as the entry of a `checked-f64-utf16-v38` project
that maps no `node:` module refuses all three. `stream.test.ts` and
`transport.test.ts` are refused at their first import, `node:assert/strict`
(1:20). `sse-parity.test.ts` is refused earlier, at parse time, on
`import.meta.url` (12:64), with the message that `import.meta` cannot be used
outside module code. Running the suite under Varta needs a harness (M7).

## Product entry and the console end to end

`varta build --config varta.json` builds the product entry `src/app/main.tsx` into
one `dist/app.wasm` and its manifest `dist/app.manifest.json`. The manifest
requests exactly:

| Grant | Scope | Why |
| --- | --- | --- |
| `net.request` | `https://api.snaga.ai/**` (`API_BASE_URL`), `GET` and `POST`, response headers `retry-after`, `x-correlation-id`, `x-request-id`, `x-should-retry` | The SDK's requests and the session stream; the four headers are the ones the SDK reads (retry timing, `APIError` ids) |
| `kv.read`, `kv.write` | `/data/console.kv` (`KEY_DATABASE`, root `data`) | The settings (`src/app/key.ts`): the API key and the base URL, written by the settings screen; `kv.write` also covers forgetting the key (`Kv.delete`) |
| `time.timer` | — | The SDK's `Transport.request` arms a `setTimeout` for its timeout on every request, so without it every request fails before the network (`NotAllowedError: <capability denied: time.timer> (setTimeout)`, observed) |
| `time.now` | — | The SDK's `EventStream` reads `Date.now` (its default clock) when each connection opens (`core/sse.ts`, `connectedAt`) |

**Owner decision (2026-11-05): the two `time` grants are accepted.** The brief
asked for `net.request` and `kv` only; the unmodified SDK arms `setTimeout` on every
request and reads `Date.now` on every stream connection, so the product manifest
requests `time.timer` and `time.now` as well, and nothing wider: the `app-manifest`
group fails on any other grant. `varta dev --config varta.json` maps
the `data` root to `data/` here (git-ignored except its `.gitignore`).

At start the console reads the settings from KV (the key, and the base URL, which
defaults to `API_BASE_URL`), constructs `UarpClient` with them, reads the tenant
(`tenants.getCurrentTenant`) and stitches its Head Agent's sessions
(`src/chat/stitch.ts`, design/chat-timeline.md "Session stitching"):

- `sessions.list({ agent_id, limit: 20 })` gives the newest page, newest first. Its
  first (newest) session is live when its `status` is `active`: the console reads
  its transcript (`sessions.getSessionMessages`), which alone holds the person's
  messages, and then follows it; the stream's replay of a run the transcript holds
  is dropped (`fromHistory`), so nothing shows twice. Every other session of the
  page, and of each further page behind the list's `cursor`, waits as history.
  Segments stay in creation order with the live one last, so when the newest
  session is closed or expired none is live, even if an older one is still active:
  that older session loads as history above the newer one, and the next message
  creates a new session below them (`chat-stitch`).
- Scrolling the timeline to its top (a `scroll` event at most 48 px from the top)
  or the "Показати раніші розмови" button loads the next older session as a history
  segment from its transcript, prepended before the segments shown; one session per
  load, the next page only once the queued sessions are used up. A closed session's
  transcript is gone (404): its segment shows a note. A failed load shows a toast and
  can be retried.
- Without a live session the console is ready: the first message opens a local
  segment at once, `sessions.create({ agent_id })` creates the session, the segment
  takes its id and day, the message is sent and then the session is followed.
  Messages sent meanwhile wait behind the creation; a failed creation marks them as
  not sent with the API's code and closes the segment, and the next message starts
  again. After `session_closed` the next message creates a new session the same way
  (`chat-requests`, through the dispatcher and the request layer; no end-to-end
  group sends after `session_closed`).
- Each segment starts with a quiet date line, the UTC day of its session's
  `created_at` ("3 січня 2026"; relative days such as "Сьогодні" in the mockup are
  not shown, since they would need the clock in the view).

The request layer is wired to the store's intents once the start has read the
session list, and the live session is followed (`src/chat/follow.ts`):

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
connection ends without `[DONE]` at its drop point (`DROP_AFTER`). The mock also
serves the Head Agent's session list (`src/tests/sdk-server/stitch.ts`), checking its
query: a recorded scenario lists no session, so the person's first message creates
one with the recorded `create_session`; a generated stream's scenario lists one live
session, `session-1`, with an empty transcript, which the console follows at once;
the stitch scenarios list the sessions of `src/tests/sessions.ts` and replay the
recorded plain scenario for the live stream, the creation and the message. No
`GET /sessions` page was recorded: those pages are composed in the list's served
shape (`{items, cursor, has_more}`) from the recorded `create_session` body, with
ids, status and times changed; the transcripts are the recorded ones, the runs of
each older session renamed after it (every recording names its run `run-1`).

| Group | What runs |
| --- | --- |
| `app-manifest` | The built `dist/app.manifest.json` requests exactly the grants above, with the `data` root only; `scripts/test.sh` also checks that neither it nor `dist/app.wasm` contains a string of the API key form (`uarp_<prefix>_<secret>`, the test key included), searching the module with its NUL bytes dropped because string literals are stored as UTF-16 code units |
| `app-start` | No stored key: no request. An ungranted database: `NotAllowedError` from `kv.read` before any effect. An ungranted origin (the product's own `fetch`): the SDK's `APIConnectionError` caused by the engine's `NotAllowedError` for `net.request`, which the reason shows, nothing followed. A tenant without a Head Agent: the tenant is read and no session is created. A key entry that does not decode as a string: the start reports the read's `TypeError`, and more such starts than the engine keeps databases open all report it, so the database is closed on that path too |
| `app-e2e` | `approval_approve` and `ask_user`: no session listed, so the first message creates one (`POST /sessions`, then the message, then the stream), the stream to the wait, the drop, approve or answer, the reconnect with the model's `Last-Event-ID`, the run to its end; the timeline equals the reviewed expectation of that recording (scenarios.test.ts) plus the person's message, and the requests come in the expected order. `session_end`: a drop mid-run, a reconnect the mock answers with a 503, the SDK's reconnect from the same event, then `session_closed` and `[DONE]` end following with `over`. A final `error` frame (`SESSION_NOT_FOUND`), `[DONE]` alone and a 401 end following without a reconnect. Connections that deliver only `connected`, and connections that deliver only a keep-alive comment, end with `gave_up` after `maxReconnects`. A run event missing a required field ends with `invalid` and the strict parser's `TypeError`. A stream open answered 403, or sent to an ungranted origin, ends with `refused` and the `PermissionDeniedError` or `NotAllowedError` after two requests; one answered 503 every time ends with `gave_up` and the `ServiceUnavailableError`. Each end checks the streams opened, the stream requests and the note with the real reason |
| `app-e2e-long` | One connection whose head the mock delays 200 ms (within the 250 ms window) and whose 100 content chunks (about 24 KB, several reads) are applied one per turn: the stream lasts longer than one per-turn deadline although no single wait does, every chunk is applied and `session_closed` + `[DONE]` end it |
| `app-stitch` | `stitch`: the newest page lists an active, an expired and a closed session, with a second page behind its cursor; the active one is followed, its timeline starts from its recorded transcript, and the stream's replay of the same run adds nothing (the reconnect resumes from the start, `- -`). Three loads, as three scrolls, prepend the expired session's transcript, the closed session's note (its transcript is recorded as 404) and, from the second page, the oldest session's transcript, each with its date; a fourth makes no request. `stitch-new`: an expired and a closed session only, so the console is ready; the first message creates the session, is sent, and the stream of the recorded plain scenario gives the reviewed timeline plus the message; a load then puts the expired session before it. Both check every request in order |
| `app-settings` | The product's settings store (`KvSettings`) on a test database, with the product's default base URL, which no test grant names: no key, no request; an empty key, a base URL without an HTTP scheme and a plaintext `http:` base URL (the mock's own origin) are refused and nothing is stored; a valid save of an https base URL writes the key and base URL to the database and starts the console with them (the test's fetch sends those requests on to the plain-http mock; the mock accepts only the test key); the key is in no state the screen shows (start state, base URL, tenant name, toasts, timeline); a later save applies at the next start; forgetting deletes the key and keeps the base URL; an https base URL no grant names is saved but denied by the engine (`NotAllowedError` for `net.request`); an `http:` base URL already in the database stops the start as `url_invalid` before any request, although the engine would allow the mock, and an https save then starts the console. `scripts/test.sh` checks that the group's output, logs included, does not contain the test key |

`app-reconnects` is a measurement, not part of the default run (below).

### Reconnects and memory

A wave-2 review found memory growing per reconnect in a test; the growth was the
test's own instrument (`GatedFetch` records every request). `app-reconnects`
measures the product follower without it: the engine's own `fetch`, the
connected-only stream (no progress), 1000 connections with waits of 1–4 ms until
the follower gives up. `scripts/test.sh app-reconnects` runs it; for the numbers,
run it with a `VARTA_BIN` that adds `--budget-report` to `varta run`.

Measured 2026-11-05 (debug `varta` of `49186cf`, this example's working tree, a
heavily loaded machine): the group passes, all 1000 connections are made and the
follower gives up as expected, in 5009 turns (about five per connection). The
instance's memory starts at 2,424,832 bytes and grows by about 45 KB per
connection (3,866,624 bytes after turn 205, 4,456,448 after turn 272: nine 64 KiB
pages over about 13 connections) although the idle collector completes a cycle
every seven or eight turns, and reaches the 8 MiB memory budget near turn 475
(about 95 connections). The remaining ~900 connections then run at the budget
without a failure, so what the follower keeps per connection is reclaimed: there is
no unbounded growth to fix in the product follower. Not established: why the heap
grows below the budget even after completed collection cycles (the engine's growth
policy or garbage it collects late); that question belongs to the core.

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
- **F8. Refused constructs met while writing the wave-3 screens and stitching**,
  each rewritten in the console:
  - `Array.prototype.unshift` (`list.unshift(y)` on an array of records):
    `unsupported call` (a new array is built instead, `prependSegment`);
  - a method call on a field of a call's result (`this.list(s).get()`, `get` a
    function field): `unsupported call` (a local binding first);
  - `Promise<never>` as a return type: `unsupported Promise value type never`
    (methods that only throw are `async` with their own result type);
  - `await` inside a `+` expression: `unsupported await inside \`+\` expression:
    the suspension foundation does not admit it yet` (awaited into a local first);
  - an empty array literal inside an object literal passed to `JSON.stringify`:
    `empty array requires an element type annotation` (the JSON text is written out);
  - a module constant whose initializer calls a function (`page(…)`,
    `String(PAGE_LIMIT)`): `unsupported call in a module constant initializer`
    (functions instead; F6 reports the same rule for a function reference);
  - a local binding named like a module-level function (`list`, `recorded`):
    `reserved binding` (renamed);
  - an import cycle made only of `import type`: `cyclic module dependency` (the
    shared interface moved to its own module, `sdk-server/responder.ts`).
- **F9. Two commits in one turn end the instance.** A view event turn commits
  right after its handler, before the handler's microtasks
  (`dispatchViewEvent` in the view prelude), so a microtask of that turn that
  commits pending operations again ends the instance. Observed with a minimal
  view (a click handler that changes a class, then a `Promise.resolve().then`
  that changes it again and calls `View.commit()`): `varta failed: view_commit
  called twice in one turn`. This is the engine's documented one-commit rule, not
  a defect, but it made the console's old microtask `commitSoon` unsafe once a
  click awaits an in-process promise (a history load in the view tests).
  `src/ui/commit.ts` now commits in a zero-delay timer turn of its own.
