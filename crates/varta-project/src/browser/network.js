// A bounded slot/operation broker. Transport completions never run guest code;
// the event loop must explicitly deliver one event and drain its checkpoint.
// `transport` is Fetch's browserTransport or a recorded log's replay
// (replayTransport). A replay claims its logged request when the request
// starts (`claim`), in start order as natively, and creates the transfer of
// that claim later (`transport(plan, claim)`); it is told of a request
// aborted before its transfer was created (`abandon(claim)`), and an ended
// instance closes its open transfers instead of aborting them.
function createNetwork(scheduler, policy, emit, readDescriptor, writeResult, transport) {
  const N = registry.network, C = N.completion, encoder = new TextEncoder();
  const requests = new Map(), events = new Map(), deferred = new Map();
  let nextId = 0, taken, notify, observer;
  const fail = (message = N.errors.state) => { throw new Error(message); };
  const request = (id) => { if (!Number.isInteger(id) || id < 1 || !requests.has(id)) fail(); return requests.get(id); };
  const terminal = (kind) => kind !== C.head && kind !== C.chunk;
  const publish = (id, completion) => {
    const item = requests.get(id);
    if (!item || item.phase !== "armed" || item.cancelled) return;
    events.set(id, {completion,arrived:(globalThis.performance?.timeOrigin??0)+(globalThis.performance?.now()??0)}); notify?.(); observer?.();
  };
  const parse = (text) => {
    if (typeof text !== "string" || text.length * 2 > N.maxRequestBytes) fail(N.errors.descriptor);
    let offset = 0;
    const field = () => { const end = text.indexOf(N.separator, offset); if (end < 0) fail(N.errors.descriptor); const value = text.slice(offset, end); offset = end + N.separator.length; return value; };
    if (field() !== N.version) fail(N.errors.descriptor);
    const method = field(), url = field(), countText = field();
    if (!/^\+?[0-9]+$/.test(countText)) fail(N.errors.descriptor);
    const count = Number(countText); if (count > N.maxRequestHeaders) fail(N.errors.descriptor);
    const headers = []; for (let i = 0; i < count; i++) headers.push([field(), field()]);
    const flag = field(), rest = text.slice(offset);
    let body;
    if (flag === N.noBody && !rest) body = undefined;
    else if (flag === N.textBody) body = encoder.encode(rest);
    else if (flag === N.byteBody) {
      body = new Uint8Array(rest.length);
      for (let i = 0; i < rest.length; i++) { const unit = rest.charCodeAt(i); if (unit > 255) fail(N.errors.descriptor); body[i] = unit; }
    } else fail(N.errors.descriptor);
    return { method, url, headers, body };
  };
  const plan = (value) => {
    const failed = (text) => ({ kind: C.failed, text });
    if (!policy.method(value.method)) return failed(N.errors.method.replace("{value}", value.method));
    let url; try { url = new URL(value.url); } catch { return failed(N.errors.url.replace("{value}", value.url)); }
    if (!policy.allows(value.url, value.method)) {
      const text = `<capability denied: ${N.capability}> (${value.method} ${value.url})`;
      try { emit(text); } catch { return failed("output budget exhausted"); }
      return { kind: C.denied, text };
    }
    if (value.body !== undefined && ["GET", "HEAD"].includes(value.method)) return failed(N.errors.bodyNotAllowed);
    let size = 0;
    for (const [name, data] of value.headers) {
      size += encoder.encode(name).length + encoder.encode(data).length;
      if (size > N.maxRequestHeaderBytes) return failed(N.errors.headerBytes);
      if (N.runtimeHeaders.includes(name.toLowerCase())) return failed(N.errors.runtimeHeader.replace("{value}", name));
      if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name)) return failed(N.errors.headerName.replace("{value}", name));
      // Hyper HeaderValue permits HTAB and visible/obs-text bytes, but no DEL.
      for (const unit of data) { const code = unit.codePointAt(0); if (code > 255 || code === 127 || code < 32 && code !== 9) return failed(N.errors.headerValue.replace("{value}", name.toLowerCase())); }
    }
    return { ...value, url: url.href, visible: policy.visibleHeaders() };
  };
  const cancel = (item) => { item.cancelled = true; item.transfer?.cancel(); };
  const implementations = {
    net_request_start_v2(ptr, length) {
      const value = parse(readDescriptor(ptr, length));
      if (requests.size >= N.maxPending) fail(N.errors.budget);
      const outcome = plan(value);
      if (!Number.isSafeInteger(nextId + 1)) fail();
      const id = ++nextId;
      // Only a request that reaches the transport claims a logged request.
      const claim = outcome.kind ? undefined : transport.claim?.(outcome);
      const item = { phase: outcome.kind ? "ready" : "armed", completion: outcome.kind ? outcome : undefined, watcher: undefined, cancelled: false, transfer: undefined, claim };
      requests.set(id, item);
      if (!outcome.kind) deferred.set(id, () => {
        if (requests.get(id) !== item || item.cancelled) return;
        const transfer = transport(outcome, claim); item.transfer = transfer;
        transfer.head.then((completion) => publish(id, completion), (error) => publish(id, { kind: C.failed, text: String(error.message ?? error) }));
      });
      return id;
    },
    net_request_watch(serial, id) {
      serial = scheduler.watchable(serial);
      const item = request(id); if (item.watcher !== undefined) fail();
      if (item.phase === "armed") item.watcher = serial;
      else if (item.phase === "ready") scheduler.wake(serial);
      else fail();
    },
    net_request_take(id) {
      const item = request(id); if (item.phase !== "ready") fail();
      const completion = item.completion; item.completion = undefined; item.phase = "idle";
      if (terminal(completion.kind)) { cancel(item); requests.delete(id); events.delete(id); }
      taken = { id, text: completion.text }; return completion.kind;
    },
    net_request_text(id) {
      const value = taken; taken = undefined; if (!value || value.id !== id) fail();
      return writeResult(value.text);
    },
    net_body_read(id) { read(id, false); },
    net_body_read_bytes(id) { read(id, true); },
    net_request_abort(id) {
      const item = requests.get(id); if (!item) return;
      cancel(item); events.delete(id);
      if (deferred.delete(id) && !item.transfer && item.claim !== undefined) transport.abandon?.(item.claim);
      if (item.phase === "idle") { requests.delete(id); return; }
      item.phase = "ready"; item.completion = { kind: C.aborted, text: "" };
      if (item.watcher !== undefined) { scheduler.wake(item.watcher); item.watcher = undefined; }
    },
  };
  const read = (id, bytes) => {
    const item = request(id); if (item.phase !== "idle" || item.watcher !== undefined || !item.transfer || item.cancelled) fail();
    item.phase = "armed";
    deferred.set(id, () => {
      if (requests.get(id) !== item || item.cancelled) return;
      item.transfer.pull(bytes).then((completion) => publish(id, completion), (error) => publish(id, { kind: C.failed, text: String(error.message ?? error) }));
    });
  };
  return {
    implementations, parse,
    onReady(callback) { observer = callback; },
    arrival() { return events.values().next().value?.arrived; },
    alive() { return [...requests.values()].some((item) => item.phase === "armed" && item.watcher !== undefined); },
    pump() { for (const [id, run] of deferred) { deferred.delete(id); run(); } },
    deliver() {
      const first = events.entries().next(); if (first.done) return false;
      const [id, {completion}] = first.value; events.delete(id);
      const item = request(id); if (item.phase !== "armed") fail();
      item.phase = "ready"; item.completion = completion;
      if (item.watcher !== undefined) { scheduler.wake(item.watcher); item.watcher = undefined; }
      return true;
    },
    wait(milliseconds) {
      if (events.size) return Promise.resolve();
      return new Promise((resolve) => {
        const timer = setTimeout(() => { notify = undefined; resolve(); }, milliseconds);
        notify = () => { clearTimeout(timer); notify = undefined; resolve(); };
      });
    },
    close() { for (const item of requests.values()) { item.cancelled = true; if (item.transfer?.close) item.transfer.close(); else item.transfer?.cancel(); } requests.clear(); events.clear(); deferred.clear(); notify?.(); },
  };
}
