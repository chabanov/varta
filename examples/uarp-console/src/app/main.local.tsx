// The console's local build (`varta.local.json`): the product console whose
// requests go to the loopback API proxy (`varta.proxy.json`, src/proxy/), for
// running it in a local browser, which refuses the API's answers to a page
// origin the API does not allow (CORS).
import { startConsole } from "./main.js";
import { LOCAL_PROXY_URL } from "./config.js";

export function main(): void {
  startConsole(LOCAL_PROXY_URL, LOCAL_PROXY_URL);
}
