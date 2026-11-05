// The product's fixed configuration. `varta.json` grants `net.request` on the
// API base URL alone and `kv.read`/`kv.write` on the key database alone; the
// `app-manifest` test checks the built manifest against these values.

/** The UARP API every request goes to (the SDK's own default). */
export const API_BASE_URL = "https://api.snaga.ai";
/** The Deno KV database holding the API key, under the `data` root. */
export const KEY_DATABASE = "/data/console.kv";
/**
 * The local build's API base URL (`varta.local.json`): the loopback proxy of
 * `varta.proxy.json`, which forwards to the API from outside the browser.
 */
export const LOCAL_PROXY_URL = "http://127.0.0.1:8771";
