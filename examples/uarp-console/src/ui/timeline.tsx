// The Single Chat timeline as live L0 nodes over the chat store, with the
// mockups' classes and roles. Segments and entries are appended as they arrive,
// tool lines are a keyed list; each entry re-renders only what changed. The answer streams
// through MarkdownStream: blocks before the last boundary keep their nodes and
// only the open tail is rebuilt. Thinking is collapsed by default and every tool
// call is one compact line (owner's decisions); approvals, questions and
// failures are full cards.
//
// Segments and entries are append-only: each is rendered once when it arrives,
// and the effects it creates live as long as the timeline.
import { Runtime, signal, effect, untracked, type Accessor, type Signal } from "../l1/reactive.js";
import { mapKeyed } from "../l1/list.js";
import { MarkdownStream } from "../markdown/stream.js";
import { type Segment, type Entry, type Ask, type Turn, type ToolCard, type ChildCard, type Approval, type Question, type Failure } from "../chat/timeline.js";
import type { ChatStore } from "../chat/store.js";
import { blockNode } from "./markdown.js";
import {
  APPROVAL_TEXT,
  APPROVAL_TITLE,
  APPROVE,
  BLOCKED_TITLE,
  CANCELLED_TITLE,
  CODE,
  DELEGATED,
  FAILURE_TITLE,
  QUESTION_TITLE,
  REJECT,
  REJECTED_TITLE,
  STATE_ANSWERED,
  STATE_APPROVED,
  STATE_DONE,
  STATE_ERROR,
  STATE_FAILED_TO_SEND,
  STATE_NEEDS_ANSWER,
  STATE_REJECTED,
  STATE_RUNNING,
  STATE_SENDING,
  STATE_WAITING,
  STATE_WRITING,
  THINKING,
  TIMELINE_LABEL,
  TOOL,
} from "./text.js";

const NO_NODE = 0;
const SHOWN = "block";
const SHOWN_INLINE = "inline";
const HIDDEN = "none";
const INITIALS = 2;
const TURN_PREFIX = "turn:";
const ASK_PREFIX = "ask:";

function unreachable(value: never): never {
  throw new Error("unreachable timeline entry");
}

function shown(visible: boolean): string {
  return visible ? SHOWN : HIDDEN;
}

function initials(name: string): string {
  return name
    .split(" ")
    .filter((word: string): boolean => word !== "")
    .slice(0, INITIALS)
    .map((word: string): string => word.charAt(0).toUpperCase())
    .join("");
}

/** Keeps `parent`'s children in the order of `nodes`. */
function ordered(rt: Runtime, parent: number, nodes: Accessor<number[]>): void {
  effect(rt, () => {
    for (const node of nodes()) View.insert(parent, node, NO_NODE);
  });
}

// ------------------------------------------------------------------- asks

function askLabel(a: Ask): string {
  if (a.state === "sending") return STATE_SENDING;
  if (a.state === "failed") return STATE_FAILED_TO_SEND;
  return "";
}

function askNode(store: ChatStore, a: Ask): number {
  const v = store.version(ASK_PREFIX + a.key);
  const label = (): string => {
    v();
    return askLabel(a);
  };
  const failed = (): boolean => {
    v();
    return a.state === "failed";
  };
  return (
    <div class="msg user">
      <div class="bubble">{a.text}</div>
      <span class={failed() ? "badge danger" : "badge"} style:display={label() === "" ? HIDDEN : SHOWN_INLINE}>
        {label()}
      </span>
    </div>
  );
}

// ------------------------------------------------------------------ turns

function thinkingNode(rt: Runtime, t: Turn, text: Accessor<string>): number {
  const open = signal<boolean>(rt, false);
  return (
    <div class="thinking" style:display={shown(text() !== "")}>
      <button
        id={"thinking-" + t.runId}
        class="event toggle"
        type="button"
        aria-expanded={open.get() ? "true" : "false"}
        onClick={() => open.set(!untracked<boolean>(rt, open.get))}
      >
        <span class="event-icon" aria-hidden="true">◇</span>
        <span class="event-text">
          <span class="event-title">{THINKING}</span>
        </span>
        <span class="subtle" aria-hidden="true">{open.get() ? "▾" : "▸"}</span>
      </button>
      <p class="p muted" style:display={shown(open.get())}>{text()}</p>
    </div>
  );
}

