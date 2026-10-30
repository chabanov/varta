// Cases shared by the Node differential test and the browser runner. Log lines are
// compared verbatim: their reference text is normative (embedding.md "Output and
// its budget"); only the human `error` message of failures is ignored.

export const DENY_ALL_CEILING = JSON.stringify({ capabilities: [] });

export const invokeCases = [
  { label: "echo cyrillic+emoji", tool: "echo", params: JSON.stringify({ value: "Привіт, Varta 😀" }) },
  { label: "echo escapes", tool: "echo", params: '{"value":"tab\\tquote\\"slash\\\\ \\u00e9"}' },
  { label: "add", tool: "add", params: '{"a":40,"b":2}' },
  { label: "add wraps", tool: "add", params: '{"a":2147483647,"b":1}' },
  { label: "add negative", tool: "add", params: '{"a":-5,"b":-7}' },
  { label: "negate", tool: "negate", params: '{"flag":true}' },
  { label: "void with logs", tool: "shout", params: '{"value":"hey"}' },
  { label: "env granted", tool: "app.name", params: "{}", env: { APP_NAME: "Console" } },
  { label: "env unset", tool: "app.name", params: "{}" },
  { label: "env denied by ceiling", tool: "app.name", params: "{}", env: { APP_NAME: "Console" }, ceiling: DENY_ALL_CEILING },
  { label: "unknown tool", tool: "missing", params: "{}" },
  { label: "missing argument", tool: "add", params: '{"a":1}' },
  { label: "extra argument", tool: "add", params: '{"a":1,"b":2,"c":3}' },
  { label: "duplicate argument", tool: "add", params: '{"a":1,"a":2,"b":3}' },
  { label: "fraction", tool: "add", params: '{"a":1.0,"b":2}' },
  { label: "exponent", tool: "add", params: '{"a":1e2,"b":2}' },
  { label: "negative zero", tool: "add", params: '{"a":-0,"b":2}' },
  { label: "outside i32", tool: "add", params: '{"a":2147483648,"b":2}' },
  { label: "string for integer", tool: "add", params: '{"a":"1","b":2}' },
  { label: "lone surrogate", tool: "echo", params: '{"value":"\\ud800"}' },
  { label: "trailing data", tool: "add", params: '{"a":1,"b":2} x' },
  { label: "not an object", tool: "add", params: "[1,2]" },
  { label: "boolean as number", tool: "negate", params: '{"flag":1}' },
];

export const runCases = [
  { label: "R1 i32 main with env", fixture: "run-i32", env: { APP_NAME: "Console" } },
  { label: "R1 i32 main without env", fixture: "run-i32", env: {} },
  { label: "R1 denied clock and exit, env.print", fixture: "denied-i32", env: { APP_NAME: "Console" } },
  { label: "R1 authorized exit", fixture: "exit-i32", env: {} },
  { label: "R1 log_f64 text (f64-utf8-v1)", fixture: "f64-v1", env: {} },
  { label: "R2 v36 class/async/try with frame admission", fixture: "run-v36", env: {} },
  { label: "R2 v36 timers: setTimeout, clearTimeout, setInterval", fixture: "timers-v36", env: {} },
  { label: "R2 v36 timer without time.timer is fatal (TimerDenied text)", fixture: "timers-denied-v36", env: {}, errorText: true },
  { label: "R2 v36 timer due beyond the deadline (TimerDeadline text)", fixture: "timer-deadline-v36", env: {}, errorText: true },
  { label: "R2 v36 33 pending timers (TimerBudget text)", fixture: "timer-budget-v36", env: {}, errorText: true },
  { label: "R2 v36 unhandled rejection (UnhandledRejection text, reason normalized)", fixture: "unhandled-v36", env: {}, errorText: true },
];

