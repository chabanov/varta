// Whole-scenario tests: each recorded session stream (fixtures/events-v1, read
// through the virtual root /project) is applied frame by frame through the
// dispatcher, as the wire delivers it, and the resulting timeline is compared
// with the reviewed compact form. Frames are applied a few per turn, as the
// stream would deliver them.
import type { JsonObject, JsonValue } from "../../node_modules/uarp-sdk/src/core/json.js";
import { Runtime } from "../l1/reactive.js";
import { ChatStore } from "../chat/store.js";
import { applyFrame, type FrameOutcome } from "../chat/dispatch.js";
import { show } from "../chat/show.js";
import { equal, type Report } from "./check.js";

const FIXTURES = "/project/fixtures/events-v1/";
const FRAMES_PER_TURN = 8;
const TERMINATOR_EVENT = "message";

interface RecordedFrame {
  event: string;
  id?: string;
  data: JsonValue;
}

interface Recording {
  scenario: string;
  description: string;
  stream: string;
  recorded: string;
  not_sent_in_v1: JsonObject;
  requests: JsonValue[];
  frames: RecordedFrame[];
}

function nextTurn(): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(() => resolve(), 0);
  });
}

/** The data text the wire carried: the terminator as is, every other frame as compact JSON. */
function wireData(frame: RecordedFrame): string {
  const data = frame.data;
  if (frame.event === TERMINATOR_EVENT && typeof data === "string") return data;
  return JSON.stringify(data);
}

function outcomeName(o: FrameOutcome): string {
  return o.kind === "error" ? "error " + o.frame.code : o.kind;
}

/** Applies a recording; returns the timeline's compact form and the stream's last outcome. */
async function replay(name: string): Promise<string> {
  const recording = JSON.parse(varta.fs.read(FIXTURES + name + ".json")) as Recording;
  const store = new ChatStore(new Runtime());
  const s = store.open("session-1");
  let last = "none";
  for (let i = 0; i < recording.frames.length; i++) {
    if (i > 0 && i % FRAMES_PER_TURN === 0) await nextTurn();
    const frame = recording.frames[i]!;
    last = outcomeName(applyFrame(store, s, frame.event, wireData(frame)));
  }
  return show(store.timeline) + " " + last;
}

async function scenario(r: Report, name: string, expected: string): Promise<void> {
  equal(r, "scenario: " + name, await replay(name), expected);
}

/**
 * Expected timelines, derived independently of the console's code from the
 * recordings by the reviewer's reference script, then checked by hand.
 */
const EXPECTED: { [scenario: string]: string } = {
  plain: "(timeline (context 42343/524288) (segment live (turn completed (thinking 200) (answer \"ok\")))) applied",
  reasoning: "(timeline (context 50968/524288) (segment live (turn completed (thinking 635) (answer \"391\")))) applied",
  tool_call: "(timeline (context 29812/524288) (segment live (turn completed (thinking 298) (answer \"2026-01-01T00:00:02.838Z UTC\") (tool now done {} 5ms \"2026-01-01T00:00:03.537Z\")))) applied",
  delegation: "(timeline (context 30544/524288) (segment live (turn completed (thinking 339) (answer \"ok\") (tool start_run done {\"agent_id\":\"agent-research\",\"input\":{\"message\":\"Reply with: ok\"}} 18ms \"{\\\"run_id\\\":\\\"id-2\\\",\\\"agent_id\\\":\\\"agent-research\\\",\\\"async\\\":false,\\\"summary\\\":\\\"Started run for agent agent-research\\\"}\") (tool get_run_result done {\"run_id\":\"id-2\",\"wait_ms\":60000} 517ms \"{\\\"run_id\\\":\\\"id-2\\\",\\\"status\\\":\\\"completed\\\",\\\"output\\\":{\\\"response\\\":\\\"ok\\\"},\\\"metrics\\\":{\\\"steps_count\\\":1,\\\"input_tokens\\\":25345,\\\"output_tokens\\\":30,\\\"thinking_tokens\\\":0,\\\"tool_calls_count\\\":0,\\\"llm_calls_count\\\":1,\\\"guardrail_checks\\\":0,\\\"guardrail_violations\\\":0,\\\"memory_retrievals\\\":0,\\\"memory_extractions\\\":0,\\\"context_window_tokens\\\":524288,\\\"last_input_tokens\\\":25345,\\\"duration_ms\\\":1886,\\\"total_cost_usd\\\":0.005343449999999999,\\\"pricing_confidence\\\":\\\"inferred\\\"},\\\"terminal\\\":true,\\\"summary\\\":\\\"Run completed\\\"}\") (child \"Canon Research Agent\" completed \"ok\")))) applied",
  approval_approve: "(timeline (context 31420/524288) (segment live (turn completed (thinking 428) (answer \"Result: no permission record exists for that agent (`exists: false`, empty permissions).\") (tool manage_permissions done {\"action\":\"get\",\"agent_id\":\"agent-probe\"} 12ms \"{\\\"agent_id\\\":\\\"agent-probe\\\",\\\"exists\\\":false,\\\"permissions\\\":{}}\") (approval approved (call manage_permissions {\"action\":\"get\",\"agent_id\":\"agent-probe\"}))))) applied",
  approval_reject: "(timeline (segment live (turn failed (thinking 310) (approval rejected (call manage_permissions {\"action\":\"get\",\"agent_id\":\"agent-probe\"})) (failure rejected \"Not now.\" approval_rejected)))) applied",
  ask_user: "(timeline (context 51036/524288) (segment live (turn completed (thinking 323) (answer \"A\") (question sent \"A or B?\" \"A\" \"B\")))) applied",
  error: "(timeline (segment live (turn cancelled (failure cancelled \"Stopped by a person.\" RUN_CANCELLED)))) applied",
  session_end: "(timeline (context 43880/524288) (segment closed (turn completed (thinking 98) (answer \"bye\")))) done",
};

export async function scenarioTests(r: Report, name: string): Promise<void> {
  const expected = EXPECTED[name];
  if (expected === undefined) {
    equal(r, "scenario: " + name, "unknown scenario", "a recorded scenario");
    return;
  }
  await scenario(r, name, expected);
}
