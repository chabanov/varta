// Session history tests on recorded transcripts: a transcript becomes a segment
// keyed by run id, and the live stream's replay of a run already in it is dropped.
import type { GetSessionMessagesResponse, RunEventV1LLMChunk, RunEventV1RunCompleted, RunEventV1RunStarted } from "../../node_modules/uarp-sdk/src/generated/models.js";
import { timeline, openSegment, onRunStarted, onChunk, onRunCompleted, type Segment } from "../chat/timeline.js";
import { loadTranscript } from "../chat/history.js";
import { showSegment } from "../chat/show.js";
import { PLAIN_STARTED, PLAIN_THINKING_1, PLAIN_CONTENT, PLAIN_COMPLETED } from "./frames.js";
import { PLAIN_TRANSCRIPT, TOOL_CALL_TRANSCRIPT, REJECT_TRANSCRIPT } from "./transcripts.js";
import { equal, type Report } from "./check.js";

const SESSION_ID = "session-1";
const NEXT_RUN = "run-2";

function loaded(transcript: string): Segment {
  const s = openSegment(timeline(), SESSION_ID);
  s.state = "history";
  loadTranscript(s, (JSON.parse(transcript) as GetSessionMessagesResponse).messages);
  return s;
}

export function historyTests(r: Report): void {
  const plain = loaded(PLAIN_TRANSCRIPT);
  equal(r, "history: plain answer", showSegment(plain), '(segment history (ask sent "Reply with the single word: ok") (turn completed (thinking 200) (answer "ok")))');

  const tool = loaded(TOOL_CALL_TRANSCRIPT);
  equal(
    r,
    "history: tool call on the answer",
    showSegment(tool),
    '(segment history (ask sent "Call the `now` tool and tell me the current UTC time in one line.") (turn completed (thinking 148) (answer "2026-01-01T00:00:02.838Z UTC") (tool now done {} "2026-01-01T00:00:03.537Z")))',
  );

  const rejected = loaded(REJECT_TRANSCRIPT);
  equal(r, "history: a rejected run leaves only its question", showSegment(rejected), '(segment history (ask sent "Call manage_permissions with action \\"get\\" and agent_id \\"agent-probe\\". Do not call anything else."))');
}

export function historyMergeTests(r: Report): void {
  const s = loaded(PLAIN_TRANSCRIPT);
  s.state = "live";
  const before = showSegment(s);
  onRunStarted(s, JSON.parse(PLAIN_STARTED) as RunEventV1RunStarted);
  onChunk(s, JSON.parse(PLAIN_THINKING_1) as RunEventV1LLMChunk);
  onChunk(s, JSON.parse(PLAIN_CONTENT) as RunEventV1LLMChunk);
  onRunCompleted(s, JSON.parse(PLAIN_COMPLETED) as RunEventV1RunCompleted);
  equal(r, "history: the stream's replay of a loaded run is dropped", showSegment(s), before);

  onRunStarted(s, JSON.parse(PLAIN_STARTED.replace('"run_id":"run-1"', '"run_id":"' + NEXT_RUN + '"')) as RunEventV1RunStarted);
  onChunk(s, JSON.parse(PLAIN_CONTENT.replace('"run_id":"run-1"', '"run_id":"' + NEXT_RUN + '"')) as RunEventV1LLMChunk);
  equal(r, "history: a new run streams after the loaded ones", showSegment(s), before.slice(0, before.length - 1) + ' (turn running (answer "ok")))');

  let outcome = "accepted";
  try {
    loaded(PLAIN_TRANSCRIPT.replace('"compacted":false', '"compacted":false,"surprise":1'));
  } catch (error) {
    outcome = "rejected";
  }
  equal(r, "history: an unknown transcript field is rejected", outcome, "rejected");
}