// `expect` defaults to "refused". Catalog cases are contract checks of the JS host
// (tampered sections the native compiler never emits), not native comparisons.
export const refusalCases = [
  { label: "refuses unstable JSON intrinsics", fixture: "json-v36", pattern: "does not implement json_stringify" },
  { label: "rule 4: bindings[].ty out of range", fixture: "run-v36", catalog: (c) => (c.bindings[0].ty = 9999), pattern: "bindings[].ty index out of range" },
  { label: "rule 4: frames[].slots[].binding out of range", fixture: "run-v36", catalog: (c) => (c.frames[0].slots.push({ binding: 9999, shared: false })), pattern: "frames[].slots[].binding index out of range" },
  { label: "rule 4: duplicate frames[].code", fixture: "run-v36", catalog: (c) => c.frames.push(JSON.parse(JSON.stringify(c.frames[0]))), pattern: "frames[].code must be unique" },
  { label: "rule 4: functions[].signature is bounded, not an index", fixture: "run-v36", catalog: (c) => c.functions.push({ code: 250, signature: 9999, captures: [] }), pattern: "functions[].signature must be below" },
  { label: "rule 4: unknown type kind", fixture: "run-v36", catalog: (c) => c.types.push({ kind: "symbol" }), pattern: "unknown kind" },
  { label: "contract check, optionality: states[].yielded is never null", fixture: "run-v36", catalog: (c) => (c.frames[0].states[0].yielded = null), pattern: "yielded must not be null" },
  { label: "contract check, optionality: generator may be absent but not null", fixture: "run-v36", catalog: (c) => (c.frames[0].generator = null), pattern: "generator must not be null" },
  { label: "contract check, optionality: saved may be absent but not null", fixture: "run-v36", catalog: (c) => (c.frames[0].states[0].saved = null), pattern: "saved must not be null" },
  { label: "contract check, optionality: liveness layouts require capacity", fixture: "run-v36", catalog: (c) => (c.frames[0].capacity = null), pattern: "capacity is required" },
  { label: "contract check, optionality: unknown binding member", fixture: "run-v36", catalog: (c) => (c.bindings[0].extra = true), pattern: "unknown member extra" },
  { label: "contract check, optionality: result and return_input may be absent", fixture: "run-v36", catalog: (c) => { delete c.frames[0].result; delete c.frames[0].return_input; }, expect: "accepted" },
  { label: "contract check, optionality: bad states[].input kind", fixture: "run-v36", catalog: (c) => (c.frames[0].states[0].input = "number"), pattern: "input must be one of" },
  { label: "contract check, catalog: input is never the string \"null\"", fixture: "run-v36", catalog: (c) => (c.frames[0].states[0].input = "null"), pattern: "input must be one of" },
  { label: "contract check, catalog: return_input is never the string \"null\"", fixture: "run-v36", catalog: (c) => (c.frames[0].return_input = "null"), pattern: "return_input must be one of" },
  { label: "contract check, catalog: return_input may be JSON null", fixture: "run-v36", catalog: (c) => (c.frames[0].return_input = null), expect: "accepted" },
  { label: "contract check, catalog: saved[] elements are exactly {binding, shared}", fixture: "run-v36", catalog: (c) => (c.frames[0].states[0].saved = [{ binding: 0, shared: false, extra: 1 }]), pattern: "saved[] has unknown member extra" },
  { label: "contract check, catalog: kinds without members accept only kind", fixture: "run-v36", catalog: (c) => c.types.push({ kind: "promise", members: [] }), pattern: "types[] has unknown member members" },
  { label: "contract check, catalog: error class may be absent", fixture: "run-v36", catalog: (c) => c.types.push({ kind: "error" }), expect: "accepted" },
  { label: "contract check, catalog: record fields ascending by UTF-8 bytes", fixture: "run-v36", catalog: (c) => c.types.push({ kind: "record", fields: [{ name: "b", ty: 0, optional: false }, { name: "a", ty: 0, optional: false }] }), pattern: "ascending by UTF-8 bytes" },
  { label: "contract check, catalog: UTF-8 order, not UTF-16 order", fixture: "run-v36", catalog: (c) => c.types.push({ kind: "record", fields: [{ name: "\u{10000}", ty: 0, optional: false }, { name: "\uffff", ty: 0, optional: false }] }), pattern: "ascending by UTF-8 bytes" },
  { label: "contract check, catalog: union members distinct", fixture: "run-v36", catalog: (c) => c.types.push({ kind: "primitiveUnion", members: [0, 0] }), pattern: "at least two distinct" },
  { label: "contract check, catalog: union needs two members", fixture: "run-v36", catalog: (c) => c.types.push({ kind: "primitiveUnion", members: [0] }), pattern: "at least two distinct" },
];

