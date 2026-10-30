# Chat timeline model

The Single Chat shows one stream: the conversation with the tenant's Head Agent
(`Tenant.head_agent_id`) across all of its sessions. This document fixes the
model that stream is built from, before any code. It is grounded in the nine
recorded `event_schema=1` scenarios and the closed v1 types of uarp-sdk 0.13.0.

## Sources

| Source | Gives | API (uarp-sdk 0.13.0) |
| --- | --- | --- |
| Session list | The Head Agent's sessions, newest first, cursor-paged | `sessions.listAgentSessions(head_agent_id)` |
| Transcript | One session's finished messages, whole (no paging) | `sessions.getSessionMessages(session_id)` |
| Live stream | Run events of the active session | `sessions.streamSessionEvents(session_id, {event_schema: 1})` |
| Local input | The person's message between the POST and `run_added` | `sessions.sendSessionMessage` → `{run_id}` |

The person's own message never appears in the stream (`run.started` carries no
input in any recording). It comes from the local send, and after a reload from
the transcript.

## Identity

Everything is keyed by `run_id`. The transcript already derives its message
ids from it: the person's turn is `{run_id}`, the first reply
`{run_id}-reply`, later ones `{run_id}-reply-2`…, tool results
`{run_id}-tool-N`. A live turn and the same turn loaded from the transcript
therefore have the same key, and a turn is never shown twice.

## Model

```ts
interface Timeline {
  segments: Segment[];          // oldest → newest; the last one may be live
  olderCursor: string | null;   // next page of older sessions, null when none
}

interface Segment {
  sessionId: string;
  createdAt: string;
  state: "live" | "closed" | "history";
  entries: Entry[];             // in run order
}

type Entry = Ask | Turn | Note;

interface Ask {                 // the person's message
  kind: "ask";
  id: string;                   // run_id, or a local id until the POST answers
  text: string;
  state: "sending" | "sent" | "failed";
}

interface Turn {                // one run of the Head Agent
  kind: "turn";
  runId: string;
  state: TurnState;
  lastSeq: number;              // dedupe and Last-Event-ID
  thinking: string;             // chunk_type "thinking"
  answer: string;               // chunk_type "content"; replaced by run.completed output
  tools: ToolCard[];            // in call order
  children: ChildCard[];        // delegated runs
  wait: Wait | null;            // what the run waits for from the person
  failure: Failure | null;
}

type TurnState =
  | "queued" | "running" | "awaiting_approval" | "awaiting_input" | "paused"
  | "completed" | "failed" | "cancelled" | "blocked";

interface ToolCard {
  callId: string;
  name: string;
  arguments: string;            // compact JSON text, for display only
  state: "running" | "done" | "error";
  result: string;               // preview text
  latencyMs: number | null;
}

type Wait = Approval | Question;
interface Approval { kind: "approval"; calls: ToolCard[]; decision: "open" | "sending" | "approved" | "rejected" }
interface Question { kind: "question"; callId: string; text: string; options: string[]; answer: "open" | "sending" | "sent" }

interface ChildCard { childRunId: string; agentName: string; state: "completed" | "failed"; preview: string }
interface Failure { reason: "error" | "rejected" | "cancelled" | "blocked"; message: string; code: string | null }

interface Note { kind: "note"; text: string }   // context compacted, guardrail block
```

Every union here is closed and small, so it stays within the core's limits
independently of the 24-member `RunEventV1`.

## Reducer

One pure function per frame, `apply(timeline, frame) → timeline`, with no
clock, network or randomness. The same frames always give the same timeline,
which is what the tests check.