function toolLabel(card: ToolCard): string {
  if (card.state === "done") return STATE_DONE;
  if (card.state === "error") return STATE_ERROR;
  return STATE_RUNNING;
}

function toolClass(card: ToolCard): string {
  if (card.state === "done") return "badge ok";
  if (card.state === "error") return "badge danger";
  return "badge";
}

function toolNode(card: Accessor<ToolCard>, signature: Accessor<string>): number {
  const state = (): ToolCard => {
    signature();
    return card();
  };
  return (
    <div class="event">
      <span class="event-icon" aria-hidden="true">⚙</span>
      <span class="event-text">
        <span class="event-title mono">{card().name}</span>
      </span>
      <span class={toolClass(state())}>{toolLabel(state())}</span>
    </div>
  );
}

function childNode(c: ChildCard): number {
  return (
    <div class="event">
      <span class="event-icon" aria-hidden="true">⇄</span>
      <span class="event-text">
        <span class="event-title">{DELEGATED + " «" + c.agentName + "»"}</span>
      </span>
      <span class={c.state === "completed" ? "badge ok" : "badge danger"}>{c.state === "completed" ? STATE_DONE : STATE_ERROR}</span>
    </div>
  );
}

function approvalBadge(a: Approval): number {
  if (a.decision === "approved") return <span class="badge ok">{STATE_APPROVED}</span>;
  if (a.decision === "rejected") return <span class="badge danger">{STATE_REJECTED}</span>;
  if (a.decision === "sending") return <span class="badge">{STATE_SENDING}</span>;
  return <span class="badge warn">{STATE_WAITING}</span>;
}

function approvalNode(runId: string, a: Approval, decide: (approved: boolean) => void): number {
  const head = (
    <div class="card-head">
      <span aria-hidden="true">✋</span>
      <h2 class="card-title">{APPROVAL_TITLE}</h2>
    </div>
  );
  View.insert(head, approvalBadge(a), NO_NODE);
  const card = a.decision === "open" ? <div class="card attention" role="alert" /> : <div class="card" />;
  View.insert(card, head, NO_NODE);
  View.insert(card, <p class="p">{APPROVAL_TEXT}</p>, NO_NODE);
  for (const call of a.calls) {
    View.insert(
      card,
      <div class="kv">
        <span class="kv-key">{TOOL}</span>
        <span class="kv-value mono">{call.name}</span>
      </div>,
      NO_NODE,
    );
    View.insert(card, <pre class="pre small">{call.arguments}</pre>, NO_NODE);
  }
  if (a.decision === "open") {
    View.insert(
      card,
      <div class="actions">
        <button id={"approve-" + runId} class="button primary" type="button" onClick={() => decide(true)}>
          {APPROVE}
        </button>
        <button id={"reject-" + runId} class="button" type="button" onClick={() => decide(false)}>
          {REJECT}
        </button>
      </div>,
      NO_NODE,
    );
  }
  return card;
}

function questionNode(runId: string, q: Question, reply: (text: string) => void): number {
  const card = (
    <div class="card">
      <div class="card-head">
        <span aria-hidden="true">?</span>
        <h2 class="card-title">{QUESTION_TITLE}</h2>
        <span class={q.answer === "open" ? "badge accent" : "badge ok"}>{questionLabel(q)}</span>
      </div>
      <p class="p">{q.text}</p>
    </div>
  );
  if (q.answer === "open" && q.options.length > 0) {
    const actions = <div class="actions" />;
    for (let i = 0; i < q.options.length; i++) {
      const option = q.options[i]!;
      View.insert(
        actions,
        <button id={"option-" + runId + "-" + String(i)} class="button" type="button" onClick={() => reply(option)}>
          {option}
        </button>,
        NO_NODE,
      );
    }
    View.insert(card, actions, NO_NODE);
  }
  return card;
}

function questionLabel(q: Question): string {
  if (q.answer === "open") return STATE_NEEDS_ANSWER;
  if (q.answer === "sending") return STATE_SENDING;
  return STATE_ANSWERED;
}

/** A wait card changes only with the wait's kind and its decision. */
function waitSignature(t: Turn): string {
  const w = t.wait;
  if (w === null) return "";
  return w.kind === "approval" ? "approval:" + w.decision : "question:" + w.answer;
}

