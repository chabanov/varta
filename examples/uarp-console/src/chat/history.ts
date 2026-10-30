// A session's finished transcript (GET /api/v1/sessions/{sessionId}/messages)
// as a timeline segment. Entries are keyed by run id like the live stream, so a
// session loaded from its transcript and then followed live shows each run once:
// the stream's replay of a run already in the transcript is dropped.
//
// Recorded transcripts show one entry per human turn (`message_id` = run id) and
// one per answer (`{run_id}-reply`), with the run's tool calls and thinking on
// the answer. A run that ended without an answer (rejected, cancelled) leaves
// only its human turn.
import type { ConversationEntry, ConversationEntryToolCall } from "../../node_modules/uarp-sdk/src/generated/models.js";
import { addNote, type Segment, type Turn, type ToolCard } from "./timeline.js";

const REPLY_SEPARATOR = "\n\n";

function text(entry: ConversationEntry): string {
  const content = entry.content;
  if (typeof content === "string") return content;
  let out = "";
  for (const part of content) {
    if (part.text !== undefined) out += part.text;
  }
  return out;
}

function tool(call: ConversationEntryToolCall): ToolCard {
  const failed = call.status === "error";
  const result = failed ? (call.error === undefined ? "" : call.error) : call.output === undefined ? "" : call.output;
  return {
    callId: call.id,
    name: call.name,
    arguments: call.input === undefined ? "" : call.input,
    state: failed ? "error" : "done",
    result: result,
    latencyMs: call.duration_ms === undefined ? null : call.duration_ms,
  };
}

function findTurn(s: Segment, runId: string): Turn | undefined {
  for (const e of s.entries) {
    if (e.kind === "turn" && e.runId === runId) return e;
  }
  return undefined;
}

/** A later reply of the same run continues its answer after a blank line. */
function answer(s: Segment, entry: ConversationEntry): void {
  const existing = findTurn(s, entry.run_id);
  if (existing !== undefined) {
    existing.answer = existing.answer === "" ? text(entry) : existing.answer + REPLY_SEPARATOR + text(entry);
    return;
  }
  const tools: ToolCard[] = entry.tool_calls === undefined ? [] : entry.tool_calls.map(tool);
  s.entries.push({
    kind: "turn",
    runId: entry.run_id,
    state: "completed",
    lastSeq: 0,
    thinking: entry.thinking === undefined ? "" : entry.thinking,
    answer: text(entry),
    tools: tools,
    children: [],
    wait: null,
    failure: null,
    fromHistory: true,
  });
}

/** Appends a transcript to a segment; `state` stays as the caller set it. */
export function loadTranscript(s: Segment, entries: ConversationEntry[]): void {
  for (const entry of entries) {
    switch (entry.role) {
      case "user":
        s.entries.push({ kind: "ask", key: entry.message_id, id: entry.message_id, text: text(entry), state: "sent" });
        break;
      case "assistant":
        answer(s, entry);
        break;
      case "system":
      case "tool_result":
        addNote(s, text(entry));
        break;
    }
  }
}
