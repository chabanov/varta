// Compact, deterministic text form of a chat timeline, used by tests, in the
// style of src/markdown/show.ts. Thinking is shown by its length only.
import type { Timeline, Segment, Entry, Turn, ToolCard, Wait, ChildCard, Failure } from "./timeline.js";

function quote(text: string): string {
  return JSON.stringify(text);
}

function unreachable(value: never): never {
  throw new Error("unreachable timeline node");
}

function tool(card: ToolCard): string {
  const latency = card.latencyMs === null ? "" : " " + String(Math.round(card.latencyMs)) + "ms";
  return " (tool " + card.name + " " + card.state + " " + card.arguments + latency + (card.result === "" ? "" : " " + quote(card.result)) + ")";
}

function wait(w: Wait): string {
  switch (w.kind) {
    case "approval":
      return " (approval " + w.decision + w.calls.map((card: ToolCard): string => " (call " + card.name + " " + card.arguments + ")").join("") + ")";
    case "question":
      return " (question " + w.answer + " " + quote(w.text) + w.options.map((o: string): string => " " + quote(o)).join("") + ")";
    default:
      return unreachable(w);
  }
}

function child(c: ChildCard): string {
  return " (child " + quote(c.agentName) + " " + c.state + (c.preview === "" ? "" : " " + quote(c.preview)) + ")";
}

function failure(f: Failure): string {
  return " (failure " + f.reason + " " + quote(f.message) + (f.code === null ? "" : " " + f.code) + ")";
}

function turn(t: Turn): string {
  return (
    "(turn " +
    t.state +
    (t.thinking === "" ? "" : " (thinking " + String(t.thinking.length) + ")") +
    (t.answer === "" ? "" : " (answer " + quote(t.answer) + ")") +
    t.tools.map(tool).join("") +
    t.children.map(child).join("") +
    (t.wait === null ? "" : wait(t.wait)) +
    (t.failure === null ? "" : failure(t.failure)) +
    ")"
  );
}

function entry(e: Entry): string {
  switch (e.kind) {
    case "ask":
      return "(ask " + e.state + " " + quote(e.text) + ")";
    case "turn":
      return turn(e);
    case "note":
      return "(note " + quote(e.text) + ")";
    default:
      return unreachable(e);
  }
}

export function showSegment(s: Segment): string {
  return "(segment " + s.state + s.entries.map((e: Entry): string => " " + entry(e)).join("") + ")";
}

export function show(t: Timeline): string {
  const context = t.context === null ? "" : " (context " + String(t.context.usedTokens) + "/" + String(t.context.windowTokens) + ")";
  return "(timeline" + context + t.segments.map((s: Segment): string => " " + showSegment(s)).join("") + ")";
}
