// Store tests: the person's actions change the model at once and queue exactly
// one intent each, for the request layer to perform; a message without a live
// segment opens a local one that a created session binds, and history goes
// before it.
import type { RunEventV1RunAwaitingApproval, RunEventV1RunAwaitingInput } from "uarp-sdk";
import { Runtime, untracked } from "../l1/reactive.js";
import { addNote, onAwaitingApproval, onAwaitingInput } from "../chat/timeline.js";
import { ChatStore, type Intent } from "../chat/store.js";
import { show, showSegment } from "../chat/show.js";
import { APPROVE_AWAITING, ASK_AWAITING } from "./frames.js";
import { equal, type Report } from "./check.js";

const RUN_ID = "run-1";

function intents(store: ChatStore): string {
  return untracked<Intent[]>(store.rt, store.intents.get)
    .map((i: Intent): string => {
      switch (i.kind) {
        case "start":
          return "start " + i.sessionId + " " + i.localId + " " + JSON.stringify(i.text);
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
  stitchingStoreTests(r);
}

/** Messages without a live segment, binding a new session, history before it, a failed creation. */
function stitchingStoreTests(r: Report): void {
  const store = new ChatStore(new Runtime());
  store.say("Привіт");
  store.say("Ще одне");
  equal(r, "store: no live segment, a local one opens at once", show(store.timeline), '(timeline (segment live (ask sending "Привіт") (ask sending "Ще одне")))');
  equal(r, "store: the first message starts the session, the next waits behind it", intents(store), 'start local-session-1 local-1 "Привіт"; send local-2 "Ще одне"');

  const local = store.segment("local-session-1");
  if (local === null) throw new Error("no local segment");
  store.bind(local, "session-9", "2026-01-03");
  equal(r, "store: the created session binds the segment", (store.segment("local-session-1") === local ? "alias " : "lost ") + local.sessionId + " " + local.date, "alias session-9 2026-01-03");
  const entries = store.entries(local);
  equal(r, "store: the bound segment keeps its entries", String(untracked<number>(store.rt, () => entries().length)), "2");

  store.prepend("session-0", "2026-01-01", (s) => addNote(s, "older"));
  equal(r, "store: history goes before the live segment", show(store.timeline), '(timeline (segment history 2026-01-01 (note "older")) (segment live 2026-01-03 (ask sending "Привіт") (ask sending "Ще одне")))');

  const failing = new ChatStore(new Runtime());
  failing.say("Не дійде");
  const abandoned = failing.segment("local-session-1");
  if (abandoned === null) throw new Error("no local segment");
  failing.abandon(abandoned, "NOT_FOUND");
  failing.say("Знову");
  equal(
    r,
    "store: a failed creation closes the segment and the next message opens a new one",
    show(failing.timeline),
    '(timeline (segment closed (ask failed "Не дійде") (note "NOT_FOUND")) (segment live (ask sending "Знову")))',
  );
  equal(r, "store: the new segment starts its own session", intents(failing), 'start local-session-1 local-1 "Не дійде"; start local-session-2 local-2 "Знову"');
}
