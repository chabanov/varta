// Store tests: the person's actions change the model at once and queue exactly
// one intent each, for the request layer to perform.
import type { RunEventV1RunAwaitingApproval, RunEventV1RunAwaitingInput } from "../../node_modules/uarp-sdk/src/generated/models.js";
import { Runtime, untracked } from "../l1/reactive.js";
import { onAwaitingApproval, onAwaitingInput } from "../chat/timeline.js";
import { ChatStore, type Intent } from "../chat/store.js";
import { showSegment } from "../chat/show.js";
import { APPROVE_AWAITING, ASK_AWAITING } from "./frames.js";
import { equal, type Report } from "./check.js";

const RUN_ID = "run-1";

function intents(store: ChatStore): string {
  return untracked<Intent[]>(store.rt, store.intents.get)
    .map((i: Intent): string => {
      switch (i.kind) {
        case "send":
          return "send " + i.localId + " " + JSON.stringify(i.text);
        case "decide":
          return "decide " + i.runId + " " + (i.approved ? "approve" : "reject");
        case "answer":
          return "answer " + i.runId + " " + JSON.stringify(i.text);
      }
    })
    .join("; ");
}

export function storeTests(r: Report): void {
  const store = new ChatStore(new Runtime());
  const s = store.open("session-1");
  store.send(s, "Привіт");
  equal(r, "store: sending shows the message at once", showSegment(s), '(segment live (ask sending "Привіт"))');
  equal(r, "store: sending queues one intent", intents(store), 'send local-1 "Привіт"');

  store.change(s, RUN_ID, () => onAwaitingApproval(s, JSON.parse(APPROVE_AWAITING) as RunEventV1RunAwaitingApproval));
  store.decide(s, RUN_ID, true);
  store.decide(s, RUN_ID, false);
  equal(r, "store: a decision is taken once", intents(store), 'send local-1 "Привіт"; decide run-1 approve');
  equal(
    r,
    "store: the card shows the decision being sent",
    showSegment(s),
    '(segment live (ask sending "Привіт") (turn awaiting_approval (approval sending (call manage_permissions {"action":"get","agent_id":"agent-probe"}))))',
  );

  const q = store.open("session-2");
  store.answer(q, RUN_ID, "A");
  equal(r, "store: no question, no answer", intents(store), 'send local-1 "Привіт"; decide run-1 approve');
  store.change(q, RUN_ID, () => onAwaitingInput(q, JSON.parse(ASK_AWAITING) as RunEventV1RunAwaitingInput));
  store.answer(q, RUN_ID, "B");
  equal(r, "store: an answer is queued", intents(store), 'send local-1 "Привіт"; decide run-1 approve; answer run-1 "B"');
  equal(r, "store: the question shows the answer being sent", showSegment(q), '(segment live (turn awaiting_input (question sending "A or B?" "A" "B")))');
}