| Frame | Effect |
| --- | --- |
| `connected` | Marks the segment live; `runs` lists the runs the server will replay |
| `run_added` | Adds a `Turn` (`queued`) and binds the pending `Ask` to its `run_id` |
| `run.started` | `running`; idempotent (approve sends it twice in a row, `resumed_from` then empty) |
| `llm.chunk` `thinking` / `content` | Appends `delta` to `thinking` / `answer` |
| `llm.chunk` `tool_call` | Not shown: the arguments arrive whole in `tool.call` |
| `tool.call` | Adds a `ToolCard` (`running`) keyed by `tool_call.id` |
| `tool.result` / `tool.error` | Completes the card with `tool_call_id` |
| `run.awaiting_approval` | `Wait` = `Approval` over `tool_calls` |
| `run.awaiting_input` | `Wait` = `Question` (`question`, `options`) |
| `run.input_received` | Question answered; the run continues |
| `child_run.*` | Adds a completed or failed `ChildCard` (no recording shows a running child before it) |
| `run.completed` | `completed`; `answer` := `output.response` (authoritative, covers skipped chunks) |
| `run.failed` / `run.cancelled` / `run.guardrail_blocked` | Terminal state and `Failure` from `error` / `error_code` |
| `run.paused` | `paused` |
| `session.context` | Context meter in the header, not an entry |
| `session.context_compacted` | `Note` |
| `step.*`, `run.step_*`, `llm.error`, `team.delegate` | No entry in v1; they only advance the turn's seq |
| `run_done` | Confirms the terminal state |
| `session_closed` | Segment `closed`; the next message opens a new session |
| `error` | By `code`: `*_NOT_FOUND` is final, `STREAM_ERROR` reconnects |

A run event whose `seq` is not above its turn's `lastSeq` is dropped. This
makes replay after a reconnect, which resends every other run of the session
from its start, harmless.

## Transport

- Reconnect with `Last-Event-ID: <runId>:<lastSeq>` of the last run event
  received; the server replays that run after it and every other run from the
  start, and the reducer drops what it already has.
- `[DONE]` after `session_closed`: over. After an `error`: its `code` decides.
  With nothing before it: the credential is no longer valid. A stream that ends
  without `[DONE]` (two-hour cap, dropped connection) reconnects.
- The session stream sends no `gap`; the run stream's rule is not needed here.

## Session stitching

- On start: the newest page of the Head Agent's sessions; the newest one that is
  not closed or expired becomes the live segment, the rest load as `history` on
  scroll up (`olderCursor`).
- A history segment comes from the transcript (recorded for all nine
  scenarios): `user` → `Ask` with the run id; `assistant` → completed `Turn`
  with its `thinking`, `answer` and `tool_calls` (input and output are JSON
  text); a later reply of the same run continues the answer; `system` and
  `tool_result` → `Note` (no recording has either). A rejected or cancelled run
  leaves only its `Ask`. A closed session's transcript is gone (404).