// Contract checks of the misuse table (embedding.md "Which error each misuse
// raises"): imports called directly on a fresh execution; the observed value is the
// error name whose reference text the host reported.
const TIMER_GRANT = JSON.stringify({ capabilities: [{ id: "time.timer", enabled: true }] });
export const schedulerCases = [
  { label: "promise_react: serial not issued", expect: "UnknownSerial", steps: (call) => call("promise_react", 5, 1n) },
  { label: "promise_react: non-integer serial", expect: "UnknownSerial", steps: (call) => call("promise_react", 1.5, 1n) },
  { label: "promise_react: settled serial", expect: "UnknownSerial", steps: (call) => { call("promise_react", 0, 1n); call("promise_settle", 1); call("promise_react", 1, 2n); } },
  { label: "promise_settle: double settle", expect: "UnknownSerial", steps: (call) => { call("promise_react", 0, 1n); call("promise_settle", 1); call("promise_settle", 1); } },
  { label: "promise_handled: repeated withdrawal", expect: "UnknownSerial", steps: (call) => { call("promise_unhandled", 0n); call("promise_handled", 1); call("promise_handled", 1); } },
  { label: "promise_handled: id never reported", expect: "UnknownSerial", steps: (call) => call("promise_handled", 7) },
  { label: "scheduler_enqueue: token already held", expect: "DuplicateToken", steps: (call) => { call("scheduler_enqueue", 5n); call("scheduler_enqueue", 5n); } },
  { label: "scheduler_enqueue: QueueFull is checked before DuplicateToken", expect: "QueueFull", steps: (call) => { for (let t = 1n; t <= 64n; t += 1n) call("scheduler_enqueue", t); call("scheduler_enqueue", 1n); } },
  { label: "timer_start: TimerDenied is checked before flags", expect: "TimerDenied", steps: (call) => call("timer_start", 9, 10, 99) },
  { label: "timer_start: flags are checked before the serial", manifest: TIMER_GRANT, expect: "TimerState", steps: (call) => call("timer_start", 9, 10, 99) },
  { label: "timer_start: serial not issued", manifest: TIMER_GRANT, expect: "UnknownSerial", steps: (call) => call("timer_start", 9, 10, 0) },
  { label: "timer_start: serial 0", manifest: TIMER_GRANT, expect: "TimerState", steps: (call) => call("timer_start", 0, 10, 0) },
  { label: "timer_start: not exactly one reaction", manifest: TIMER_GRANT, expect: "TimerState", steps: (call) => { call("promise_react", 0, 1n); call("promise_react", 1, 2n); call("timer_start", 1, 10, 0); } },
  { label: "timer_start: timer already armed", manifest: TIMER_GRANT, expect: "TimerState", steps: (call) => { call("promise_react", 0, 1n); call("timer_start", 1, 10, 0); call("timer_start", 1, 10, 0); } },
  { label: "timer_start: pending timers reach the limit", manifest: TIMER_GRANT, expect: "TimerBudget", steps: (call) => { for (let i = 1; i <= 33; i += 1) { call("promise_react", 0, BigInt(i)); call("timer_start", i, 10, 0); } } },
  { label: "timer_take: serial not issued", manifest: TIMER_GRANT, expect: "UnknownSerial", steps: (call) => call("timer_take", 3) },
  { label: "timer_take: no timer for the serial", manifest: TIMER_GRANT, expect: "TimerState", steps: (call) => { call("promise_react", 0, 1n); call("timer_take", 1); } },
];

