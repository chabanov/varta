// Network requests (embedding.md "Network requests (v37)"): the `net_request_*`
// imports over the host's `fetch`. Checks run in the order of the generated
// failure and misuse tables; completions go into one FIFO queue that the event
// loop delivers one per turn.
//
// Under v38 the head record is the contract's canonical head: Fetch `Headers`
// already sort and combine, and only the grant's visible names are kept (a
// cross-origin server must expose declared names with
// `Access-Control-Expose-Headers`, which is deployment configuration).
// Browser and fetch deviations, stated by `guarantees`: v37 response headers
// come from the Fetch `Headers` too (names sorted, repeated fields combined,
// except `set-cookie` where the runtime exposes it), a browser reads only the
// headers CORS exposes and adds its own request headers, and a browser cannot
// observe a redirect (`redirect: "manual"` yields an opaque response, reported
// as a failure).
import { ExecutionFailure } from "./errors.js";
import { hostLimit, now } from "./context.js";
import { parseDescriptor, descriptorRules, checkDescriptorExamples, DescriptorError } from "./net-descriptor.js";

const TOKEN = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
const BYTE_STRING_MAX = 0xff;
const HTAB = 0x09;
const SPACE = 0x20;
const DELETE = 0x7f;
const UTF16_UNIT_BYTES = 2;
const VISIBLE_ASCII = /^[\x21-\x7e]*$/;
const BODYLESS_METHODS = new Set(["GET", "HEAD"]);
const REPLACEMENT = "�";
const lenientUtf16 = new TextDecoder("utf-16le");
const lenientUtf8 = new TextDecoder("utf-8");

const NET_LIMIT = {
  pending: "Requests holding a runtime slot at once",
  descriptorBytes: "Request descriptor, body included (UTF-16 bytes)",
  headerBytes: "Request header name and value bytes",
  chunkBytes: "Body bytes of one `net_body_read` completion",
  bodyBytes: "Body bytes of one response",
};

/** Contract facts of the network imports, resolved once per host. */
export function netConstants(contract) {
  const net = contract.net;
  const rules = descriptorRules(net);
  checkDescriptorExamples(rules, net);
  const forbiddenResponseHeaders = new Set(contract.manifestLimits.forbiddenResponseHeaders);
  if (net.headExample !== null) checkHeadExample(net.headExample, forbiddenResponseHeaders);
  const limit = (label) => hostLimit(contract, label, null);
  const failure = (text) => {
    const row = net.failures.find((f) => f.text.startsWith(text));
    if (!row) throw new Error(`contract has no network failure ${text}`);
    return row.text;
  };
  return {
    rules,
    code: Object.fromEntries(Object.entries(net.completions).map(([kind, c]) => [kind, c.code])),
    releases: new Set(Object.values(net.completions).filter((c) => c.releases).map((c) => c.code)),
    runtimeHeaders: new Set(net.runtimeHeaders),
    errors: net.errors,
    failures: {
      method: failure("invalid method: "),
      url: failure("Invalid URL: "),
      denied: failure("<capability denied: net.request>"),
      output: failure("output budget exhausted"),
      bodyless: failure("a GET or HEAD request cannot have a body"),
      headerBytes: failure("request headers are too large"),
      runtimeHeader: failure("header <name> is set by the runtime"),
      headerName: failure("invalid header name: "),
      headerValue: failure("invalid header value for "),
      connect: failure("connect failed: "),
      request: failure("request failed: "),
      body: failure("response body failed: "),
      bodyTooLarge: failure("response body exceeds "),
    },
    pending: limit(NET_LIMIT.pending),
    descriptorBytes: limit(NET_LIMIT.descriptorBytes),
    headerBytes: limit(NET_LIMIT.headerBytes),
    chunkBytes: limit(NET_LIMIT.chunkBytes),
    bodyBytes: limit(NET_LIMIT.bodyBytes),
    forbiddenMethods: new Set(contract.manifestLimits.forbiddenMethods),
    forbiddenResponseHeaders,
  };
}

const fill = (template, values) => template.replace(/<([a-z]+)>/g, (whole, key) => (key in values ? values[key] : whole));

