// The UARP console's product entry (`varta.json`): the console screen over the
// chat store, started with the settings from the KV database and the real
// UarpClient against the configured API.
import { Runtime, effect } from "../l1/reactive.js";
import { ChatStore } from "../chat/store.js";
import { FOLLOW_DEFAULTS } from "../chat/follow.js";
import { ConsoleView } from "../ui/console.js";
import { Confirmation, TOAST_MS, Toasts } from "../ui/notices.js";
import { commitSoon } from "../ui/commit.js";
import { installStyles } from "../ui/styles.js";
import { ConsoleApp } from "./console.js";
import { clientApi } from "./api.js";
import { KvSettings } from "./key.js";
import { API_BASE_URL, KEY_DATABASE } from "./config.js";

export function main(): void {
  startConsole(API_BASE_URL, null);
}

/**
 * The console over `baseURL` (when the settings hold none); `localBaseURL` is
 * the one plaintext base URL the build admits besides https (the local
 * build's loopback proxy), null in the product.
 */
export function startConsole(baseURL: string, localBaseURL: string | null): void {
  const rt = new Runtime();
  View.setEffect((run: () => void): void => {
    effect(rt, run);
  });
  installStyles();
  const store = new ChatStore(rt);
  store.afterChange = commitSoon;
  const toasts = new Toasts(rt, TOAST_MS, commitSoon);
  const confirmation = new Confirmation(rt);
  const app = new ConsoleApp(store, {
    defaultBaseURL: baseURL,
    localBaseURL: localBaseURL,
    settings: new KvSettings(KEY_DATABASE),
    connect: clientApi(null),
    follow: FOLLOW_DEFAULTS,
    toasts: toasts,
  });
  View.mount(<ConsoleView store={store} app={app} toasts={toasts} confirmation={confirmation} />);
  // Started in a turn of its own: the mount turn commits by itself, and the
  // start can finish in it (KV operations complete within their call), which
  // would commit that turn twice.
  setTimeout((): void => {
    app.start().then((): void => commitSoon());
  }, 0);
}
