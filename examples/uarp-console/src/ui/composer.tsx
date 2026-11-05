// The message box under the timeline: Enter sends, Shift+Enter inserts a
// newline, IME composition never sends. Sending goes to the store, which shows
// the message at once and queues the request (creating the session first when
// none is live); an empty draft sends nothing. Until the console is connected
// the box is disabled and says so.
import { Runtime, signal, untracked } from "../l1/reactive.js";
import type { ChatStore } from "../chat/store.js";
import { COMPOSER_DISABLED, COMPOSER_HINT, COMPOSER_LABEL, PLACEHOLDER_PREFIX, SEND } from "./text.js";

export function Composer(props: { store: ChatStore; agentName: string; enabled: () => boolean }): number {
  const store = props.store;
  const rt: Runtime = store.rt;
  const enabled = props.enabled;
  const draft = signal<string>(rt, "");
  const send = (): void => {
    const text = untracked<string>(rt, draft.get).trim();
    if (text === "" || !untracked<boolean>(rt, enabled)) return;
    store.say(text);
    draft.set("");
  };
  return (
    <div class="composer-wrap">
      <form class="composer" onSubmit={() => send()}>
        <textarea
          id="composer"
          class="composer-input"
          rows="1"
          aria-label={COMPOSER_LABEL}
          placeholder={enabled() ? PLACEHOLDER_PREFIX + props.agentName + "…" : COMPOSER_DISABLED}
          disabled={!enabled()}
          value={draft.get()}
          onInput={(event: ViewEvent) => draft.set(event.value)}
          onKeyDown:Enter-noShift-notComposing-preventDefault={() => send()}
        />
        <div class="composer-bar">
          <span class="hint">{COMPOSER_HINT}</span>
          <button id="send" class="button round primary" type="submit" aria-label={SEND} disabled={!enabled() || draft.get().trim() === ""}>
            ↑
          </button>
        </div>
      </form>
    </div>
  );
}