- A turn loaded from a transcript is final: the stream's replay of that run is
  dropped, so the live session is loaded from its transcript (which alone has
  the person's messages) and then followed live without duplicates.
- Sending with no live session creates one first (`sessions.create` with the
  Head Agent), then sends.
- A segment boundary shows as a quiet date line, not as a separate chat.

## Rendering budget

- The view holds a window of about 30 entries around the scroll position;
  finished entries are immutable and keyed by id, so the keyed list only diffs
  the window.
- Markdown is parsed per finished block: a blank line outside a fence resets the
  parser, so blocks before the last one are parsed once and kept, and each chunk
  reparses only the text after it (`src/markdown/stream.ts`). A whole parse of a
  111-character reply costs about 190k fuel today.
- Chunk deltas are coalesced per host turn before they reach the signals.
- Thinking is collapsed by default ("Thinking…"), expandable (owner's decision).
- Every tool call is shown, compactly: one line with the tool name and its state;
  arguments and result open on demand (owner's decision). Approvals and failures
  are full cards.

## Tests (executed by Varta)

- Each of the nine scenarios → the expected compact text form of the timeline,
  in the style of `src/markdown/show.ts`.
- Replay: the same stream twice, and a reconnect from the middle of each
  scenario, give the same timeline as one clean pass.
- Strict parsing: an unknown event type, an unknown field or a wrong
  discriminant is a recoverable error, never a silently dropped frame.

## Raised with the API lane

`error_code` is an open string and its casing differs (`approval_rejected` in
`run.failed`, `RUN_CANCELLED` in `run.cancelled`); `run.awaiting_approval`
names the tool arguments `args`, `tool.call` names them `arguments`. Until the
contract answers, `Failure.code` is kept as text and shown to no one.

The API lane's answer: `error_code` comes from the same code vocabulary as
`Error.code`, and casing stays as it is. What a failure means is decided by the
event, not by the code: `run.cancelled` is cancelled, `run.failed` with
`rejected: true` is rejected by the person, any other `run.failed` is an error,
and the code only refines it. The model follows this rule already now.

## Implemented

- `src/chat/timeline.ts`: the model and one handler per closed v1 type;
  `src/chat/show.ts`: its compact text form.
- `src/chat/stream.ts`: Last-Event-ID and what to do when the stream ends.
- `src/chat/history.ts`: a transcript as a segment.
- `src/markdown/stream.ts`: incremental Markdown for a streaming answer.
- Tests executed by Varta on recorded frames (`src/tests/frames.ts`) and
  transcripts (`src/tests/transcripts.ts`), each parsed strictly into its
  uarp-sdk type: answers, replay, skipped chunks, cancellation, session close,
  approvals, a question, a delegated child run, stream ends, history and its
  merge with the live stream, every streamed Markdown prefix against a whole
  parse, and strict rejections. Key checks were confirmed by mutation.

## Screen

- `src/chat/store.ts` gives the view reactive access: a signal per segment's
  entry list and a version per entry, bumped by `change` after a handler runs.
  The person's actions (`send`, `decide`, `answer`) change the model at once
  ("надсилається…") and queue one intent each for the request layer.
- `src/ui/timeline.tsx` renders segments and entries as they are appended (an
  addition costs only the new entry), copies each turn into one signal per
  aspect so a content chunk re-renders only the prose and a tool result only its
  line, and streams the answer through `MarkdownStream` (stable blocks keep their
  nodes). `src/ui/composer.tsx`: Enter sends, Shift+Enter is a newline, IME
  composition never sends.
- View tests (`src/tests/view/`) deliver recorded frames one per turn, as the
  stream does, and compare the final tree and accessibility snapshot.
- Measured under v38 (cb6795d), one 1M-fuel turn each: a content chunk
  100–150k fuel and 33 host calls; a new turn's node about 400k; an approval
  card about 250k; the person's message 50–130k; a click 55–130k.

- `src/chat/requests.ts` performs intents one at a time, in queue order,
  through `ChatApi`: the part of the uarp-sdk client the chat uses
  (`sessions.sendSessionMessage`, `runs.approveRun`/`rejectRun`/`respondToRun`),
  so the client itself is passed in. A send binds the message to its run; a
  decision or an answer waits for the stream to confirm it; a failed request
  reopens what it changed and adds a note with the API's code (`APIError.code`).
- The store's `afterChange` commits the view after changes that do not happen in
  a view event turn (timers, request completions).

Next: the real client and stream (after the SDK upload fix and `ReadableStream`),
the stylesheet through the CSS text import (`mockups/console.css` stays the one
source), scripted clicks and typing on timer-delivered content once
`--view-script` orders inputs after due timers deterministically.

## Dispatch and scenarios

`src/chat/dispatch.ts` takes a session-stream frame as the wire delivers it (the
SSE event name and the data text), parses it strictly into its closed uarp-sdk
type, checks that a run event carries the type its name announces, and applies
it through the store; an unknown name, field or discriminant throws. The nine
recorded scenarios (`fixtures/events-v1/`) are replayed from their files, eight
frames per turn, and each final timeline equals one derived independently of
this code from the same recordings.

## Waiting for

- `typeof Blob` in the SDK transport folded to a constant (v38, in the core):
  the real client and the live stream.
- The browser host for v38 (this lane).
