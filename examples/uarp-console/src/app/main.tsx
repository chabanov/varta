// The UARP console's product entry (`varta.json`): the Single Chat screen over
// the chat store, started with the real UarpClient against the configured API
// and following the new session's live stream.
import { Runtime, effect } from "../l1/reactive.js";
import { ChatStore } from "../chat/store.js";
import { FOLLOW_DEFAULTS } from "../chat/follow.js";
import { ChatView } from "../ui/timeline.js";
import { Composer } from "../ui/composer.js";
import { commitSoon } from "../ui/commit.js";
import { installStyles } from "../ui/styles.js";
import { HEAD_AGENT } from "../ui/text.js";
import { ConsoleApp, startText } from "./console.js";
import { API_BASE_URL, KEY_DATABASE } from "./config.js";

export function main(): void {
  const rt = new Runtime();
  View.setEffect((run: () => void): void => {
    effect(rt, run);
  });
  installStyles();
  const store = new ChatStore(rt);
  store.afterChange = commitSoon;
  const app = new ConsoleApp(store, { baseURL: API_BASE_URL, keyDatabase: KEY_DATABASE, fetcher: null, follow: FOLLOW_DEFAULTS });
  View.mount(
    <main id="app" class="main">
      <div class={startText(app.state.get()) === "" ? "banner hidden" : "banner"} role="status">
        <span>{startText(app.state.get())}</span>
      </div>
      <ChatView store={store} agentName={HEAD_AGENT} />
      <Composer store={store} agentName={HEAD_AGENT} />
    </main>,
  );
  // Started in a turn of its own: the mount turn commits by itself, and the
  // start can finish in it (KV operations complete within their call), which
  // would commit that turn twice.
  setTimeout((): void => {
    app.start().then((): void => commitSoon());
  }, 0);
}
