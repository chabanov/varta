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
  and is never vendored into the repository. The console imports its closed v1
  types type-only from `node_modules/uarp-sdk/src/generated/models.ts`.
- The application uses Varta only through its public tooling and contracts
  (`varta check`, `varta build`, the embedding contract and the L0 view protocol).
  Where Varta cannot yet compile or run something, the gap is reported to the
  core, not worked around here.

## Layout

| Path | Content |
| --- | --- |
| `src/markdown/` | Deterministic Markdown subset for chat replies, parsed into a closed tree of blocks and inline nodes that maps onto L0; incremental parsing for streaming answers |
| `src/l1/` | L1 reactive core: signals, lazy computeds, effects, batches and keyed lists over an explicit runtime; accessors are functions, as in SolidJS |
| `src/chat/` | Chat timeline model: the dispatcher of session-stream frames as the wire delivers them and one handler per closed v1 event type, stream resume and end rules, transcripts as history, the reactive store with the person's actions as queued intents, the request layer that performs them through the uarp-sdk client, and a compact text form for tests |
| `src/ui/` | Single Chat screen in TSX (L0 view protocol, v38): the live timeline, the composer, Markdown as L0 nodes, interface strings; the stylesheet is `mockups/console.css`, imported as text |
| `src/tests/` | Tests executed by Varta itself (`varta.test.json`, one execution per group); `frames.ts`, `transcripts.ts` and `responses.ts` hold recorded v1 frames, transcripts and API responses |
| `fixtures/events-v1/` | The API lane's nine recorded `event_schema=1` session streams and their transcripts (anonymised; provenance in its README), read by the scenario tests through the virtual root `/project` |
| `src/tests/view/` | View tests: a timeline from recorded frames rendered headlessly by `varta dev --view-script` (`varta.view.json`), compared with the reviewed tree and accessibility snapshot |
| `design/agent-studio.md` | Draft of Agent Studio (building a personal agent with Varta tools); built after the rest of the console works |
| `design/chat-timeline.md` | Draft model of the chat timeline: sources, entries, reducer, transport, session stitching |
| `scripts/test.sh` | Runs the Varta-executed tests group by group |
| `mockups/` | Static Single Chat screens in plain HTML/CSS restricted to the L0 v1 subset. `console.css` is the guest stylesheet, `frame.css` the embedding page, `lint.html` checks every screen against the generated subset tables |
| `mockups/conformance/` | Renders every screen through the L0 applier and compares structure and boxes with the mockup |

The browser host, the L0 applier and the subset rules the linter uses live in
[`../varta-web-host`](../varta-web-host/README.md); this application depends on
them, never the other way round. L1 and the Markdown parser stay in `src/` until
Varta projects can depend on packages outside their own folder.

Secrets such as the UARP API key live in the ignored `.env.local`, never in the
repository.

## Status

| Piece | State | Waits for |
| --- | --- | --- |
| L0 conformance | Every mockup rendered by the applier matches it in structure and boxes (±1 px) at three sizes | The headless and native renderers for the cross-renderer criterion |
| Static mockups (Single Chat) | Five screens in `mockups/`; 0 subset violations per `mockups/lint.html` | Owner review |
| Markdown subset | Compiled and tested by Varta (v38), including incremental parsing while streaming; a 99-character reply parses in about 59k fuel | — |
| L1 reactive core and keyed lists | Compiled and tested by Varta (v38); a 1M-fuel turn fits 28 rows mounted, 16 updated | v38 wave 2 codegen hotspots |
| Chat model | Every frame of the session stream dispatched and parsed strictly into the closed v1 types; the nine recorded scenarios replayed from their files match an independently derived timeline; resume and end rules, history and its merge with the live stream, the store, intents and the request layer | — |
| Single Chat screen | Live timeline over the store with the stylesheet, checked headlessly and deterministically (tree with cascaded styles, accessibility snapshot): thinking toggle, tool lines, approval, question and failure cards with their actions, streaming Markdown, the composer | The browser host for v38 (this lane, next) |
| Real requests to UARP | Blocked: the SDK client stops at `typeof Blob !== 'undefined'` in `core/transport.ts` | `typeof` of a known engine global folded to a constant (v38, in progress in the core) |

## Running the checks

```bash
VARTA_BIN=/path/to/varta scripts/test.sh
python3 ../varta-web-host/scripts/serve.py 8766
```

Then open `http://127.0.0.1:8766/uarp-console/mockups/lint.html` and
`http://127.0.0.1:8766/uarp-console/mockups/conformance/conformance.html`.