/**
 * The head record (embedding.md "Head record"): compact JSON, members in name
 * order, lowercase header names as name, value pairs. Fetch `Headers` iterate
 * sorted by name with each `set-cookie` its own pair and other repeated fields
 * combined: under v38 (`visible` given) that is the canonical head once the
 * names outside the visible set and the forbidden names are dropped; under
 * v37 (`visible` null) the order and combining are this host's stated
 * deviation.
 */
function headRecord(headers, status, statusText, url, visible, forbidden) {
  const pairs = [];
  for (const [name, value] of headers) {
    if (visible !== null && (!visible.has(name) || forbidden.has(name))) continue;
    pairs.push(name, value);
  }
  const locationValue = headers.get("location");
  let location = "";
  if (locationValue !== null && VISIBLE_ASCII.test(locationValue)) {
    try {
      location = new URL(locationValue, url).href;
    } catch {
      location = "";
    }
  }
  return JSON.stringify({ headers: pairs, location, status, statusText, url });
}

/**
 * The contract's v38 head example, rebuilt through `headRecord` from its own
 * pairs plus a forbidden field, with its names as the visible set; a
 * disagreement stops the host.
 */
function checkHeadExample(example, forbidden) {
  const record = JSON.parse(example);
  const fields = [];
  for (let i = 0; i < record.headers.length; i += 2) fields.push([record.headers[i], record.headers[i + 1]]);
  const visible = new Set(fields.map(([name]) => name));
  for (const name of forbidden) fields.push([name, "hidden"]);
  const rebuilt = headRecord(new Headers(fields), record.status, record.statusText, record.url, visible, forbidden);
  if (rebuilt !== example) throw new Error(`head record disagrees with the contract's example: ${rebuilt}`);
}

/** Network fields of one execution; `notify` wakes a loop waiting for a completion. */
export function netState() {
  return { requests: new Map(), nextId: 1, queue: [], lastTake: null, notify: null };
}

/** A field value admits HTAB, SP, visible ASCII and obs-text; never another control or DEL. */
function validFieldValue(value) {
  for (let i = 0; i < value.length; i += 1) {
    const unit = value.charCodeAt(i);
    if (unit > BYTE_STRING_MAX) return false;
    if (unit !== HTAB && (unit < SPACE || unit === DELETE)) return false;
  }
  return true;
}

/** Bytes of an incomplete UTF-8 sequence at the end of `bytes` that could still become valid. */
function heldBackLength(bytes) {
  for (let back = 1; back <= Math.min(3, bytes.length); back += 1) {
    const lead = bytes[bytes.length - back];
    if ((lead & 0xc0) === 0x80) continue;
    const need = lead >= 0xf0 && lead <= 0xf4 ? 4 : lead >= 0xe0 ? 3 : lead >= 0xc2 && lead < 0xe0 ? 2 : 1;
    return need > back ? back : 0;
  }
  return 0;
}

const byteUnits = (bytes) => {
  let text = "";
  for (const b of bytes) text += String.fromCharCode(b);
  return text;
};

/**
 * The network imports of one execution. `io` supplies the execution's helpers:
 * guestRange, emit (a log line, charged to output), writeResult, authorize(id, url,
 * method), requireIssued(serial), isTimerSerial(serial), wake(serial), schedulerError.
 */
