// The message box under the timeline: Enter sends, Shift+Enter inserts a
// newline, IME composition never sends. Sending goes to the store, which shows
// the message at once and queues the request; an empty draft sends nothing.
import { Runtime, signal, untracked } from "../l1/reactive.js";
import type { ChatStore } from "../chat/store.js";
import { COMPOSER_HINT, COMPOSER_LABEL, PLACEHOLDER_PREFIX, SEND } from "./text.js";

export function Composer(props: { store: ChatStore; agentName: string }): number {
  const store = props.store;
  const rt: Runtime = store.rt;
  const draft = signal<string>(rt, "");
  const send = (): void => {
    const text = untracked<string>(rt, draft.get).trim();
    const s = untracked(rt, () => store.live());
    if (text === "" || s === null) return;
    store.send(s, text);
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
          placeholder={PLACEHOLDER_PREFIX + props.agentName + "…"}
          value={draft.get()}
          onInput={(event: ViewEvent) => draft.set(event.value)}
          onKeyDown:Enter-noShift-notComposing-preventDefault={() => send()}
        />
        <div class="composer-bar">
          <span class="hint">{COMPOSER_HINT}</span>
          <button id="send" class="button round primary" type="submit" aria-label={SEND} disabled={draft.get().trim() === ""}>
            ↑
          </button>
        </div>
      </form>
    </div>
  );
}
