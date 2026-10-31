# Varta web host prototype

Platform code, not an application: a host for Varta portable Wasm in the browser
(module Worker) and Node, and the browser applier of the L0 view protocol, written
**only** from the published embedding contract (`docs/embedding.md`) as an external
check that the document is sufficient. It is a prototype: the official browser host
will be generated from the `HostOperation` registry (platform increments P1 and P9).

| Level | State |
| --- | --- |
| T1 tool host | Implemented: catalog validation, strict argument parsing, string transfer, envelopes like `varta invoke --json` |
| R1 run host | Implemented: logging, clock, environment (embedder map), exit, Math and UTF-16 character intrinsics |
| R2 scheduled run host | Implemented with reduced catalog validation: catalog rules 1–4 (exact index fields and bounds) and frame admission rules 1–5; guest memory is never read through the catalog. Includes the v36 timer event loop and, for `checked-f64-utf16-v38`, event turns (host calls and microtasks renewed per fired timer, one deadline per bounded run), the recoverable `timer_start_v2` and the v38 structure rules (functions counted with imports, `varta.static`/`varta.intrinsics` regions, data segment placement) |
| v38 manifest | `budgets`, grant `required`/`purpose`, `fsRoots` (names in a package, directories in a ceiling), request `methods` and external-link patterns, checked against the generated manifest tables; root-name grammar checked against the contract's examples at start. Effective budgets (request capped by the ceiling) set host calls per turn, the deadline and output; memory is capped only by the module's declared maximum, so a lower `memoryBytes` is refused. Optional grants are denied: this host stores no decisions and has no prompt |
| v38 view applications | `host.start` runs a long-lived application instance: `view_commit` (at most once per turn) hands each batch to the page's applier; view records come back as turns through `varta.view_abi.event` (written at the result base, input/scroll coalesced, the pending queue bounded); timers and records share one turn order; a refused batch ends the instance. The event export is admitted only under v38 with its registry signature |
| `net.request` (v37/v38) | The eight `net_request_*`/`net_body_read*` imports over the host's `fetch`: the descriptor parser is checked at start against the contract's accepted and rejected examples, and the head record against its head example; method, URL, grant (URL-aware: scheme, host and port exact, `*` after the authority; methods), slot, body and header checks in the order of the failure table; completions in one FIFO queue, one per turn, a due timer first in bounded runs and the earliest instant first in application instances; UTF-8 held back across chunks; the body bound. **Deviations:** response headers come from Fetch `Headers` (sorted, repeated fields combined except `set-cookie`), which the guest `Headers` sort-and-combine list makes unobservable (the loopback case sends repeats out of order), a browser reads only CORS-exposed headers and adds its own request headers, and a browser cannot observe redirects (an opaque redirect is a `Failed` completion) |
| v38 effects still missing | File roots for `fs.*`, `env_snapshot`, host events, permission prompts, `copy_text` and external links through a broker |
| H1 HTTP host | Not implemented; HTTP modules are refused |

Modules that import layout-bound intrinsics (`json_*`, `value_*`, `union_*`,
`primitive_*`, `bigint_*`, `number_to_*`, `number_parse_*`) or `fs`/`net`/`http`
operations are refused before instantiation, never stubbed.

**Timers** (embedding.md "Timers"): `timer_start`/`timer_clear`/`timer_take`/
`timer_uncaught` with the generated constants (flags, delay range, pending
limit). A denied `time.timer` grant is fatal. After `_start` the host runs the
event loop: drain microtasks, fail on unhandled rejections, stop when no armed
timer without the unref bit is left, fire the timer with the smallest (due time,
sequence) after waiting for it on the wall clock, fail when it is due at or
beyond the deadline. `timer_start` is a counted host call; the other three are
bookkeeping. A timer holds no token of its own (only the waiting frame's single
reaction counts against the pending-continuation bound); interval firings before
`timer_take` coalesce into one fired mark.

**Scheduler errors** use the generated reference diagnostics verbatim, with their
placeholders filled (the reason of an unhandled rejection is rendered as an opaque
value, since the value layout is unstable; `<due_ms>` is rounded up to a decimal
integer). Each misuse of a scheduler import raises the error of the generated
misuse table, checking rows in table order (for example `timer_start`: grant, then
flags, then serial, then reactions, then the timer and id budgets; `QueueFull` before
`DuplicateToken`). `debugScheduler()` exposes the imports of a fresh execution for
these contract checks. A failed run still returns the lines it emitted before
failing, with `status: "failed"`; tool envelopes carry no logs on failure.

