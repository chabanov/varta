// The console screen of the accepted design (mockups/chat.html, chat-empty.html,
// chat-context.html, settings.html), over the chat store and the console app:
// the top bar, the start banner with its loading and error states, the
// stitched timeline with history on scroll and the empty chat, the composer,
// the context panel of the agents and runs mentioned in the conversation, the
// settings dialog, the confirm dialog and the toasts. Every class comes from
// the shared stylesheet (mockups/console.css).
//
// The settings dialog never puts the API key into a signal or an attribute:
// the typed key is held only until the form is submitted or the dialog closes,
// handed to the app's saveSettings (which writes it to the settings store),
// and the field is cleared, so no view snapshot carries it.
import { Runtime, signal, effect, untracked, type Accessor } from "../l1/reactive.js";
import { mapKeyed } from "../l1/list.js";
import type { ChatStore } from "../chat/store.js";
import type { ChildCard, Turn, TurnState } from "../chat/timeline.js";
import { startText, type ConsoleApp, type StartState } from "../app/console.js";
import { Composer } from "./composer.js";
import { segmentsInto } from "./timeline.js";
import { commitSoon } from "./commit.js";
import type { Confirmation, ConfirmRequest, Toast, Toasts } from "./notices.js";
import {
  AGENTS,
  API_KEY_HELP,
  API_KEY_LABEL,
  API_KEY_PLACEHOLDER,
  BASE_URL_HELP,
  BASE_URL_LABEL,
  CANCEL,
  CLOSE_CONTEXT,
  CONTEXT,
  CONTEXT_METER,
  CONTEXT_PANEL,
  EMPTY_TEXT,
  EMPTY_TITLE,
  FORGET_ACTION,
  FORGET_KEY,
  FORGET_TEXT,
  FORGET_TITLE,
  HEAD_AGENT,
  KEY_MISSING,
  LOADING_OLDER,
  NOT_CONNECTED,
  NOTHING_MENTIONED,
  OLDER,
  RUNS,
  SAVE,
  SETTINGS,
  SETTINGS_TITLE,
  SPACE_PREFIX,
  STATE_DONE,
  STATE_ERROR,
  STATE_RUNNING,
  STATE_WAITING,
  SUGGESTIONS,
  TIMELINE_LABEL,
  URL_INVALID,
} from "./text.js";

const NO_NODE = 0;
const SHOWN = "block";
const SHOWN_FLEX = "flex";
const SHOWN_INLINE = "inline";
const HIDDEN = "none";
const INITIALS = 2;
const TURN_PREFIX = "turn:";
/** View command codes of the L0 registry (docs/embedding.md, the generated view-commands table). */
const COMMAND_SHOW_MODAL = 6;
const COMMAND_CLOSE = 7;
/** The command argument every command but `scroll_to` requires. */
const NO_ARGUMENT = 0;
/** Scrolled this close to the top, the timeline loads the next older session. */
const OLDER_THRESHOLD_PX = 48;

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

function connected(state: StartState): boolean {
  return state.kind === "ready" || state.kind === "following";
}

/** States in which the banner offers the settings: no key, no agent, a stored base URL without https, or the API refused or failed. */
function needsSettings(state: StartState): boolean {
  return state.kind === "no_key" || state.kind === "no_agent" || state.kind === "url_invalid" || state.kind === "unavailable";
}

// ------------------------------------------------------------- the timeline

/** The stitched timeline: the older-history control, the empty chat and the segments. */
function timelineNode(store: ChatStore, app: ConsoleApp): number {
  const rt = store.rt;
  const history = app.history.get;
  const older = (
    <div class="separator" style:display={history() === "none" ? HIDDEN : SHOWN_FLEX}>
      <span class="separator-line" />
      <button id="older" class="link" type="button" disabled={history() === "loading"} onClick={() => app.older().then((): void => commitSoon())}>
        {history() === "loading" ? LOADING_OLDER : OLDER}
      </button>
      <span class="separator-line" />
    </div>
  );
  const suggestions = <div class="suggestions" />;
  for (let i = 0; i < SUGGESTIONS.length; i++) {
    const suggestion = SUGGESTIONS[i]!;
    View.insert(
      suggestions,
      <button
        id={"suggestion-" + String(i)}
        class="suggestion"
        type="button"
        aria-label={suggestion.title}
        aria-describedby={"suggestion-hint-" + String(i)}
        onClick={() => store.say(suggestion.title)}
      >
        <strong>{suggestion.title}</strong>
        <span class="muted" id={"suggestion-hint-" + String(i)}>
          {suggestion.hint}
        </span>
      </button>,
      NO_NODE,
    );
  }
  const empty = (
    <div class="empty" style:display={connected(app.state.get()) && !store.hasEntries() ? SHOWN_FLEX : HIDDEN}>
      <div class="avatar" aria-hidden="true">{initials(HEAD_AGENT)}</div>
      <h2 class="empty-title">{EMPTY_TITLE}</h2>
      <p class="p muted">{EMPTY_TEXT}</p>
    </div>
  );
  View.insert(empty, suggestions, NO_NODE);
  const thread = <div class="thread" />;
  View.insert(thread, older, NO_NODE);
  View.insert(thread, empty, NO_NODE);
  segmentsInto(store, thread, HEAD_AGENT);
  const busy = (): boolean => app.state.get().kind === "starting" || history() === "loading";
  const section = (
    <section
      id="timeline"
      class="timeline"
      aria-label={TIMELINE_LABEL}
      aria-live="polite"
      aria-busy={busy() ? "true" : "false"}
      onScroll={(event: ViewEvent) => {
        if (event.scrollTop <= OLDER_THRESHOLD_PX) app.older().then((): void => commitSoon());
      }}
    />
  );
  View.insert(section, thread, NO_NODE);
  return section;
}

