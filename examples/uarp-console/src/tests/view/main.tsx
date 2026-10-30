// View test entry, rendered headlessly by `varta dev --view-script`;
// scripts/test.sh compares the final tree and accessibility snapshot with the
// reviewed expected output. TEST_VIEW picks the scenario. Data arrives one part
// per turn, as stream frames and transcripts do.
import type {
  GetSessionMessagesResponse,
  RunEventV1LLMChunk,
  RunEventV1RunAwaitingApproval,
  RunEventV1RunAwaitingInput,
  RunEventV1RunCancelled,
  RunEventV1RunCompleted,
  RunEventV1RunStarted,
  RunEventV1ToolCall,
  RunEventV1ToolResult,
} from "../../../node_modules/uarp-sdk/src/generated/models.js";
import { Runtime, effect } from "../../l1/reactive.js";
import {
  ask,
  askSent,
  onRunStarted,
  onChunk,
  onAwaitingApproval,
  onAwaitingInput,
  onRunCancelled,
  onRunCompleted,
  onToolCall,
  onToolResult,
  type Segment,
} from "../../chat/timeline.js";
import { loadTranscript } from "../../chat/history.js";
import { ChatStore } from "../../chat/store.js";
import { ChatView } from "../../ui/timeline.js";
import { Composer } from "../../ui/composer.js";
import { commitSoon } from "../../ui/commit.js";
import { installStyles } from "../../ui/styles.js";
import {
  PLAIN_STARTED,
  PLAIN_THINKING_1,
  APPROVE_AWAITING,
  APPROVE_STARTED,
  APPROVE_RESUMED,
  APPROVE_RESTARTED,
  APPROVE_TOOL_CALL,
  APPROVE_TOOL_RESULT,
  APPROVE_CONTENT,
  APPROVE_COMPLETED,
  ASK_AWAITING,
  ERROR_CANCELLED,
} from "../frames.js";
import { PLAIN_TRANSCRIPT } from "../transcripts.js";

const AGENT_NAME = "Chief of Staff";
const RECORDED_RUN = '"run_id":"run-1"';
const APPROVE_RUN = "run-2";
const QUESTION_RUN = "run-3";
const CANCELLED_RUN = "run-4";
const STREAM_RUN = "run-1";

function asRun(frame: string, runId: string): string {
  return frame.replace(RECORDED_RUN, '"run_id":"' + runId + '"');
}

/** Runs each step in its own timer turn, in order. */
function turns(steps: (() => void)[]): void {
  let next = 0;
  const step = (): void => {
    const run = steps[next]!;
    run();
    next += 1;
    if (next < steps.length) setTimeout(step, 0);
  };
  if (steps.length > 0) setTimeout(step, 0);
}

function chatScenario(store: ChatStore): void {
  const earlier = store.open("session-0");
  earlier.state = "history";
  store.change(earlier, null, () => loadTranscript(earlier, (JSON.parse(PLAIN_TRANSCRIPT) as GetSessionMessagesResponse).messages));
  let s: Segment = earlier;
  turns([
    () => {
      s = store.open("session-1");
      store.change(s, null, () => ask(s, "recorded-2", 'Call manage_permissions with action "get".'));
    },
    () => store.changeAsk(s, "recorded-2", () => askSent(s, "recorded-2", APPROVE_RUN)),
    () => store.change(s, APPROVE_RUN, () => onRunStarted(s, JSON.parse(asRun(PLAIN_STARTED, APPROVE_RUN)) as RunEventV1RunStarted)),
    () => store.change(s, APPROVE_RUN, () => onChunk(s, JSON.parse(asRun(PLAIN_THINKING_1, APPROVE_RUN)) as RunEventV1LLMChunk)),
    () => store.change(s, APPROVE_RUN, () => onAwaitingApproval(s, JSON.parse(asRun(APPROVE_AWAITING, APPROVE_RUN)) as RunEventV1RunAwaitingApproval)),
    () => store.change(s, null, () => ask(s, "recorded-3", "A or B?")),
    () => store.changeAsk(s, "recorded-3", () => askSent(s, "recorded-3", QUESTION_RUN)),
    () => store.change(s, QUESTION_RUN, () => onRunStarted(s, JSON.parse(asRun(PLAIN_STARTED, QUESTION_RUN)) as RunEventV1RunStarted)),
    () => store.change(s, QUESTION_RUN, () => onAwaitingInput(s, JSON.parse(asRun(ASK_AWAITING, QUESTION_RUN)) as RunEventV1RunAwaitingInput)),
    () => store.change(s, null, () => ask(s, "recorded-4", "Count slowly from 1 to 200.")),
    () => store.changeAsk(s, "recorded-4", () => askSent(s, "recorded-4", CANCELLED_RUN)),
    () => store.change(s, CANCELLED_RUN, () => onRunStarted(s, JSON.parse(asRun(PLAIN_STARTED, CANCELLED_RUN)) as RunEventV1RunStarted)),
    () => store.change(s, CANCELLED_RUN, () => onRunCancelled(s, JSON.parse(asRun(ERROR_CANCELLED, CANCELLED_RUN)) as RunEventV1RunCancelled)),
  ]);
}

/** The recorded approval run, one frame per turn, as the stream delivers it. */
function streamScenario(store: ChatStore): void {
  const s = store.open("session-1");
  store.change(s, null, () => {
    ask(s, "recorded-1", 'Call manage_permissions with action "get" and agent_id "agent-probe".');
    askSent(s, "recorded-1", STREAM_RUN);
  });
  const steps: (() => void)[] = [
    () => store.change(s, STREAM_RUN, () => onRunStarted(s, JSON.parse(APPROVE_STARTED) as RunEventV1RunStarted)),
    () => store.change(s, STREAM_RUN, () => onAwaitingApproval(s, JSON.parse(APPROVE_AWAITING) as RunEventV1RunAwaitingApproval)),
    () => store.change(s, STREAM_RUN, () => onRunStarted(s, JSON.parse(APPROVE_RESUMED) as RunEventV1RunStarted)),
    () => store.change(s, STREAM_RUN, () => onRunStarted(s, JSON.parse(APPROVE_RESTARTED) as RunEventV1RunStarted)),
    () => store.change(s, STREAM_RUN, () => onToolCall(s, JSON.parse(APPROVE_TOOL_CALL) as RunEventV1ToolCall)),
    () => store.change(s, STREAM_RUN, () => onToolResult(s, JSON.parse(APPROVE_TOOL_RESULT) as RunEventV1ToolResult)),
  ];
  for (const chunk of APPROVE_CONTENT) steps.push(() => store.change(s, STREAM_RUN, () => onChunk(s, JSON.parse(chunk) as RunEventV1LLMChunk)));
  steps.push(() => store.change(s, STREAM_RUN, () => onRunCompleted(s, JSON.parse(APPROVE_COMPLETED) as RunEventV1RunCompleted)));
  turns(steps);
}

export function main(): void {
  const rt = new Runtime();
  View.setEffect((run: () => void): void => {
    effect(rt, run);
  });
  installStyles();
  const store = new ChatStore(rt);
  store.afterChange = commitSoon;
  if (varta.env.get("TEST_VIEW") === "stream") streamScenario(store);
  else chatScenario(store);
  View.mount(
    <main id="app" class="main">
      <ChatView store={store} agentName={AGENT_NAME} />
      <Composer store={store} agentName={AGENT_NAME} />
    </main>,
  );
}