function waitNode(store: ChatStore, s: Segment, t: Turn, signature: Accessor<string>): number {
  const rt = store.rt;
  const wait = <div class="wait" />;
  let card = NO_NODE;
  effect(rt, () => {
    signature();
    if (card !== NO_NODE) View.remove(card);
    card = NO_NODE;
    const w = t.wait;
    if (w === null) return;
    card =
      w.kind === "approval"
        ? approvalNode(t.runId, w, (approved: boolean): void => store.decide(s, t.runId, approved))
        : questionNode(t.runId, w, (text: string): void => store.answer(s, t.runId, text));
    View.insert(wait, card, NO_NODE);
  });
  return wait;
}

function failureTitle(f: Failure): string {
  switch (f.reason) {
    case "error":
      return FAILURE_TITLE;
    case "rejected":
      return REJECTED_TITLE;
    case "cancelled":
      return CANCELLED_TITLE;
    case "blocked":
      return BLOCKED_TITLE;
  }
}

function failureCard(f: Failure): number {
  const card = f.reason === "error" ? <div class="card error" role="alert" /> : <div class="card" />;
  View.insert(
    card,
    <div class="card-head">
      <span aria-hidden="true">✕</span>
      <h2 class="card-title">{failureTitle(f)}</h2>
    </div>,
    NO_NODE,
  );
  if (f.message !== "") View.insert(card, <p class="p">{f.message}</p>, NO_NODE);
  const code = f.code;
  if (code !== null) {
    View.insert(
      card,
      <div class="kv">
        <span class="kv-key">{CODE}</span>
        <span class="kv-value mono">{code}</span>
      </div>,
      NO_NODE,
    );
  }
  return card;
}

/** A failure is terminal: its card is added once. */
function failureNode(rt: Runtime, t: Turn, failed: Accessor<boolean>): number {
  const failure = <div class="failure" />;
  let added = false;
  effect(rt, () => {
    failed();
    const f = t.failure;
    if (f === null || added) return;
    added = true;
    View.insert(failure, failureCard(f), NO_NODE);
  });
  return failure;
}

/** Delegated runs are append-only. */
function childrenNode(rt: Runtime, t: Turn, length: Accessor<number>): number {
  const children = <div class="children" />;
  let count = 0;
  effect(rt, () => {
    length();
    while (count < t.children.length) {
      View.insert(children, childNode(t.children[count]!), NO_NODE);
      count += 1;
    }
  });
  return children;
}

function toolsNode(rt: Runtime, t: Turn, signature: Accessor<string>): number {
  const tools = <div class="tools" />;
  const rows = mapKeyed<ToolCard, number>(
    rt,
    (): ToolCard[] => {
      signature();
      return t.tools.slice(0);
    },
    (card: ToolCard): string => card.callId,
    (card, index) => toolNode(card, signature),
    (node: number): void => View.remove(node),
  );
  ordered(rt, tools, rows.results);
  return tools;
}

/** The answer as Markdown: stable blocks keep their nodes, the open tail is rebuilt. */
function proseNode(rt: Runtime, answer: Accessor<string>): number {
  const prose = <div class="prose" style:display={shown(answer() !== "")} />;
  const stream = new MarkdownStream();
  let stableNodes: number[] = [];
  let tailNodes: number[] = [];
  let rendered = "";
  effect(rt, () => {
    const text = answer();
    if (text === rendered) return;
    rendered = text;
    const before = stream.stable.length;
    const doc = stream.update(text);
    for (const node of tailNodes) View.remove(node);
    tailNodes = [];
    if (stream.stable.length < before) {
      for (const node of stableNodes) View.remove(node);
      stableNodes = [];
    }
    for (let i = stableNodes.length; i < stream.stable.length; i++) {
      const node = blockNode(stream.stable[i]!);
      stableNodes.push(node);
      View.insert(prose, node, NO_NODE);
    }
    for (let i = stream.stable.length; i < doc.blocks.length; i++) {
      const node = blockNode(doc.blocks[i]!);
      tailNodes.push(node);
      View.insert(prose, node, NO_NODE);
    }
  });
  return prose;
}

/**
 * One signal per aspect of a turn. A single effect copies the model into them on
 * every version bump; a signal ignores an equal value, so a content chunk
 * re-renders only the prose and a tool result only its line.
 */
