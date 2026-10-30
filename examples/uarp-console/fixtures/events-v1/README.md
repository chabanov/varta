# event_schema=1 fixtures, recorded on e2e-canon (2026-10-30)

Nine Single Chat scenarios recorded from the production session stream
(`GET /api/v1/sessions/{id}/events`, build 34582ffb) on the e2e-canon tenant,
with the owner's word in the API lane's session. Each frame was then
projected to `event_schema=1` with the registry in
`packages/types/run-event-schema.ts` on branch `session/event-schema`
(df4e35176), the same function the server applies when a client sends
`?event_schema=1`. Production does not serve v1 yet; until it does, these
frames are what the server will send, byte-shaped by the same code.

Anonymised: every UUID became a stable placeholder per file (`run-1`,
`session-1`, `evt-N`, `agent-head`, `agent-research`, `agent-probe`,
`tenant-1`), and every timestamp is shifted so the file starts at
2026-01-01T00:00:00Z with the real offsets kept. LLM text is as produced.

Checked: every frame passes a strict v1 parse (no unknown field, no unknown
value), and no field production wrote for a v1 type is outside the registry.

Each file: `requests` (the HTTP calls made, with status and body),
`frames` (the v1 stream, in order), `not_sent_in_v1` (frames production sent
that v1 does not carry: run.affect, streaming.*, llm.response).

`session_end.json` was re-recorded on build 43f78a92 (2026-10-30 03:4xZ),
where the fix for the stream that stayed silent after `DELETE /sessions/{id}`
is live: the stream now ends with `session_closed` and `[DONE]`. The other
eight files are from build 34582ffb; v1 projects them identically.

## Transcripts: `<scenario>.messages.json`

`GET /api/v1/sessions/{sessionId}/messages` for each scenario's session, read
on production (build 3d07d69d, 2026-10-30 13:xxZ) after the scenario had run,
with the owner's key for e2e-canon; reads only. Anonymised with the SAME
placeholders and time shift as the scenario's own file: the mapping was
recovered by aligning the raw recording with that file's `requests` and
`frames`, so `run-1` here is `run-1` there, and the child run of `delegation`
is `id-2` in both. Each file: `status` and `body` as the API answered.

What the bytes show (one tenant, one recording each — "happens", not "always"):
- One entry per human turn (`message_id` = `{run_id}`) and one per answer
  (`{run_id}-reply`). No `{run_id}-tool-N` entry in any of the nine: tool
  calls sit on the answer as `tool_calls[]` (`id`, `name`, `status`, `input`
  and `output` as JSON TEXT).
- `thinking` is kept on the answer (plain, reasoning, tool_call, ask_user,
  approval_approve, delegation).
- A run that ended without an answer leaves only its human turn:
  approval_reject (rejected) and error (cancelled) have one entry.
- ask_user: the person's reply through `POST /runs/{id}/respond` is not a turn
  of its own; the run's one answer follows the question.
- `messages` and `items` are the same list; `total` is its length.
- session_end: after `DELETE /sessions/{id}` the transcript is gone —
  `404` `code: NOT_FOUND`, and `GET /sessions/{id}` is 404 too.

## Re-projected on build bd5dc091 (PR #626, 2026-10-30)

v1 now names an approval call's arguments `arguments` (as `tool.call` does)
and keeps `args` beside them, deprecated; `error_code` on run.failed,
run.cancelled and run.guardrail_blocked is a closed list. All nine files were
projected again from the same raw recordings with the registry of bd5dc091 —
no new run on production. Two files changed, by one field each:
`approval_approve.json` and `approval_reject.json`, whose
`run.awaiting_approval` frame gained `tool_calls[0].arguments` (equal to
`args`). The other seven came out byte-identical, so they were left as they
were. Every frame of all nine still passes a strict v1 parse.