// -------------------------------------------------------- the context panel

/** One agent or run the conversation mentions, as a panel row. */
interface Mention {
  key: string;
  icon: string;
  title: string;
  detail: string;
  label: string;
  tone: string;
}

function turnLabel(state: TurnState): string {
  switch (state) {
    case "queued":
    case "running":
      return STATE_RUNNING;
    case "awaiting_approval":
    case "awaiting_input":
    case "paused":
      return STATE_WAITING;
    case "completed":
      return STATE_DONE;
    case "failed":
    case "cancelled":
    case "blocked":
      return STATE_ERROR;
  }
}

function turnTone(state: TurnState): string {
  switch (state) {
    case "completed":
      return "badge ok";
    case "failed":
    case "cancelled":
    case "blocked":
      return "badge danger";
    case "awaiting_approval":
    case "awaiting_input":
    case "paused":
      return "badge warn";
    default:
      return "badge accent";
  }
}

function runMention(t: Turn): Mention {
  const tools = t.tools.map((card): string => card.name).join(", ");
  return { key: "run:" + t.runId, icon: "●", title: t.runId, detail: tools === "" ? "" : " · " + tools, label: turnLabel(t.state), tone: turnTone(t.state) };
}

function agentMention(c: ChildCard): Mention {
  const done = c.state === "completed";
  return { key: "agent:" + c.childRunId, icon: "⇄", title: c.agentName, detail: " · " + c.childRunId, label: done ? STATE_DONE : STATE_ERROR, tone: done ? "badge ok" : "badge danger" };
}

/** The runs and delegated agents of every segment, newest last; reading it subscribes to their changes. */
function mentioned(store: ChatStore, kind: "run" | "agent"): Mention[] {
  const out: Mention[] = [];
  for (const s of store.segments.get()) {
    for (const e of store.entries(s)()) {
      if (e.kind !== "turn") continue;
      store.version(TURN_PREFIX + e.runId)();
      if (kind === "run") out.push(runMention(e));
      else for (const child of e.children) out.push(agentMention(child));
    }
  }
  return out;
}

function mentionNode(m: Accessor<Mention>): number {
  return (
    <div class="event">
      <span class="event-icon" aria-hidden="true">{m().icon}</span>
      <span class="event-text">
        <span class="event-title mono">{m().title}</span>
        <span class="muted">{m().detail}</span>
      </span>
      <span class={m().tone}>{m().label}</span>
    </div>
  );
}

/** A card of mentions, shown while it has any; rows are keyed, so a change re-renders only its row. */
function mentionCard(rt: Runtime, title: string, items: Accessor<Mention[]>): number {
  const card = (
    <div class="card" style:display={items().length === 0 ? HIDDEN : SHOWN_FLEX}>
      <div class="card-head">
        <h3 class="card-title">{title}</h3>
        <span class="badge accent">{String(items().length)}</span>
      </div>
    </div>
  );
  const rows = mapKeyed<Mention, number>(
    rt,
    items,
    (m: Mention): string => m.key,
    (m, index) => mentionNode(m),
    (node: number): void => View.remove(node),
  );
  ordered(rt, card, rows.results);
  return card;
}

