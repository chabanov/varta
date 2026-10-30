// Test entry executed by Varta: `TEST_GROUP=<name> varta dev --config varta.test.json`.
// Each group is one execution, so every group stays within the production budgets.
import { report, summary, type Report } from "./check.js";
import { markdownInlineTests, markdownBlockTests, markdownCodeAndTableTests, markdownStreamEarlyTests, markdownStreamLateTests } from "./markdown.test.js";
import { reactiveTests } from "./reactive.test.js";
import { listMappingTests, listUpdateTests, listMoveTests, listRemoveTests } from "./list.test.js";
import { chatTurnTests, chatWaitTests, chatStrictTests, chatStreamTests, chatDispatchTests } from "./chat.test.js";
import { historyTests, historyMergeTests } from "./history.test.js";
import { storeTests } from "./store.test.js";
import { requestTests } from "./requests.test.js";
import { scenarioTests } from "./scenarios.test.js";

const SCENARIO_PREFIX = "scenario-";
const GROUPS: string[] = ["markdown-inline", "markdown-blocks", "markdown-code", "markdown-stream-early", "markdown-stream-late", "reactive", "list-mapping", "list-update", "list-move", "list-remove", "chat-turns", "chat-waits", "chat-strict", "chat-stream", "chat-dispatch", "chat-history", "chat-history-merge", "chat-store", "chat-requests"];

function run(group: string, r: Report): boolean {
  if (group === "markdown-inline") markdownInlineTests(r);
  else if (group === "markdown-blocks") markdownBlockTests(r);
  else if (group === "markdown-code") markdownCodeAndTableTests(r);
  else if (group === "markdown-stream-early") markdownStreamEarlyTests(r);
  else if (group === "markdown-stream-late") markdownStreamLateTests(r);
  else if (group === "reactive") reactiveTests(r);
  else if (group === "list-mapping") listMappingTests(r);
  else if (group === "list-update") listUpdateTests(r);
  else if (group === "list-move") listMoveTests(r);
  else if (group === "list-remove") listRemoveTests(r);
  else if (group === "chat-turns") chatTurnTests(r);
  else if (group === "chat-waits") chatWaitTests(r);
  else if (group === "chat-strict") chatStrictTests(r);
  else if (group === "chat-stream") chatStreamTests(r);
  else if (group === "chat-dispatch") chatDispatchTests(r);
  else if (group === "chat-history") historyTests(r);
  else if (group === "chat-history-merge") historyMergeTests(r);
  else if (group === "chat-store") storeTests(r);
  else return false;
  return true;
}

export async function main(): Promise<void> {
  const group = varta.env.get("TEST_GROUP");
  const r = report();
  if (group === "chat-requests") {
    await requestTests(r);
    summary(r);
    return;
  }
  if (group.startsWith(SCENARIO_PREFIX)) {
    await scenarioTests(r, group.slice(SCENARIO_PREFIX.length));
    summary(r);
    return;
  }
  if (!run(group, r)) {
    console.log(`UNKNOWN GROUP ${group}; groups: ${GROUPS.join(" ")}`);
    return;
  }
  summary(r);
}
