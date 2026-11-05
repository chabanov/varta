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
  const pull = async (bytes) => {
    // V38 (net::BodyBound::Read): the bound counts what this read receives;
    // a body has no total byte bound.
    let received = 0;
    try {
      while (true) {
        if (bytes && (carry.length || buffer.length)) {
          const count = Math.min(buffer.length, N.maxChunkBytes - carry.length), data = new Uint8Array(carry.length + count);
          data.set(carry); data.set(buffer.subarray(0, count), carry.length); carry = new Uint8Array(0); buffer = buffer.subarray(count);
          let text = "";
          // Keep argument counts bounded even on engines with smaller JS stacks.
          const unitsPerCopy = 4096;
          for (let at = 0; at < data.length; at += unitsPerCopy) text += String.fromCharCode(...data.subarray(at, at + unitsPerCopy));
          return { kind: C.chunk, text };
        }
        if (buffer.length) {
          const count = Math.min(buffer.length, N.maxChunkBytes), input = new Uint8Array(carry.length + count);
          input.set(carry); input.set(buffer.subarray(0, count), carry.length); buffer = buffer.subarray(count);
          const keep = incomplete(input); carry = input.slice(input.length - keep);
          const text = utf8.decode(input.subarray(0, input.length - keep));
          if (text) return { kind: C.chunk, text };
          continue;
        }
        if (ended) {
          if (carry.length) { carry = new Uint8Array(0); return { kind: C.chunk, text: "�" }; }
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