/** Agents and runs mentioned in the conversation; computed only while the panel is open. */
function contextPanel(store: ChatStore, open: Accessor<boolean>, close: () => void): number {
  const rt = store.rt;
  const runs = (): Mention[] => (open() ? mentioned(store, "run") : []);
  const agents = (): Mention[] => (open() ? mentioned(store, "agent") : []);
  const meter = (): string => {
    if (!open()) return "";
    runs();
    const context = store.timeline.context;
    return context === null ? "" : String(context.usedTokens) + " / " + String(context.windowTokens);
  };
  const panel = (
    <aside id="context" class="panel" aria-label={CONTEXT_PANEL} style:display={open() ? SHOWN_FLEX : HIDDEN}>
      <div class="panel-head">
        <h2 class="panel-title">{CONTEXT}</h2>
        <button id="context-close" class="button ghost round" type="button" aria-label={CLOSE_CONTEXT} onClick={() => close()}>
          ✕
        </button>
      </div>
      <div class="kv" style:display={meter() === "" ? HIDDEN : "grid"}>
        <span class="kv-key">{CONTEXT_METER}</span>
        <span class="kv-value">{meter()}</span>
      </div>
      <p class="p muted" style:display={runs().length === 0 ? SHOWN : HIDDEN}>{NOTHING_MENTIONED}</p>
    </aside>
  );
  View.insert(panel, mentionCard(rt, AGENTS, agents), NO_NODE);
  View.insert(panel, mentionCard(rt, RUNS, runs), NO_NODE);
  return panel;
}

// ---------------------------------------------------- dialogs and toasts

interface SettingsDialog {
  node: number;
  open: () => void;
}

function settingsDialog(app: ConsoleApp, confirmation: Confirmation): SettingsDialog {
  const rt = app.store.rt;
  const url = signal<string>(rt, "");
  const problem = signal<string>(rt, "");
  // The typed key: a plain variable, never a signal or an attribute.
  let key = "";
  let dialog = NO_NODE;
  let keyField = NO_NODE;
  const clearKey = (): void => {
    key = "";
    if (keyField !== NO_NODE) View.setValue(keyField, "");
  };
  const close = (): void => {
    clearKey();
    View.command(dialog, COMMAND_CLOSE, NO_ARGUMENT);
  };
  const submit = (): void => {
    const typed = key;
    const baseURL = untracked<string>(rt, url.get);
    const check = app.check(baseURL, typed);
    if (check !== null) {
      problem.set(check.kind === "key_missing" ? KEY_MISSING : URL_INVALID);
      return;
    }
    problem.set("");
    close();
    app.saveSettings(baseURL, typed).then((): void => commitSoon());
  };
  const forget = (): void => {
    close();
    const request: ConfirmRequest = {
      title: FORGET_TITLE,
      text: FORGET_TEXT,
      action: FORGET_ACTION,
      run: (): void => {
        app.forgetKey().then((): void => commitSoon());
      },
    };
    confirmation.ask(request);
  };
  const node = (
    <dialog id="settings" class="dialog" aria-labelledby="settings-title" ref={(n: number) => (dialog = n)} onClose={() => clearKey()}>
      <form class="dialog-body" onSubmit={() => submit()}>
        <h2 class="dialog-title" id="settings-title">
          {SETTINGS_TITLE}
        </h2>
        <div class="field">
          <label class="label" for="base-url">
            {BASE_URL_LABEL}
          </label>
          <input
            class="input"
            id="base-url"
            type="text"
            value={url.get()}
            aria-invalid={problem.get() === URL_INVALID ? "true" : "false"}
            onInput={(event: ViewEvent) => url.set(event.value)}
          />
          <span class="help">{BASE_URL_HELP}</span>
        </div>
        <div class="field">
          <label class="label" for="api-key">
            {API_KEY_LABEL}
          </label>
          <input
            class="input"
            id="api-key"
            type="password"
            placeholder={API_KEY_PLACEHOLDER}
            aria-describedby="api-key-help"
            aria-invalid={problem.get() === KEY_MISSING ? "true" : "false"}
            ref={(n: number) => (keyField = n)}
            onInput={(event: ViewEvent) => {
              key = event.value;
            }}
          />
          <span class="help" id="api-key-help">
            {API_KEY_HELP}
          </span>
        </div>
        <p class="help" role="alert" style:display={problem.get() === "" ? HIDDEN : SHOWN}>
          {problem.get()}
        </p>
        <div class="actions">
          <button id="settings-save" class="button primary" type="submit">
            {SAVE}
          </button>
          <button id="settings-cancel" class="button" type="button" onClick={() => close()}>
            {CANCEL}
          </button>
          <button id="settings-forget" class="button danger" type="button" onClick={() => forget()}>
            {FORGET_KEY}
          </button>
        </div>
      </form>
    </dialog>
  );
  const open = (): void => {
    url.set(untracked<string>(rt, app.baseURL.get));
    problem.set("");
    View.command(dialog, COMMAND_SHOW_MODAL, NO_ARGUMENT);
  };
  return { node: node, open: open };
}