interface TurnAspects {
  state: Signal<string>;
  thinking: Signal<string>;
  answer: Signal<string>;
  tools: Signal<string>;
  children: Signal<number>;
  wait: Signal<string>;
  failed: Signal<boolean>;
}

function toolsSignature(t: Turn): string {
  return t.tools.map((card: ToolCard): string => card.callId + ":" + card.state).join(" ");
}

function aspects(rt: Runtime, t: Turn, v: Accessor<number>): TurnAspects {
  const a: TurnAspects = {
    state: signal<string>(rt, t.state),
    thinking: signal<string>(rt, t.thinking),
    answer: signal<string>(rt, t.answer),
    tools: signal<string>(rt, toolsSignature(t)),
    children: signal<number>(rt, t.children.length),
    wait: signal<string>(rt, waitSignature(t)),
    failed: signal<boolean>(rt, t.failure !== null),
  };
  effect(rt, () => {
    v();
    a.state.set(t.state);
    a.thinking.set(t.thinking);
    a.answer.set(t.answer);
    a.tools.set(toolsSignature(t));
    a.children.set(t.children.length);
    a.wait.set(waitSignature(t));
    a.failed.set(t.failure !== null);
  });
  return a;
}

function turnNode(store: ChatStore, s: Segment, t: Turn, agentName: string): number {
  const rt = store.rt;
  const a = aspects(rt, t, store.version(TURN_PREFIX + t.runId));
  const writing = (): boolean => {
    const state = a.state.get();
    return state === "running" || state === "queued";
  };
  const body = (
    <div class="msg-body">
      <div class="author">
        <span class="author-name">{agentName}</span>
        <span class="badge accent" style:display={writing() ? SHOWN_INLINE : HIDDEN}>
          {STATE_WRITING}
        </span>
      </div>
    </div>
  );
  View.insert(body, thinkingNode(rt, t, a.thinking.get), NO_NODE);
  View.insert(body, toolsNode(rt, t, a.tools.get), NO_NODE);
  View.insert(body, childrenNode(rt, t, a.children.get), NO_NODE);
  View.insert(body, waitNode(store, s, t, a.wait.get), NO_NODE);
  View.insert(body, proseNode(rt, a.answer.get), NO_NODE);
  View.insert(body, failureNode(rt, t, a.failed.get), NO_NODE);
  const msg = (
    <div class="msg">
      <div class="avatar small" aria-hidden="true">{initials(agentName)}</div>
    </div>
  );
  View.insert(msg, body, NO_NODE);
  return msg;
}

function noteNode(text: string): number {
  return (
    <div class="separator">
      <span class="separator-line" />
      <span>{text}</span>
      <span class="separator-line" />
    </div>
  );
}

function entryNode(store: ChatStore, s: Segment, e: Entry, agentName: string): number {
  switch (e.kind) {
    case "ask":
      return askNode(store, e);
    case "turn":
      return turnNode(store, s, e, agentName);
    case "note":
      return noteNode(e.text);
    default:
      return unreachable(e);
  }
}

/**
 * Renders the items of an append-only list as they arrive: each new item is
 * made once and appended, so an addition costs only the new item.
 */
function appended<T>(rt: Runtime, parent: number, items: Accessor<T[]>, make: (item: T) => number): void {
  let count = 0;
  effect(rt, () => {
    const list = items();
    while (count < list.length) {
      const item = list[count]!;
      View.insert(parent, untracked<number>(rt, () => make(item)), NO_NODE);
      count += 1;
    }
  });
}

function segmentNode(store: ChatStore, s: Segment, agentName: string): number {
  const node = <div class="segment" />;
  appended<Entry>(store.rt, node, store.entries(s), (e: Entry): number => entryNode(store, s, e, agentName));
  return node;
}

export function ChatView(props: { store: ChatStore; agentName: string }): number {
  const store = props.store;
  const thread = <div class="thread" />;
  appended<Segment>(store.rt, thread, store.segments.get, (s: Segment): number => segmentNode(store, s, props.agentName));
  const section = <section class="timeline" aria-label={TIMELINE_LABEL} aria-live="polite" />;
  View.insert(section, thread, NO_NODE);
  return section;
}