export function netImports(N, exec, io) {
  const state = exec.net;
  const fatal = (name) => new ExecutionFailure(N.errors[`NetError::${name}`]);
  const positiveId = (id) => Number.isInteger(id) && id > 0;

  function request(id) {
    if (!positiveId(id) || !state.requests.has(id)) throw fatal("State");
    return state.requests.get(id);
  }

  /** A completion produced by the transfer: queued, delivered one per turn. */
  function report(r, code, text) {
    state.queue.push({ id: r.id, generation: r.generation, completion: { code, text }, arrived: now() });
    state.notify?.();
  }

  /** A completion that is waiting when start or abort returns: no turn, its waiter wakes now. */
  function settleNow(r, code, text) {
    r.completion = { code, text };
    r.armed = false;
    if (r.waiter !== null) {
      io.wake(r.waiter);
      r.waiter = null;
    }
  }

  async function transfer(r, fields) {
    let response;
    try {
      response = await fetch(r.url, { method: fields.method, headers: fields.headers, body: fields.body ?? undefined, redirect: "manual", signal: r.controller.signal });
    } catch (error) {
      if (r.controller.signal.aborted) return;
      const cause = error?.cause;
      const connect = cause && ["ECONNREFUSED", "ENOTFOUND", "ECONNRESET", "EAI_AGAIN"].includes(cause.code);
      report(r, N.code.Failed, fill(connect ? N.failures.connect : N.failures.request, { error: String(cause?.message ?? error?.message ?? error) }));
      return;
    }
    if (response.type === "opaqueredirect") {
      report(r, N.code.Failed, fill(N.failures.request, { error: "the browser does not expose redirect responses" }));
      return;
    }
    r.reader = response.body ? response.body.getReader() : null;
    report(r, N.code.Head, headRecord(response.headers, response.status, response.statusText, r.url, r.visible, N.forbiddenResponseHeaders));
  }

  /** Serves one armed read: the buffered bytes now, else waits for the body stream. */
  async function serveRead(r) {
    const generation = r.generation;
    for (;;) {
      if (r.generation !== generation) return;
      const available = r.held.length + r.buffered.length;
      if (available > 0 && (r.mode === "bytes" || r.buffered.length > 0 || r.done)) {
        const piece = r.buffered.subarray(0, Math.max(0, N.chunkBytes - r.held.length));
        r.buffered = r.buffered.subarray(piece.length);
        const data = new Uint8Array(r.held.length + piece.length);
        data.set(r.held);
        data.set(piece, r.held.length);
        r.held = new Uint8Array(0);
        if (r.mode === "bytes") {
          report(r, N.code.Chunk, byteUnits(data));
          return;
        }
        const hold = r.done && r.buffered.length === 0 ? 0 : heldBackLength(data);
        const text = lenientUtf8.decode(data.subarray(0, data.length - hold));
        r.held = data.slice(data.length - hold);
        if (text !== "") {
          report(r, N.code.Chunk, text);
          return;
        }
      }
      if (r.done) {
        if (r.held.length > 0 && r.mode === "text") {
          r.held = new Uint8Array(0);
          report(r, N.code.Chunk, REPLACEMENT);
          return;
        }
        report(r, N.code.End, "");
        return;
      }
      let next;
      try {
        next = await r.reader.read();
      } catch (error) {
        if (r.controller.signal.aborted) return;
        report(r, N.code.Failed, fill(N.failures.body, { error: String(error?.message ?? error) }));
        return;
      }
      if (next.done) r.done = true;
      else {
        r.received += next.value.length;
        if (r.received > N.bodyBytes) {
          report(r, N.code.Failed, N.failures.bodyTooLarge);
          return;
        }
        const merged = new Uint8Array(r.buffered.length + next.value.length);
        merged.set(r.buffered);
        merged.set(next.value, r.buffered.length);
        r.buffered = merged;
      }
    }
  }

  function start(importName, ptr, len) {
    const descriptorError = () => {
      throw fatal("Descriptor");
    };
    if (!Number.isInteger(ptr) || !Number.isInteger(len) || ptr < 0 || len < 0 || len % UTF16_UNIT_BYTES || len > N.descriptorBytes) descriptorError();
    const bytes = io.guestRange(ptr, len, N.descriptorBytes, descriptorError);
    let fields;
    try {
      fields = parseDescriptor(N.rules, importName, lenientUtf16.decode(bytes));
    } catch (error) {
      if (error instanceof DescriptorError) descriptorError();
      throw error;
    }
    const id = state.nextId++;
    // v38 (`net_request_start_v2`) shows only the visible response headers; v37 every one.
    const visible = importName === "net_request_start_v2" ? io.responseHeaders() : null;
    const r = { id, generation: 0, url: fields.url, visible, armed: false, completion: null, waiter: null, controller: new AbortController(), reader: null, buffered: new Uint8Array(0), held: new Uint8Array(0), received: 0, done: false, mode: "text" };
    const failNow = (text) => {
      state.requests.set(id, r);
      settleNow(r, N.code.Failed, text);
      return id;
    };
    const method = fields.method;
    if (!TOKEN.test(method) || N.forbiddenMethods.has(method.toUpperCase())) return failNow(fill(N.failures.method, { method }));
    let parsed;
    try {
      parsed = new URL(fields.url);
    } catch {
      return failNow(fill(N.failures.url, { url: fields.url }));
    }
    r.url = parsed.href;
    if (!io.authorize(parsed, method)) {
      const text = fill(N.failures.denied, { method, url: fields.url });
      try {
        io.emit(text);
      } catch (error) {
        if (!(error instanceof ExecutionFailure)) throw error;
        return failNow(N.failures.output);
      }
      if (state.requests.size >= N.pending) throw fatal("Budget");
      state.requests.set(id, r);
      settleNow(r, N.code.Denied, text);
      return id;
    }
    if (state.requests.size >= N.pending) throw fatal("Budget");
    if (fields.body !== null && BODYLESS_METHODS.has(method.toUpperCase())) return failNow(N.failures.bodyless);
    let headerBytes = 0;
    for (const [name, value] of fields.headers) {
      headerBytes += name.length + value.length;
      if (headerBytes > N.headerBytes) return failNow(N.failures.headerBytes);
      if (N.runtimeHeaders.has(name.toLowerCase())) return failNow(fill(N.failures.runtimeHeader, { name }));
      if (!TOKEN.test(name)) return failNow(fill(N.failures.headerName, { name }));
      if (!validFieldValue(value)) return failNow(fill(N.failures.headerValue, { name: name.toLowerCase() }));
    }
    state.requests.set(id, r);
    r.armed = true;
    transfer(r, { method, headers: fields.headers.map(([name, value]) => [name.toLowerCase(), value]), body: fields.body });
    return id;
  }

  return {
    net_request_start: (ptr, len) => start("net_request_start", ptr, len),
    net_request_start_v2: (ptr, len) => start("net_request_start_v2", ptr, len),
    net_request_watch: (serial, id) => {
      if (!io.requireIssued(serial) || serial === 0 || io.isTimerSerial(serial) || (exec.reactions.get(serial) ?? []).length !== 1) throw io.schedulerError("UnknownSerial");
      const r = request(id);
      if (r.waiter !== null) throw fatal("State");
      if (!r.armed && r.completion === null) throw fatal("State");
      if (r.completion !== null) io.wake(serial);
      else r.waiter = serial;
    },
    net_request_take: (id) => {
      const r = request(id);
      if (r.completion === null) throw fatal("State");
      const { code, text } = r.completion;
      r.completion = null;
      state.lastTake = { id, text };
      if (N.releases.has(code)) state.requests.delete(id);
      return code;
    },
    net_request_text: (id) => {
      const last = state.lastTake;
      if (!positiveId(id) || last === null || last.id !== id) throw fatal("State");
      state.lastTake = null;
      return io.writeResult(last.text);
    },
    net_body_read: (id) => {
      const r = request(id);
      if (r.armed || r.completion !== null || r.waiter !== null) throw fatal("State");
      r.armed = true;
      r.mode = "text";
      serveRead(r);
    },
    net_body_read_bytes: (id) => {
      const r = request(id);
      if (r.armed || r.completion !== null || r.waiter !== null) throw fatal("State");
      r.armed = true;
      r.mode = "bytes";
      serveRead(r);
    },
    net_request_abort: (id) => {
      if (!positiveId(id) || !state.requests.has(id)) return;
      const r = state.requests.get(id);
      r.generation += 1;
      r.controller.abort();
      if (!r.armed && r.completion === null) {
        state.requests.delete(id);
        return;
      }
      settleNow(r, N.code.Aborted, "");
    },
  };
}

/** True while some request keeps the loop alive: an operation armed and a waiter registered. */
export function requestKeepsAlive(exec) {
  for (const r of exec.net.requests.values()) if (r.armed && r.waiter !== null) return true;
  return false;
}

/**
 * Delivers the oldest queued completion (one turn): its waiter's reaction moves to
 * the microtask queue. A completion of an aborted or released request is discarded.
 */
export function deliverCompletion(exec, io) {
  const item = exec.net.queue.shift();
  const r = exec.net.requests.get(item.id);
  if (!r || r.generation !== item.generation || !r.armed) return;
  r.armed = false;
  r.completion = item.completion;
  if (r.waiter !== null) {
    io.wake(r.waiter);
    r.waiter = null;
  }
}

/** Waits until a completion is queued or `ms` passes; true when one is queued. */
export function waitForCompletion(exec, ms) {
  const state = exec.net;
  if (state.queue.length > 0) return Promise.resolve(true);
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      state.notify = null;
      resolve(false);
    }, Math.max(0, ms));
    state.notify = () => {
      clearTimeout(timer);
      state.notify = null;
      resolve(true);
    };
  });
}