function confirmDialog(rt: Runtime, confirmation: Confirmation): number {
  let dialog = NO_NODE;
  const request = confirmation.request.get;
  const part = (pick: (r: ConfirmRequest) => string): string => {
    const r = request();
    return r === null ? "" : pick(r);
  };
  const close = (): void => View.command(dialog, COMMAND_CLOSE, NO_ARGUMENT);
  const node = (
    <dialog
      id="confirm"
      class="dialog"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-text"
      ref={(n: number) => (dialog = n)}
      onClose={() => confirmation.answer(false)}
    >
      <div class="dialog-body">
        <h2 class="dialog-title" id="confirm-title">
          {part((r: ConfirmRequest): string => r.title)}
        </h2>
        <p class="p" id="confirm-text">
          {part((r: ConfirmRequest): string => r.text)}
        </p>
        <div class="actions">
          <button
            id="confirm-yes"
            class="button danger"
            type="button"
            onClick={() => {
              confirmation.answer(true);
              close();
            }}
          >
            {part((r: ConfirmRequest): string => r.action)}
          </button>
          {/* Opening the dialog focuses Cancel, not the destructive action: Enter
              right after it opens never confirms by accident. */}
          <button id="confirm-no" class="button" type="button" autofocus={true} onClick={() => close()}>
            {CANCEL}
          </button>
        </div>
      </div>
    </dialog>
  );
  // Opened when a request arrives; the dialog is mounted by then.
  effect(rt, () => {
    if (request() !== null) untracked<void>(rt, () => View.command(dialog, COMMAND_SHOW_MODAL, NO_ARGUMENT));
  });
  return node;
}

function toastStack(rt: Runtime, toasts: Toasts): number {
  const stack = <div class="toasts" role="status" aria-live="polite" />;
  const rows = mapKeyed<Toast, number>(
    rt,
    toasts.list.get,
    (toast: Toast): string => String(toast.id),
    (toast, index) => <div class="toast">{toast().text}</div>,
    (node: number): void => View.remove(node),
  );
  ordered(rt, stack, rows.results);
  return stack;
}

// ------------------------------------------------------------- the screen

export function ConsoleView(props: { store: ChatStore; app: ConsoleApp; toasts: Toasts; confirmation: Confirmation }): number {
  const store = props.store;
  const app = props.app;
  const rt = store.rt;
  const panel = signal<boolean>(rt, false);
  const settings = settingsDialog(app, props.confirmation);
  const state = app.state.get;
  const banner = (): string => startText(state());
  const subtitle = (): string => (connected(state()) ? SPACE_PREFIX + app.tenantName.get() : NOT_CONNECTED);
  const main = (
    <main class="main">
      <header class="topbar">
        <div class="avatar" aria-hidden="true">{initials(HEAD_AGENT)}</div>
        <div class="heading">
          <h1 class="title">{HEAD_AGENT}</h1>
          <span class="subtitle">{subtitle()}</span>
        </div>
        <button
          id="context-toggle"
          class="button ghost"
          type="button"
          aria-expanded={panel.get() ? "true" : "false"}
          onClick={() => panel.set(!untracked<boolean>(rt, panel.get))}
        >
          {CONTEXT}
        </button>
        <button id="settings-open" class="button ghost round" type="button" aria-label={SETTINGS} onClick={() => settings.open()}>
          ⚙
        </button>
      </header>
      <div class={banner() === "" ? "banner hidden" : "banner"} role="status">
        <span>{banner()}</span>
        <button id="banner-settings" class="button small" type="button" style:display={needsSettings(state()) ? SHOWN_INLINE : HIDDEN} onClick={() => settings.open()}>
          {SETTINGS}
        </button>
      </div>
    </main>
  );
  View.insert(main, timelineNode(store, app), NO_NODE);
  View.insert(main, <Composer store={store} agentName={HEAD_AGENT} enabled={() => connected(state())} />, NO_NODE);
  const root = <div id="app" class={panel.get() ? "app with-panel" : "app"} />;
  View.insert(root, main, NO_NODE);
  View.insert(root, contextPanel(store, panel.get, (): void => panel.set(false)), NO_NODE);
  View.insert(root, settings.node, NO_NODE);
  View.insert(root, confirmDialog(rt, props.confirmation), NO_NODE);
  View.insert(root, toastStack(rt, props.toasts), NO_NODE);
  // Without a stored key the settings open by themselves, once.
  let offered = false;
  effect(rt, () => {
    if (state().kind !== "no_key" || offered) return;
    offered = true;
    untracked<void>(rt, () => settings.open());
  });
  return root;
}