**Catalog rule 4** is applied exactly: array bounds from the entry-point limits,
only the listed index fields (`bindings[].ty`, record `fields[].ty`, `element`,
`value`, union `members[]`, `functions[].captures[]`, frame `slots[].binding`,
`states[].saved[].binding`, `states[].yielded`, `result`, `exception`), and the
stated ranges of the non-index fields (`code` unique and below the user-function
limit, `signature` below `function::MAX_TYPES`, `capacity` at most
`frame::MAX_FRAME_BINDINGS`). The catalog **optionality table** is applied too:
which members may be absent or `null`, no unknown members, `capacity` required in
liveness layouts and `capacity`/`saved` forbidden in cell layouts. Type members come
from the generated per-kind table (kinds without members accept only `kind`;
record `fields` sorted by name), `saved[]` elements are `{binding, shared}`, and
`input`/`return_input` take only the listed JSON strings (`return_input` also JSON
`null` or absent).

`applier.js` applies the **L0 view protocol draft v0** to a closed shadow root:
whole-batch validation before any change, no HTML parsing, URL and CSS checks,
delegated events encoded as event records, and the seq rule for controlled fields.
No compiler emits `view_commit` yet, so its tests encode batches themselves.

**Budgets.** Memory: the module's declared maximum, checked before instantiation.
Host calls and output: counted (v38: host calls and microtasks per turn). Deadline:
checked at host calls only; the page terminates the Worker for runaway loops.
**Fuel is not enforced**, neither guest fuel nor the v38 fair host fuel, so this
host claims no fuel conformance.

## Files

| Path | Content |
| --- | --- |
| `contract.json` | Tables extracted from the contract; regenerate after the contract changes |
| `tools/extract-contract.mjs` | The extractor |
| `host.js` | Entry point `createHost`: policies and the `run`/`invoke` lifecycle |
| `context.js` | Contract numbers resolved once per host and the helpers the modules share |
| `validate.js` | Module validation before instantiation, including the tool catalog |
| `catalog.js` | Function and frame catalogs (normative minimum) and frame admission |
| `execution.js` | One execution: import bodies and the call protocol |
| `scheduler.js` | Scheduler and timer imports, microtask checkpoint, event loop |
| `exception.js` | Exception channel checks |
| `tool-abi.js` | Tool ABI v1: arguments, string transfer, results |
| `errors.js` | Error classes |
| `applier.js` | L0 view protocol draft v0 applier for the browser |
| `view-css.js`, `view-rules.js` | L0 subset rules from the generated tables (CSS value and stylesheet grammars, attribute grammars, strings, URL grants), shared by the applier and by application linters |
| `view-encoder.js` | Encodes L0 batches from operation names (tests and mockup conformance) |
| `wasm-binary.js`, `wasm-code.js` | Section reader; function-body scanner for frame admission |
| `tests/cases.js` | Shared cases, including catalog tampering through `replaceCustomSection` |
| `strict-json.js`, `policy.js` | Duplicate-rejecting JSON; manifests, ceilings and patterns |
| `net.js`, `net-descriptor.js` | `net.request`: imports over `fetch`, completions, the request descriptor grammar |
| `cli.mjs` | Node runner: `run`, `invoke`, `validate` |
| `fixtures/` | Programs compiled by the native `varta` for comparison |
| `tests/compare.mjs` | Differential test against the native runtime, including a loopback HTTP server for `net.request`; writes `tests/out/expected.json` |
| `tests/browser.html` | The same cases in a real browser, compared with `expected.json` |
| `tests/applier.html` | Applier tests with batches encoded from the published tables |
| `tests/view.html`, `tests/view-worker.js` | v38 view applications as long-lived instances in a Worker, rendered by the applier, driven by the scripted inputs and compared with the native headless tree (`out/view-expected.json`) |
| `scripts/serve.py` | Serves `examples/` without caching for the browser test pages |

## Running

```bash
node tools/extract-contract.mjs ../../docs/embedding.md ../../docs/architecture.md contract.json
VARTA_BIN=/path/to/varta node tests/compare.mjs
python3 scripts/serve.py 8766
```

Then open `http://127.0.0.1:8766/varta-web-host/tests/browser.html`,
`http://127.0.0.1:8766/varta-web-host/tests/applier.html` and
`http://127.0.0.1:8766/varta-web-host/tests/view.html`. `compare.mjs` must run
first: it builds the fixtures and records the native observations.
