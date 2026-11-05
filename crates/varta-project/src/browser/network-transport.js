// Browser transport limits supplement package/host policy. Redirects are never
// followed here: an opaque manual redirect cannot expose its checked next hop.
// V38 timing: plan.timeoutMs (the per-turn deadline) is an inactivity window.
// The head must arrive within one window of the start, and each body read
// waits at most one window for its next bytes, so a chunk renews it.
function browserTransport(plan, fetcher = fetch) {
  const N = registry.network, C = N.completion, controller = new AbortController();
  let reader, buffer = new Uint8Array(0), carry = new Uint8Array(0), ended = false, timer;
  const utf8 = new TextDecoder("utf-8", { ignoreBOM: true }), strict = new TextDecoder("utf-8", { fatal: true });
  const failed = (text) => ({ kind: C.failed, text });
  const cancel = () => { clearTimeout(timer); controller.abort(); reader?.cancel().catch(() => {}); buffer = carry = new Uint8Array(0); ended = true; };
  // Headers the browser owns in addition to native framing headers. Refuse
  // them instead of allowing Fetch to silently discard a guest's requested field.
  const browserHeaders = new Set(["accept-charset", "access-control-request-headers", "access-control-request-method", "cookie", "cookie2", "date", "dnt", "origin", "permissions-policy", "referer", "set-cookie", "via", "x-http-method", "x-http-method-override", "x-method-override"]);
  // net::HOST_REQUEST_HEADERS: every host owns them. The guest's value was
  // checked like any header and is ignored here exactly as the native host
  // ignores it; the browser sends its own.
  const hostHeaders = new Set(["accept-encoding", "user-agent"]);
  // A response the browser decoded no longer has these as received.
  const decodedHeaders = new Set(["content-encoding", "content-length"]);
  const expired = Symbol("inactivity window");
  // Settle with `promise`, or reject with `expired` after one window.
  const within = (promise) => {
    if (!Number.isFinite(plan.timeoutMs)) return promise;
    return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(expired), plan.timeoutMs); })])
      .finally(() => clearTimeout(timer));
  };
  const head = (async () => {
    const headers = [];
    for (const [name, value] of plan.headers) {
      const lower = name.toLowerCase();
      if (browserHeaders.has(lower) || lower.startsWith("sec-") || lower.startsWith("proxy-")) { cancel(); return failed(`browser transport cannot set header ${name}`); }
      if (!hostHeaders.has(lower)) headers.push([name, value]);
    }
    try {
      const response = await within(fetcher(plan.url, { method: plan.method, headers, body: plan.body,
        credentials: "omit", redirect: "manual", mode: "cors", cache: "no-store", referrerPolicy: "no-referrer", signal: controller.signal }));
      if (response.type === "opaqueredirect" || response.status === 0) { cancel(); return failed("browser transport cannot inspect a manual redirect"); }
      // Fetch decodes every content coding; the native host asks for the
      // identity coding. A decoded body shows neither its coding nor the
      // coded length (a cross-origin coding is visible only when exposed).
      let coded = false;
      for (const coding of (response.headers.get("content-encoding") ?? "").split(",")) {
        const name = coding.trim().toLowerCase();
        if (name && name !== "identity") coded = true;
      }
      let count = 0, size = 0;
      const visible = [];
      for (const [name, value] of response.headers) {
        count++; size += name.length + value.length;
        if (count > N.maxResponseHeaders || size > N.maxResponseHeaderBytes) { cancel(); return failed("response headers exceed browser host limits"); }
        if (coded && decodedHeaders.has(name)) continue;
        if (plan.visible.has(name) && !N.forbiddenResponseHeaders.includes(name)) visible.push([name, value]);
      }
      visible.sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
      const location = response.headers.get("location");
      let target = ""; if (location) { try { target = new URL(location, plan.url).href; } catch {} }
      const [status, statusText, url, locationField, fields] = N.headFields;
      const record = { [status]: response.status, [statusText]: response.statusText, [url]: plan.url, [locationField]: target, [fields]: visible.flat() };
      const text = JSON.stringify(Object.fromEntries(Object.entries(record).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)));
      reader = response.body?.getReader(); ended = !reader;
      return { kind: C.head, text };
    } catch (error) {
      cancel();
      return failed(error === expired ? N.errors.timeout : `browser request failed: ${error.message ?? error}`);
    }
  })();
  const incomplete = (bytes) => {
    for (let back = 1; back <= Math.min(3, bytes.length); back++) {
      const lead = bytes[bytes.length - back]; if ((lead & 0xc0) === 0x80) continue;
      const width = lead >= 0xc2 && lead <= 0xdf ? 2 : lead >= 0xe0 && lead <= 0xef ? 3 : lead >= 0xf0 && lead <= 0xf4 ? 4 : 0;
      if (!width || back >= width) return 0;
      const padded = new Uint8Array(width); padded.fill(0x80); padded.set(bytes.subarray(bytes.length - back));
      try { strict.decode(padded); return back; } catch { return 0; }
    }
    return 0;
  };
  // V38 (net::MAX_READ_DELIVERY_BYTES): one read delivers at most this many
  // body bytes (a text read: the bytes it decodes, held-back bytes included),
  // so its guest work fits one turn's fuel; a larger Fetch chunk waits in
  // `buffer` for the next reads, which receive nothing new.
  const cap = N.maxReadBytes;
  const pull = async (bytes) => {
    // V38 (net::BodyBound::Read): the bound counts what this read receives;
    // a body has no total byte bound.
    let received = 0;
    try {
      while (true) {
        if (bytes && (carry.length || buffer.length)) {
          const count = Math.min(buffer.length, cap - carry.length), data = new Uint8Array(carry.length + count);
          data.set(carry); data.set(buffer.subarray(0, count), carry.length); carry = new Uint8Array(0); buffer = buffer.subarray(count);
          let text = "";
          // Keep argument counts bounded even on engines with smaller JS stacks.
          const unitsPerCopy = 4096;
          for (let at = 0; at < data.length; at += unitsPerCopy) text += String.fromCharCode(...data.subarray(at, at + unitsPerCopy));
          return { kind: C.chunk, text };
        }
        if (buffer.length) {
          const count = Math.min(buffer.length, cap - carry.length), input = new Uint8Array(carry.length + count);
          input.set(carry); input.set(buffer.subarray(0, count), carry.length); buffer = buffer.subarray(count);
          const keep = incomplete(input); carry = input.slice(input.length - keep);
          const text = utf8.decode(input.subarray(0, input.length - keep));
          if (text) return { kind: C.chunk, text };
          continue;
        }
        if (ended) {
          if (carry.length) { carry = new Uint8Array(0); return { kind: C.chunk, text: "\ufffd" }; }
          return { kind: C.end, text: "" };
        }
        const { value, done } = await within(reader.read()); ended = done;
        if (!done) {
          received += value.byteLength;
          if (received > N.maxBodyBytes) { cancel(); return failed(N.errors.bodyLimit); }
          buffer = value;
        }
      }
    } catch (error) {
      cancel();
      return failed(error === expired ? N.errors.bodyTimeout : `browser response body failed: ${error.message ?? error}`);
    }
  };
  return { head, pull, cancel };
}
// Recorded network replay (net::replay): an embedder-supplied log replaces
// Fetch. snapshotNetworkLog validates the log's bounded JSON text with the
// native reader's rules (strictLogJson refuses what serde refuses and
// JSON.parse admits); replayTransport serves it. createNetwork plans and
// authorizes every request first, as for Fetch, so a replay never bypasses
// net.request; the Worker also refuses a log naming an ungranted request
// before the instance starts. Requests claim the logged requests when they
// start, in start order (same method and URL), even when their transfer is
// created later or never; logged events are released strictly in log
// order, each once its request's operation is armed (its head, or a pull in
// the logged read mode). An abort must meet the log's abort of the same
// request. A released completion settles a promise, so it reaches the event
// loop as a Fetch completion does: after the turn and its idle period. Steps
// are not interpreted: the browser host has no scripted session.
function snapshotNetworkLog(input) {
  const R = registry.network.replay, N = registry.network;
  const fail = (reason) => { throw new TypeError(`invalid network log: ${reason}`); };
  let bytes;
  if (typeof input === "string") bytes = new TextEncoder().encode(input);
  else if (input instanceof Uint8Array) bytes = input;
  else fail("expected its JSON text");
  if (bytes.byteLength > R.maxLogBytes) fail("too large");
  // A byte order mark is no JSON whitespace for the native reader either.
  let log, text;
  try { text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes); log = JSON.parse(text); } catch { fail("no JSON"); }
  if (!strictLogJson(text)) fail("no log JSON");
  const exact = (value, keys) => value && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
  // Steps and request indices are unsigned 32-bit integers, as natively.
  const count = (value) => Number.isSafeInteger(value) && value >= 0 && value <= R.maxIndex;
  const token = (value) => typeof value === "string" && /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(value);
  const fieldText = (value) => typeof value === "string" && /^[\t -~\u0080-ÿ]*$/.test(value);
  const canonicalUrl = (value) => { try { return typeof value === "string" && new TextEncoder().encode(value).length <= N.maxUrlBytes && new URL(value).href === value; } catch { return false; } };
  if (!exact(log, ["format", "version", "requests", "events"]) || log.format !== R.format || log.version !== R.version) fail("format");
  if (!Array.isArray(log.requests) || log.requests.length > R.maxRequests) fail("requests");
  if (!Array.isArray(log.events) || log.events.length > R.maxEvents) fail("events");
  const requests = log.requests.map((request) => {
    if (!exact(request, ["method", "url"]) || !token(request.method)
      || N.forbiddenMethods.some((name) => name.toLowerCase() === request.method.toLowerCase()) || !canonicalUrl(request.url)) fail("request");
    return { method: request.method, url: request.url };
  });
  const fields = { head: ["status", "statusText", "location", "headers"], chunk: ["read", "text"], end: [], failed: ["text"], abort: [] };
  const [head, chunk, end, failed, abort] = R.kinds;
  const lives = requests.map(() => "started");
  let step = R.unscriptedStep;
  const events = log.events.map((event) => {
    const kind = event?.kind;
    if (!Object.hasOwn(fields, kind) || !exact(event, ["kind", "step", "request", ...fields[kind]]) || !count(event.step) || !count(event.request) || event.request >= requests.length) fail("event");
    if (event.step < step) fail("step"); step = event.step;
    const life = lives[event.request];
    const next = kind === head && life === "started" ? "open" : kind === chunk && life === "open" ? "open"
      : kind === end && life === "open" || (kind === failed || kind === abort) && life !== "closed" ? "closed" : fail("order");
    lives[event.request] = next;
    if (kind === head) {
      if (!Number.isInteger(event.status) || event.status < R.minStatus || event.status > R.maxStatus || !fieldText(event.statusText)) fail("head");
      if (typeof event.location !== "string" || event.location !== "" && (new TextEncoder().encode(event.location).length > N.maxUrlBytes || !URL.canParse(event.location))) fail("location");
      if (!Array.isArray(event.headers) || event.headers.length % 2 || event.headers.length / 2 > N.maxResponseHeaders) fail("headers");
      let size = 0, previous;
      for (let i = 0; i < event.headers.length; i += 2) {
        const name = event.headers[i], value = event.headers[i + 1];
        if (!token(name) || name !== name.toLowerCase() || name.length > N.maxHeaderNameBytes || N.forbiddenResponseHeaders.includes(name)
          || previous !== undefined && previous >= name || !fieldText(value)) fail("headers");
        // Field text is one byte per code unit, on the wire and natively.
        size += name.length + value.length; previous = name;
        if (size > N.maxResponseHeaderBytes) fail("headers");
      }
      return { kind, step: event.step, request: event.request, status: event.status, statusText: event.statusText, location: event.location, headers: Array.from(event.headers) };
    }
    if (kind === chunk) {
      if (!R.reads.includes(event.read) || typeof event.text !== "string" || !event.text.length || event.text.length > N.maxReadBytes
        || event.read === R.reads[1] && !/^[\u0000-ÿ]*$/.test(event.text) || !event.text.isWellFormed()) fail("chunk");
      return { kind, step: event.step, request: event.request, read: event.read, text: event.text };
    }
    if (kind === failed) {
      if (typeof event.text !== "string" || !event.text.length || new TextEncoder().encode(event.text).length > R.maxFailureBytes) fail("failure");
      return { kind, step: event.step, request: event.request, text: event.text };
    }
    return { kind, step: event.step, request: event.request };
  });
  return { format: log.format, version: log.version, requests, events };
}
// What JSON.parse admits but the native (serde) reader of a log refuses, in
// text JSON.parse already accepted: a duplicate member name, a number that
// is no unsigned integer literal (every number of a log is one; a fraction,
// exponent or sign makes it a float or a negative value natively) and a
// string escaping a lone surrogate.
function strictLogJson(text) {
  const integer = /^(?:0|[1-9][0-9]*)$/, numeric = /[-+.eE0-9]/, quote = '"', escape = "\\";
  // Per open container: its member names (an object) or null (an array).
  const open = [];
  let name = false;
  for (let i = 0; i < text.length; i++) {
    const unit = text[i];
    if (unit === "{") { open.push(new Set()); name = true; }
    else if (unit === "[") { open.push(null); name = false; }
    else if (unit === "}" || unit === "]") { open.pop(); name = false; }
    else if (unit === ",") name = open.at(-1) instanceof Set;
    else if (unit === quote) {
      let end = i + 1;
      while (text[end] !== quote) end += text[end] === escape ? 2 : 1;
      const value = JSON.parse(text.slice(i, end + 1));
      if (!value.isWellFormed()) return false;
      if (name) { const names = open.at(-1); if (names.has(value)) return false; names.add(value); name = false; }
      i = end;
    } else if (numeric.test(unit)) {
      let end = i;
      while (end < text.length && numeric.test(text[end])) end++;
      if (!integer.test(text.slice(i, end))) return false;
      i = end - 1;
    }
  }
  return true;
}
function replayTransport(log) {
  const R = registry.network.replay, N = registry.network, C = N.completion;
  const [head, chunk, end, failed, abort] = R.kinds, [textRead, bytesRead] = R.reads;
  // A function replacement: a URL's `$` is never a replacement pattern.
  const fill = (template, values) => Object.entries(values).reduce((text, [name, value]) => text.replace(`{${name}}`, () => String(value)), template);
  const open = new Map();
  let claimed = 0, cursor = 0;
  const record = (event, plan) => {
    const [status, statusText, url, location, fields] = N.headFields, visible = [];
    for (let i = 0; i < event.headers.length; i += 2) {
      const name = event.headers[i];
      if (plan.visible.has(name) && !N.forbiddenResponseHeaders.includes(name)) visible.push(name, event.headers[i + 1]);
    }
    const value = { [status]: event.status, [statusText]: event.statusText, [url]: plan.url, [location]: event.location, [fields]: visible };
    return JSON.stringify(Object.fromEntries(Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)));
  };
  // Release logged events in order until one cannot be released yet.
  const release = () => {
    while (cursor < log.events.length) {
      const event = log.events[cursor], transfer = open.get(event.request);
      if (event.kind === abort || !transfer?.waiting) break;
      const { resolve, read } = transfer.waiting;
      let completion;
      if (event.kind === head) completion = { kind: C.head, text: record(event, transfer.plan) };
      else if (event.kind === chunk) {
        if (read === undefined) break;
        if (read !== event.read) throw new Error(fill(R.errors.read, { request: event.request, actual: read, logged: event.read }));
        completion = { kind: C.chunk, text: event.text };
      } else completion = event.kind === end ? { kind: C.end, text: "" } : { kind: C.failed, text: event.text };
      transfer.waiting = undefined; cursor++;
      if (event.kind !== head && event.kind !== chunk) { transfer.closed = true; open.delete(event.request); }
      resolve(completion);
    }
  };
  const claim = (plan) => {
    const logged = log.requests[claimed];
    if (!logged) throw new Error(fill(R.errors.unlogged, { method: plan.method, url: plan.url }));
    if (logged.method !== plan.method || logged.url !== plan.url) throw new Error(fill(R.errors.request, { request: claimed, method: plan.method, url: plan.url, loggedMethod: logged.method, loggedUrl: logged.url }));
    return claimed++;
  };
  const aborted = (index) => {
    const event = log.events[cursor];
    if (event?.kind !== abort || event.request !== index) throw new Error(fill(R.errors.abort, { request: index }));
    cursor++; release();
  };
  // The transfer of logged request `index`, claimed when its request started.
  const transport = (plan, index) => {
    const transfer = { plan, waiting: undefined, closed: false };
    const wait = (read) => new Promise((resolve) => { transfer.waiting = { resolve, read }; });
    open.set(index, transfer);
    const pending = wait(undefined);
    release();
    return {
      head: pending,
      pull(bytes) { const pulled = wait(bytes ? bytesRead : textRead); release(); return pulled; },
      // The guest aborted the open transfer; after its terminal event this is
      // the release that taking the event performs.
      cancel() { if (transfer.closed) return; transfer.closed = true; open.delete(index); aborted(index); },
      // The instance ended: no abort.
      close() { transfer.closed = true; open.delete(index); },
    };
  };
  return {
    claim,
    transport,
    // A request aborted before its transfer was created claimed its logged
    // request when it started, as natively; its abort is the log's next event.
    abandon(index) { aborted(index); },
    finish() { if (cursor < log.events.length) throw new Error(fill(R.errors.unconsumed, { remaining: log.events.length - cursor })); },
    position() { return { event: cursor, remaining: log.events.length - cursor }; },
  };
}