/** Replaces one custom section of a Wasm binary (for catalog tampering cases). */
export function replaceCustomSection(bytes, name, data) {
  const out = [...bytes.slice(0, 8)];
  let at = 8;
  const u32 = () => {
    let result = 0;
    let shift = 0;
    for (;;) {
      const b = bytes[at++];
      result += (b & 0x7f) * 2 ** shift;
      if (!(b & 0x80)) return result;
      shift += 7;
    }
  };
  const leb = (n) => {
    const res = [];
    do {
      let b = n & 0x7f;
      n = Math.floor(n / 128);
      if (n) b |= 0x80;
      res.push(b);
    } while (n);
    return res;
  };
  const enc = new TextEncoder();
  while (at < bytes.length) {
    const start = at;
    const id = bytes[at++];
    const size = u32();
    const body = at;
    at += size;
    if (id === 0) {
      let p = body;
      let len = 0;
      let shift = 0;
      for (;;) {
        const b = bytes[p++];
        len += (b & 0x7f) * 2 ** shift;
        if (!(b & 0x80)) break;
        shift += 7;
      }
      if (new TextDecoder().decode(bytes.slice(p, p + len)) === name) {
        const payload = [...leb(enc.encode(name).length), ...enc.encode(name), ...data];
        out.push(0, ...leb(payload.length), ...payload);
        continue;
      }
    }
    out.push(...bytes.slice(start, at));
  }
  return Uint8Array.from(out);
}

function tamperedCatalog(bytes, edit) {
  const SECTION = "varta.functions";
  const module = new WebAssembly.Module(bytes);
  const catalog = JSON.parse(new TextDecoder().decode(WebAssembly.Module.customSections(module, SECTION)[0]));
  edit(catalog);
  return replaceCustomSection(bytes, SECTION, new TextEncoder().encode(JSON.stringify(catalog)));
}

export function comparableInvoke(r) {
  return { status: r.status, code: r.code ?? null, value: r.result?.value ?? null, logs: r.logs ?? null };
}

// The reference CLI prints no output of a failed execution, and the contract does
// not say whether lines emitted before a fatal failure are delivered: for failures
// only the status is compared.
export function comparableRun(r, c = {}) {
  return { status: r.status, exitCode: r.exitCode ?? null, logs: r.status === "failed" ? null : r.logs, error: c.errorText ? normalizeError(r.error ?? null) : null };
}

// The reason of an unhandled rejection renders a boxed value (unstable layout), so it
// is the one placeholder left out of the comparison.
function normalizeError(text) {
  return text === null ? null : text.replace(/^(unhandled promise rejection: ).*( \(\d+ rejected promise)/, "$1<reason>$2");
}

function schedulerOutcome(host, contract, c) {
  const { call } = host.debugScheduler(c.manifest ?? null);
  try {
    c.steps(call);
    return "ok";
  } catch (error) {
    const name = Object.keys(contract.schedulerErrors).find((key) => contract.schedulerErrors[key] === error.message);
    return name ?? `other: ${error.message}`;
  }
}

/** Runs every case in the JS host; `load(fixture)` returns {bytes, manifest}. */
export async function runJsCases(host, load, contract) {
  const results = [];
  const tools = await load("tools-i32");
  for (const c of invokeCases) {
    const r = await host.invoke(tools.bytes, c.tool, c.params, {
      manifest: tools.manifest,
      ceiling: c.ceiling ?? null,
      env: new Map(Object.entries(c.env ?? {})),
    });
    results.push({ label: `T1 ${c.label}`, observed: comparableInvoke(r) });
  }
  for (const c of runCases) {
    const fixture = await load(c.fixture);
    const r = await host.run(fixture.bytes, { manifest: fixture.manifest, env: new Map(Object.entries(c.env)) });
    results.push({ label: c.label, observed: comparableRun(r, c) });
  }
  for (const c of refusalCases) {
    const fixture = await load(c.fixture);
    let observed;
    try {
      host.validate(c.catalog ? tamperedCatalog(fixture.bytes, c.catalog) : fixture.bytes);
      observed = "accepted";
    } catch (error) {
      observed = c.pattern && error.message.includes(c.pattern) ? "refused" : `refused differently: ${error.message}`;
    }
    results.push({ label: c.label, observed });
  }
  for (const c of schedulerCases) results.push({ label: `contract check, scheduler misuse: ${c.label}`, observed: schedulerOutcome(host, contract, c) });
  return results;
}
