# Changelog

## Unreleased

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The
workspace uses package version `0.1.0`; these entries describe the development
snapshot and do not announce a published release. Counts quoted in an entry
describe the state when it was written. Current status is in the
[development plan](docs/development-plan.md#status) and current conformance counts
in [conformance/README.md](conformance/README.md).

### Added

- V38 links every BigInt operation: a v38 host implements 54 imports instead
  of 67. `varta-bigint` builds without `std`, and the BigInt operations, the
  value13+ primitive conversions and the abstract comparisons are
  `varta_intrinsics::bigint`, which the v28–v37 host imports now call too.
  Operations with two i32 results are exported packed in one i64 and
  unpacked by a linker-synthesized adapter. The value18 conversions and
  comparisons stay host imports (linked, they cost far more fuel on string
  concatenation and union comparisons), as do JSON and the value18 readers.
  The intrinsic region gains a reserved arena, so small linked calls need no
  memory growth when the guest heap fills the memory budget.

- V38 linked code reports an exhausted memory budget as the typed
  `RuntimeError::Resource` (memory, with the request) through the module's
  `resource_failure`/`resource_request` exports instead of a bare trap,
  reads string arguments in place (`charAt` on 1,024 non-ASCII units: 24,533
  to 738 fuel per call), and reuses freed arena blocks. The region's stack
  and reserved arena are measured (5,016 and 6,984 bytes at most over
  adversarial inputs) and set to 16 KiB each, pinned by a test: one page per
  linking module instead of two to three.

- `varta-common` is the one owner of the value layout: a default `std`
  feature, and without it a `no_std` + `alloc` core (`abi`, `memory`, the
  `value` constants and primitive-cell reader, the `limits` constants,
  `text`) that `varta-intrinsics` and the wasm32 shim depend on with default
  features off. The intrinsics' restated layout constants, cell reader and
  UTF-16 helpers and their agreement tests are gone; linked v38 code reads
  string arguments and cells within the v38 data limit instead of 1 MiB.
  The full verification plan and CI check the core build.

- V38 linked intrinsics run in memory 0: a linked module has exactly one
  memory (no multi-memory, so Safari and embedded engines run it). Emission
  reserves an intrinsic region after the static extension (`varta.intrinsics`,
  `abi::StaticRegion::intrinsic_base`) holding the library's shadow
  stack and the data segments its linked operations reach; the compiler
  relocates those segments through the library's kept wasm-ld relocations,
  checks every stack-pointer write against the region, and starts the
  per-call arena at the guest heap's end. Validation admits one memory for
  every profile and the library's data only inside the declared region.

- V38 hashes dictionaries and mixed records under value ABI and layout 18
  (`dictionary_hashing`): a four-slot storage record (`holes`, `keys`,
  `table`, `values`) with an open-addressing table probed from a seedless
  FNV-1a hash of the UTF-16 key units by allocation-free guest helpers,
  tombstones and vector holes for deletions, compaction before enumeration and
  amortized rebuilds at half load. Key order, output and errors are v37's
  (pinned against Node 24); every reader validates the table against the
  entries and rejects a corrupt one. V38 binds twelve value18 readers (host
  ABI 34), JSON schema 18 and exception channel v9; v37 keeps value17.
  Operations cost 1,200–2,500 fuel at 1k–10k entries where the linear layout
  took 54,000–2,100,000; a dictionary holds at most 2^26 entries.

- V38 code generator hotspots: `push` with one to four arguments appends
  without an argument array, checked element reads load the slot without a
  union cell, string `+` chains concatenate up to eight operands per
  allocation, calls of function values with quiet operands take no roots,
  small literal and grown array backings zero their presence bitmap word by
  word (Wasmtime charges bulk memory per byte) and literals reserve four
  slots, and the allocator keeps blocks up to 280 bytes in exact-size free
  lists. Against the integrated v38 tip: array push + index sum 464,755 →
  112,049 fuel, list render of 100 rows 292,803 → 160,137, the 100-row keyed
  diff 1,785,586 → 1,445,572 (73% of it is now the dictionary's linear key
  search), and a reactive keyed view fits 129 instead of 74 mounted and 51
  instead of 35 updated rows in one production budget. V37 and earlier keep
  their bytes.

- V38 removes unreachable async functions and methods with their frame
  layouts (v37 roots every suspendable function); generator bodies stay. A
  minimal use of the 1900-model pilot mimic keeps 79 functions instead of
  every resource method. The program-limits measurement is recorded in the
  development plan.

- V38 skips the initialization of module constants that no reachable code
  reads and renumbers the rest, so startup fuel, root slots and static data no
  longer grow with unused constants (100 unread enum objects: 5991 fuel under
  v38 against 45253 under v37; adding 200 more leaves v38 unchanged).

- V38 `varta_resume` dispatches through a static per-layout table and one
  `br_table`: about a dozen bytes per frame layout instead of a ~125-byte
  check block, constant resume fuel, the same checks before any effect and
  unchanged frame admission. Every pre-v38 runtime case runs identically
  under v37 and v38.

- V38 `program_limits`: structural limits come from one registry,
  `LanguageProfile::limits()` (`varta_common::limits`), read by the compiler,
  runtime preflight, catalog, schema and closure readers, artifact and
  launcher readers and the generated tables. V38 admits the WebAssembly
  implementation limits: 1,000,000 functions and types, 7,654,321-byte
  function bodies, a 1 GiB module, and 1,000,000 record shapes, classes,
  unions, array, promise, generator and function types. V1–v37 keep their
  frozen limits, bytes and diagnostics.

- V38 `program_limits` foundation: a multi-page static region under host ABI
  34 with value layout 17 (no value 18). Static data that no longer fits the
  first page goes into an extension after the root stack, declared by the
  `varta.static` section, and the managed heap starts after it; result buffer,
  root stack and exception root keep their addresses. Runtime readers accept
  descriptors, keys and schemas in the module's own extension, data segments
  must lie in the declared region, and v38 instantiation no longer charges
  Wasmtime's per-byte data-segment copy to the guest's fuel. V37 and earlier
  keep the single 64 KiB page, their bytes and diagnostics.

- V38 data limits (`data_limits`, in development): arrays (dense, resizable
  and sparse), dictionaries, `Map`, `Set`, `Uint8Array`, spread arguments,
  strings and JSON are no longer capped at 1024 elements, 1 MiB, depth 64 or
  4096 values; the Wasm-derived limits (2^27 elements, 2^30 string bytes,
  a JSON host depth ceiling of 10000) and in practice the unchanged 8 MiB
  memory budget, fuel and deadline bound them. Sparse backings above 1024
  slots keep value layout 17 with one presence bit per slot. More than 64
  continuation frames may be live (a hashed growable table) and the host
  scheduler tracks up to one pending token per 64-byte frame block of the
  memory budget. `JSON.stringify` binds the new `json_stringify_owned` (host
  ABI 34) and writes results larger than 1 MiB into owned guest strings
  through a host buffer bounded by the memory budget; checked parsing accepts
  large and deep inputs and builds them iteratively. Exhausted resources are
  reported as `RuntimeError::Resource` (`resource limit: memory budget
  exhausted (requested …)`) through the new `resource_failure` and
  `resource_request` exports instead of an anonymous trap. The v38 heap grows
  geometrically and marks through a worklist (1000 concurrent awaits: 83.7M
  to 7.7M fuel), and `Array.prototype.join` is O(n log n). V37 and earlier
  keep their bytes, limits and diagnostics. The data limits are the `data`
  field of the program-limits registry, and runtime readers take both through
  one `value::ModuleBounds` parameter. Dictionary hashing is pending a
  value18 storage slot.

- V38 turn budgets (selector `turn_budgets`, increment P5). A v38 instance
  runs as event turns: `main` with its microtasks, then one timer, network
  completion, host event or permission answer per turn. Fuel, host calls and
  microtasks renew per turn, the deadline too in a long-lived application
  instance; memory and output stay per instance, and an exhausted budget names
  its turn. Manifests with `abiVersion` 34 may request `budgets`
  (`fuelPerTurn`, `deadlinePerTurnMs`, `memoryBytes`, `hostCallsPerTurn`,
  `outputBytes`), capped by the host ceiling and by the production defaults
  without one. The runtime gains long-lived application instances
  (`start_application`, `deliver`, `step`; a trap terminates the instance and
  nothing restarts it), host event sources (`varta.events.listen`/`close`, FIFO
  host tasks with coalescing) and permission broker v1:
  `varta.permissions.query` and user-gesture-only `request`, grant
  `required`/`purpose`, decisions keyed by the artifact's SHA-256 identity, a
  `PermissionPrompter` seam and a bounded audit log; headless hosts never
  prompt. Seven host imports use host ABI 34. `varta dev`, `varta run`, the
  runtime companion and packaged applications print output captured before a
  failure ahead of its diagnostic, and accept `--budget-report`, `--app` and
  `--decisions PATH`; `inspect` reports the application identity. Earlier
  profiles keep their budgets, texts and event loop.

- V38 code generation v2 (`codegen_v2`) and the fuel profiler. Quiet
  expressions skip root marks and operand roots, quiet statements skip their
  temporary reset, direct calls with quiet arguments and frames without
  references lose their bookkeeping, operand caches of plain reads collapse,
  statement-level `x++`/`x op= e` become plain assignments, string equality
  uses a chunked content-equality helper (inline against literals of up to 16
  bytes), local `s += t` appends in place into amortized builder blocks,
  `charAt`/`charCodeAt` and safe-integer `%` run in the guest without host
  calls, and the allocator bounds its first free-list scan and grows below a
  collection threshold. Measured workloads use 1.2× to 15× less fuel (a
  `x = x + i` loop 6.8×, 3-char string `===` 3.5×, `s = s + "x"` 15×, a
  100-row list render 5.8×); v37 and earlier keep their bytes and every case
  program behaves identically under v38. `varta dev --profile-fuel [--json]`
  runs an instrumented profile build with exact probe refunds and prints self
  fuel, total fuel and calls of every function, prelude member and compiler
  helper to stderr; profile builds are never packaged. The profiled run is
  the ordinary run path (`RunOptions::fuel_profile`) with the same manifest
  budgets, decisions and execution flags, spans every v38 turn and coexists
  with `--budget-report`.

- V38 structural subtyping (`structural_subtyping`, in development): a record
  or class instance with more members is assignable to an interface or
  object type it provides, without a copy (identity and writes are shared;
  each access through such a view is one accessor call and one descriptor
  test per source layout, and types that never receive a wider value keep
  direct slots). Mismatch diagnostics name both types and list missing,
  mismatched and extra members. The same selector admits generic method
  signatures in interfaces (erased, with checked results), `object`
  annotations, `Array.isArray` narrowing of `unknown` to `unknown[]`, object
  spread of records and optional parameters, `obj.m?.()` on class methods,
  property narrowings kept across calls and getters (reads checked),
  `JSON.stringify` typed `string`, literal `??` fallbacks, `record as
  Record<string, T>`, records held by `unknown` read as JSON objects,
  `Promise<unknown>` arrow contexts, `as void`, `for...of` over strings,
  exhaustive literal `switch` statements, constructor parameter properties,
  and `encodeURIComponent`,
  `encodeURI` and `URIError` in a new prelude. No host import, value tag or
  layout changes; v37 keeps its bytes.

- V38 links pure intrinsics into the guest module (`linked_intrinsics`, first
  increment): Math and `**`/`%`, Number text and parsing, primitive-cell
  conversions and UTF-16 character access and case mapping are compiled from
  the new `no_std` `varta-intrinsics` crate, which the native runtime also uses
  for the host imports of earlier profiles. The compiler links only the
  reachable library functions and data from a checked-in wasm32 artifact
  (`scripts/build-intrinsics.py`) onto a private, bounded second memory; a v38
  module that imports one of these operations is rejected, so a v38 host
  implements 59 instead of 93 admissible imports. V38 Math is bit-identical on
  every host (portable `libm`); JSON, the value17 readers and conversions,
  primitive comparisons and BigInt remain host imports. With code generation
  v2 each operation keeps one v38 path: `charAt` and `s[i]` serve ASCII units
  from static data and other units through the linked `string_char_at16`,
  `%` falls back from its integral fast path to the linked remainder, and an
  integral `toString(radix)` takes an exact integer-digit path. Linked code
  counts against v38's Wasm structural limits, and its private memory's image
  is covered by the v38 instantiation allowance.

- V38 view protocol and TSX (`view_protocol`, `tsx`): the `view_commit` host
  import (host ABI 34) validates each L0 draft-v0 batch as a whole against the
  `varta_common::view` registry and applies it atomically (an invalid batch or
  a second commit in a turn ends the instance); event records reach the guest
  through the compiler-synthesized `varta.view_abi.event` export and the
  engine view prelude (`View.*`, `ViewEvent`). The new `varta-view` crate is
  the headless renderer: node tree, stylesheet cascade subset, bubbling, key
  filters, edit counters, gestures, a deterministic tree and accessibility
  snapshot (no layout yet). `ui.open_url` (with the any-HTTPS form
  `https://*`, valid for that id only) and `clipboard.write` are registered
  with exact grants; every outcome is a recoverable `command_result`. `.tsx`
  modules compile JSX ahead of time into static L0 operations with getter
  holes bound through an installable effect runner; `varta.d.ts` carries a
  generated `JSX` namespace. Application instances host the view: every
  record is a host task that runs as its own turn with fresh budgets (the
  task's gesture flag gates `copy_text`, coalescible records follow the task
  queue's rule), platform inputs are host work between turns
  (`Application::view_input`), and the view capabilities are decided by
  permission broker v1 (audit log, revocation; an any-HTTPS link prompts with
  the full URL on every activation through the prompt seam and is never
  stored). `varta dev --view-script` runs a scripted headless application
  session with the `dev` execution flags and prints the snapshot.

- Opt-in v38 (batch: program and data limits, turn budgets, linked
  intrinsics, view protocol and TSX, structural subtyping and code generation
  v2, in development) is registered. It inherits every v37 feature, shares
  value ABI/layout 17 with v37 and declares its own host ABI 34
  (`PROGRAM_LIMITS_VERSION`) without a new import yet; the launcher advertises
  host ABI 34. A v38 module relabelled as v37 fails at its host ABI section,
  and runtimes that do not list v38 reject its declared profile name before
  any guest effect. Its `program_limits`, `data_limits`, `turn_budgets`,
  `linked_intrinsics`, `view_protocol`, `tsx`, `structural_subtyping` and
  `codegen_v2` selectors admit nothing yet.

- The v37 streams integrated, with their cross-stream rules. Unreachable
  functions, including unused engine prelude members, are removed before the
  256-function limit applies (a `fetch` hello world went from 228 linked to
  154 reachable functions). `JSON.stringify` imports the new
  `json_stringify_v17` (host ABI 33), which skips private state: `#private`
  fields, engine prelude internals (`Headers`, `AbortSignal`, `Request`, … as
  `{}`) and non-enumerable Error slots, so `JSON.stringify` of `unknown` works
  in programs that load Web types; object keys starting with `#` are reserved,
  and `URL` and RegExp match arrays stay out of `JSON.stringify` and
  `unknown`. `Date.now()`/`new Date()` without `time.now` throw a catchable
  `NotAllowedError` through the permission broker instead of ending the run.
  Index reads produce `undefined` where code expects it (`T | undefined`
  contexts, `??`, bindings and loop variables compared with `undefined`,
  `unknown[]`). String `split` has one dispatch path for string and RegExp
  separators. `fetch` and `Request` accept `URL` inputs; classes may
  `implements AsyncIterable<T>`; `instanceof` and `#x in` test every class
  member of a union; readonly literal fields keep literal types; fields take
  their type from `new Map<K, V>()`, `new Set<T>()` and `new Uint8Array(n)`;
  a local named like a prelude value (`fetch`) shadows it. The conformance
  inventory is pinned to v37. Debug and test builds optimize the Cranelift
  crates, removing a `build --single` probe timeout under parallel test
  load. V36 and earlier keep their bytes and diagnostics.

- The language core under opt-in v37 (selector `language_core`). Closures may
  capture `let`/`const` bindings and hoisted nested function declarations
  before their initialization, with JavaScript's temporal dead zone: an
  access through the closure before it throws an Error whose `name` is
  `ReferenceError`. `let x: T;` is admitted with TypeScript-style definite
  assignment. `for (const x of xs)`, `xs[i]` and `s[i]` have the element type
  through a checked read (a hole or a missing index throws `TypeError`, where
  Node reads `undefined`). Guards on property paths (`a.b !== undefined`,
  `typeof a.b.c`, truthiness, `a.b?.c`, literal comparisons) narrow later
  reads until a call or property write may change them. `switch` selects by
  strict equality over strings, literal unions and `typeof`, with clause
  narrowing. Value-returning functions convert to `void`-result function
  types, and function mismatches name both signatures. Endless loops end
  value-returning functions, uninitialized `T | undefined` class fields start
  as `undefined`, `new C()` types a field, constructor arrows may use
  `this.field` and `ReturnType<typeof setTimeout>` is the timer handle type.
  Unions may hold several record shapes, interfaces and classes and arrays
  of different element kinds, told apart by layout descriptor and element
  kind: discriminant, `in`, `instanceof`, `typeof` and `Array.isArray`
  narrowing, field reads and method calls on the union, `never`
  exhaustiveness, and checked JSON conversion that selects the member by its
  literal discriminant. Module-level `let`/`var` hold mutable module state in
  root slots (exports are live bindings), module constants may hold
  functions, and `const id = setInterval(() => clearInterval(id), ms)`
  compiles.
  No value tag, layout, host import or budget changes; v36 keeps its bytes
  and diagnostics.

- Class generics under opt-in v37 (selector `class_generics`): generic
  instance, static and async methods (monomorphized per type-argument list,
  inferred from arguments and from the contextual result type, so the pilot's
  `return this._client.request(spec)` instantiates `request<R>` from the
  method's `Promise<R>`), overrides of generic methods dispatched per
  instantiation, generic classes (`class Page<T>`, `extends Page<T>`,
  `class Numbers extends Page<number>`, statics on the template), method
  signatures in interfaces as function fields with object-literal methods
  that do not use `this`, checked `implements`, and `#private` fields,
  methods and getters with `#x in value`. No value tag, layout, host import
  or budget changes; v36 keeps its diagnostics and bytes.

- Collections and binary text under opt-in v37 (selector `collections`):
  `Map`/`Set` (insertion order, SameValueZero and identity keys, a hashed
  index, live iteration, `forEach`, `keys`/`values`/`entries`, spreads and
  `Array.from`, module-constant sets), tuples (`[A, B]`, `readonly`, trailing
  optional elements; `Object.entries` returns `[string, T][]`), `padStart`,
  `padEnd`, `toUpperCase`, `toLowerCase`, `split` with a string separator,
  `concat`, `codePointAt`, `String.fromCharCode`/`fromCodePoint` and
  `Array.prototype.join`, `Uint8Array` (ToUint8 writes, `set`/`slice`/`fill`/
  `indexOf`/`includes`/`join`), UTF-8 `TextEncoder`/`TextDecoder` with
  `fatal`, `ignoreBOM` and streaming, `btoa`/`atob`,
  `crypto.getRandomValues`/`randomUUID` (the `Math.random` entropy path) and
  a UTC `Date` subset (ISO-8601 and IMF-fixdate parsing, `toISOString`,
  `getUTC*`). They are typed guest code in engine builtin preludes. Three host
  imports with minimum host ABI 33: `string_to_upper_case16` and
  `string_to_lower_case16` (pure) and `date_now` (exact `time.now` grant,
  recoverable `NotAllowedError` denial). No value tag or layout: v37 keeps value layout 17, and
  logging, `JSON.stringify` and `unknown` reject values holding these
  objects. V36 keeps its bytes and diagnostics.

- RegExp under opt-in v37 (selector `regexp`). Regular expression literals
  and `new RegExp` with string literal arguments are compiled ahead of time
  (ECMAScript 2025 pattern syntax, Annex B without `u`, flags `gimsuy`) into
  matcher programs that one guest Pike VM runs in time linear in the input,
  metered as ordinary fuel, so no pattern can backtrack catastrophically. The
  engine prelude provides `RegExp` (`exec`, `test`, `lastIndex`, `source`,
  `flags` and the flag getters), `RegExpExecArray` and `RegExpMatchArray`,
  and the string methods `search`, `match`, `matchAll`, `replace`,
  `replaceAll` and `split`, with replacement templates, replacer functions
  and string patterns. Case-insensitive matching is resolved at compile time.
  Backreferences, lookaround, `\p{…}`, modifiers, run-time patterns and the
  `d`/`v` flags are rejected by name. No value tag, layout, host import,
  capability or budget changes; v36 and earlier keep their diagnostics.

- Network requests under opt-in v37 (selector `net_request`). `fetch`,
  `Request`, `Response`, a `ReadableStream` subset, `URL` and
  `URLSearchParams` are engine preludes over one new exact capability,
  `net.request` (URL scopes over `http`/`https` with an optional method list;
  host ceilings narrow both). Requests run on hyper's HTTP/1.1 client, HTTPS
  through rustls with the compiled-in Mozilla roots; completions wake a
  scheduled frame like a timer, so the runtime keeps only ids and serials.
  Bodies stream as bounded, UTF-8 decoded text chunks (bytes wait for
  `Uint8Array`); `AbortSignal` and `AbortSignal.timeout` cancel transfers;
  redirects default to `error`, with `manual` and grant-checked `follow`.
  Classes may declare `async *[Symbol.asyncIterator]()` for `for await`. Six
  host ABI 33 imports join the registry (`net_request_start` with
  `net.request`, and five bookkeeping operations).
- A permission broker for asynchronous capabilities under v37: a denied
  `net.request` or `time.timer` is decided before any effect, logged, and
  delivered as a catchable `NotAllowedError` `DOMException` (`fetch` rejects,
  `setTimeout` throws through the new `timer_start_v2`); v36 keeps its fatal
  timer denial. The broker is the documented seam for a later consent prompt.
- New dependencies, exactly pinned: `rustls` (ring provider), `tokio-rustls`
  and `webpki-roots` for HTTPS, hyper's `client` feature (adds `want` and
  `try-lock`), and the test-only `rcgen` for loopback certificates.

- Opt-in v37 (pilot batch: language core, class generics, collections and
  binary text, RegExp and `net.request`, in development) is registered. It
  inherits every v36 feature, shares value ABI/layout 17 with v36 and declares
  its own host ABI 33 (`NET_REQUEST_VERSION`), under which the batch's
  imports were added later; the launcher advertises host ABI 33. A v37 module relabelled as v36 fails at its
  host ABI section, and runtimes that do not list v37 reject its declared
  profile name before any guest effect. Its `language_core`, `class_generics`,
  `collections`, `regexp` and `net_request` selectors admitted nothing at
  registration.

- Checked declared fields under opt-in v36 (selector
  `checked_declared_fields`). A mixed record indexed by `unknown` may declare
  fields of any type a JSON value can describe, such as the pilot's
  `errors?: Array<{ field?: string; message?: string }>`, nested records,
  dictionaries, unions and nested mixed records. A declared read returns the
  stored value when its static type is proven (descriptors, element kinds, and
  non-empty exact arrays) and otherwise converts the JSON graph into a fresh
  value that it stores back; checked parsing and assertions from `unknown`
  convert the same way, and a plain `Record<string, T>` asserts to a mixed
  record of the same storage. Structural record assertions accept every key
  order of a shape. V36 also destructures dictionaries, narrows
  `typeof a.b === "string" ? a.b : x`, types `a ?? []` by `a`, admits
  `let x: unknown;` and exhaustive `switch` returns. No value tag, layout,
  host import or budget changes; v35 keeps its diagnostics.

- Timers under opt-in v36 (selector `timers`, K4.4 step 5): `setTimeout`,
  `setInterval`, `clearTimeout`, `clearInterval`, the
  `new Promise<void>((resolve) => setTimeout(resolve, ms))` sleep pattern and
  `AbortSignal.timeout` run on a runtime event loop after the microtask
  checkpoint, in Node's order (main and its microtasks, then timers by due
  time and creation order, each followed by its microtasks). Ids are numbers,
  as in browsers. Arming a timer requires the new exact boolean grant
  `time.timer` (`"time": { "timer": true }`), denied by default: without it
  the execution ends at the call. A timer due at or after the 5-second
  execution deadline ends the execution with a budget error instead of
  waiting, at most `MAX_PENDING_TIMERS` timers are pending, an exception from
  a timer callback ends the execution like one from `main`, and
  `AbortSignal.timeout` timers do not keep the loop alive. Four internal host
  imports (`timer_start`, `timer_clear`, `timer_take`, `timer_uncaught`) use
  host ABI 32; v35 and earlier keep rejecting the timer globals. Under v36 an
  arrow expression body in a `void` context discards its value, as in
  TypeScript.

- Async generators and `for await` under opt-in v36 (selector
  `async_generators`, K4.4 step 6). `async function*` declarations, generic
  instantiations and instance and static class methods return
  `AsyncGenerator<T, TReturn, TNext>` (also written `AsyncIterableIterator`,
  `AsyncIterable` or `AsyncIterator`); `next`, `return` and `throw` return
  promises, queue while the body runs and follow the ECMAScript request
  queue; `await`, `yield` and `return` work across `try`/`catch`/`finally`,
  and `yield` and `return` await their operands; `yield*` delegates to
  another async generator, forwarding `return()` and `throw()`. `for await`
  iterates async generators, arrays and synchronous generators with Node's
  job counts and closes them on `break`, `return` and throws. An async
  generator object is the frame of an internal body generator with a guest
  state record; awaits in bodies run through scheduled relay jobs, so
  suspended generators hold no live-frame slot and are collected. Under v36 a
  checked `as` may also contain `await`, and a `Math`/`Number` constant
  default types a parameter. No host import, value tag, layout or budget is
  added; `yield*` over other iterables, async generator expressions and
  `Symbol.asyncIterator` remain unsupported. Logs match Node 24.

- Opt-in v36 (pilot batch: checked declared fields, function fields, async
  generators and timers, in development) integrates its four streams. It
  inherits every v35 feature and declares its own host ABI 32
  (`TIMERS_VERSION`: the value17 readers and the timer operations) and value
  ABI/layout 17; the launcher advertises both. A v36 module relabelled as v35
  fails validation before any guest effect, and runtimes that do not list v36
  reject its declared profile name. The streams work together: timer callbacks
  and abort listeners may be held in function fields (rooted across
  collections), `for await` consumes async generators that sleep on timers,
  async generators yield function values, and declared reads of mixed records
  take optional chains (`varta-runtime` `v36_cross_stream_tests`, Node 24
  logs). `AbortSignal.any` joins the v36 abort prelude with DOM dependent-signal
  order, so `AbortSignal.any([signal, AbortSignal.timeout(ms)])` and the pilot's
  abortable sleep work. The conformance inventory pins v36: six v36 cases pass,
  the timer case with a reviewed per-case `time.timer` grant (the runner's new
  `grants` field, which admits no other capability).

- Function fields under opt-in v36 (`function_fields`): function values in
  record, interface and class fields, arrays, dictionaries, unions (one
  function type each) and `unknown`; calls through fields without `this`;
  optional and defaulted function-typed parameters such as
  `random: () => number = Math.random`; `typeof f === "function"` and
  truthiness narrowing; optional chains (`x?.p`, `x?.[i]`, `x?.m()`, `f?.()`);
  `Math.random` as a value with one identity; JSON omission of functions and
  a compile-time rejection of function-valued `toJSON` fields;
  `reject(reason)` with an `unknown` reason; `Promise.withResolvers<T>()`; and
  the v36 abort prelude's `addEventListener`/`removeEventListener`/`onabort`
  (registration order, `once`, capture identity). V35 programs compile to
  unchanged bytes (pinned digests); the conformance inventory gains three
  cases.

- Value ABI and layout 17 (`FUNCTION_FIELD_VALUE_VERSION`) for v36, owned by
  the function-fields stream: every value16 tag and layout keeps its meaning
  and the function tag becomes a reference kind of fields, elements and cells,
  read through closure envelopes. V36 binds thirteen value17 readers
  (`value_log_v11` … `bigint_from_value_v4`, host ABI 32), JSON schema 17 and
  exception channel v8; v35 keeps value16. The launcher advertises value ABI 17.

- Mixed records and JSON to `unknown` under opt-in v35 (selectors
  `mixed_index_records`, `unknown_json`). Object types
  may declare fields next to a string index signature (the pilot's
  `ProblemDocument`); they are dictionaries of the index type whose declared
  reads are typed and checked, so a value stored through a dynamic key or a
  plain alias throws `TypeError` instead of being reinterpreted. Dictionaries
  gain `for...in`, compound, logical and update assignments, spreads with a
  dictionary context, number keys (`ToPropertyKey`), `Record<string, T>`
  inference, and shared per-type read/write/delete functions that keep long
  runs within the function body budget. Classes may declare optional fields
  and redeclare an inherited field with the same type (`readonly cause?:
  unknown`), redefined after `super()` as in ES2022. `JSON.parse(text)` returns
  `unknown` (a graph of primitives, `unknown[]` and `Record<string, unknown>`),
  `varta.json.parse<T>` admits `unknown`, `JSON.parse(text) as T` is a checked
  parse at `T`, dictionary assertions are proven by the storage descriptor for
  any value type, and `typeof x === "object"` with a null test narrows
  `unknown` to `object` for `in`, `Object.keys`/`values`/`entries`/`hasOwn`
  and `for...in`, while `"key" in x` proves `x.key`. They run on v35's value
  layout 16 and JSON schema 16 and add no further value tag, layout, host
  import, schema version or budget.

- Engine Web types under opt-in v35 (selectors `web_headers`,
  `abort_signals`): `Headers`/`HeadersInit`, `AbortController`, `AbortSignal`
  and `DOMException` are typed guest preludes compiled with a program only when
  it names them, with Fetch/DOM validation, ordering and errors pinned to
  Node 24 and no host import or capability. Header iteration returns array
  snapshots; abort listeners, `AbortSignal.timeout` and `AbortSignal.any` are
  rejected with explicit diagnostics. V35 function literals may also omit
  trailing parameters of their contextual function type.

- Opt-in v35 (pilot batch: Web types, mixed records, async methods and frame
  liveness, in development) is registered. It inherits every v34 feature;
  runtimes that do not list v35 reject its declared profile name before any
  guest effect. Its selectors are `web_headers`, `abort_signals`,
  `mixed_index_records`, `unknown_json`, `async_methods`, `promise_values` and
  `frame_liveness`.

- Async class methods, async arrows and async function expressions under
  opt-in v35 (selector `async_methods`): instance methods with overrides and
  awaited `super.m()` calls, async closures that share their captured bindings
  across `await`, contextual or inferred `Promise<T>` results, optional,
  default and rest parameters in every async function, and `Promise<T>`
  inference in generic calls. Receivers and captured cells are exact frame
  roots; job order matches Node.

- Promise values under opt-in v35 (selector `promise_values`): promises in
  arrays, records, class fields, dictionaries, unions and `unknown`, compared
  by identity, `typeof` `"object"`, logged as `Promise { … }` and stringified
  as `{}`; `await` and `Promise.resolve` of `T | Promise<T>`; and
  `Promise.all`, `allSettled`, `race` and `any` over arrays of promises with
  Node's job order. `Promise.any` rejects with an `Error` (no
  `AggregateError`); a promise held by `unknown` is never adopted silently.

- Value layout 16 for v35 (`promise_values` foundation): value ABI 16 adds the
  promise value tag to every value15 layout, read by thirteen value16 host
  operations that mirror their value15 counterparts (host ABI 31), exception
  channel v7 and JSON schema 16. V35 programs behave exactly as under v34; v34
  modules keep host ABI 30, value15 and their imports, and swapped readers,
  channels or descriptor stamps are rejected before guest effects.

- Liveness-based continuation frames under opt-in v35 (selector
  `frame_liveness`): each `await`, `yield` and test-hook suspension point saves
  only the locals live across it (through the normal, throw and return
  continuations, closure-shared cells, handlers and finalizers), directly in
  frame segments of kind 3 without per-value frame cells; states reuse the same
  entries, and each segment's edge mask is the exact GC map of the state the
  frame is suspended at (zero while running and after completion). Frame
  catalog version 3 describes the per-state saved slots and the entry
  capacity; only v35 reads it, v33/v34 keep version 2 and their modules stay
  byte-identical, and a module relabelled across the two layouts fails
  admission. No host or value ABI changes. An async closure body's frame
  keeps its captured cells as shared state-0 slots, and an assignment to a
  closure-shared local keeps its cell live across a point. Measured with production budgets,
  an `await` costs about 1.6k fuel instead of 3.0k, a `yield` 1.1k instead of
  1.8k, and 141 instead of 57 suspended generators fit in one execution.

- `unknown` under opt-in v34 (selector `unknown_type`): a reference-union cell
  that holds any primitive, BigInt, record, array, native error or class
  instance (function values are rejected). Unnarrowed values admit only
  equality, nullish comparison, `typeof`, truthiness and logical operators,
  `instanceof`, `Array.isArray`, logging, `String`/template conversion and
  `JSON.stringify`; `typeof`, `=== null/undefined/<literal>` and `instanceof`
  narrow positive branches. `catch (e)` and `catch (e: unknown)` bindings are
  `unknown`, rethrowing preserves the caught value, and Error subclasses take
  `cause?: unknown`. Unknown values are exact GC roots in locals, fields,
  elements, closures and generator frames (`CaptureType::Unknown`).

- Checked assertions under opt-in v34 (selector `checked_assertions`): `x as T`
  is free for upcasts and otherwise tests the value by member tag, literal
  value, exact record descriptor, class family descriptor or primitive array
  element kind, throwing `TypeError("type assertion to <T> failed")` on
  mismatch; `x!` throws `TypeError` on `null`/`undefined`. Disjoint targets,
  distinct record or array types, arrays of non-primitive elements, function
  types and `<T>x` are rejected at compile time; `as const` is unchanged.

- Microtask scheduler and `Promise<T>` under opt-in v34 (selector `promises`,
  K4.4 step 3): one bounded FIFO microtask queue per execution, owned by the
  runtime and drained after `main`; every job is a scheduled continuation frame
  resumed only through `varta_resume` with generation-checked tokens, and the
  runtime keeps reaction serials, never guest references. `new Promise<T>`
  with a synchronous executor and one-time resolution, `Promise.resolve`,
  `Promise.reject`, `then`/`catch`/`finally` with handler-result adoption,
  promise adoption jobs and the self-resolution `TypeError`; job order matches
  Node event logs. Promises are guest-heap records without a value tag. Five
  internal scheduler imports join the v34 host ABI without a capability. A
  rejection still unhandled after the checkpoint terminates the execution
  with its reason; fuel, deadline and host-call ceilings cover every job and
  are never renewed, and exhaustion inside a job is fatal, never a rejection.
  Thenables other than internal promises, the combinators, promises in
  aggregates and async HTTP/tool handlers are not admitted.

- `async`/`await` under opt-in v34 (selector `async_functions`, K4.4 step 4):
  annotated top-level and generic `async function` declarations returning
  `Promise<T>` (including `async function main(): Promise<void>`), `await` of
  promises and plain values in normalized expression positions, `try`/`catch`/
  `finally` across `await`, rejection propagation and returned-promise
  adoption, over the same continuation frames. Async arrows, expressions and
  methods, `for await`, async generators, top-level `await` and `await` inside
  intrinsic value blocks fail with construct-specific diagnostics.

- V34 stream integration: `unknown` holds value15 dictionaries and wide records
  and reads them through the value15 operations; `x as Record<string, T>` for
  a primitive `T` tests the `Dictionary` tag and the value vector's element
  kind; promises are rejected from `unknown` with their own diagnostic;
  promise records, async frames and the scheduler run under value15
  descriptors, and recursive values (`JsonValue`, `Tree`) may be held in
  closure cells and generator/async frames through shallow back-edge catalog
  nodes. A flow join now keeps a refinement of an `unknown` binding that every
  path proves, so a narrowed `catch (e)` stays narrowed after a branch.

- Opt-in v34 (pilot batch: dynamic records, `unknown` and async, in
  development) is registered. It inherits every v33 feature and declares a new
  host ABI, so runtimes without the microtask scheduler reject its modules, and
  a new value ABI reserved for the batch's record and promise layouts; until a
  stream defines them, v34 keeps the v33 layouts, exception channel and JSON
  selectors. Its `index_signatures`, `recursive_types`, `wide_records`,
  `unknown_type`, `checked_assertions`, `promises` and `async_functions`
  selectors admit nothing yet.

- Value layout 15 under opt-in v34 (selector `dynamic_values`): v34 stamps and
  reads the value15 layouts, which keep every value13 tag and layout, through
  its own value15 readers (`value_log_v9`, `value_typeof_v8` and the other `_v8`
  value predicates, `value_is_error_class_v4`, `json_stringify_v14`,
  `json_parse_v16` with JSON schema 15, `primitive_to_string_v3`,
  `primitive_to_number_v3`, `bigint_from_value_v2`) and exception channel v6.
  V34 programs behave exactly as under v33; v33 and earlier keep value13.

- Wide records under opt-in v34 (selector `wide_records`): interfaces, object
  types, literals and classes with up to `value::MAX_WIDE_RECORD_FIELDS` (128)
  fields, a value15 layout with multiword presence words and a
  descriptor-trailing edge bitmap traced by the collector; JSON, presence,
  deletion, enumeration and rest destructuring behave as for narrow records.
  Earlier profiles keep the 32-field ceiling.

- Index signatures under opt-in v34 (selector `index_signatures`):
  `{ [key: string]: T }` and `Record<string, T>` as value15 dictionaries (tag
  13) with reads typed `T | undefined`, writes, `delete`, `in`,
  `Object.hasOwn`/`keys`/`values`/`entries`, literals, JSON stringify and
  checked parsing (JSON schema 15), ECMAScript own-key order and a
  `MAX_DICTIONARY_ENTRIES` bound. Mixed fixed/index object types are rejected.

- Recursive types under opt-in v34 (selector `recursive_types`): non-generic
  aliases and interfaces that refer to themselves through record fields, array
  elements or dictionary values, such as a `JsonValue` union and trees, with
  checked JSON parsing; non-productive cycles stay rejected.

- Synchronous generators under opt-in v33 (selector `generators`, K4.4 step 2):
  annotated top-level and generic `function*` declarations
  (`Generator<T, TReturn, TNext>`, with `void`/`undefined` defaults instead of
  `any`), `yield` in normalized expression positions and in `try`, `catch`,
  `finally` and `switch` case bodies, the `next`/`return`/`throw` protocol with
  ECMAScript state rules and the re-entry `TypeError`, `IteratorResult<T,
  TReturn>` records and `for...of` that closes the generator on `break`,
  `return` or a throw so its `finally` blocks run first. A generator object is
  its frame handle without a value tag (value ABI unchanged); it is resumed by
  a closed per-type step function and never through `varta_resume`. Frame
  catalog rows gain `generator` and the `Guest` input class. `yield*`, async
  generators, generator methods and generators inside aggregates or unions are
  rejected with diagnostics. Behavior matches pinned Node 24 logs.
- Runtime admission of the K4.4 frame surface under opt-in v33 (selector
  `resumable_frames`). Runtime preflight and artifact validation share one rule
  set: function catalog version 2 is read only for v33, `varta_resume` must be a
  closed guest function `(i64, i32, i64) -> i32` without tables or indirect
  calls, and each frame layout must agree with its creator, resume body and
  binding classes. V32 and earlier profiles reject every piece with a typed
  error before any guest effect. The runtime does not call the dispatcher yet;
  frame modules still come only from the test-only compiler hook.
- K4.4 suspension foundation (block 1), compiler-internal, admitted by no
  profile. It adds evaluation-order normalization of suspension points on IR and
  a resumable state machine over continuation frames, a separate managed kind
  that stores saved bindings in binding cells and shares captured closure cells.
  Frames have their own budgets and exact GC tracing, the exception channel is
  empty at every suspension, and the closed `varta_resume` dispatcher checks
  ownership, generation, reentrancy and inputs. Function catalog version 2
  carries the frame layouts; the version 1 reader rejects it. Unsupported
  positions fail with construct-specific diagnostics. Only compiler tests reach
  it, through a test-only lowering hook; existing modules and profiles are
  unchanged.
- Opt-in v32 (pilot batch, in development) adds nine composable features for
  the uarp-sdk pilot, each behind its own profile selector. V31 and earlier keep
  rejecting all of them. Host imports, value tags, layouts, host/value ABIs and
  production budgets are unchanged; the only versioned change is exception
  channel v5 for Error subclasses. The inventory adds one case per feature that
  a single-file case can test.
  - Module constants: top-level `const`/`export const` with a pure checked
    initializer subset and `as const` readonly bindings. Scalar constants fold to
    literals; reference values are evaluated once in dependency order before
    `main` and held in permanent collector roots.
  - Literal types: string, number and boolean literal types and their unions as
    compile-time value sets that erase to their base slots, with
    membership-checked assignability, `===`/`!==`/`||` narrowing and up to 4096
    members per set; all 356 string-literal aliases of the pilot `models.ts`
    resolve. `varta.json.parse<T>` rejects literal-typed targets.
  - Classes: typed and `readonly` fields, `public`/`protected`/`private` checks,
    constructors with a definite-initialization discipline, methods, getters,
    static methods, `extends` with `super(...)` and `super.m(...)`, nominal
    subtyping, `instanceof` and closed-world override dispatch. Instances are
    records with per-class descriptors and exact tracing. `#private`, static
    fields and setters remain rejected.
  - Module re-exports: `export { x as y } from`, `export type { … } from`,
    `export { type T } from` and `export * from` forward functions, constants,
    named types and classes without local bindings. Functions and constants share
    one value namespace; star exports follow ECMAScript rules (no `default`,
    explicit exports win, ambiguous names excluded and rejected only when named)
    and chain transitively. Namespace forms stay rejected.
  - Template literals: substitutions are evaluated once, left to right, converted
    with primitive ToString (canonical Number text, BigInt decimal text; literal
    types as their base) and joined by guest concatenation with exact UTF-16
    parts. Non-primitive substitutions (including class instances), tagged
    templates and `String.raw` fail with diagnostics. The source-complexity
    pre-pass rescans template text after each substitution, so template text and
    later doc comments no longer trip a false complexity error.
  - Default exports: `export default` declarations, bindings and constant
    expressions, `import x from`, `import x, { y } from` and `default` in export
    and re-export lists link as the ordinary name `default` (never forwarded by
    `export *`). Namespace imports are rejected with a diagnostic.
  - Interface inheritance and merging: `interface B extends A, C` builds the
    combined closed record from local, imported, aliased and generic record bases
    (`interface B<T> extends A<T>`) with checked redeclarations, and repeated
    non-generic interface declarations merge. Class bases, merging generic
    interfaces and interface/class merging (the pilot `UarpClient` pattern) are
    rejected with diagnostics.
  - Generics: generic interfaces, aliases and top-level functions by
    monomorphization, with defaults, `extends` constraints, explicit or inferred
    type arguments, `keyof`, `R[K]`, `Readonly`, `ReadonlyArray`, `Partial` and
    literal-keyed `Record`; one function per distinct instantiation within the
    existing function and body budgets. Generic functions and interfaces are
    default-exportable. `Record<string, V>`, `Promise<T>` and generic classes
    (including generic Error subclasses), methods and arrows stay rejected with
    reasons.
  - Error subclasses: classes may extend `Error`, `TypeError`, `RangeError` and
    `SyntaxError` with writable `message`/`name`/`cause` fields,
    `super(message, { cause })`, `new.target.name`, `toString()`, `instanceof`
    for the native family and every class in the chain (including downcast
    narrowing of caught values) and catch/finally/rethrow across modules.
    Instances stay class records with no new tag or layout; v32 selects
    exception channel v5 (`varta.exception.version_v5`) so the runtime reports
    an uncaught instance as `ThrownValue::ErrorInstance`. `unknown` causes,
    `stack`, JSON serialization of instances and `Error`-annotated instances
    remain rejected.

- Opt-in v33 (suspension and generators, in development) is registered. It
  inherits every v32 feature and declares a new host ABI, so runtimes without
  frame admission reject its modules; the value ABI, exception channel and JSON
  selectors are those of v32. Its `resumable_frames` and `generators` selectors
  admit nothing yet.

- Opt-in v31 adds named interface and type alias annotations, resolved per module
  and across imports, with `import type`, `{ type T }`, `export interface/type`
  and `export type { … }`. The project loader resolves `./x.js` specifiers to the
  `./x.ts` source for v31. Recursive, generic and `extends` declarations fail with
  diagnostics. Layouts, ABIs and limits are unchanged; the inventory adds one
  named-type case.

- Prepared runtime modules retain immutable verified function catalogs through
  startup, tool invocation and HTTP request execution. Stores own their module's
  metadata, reject replacement after instantiation starts and clear it on failure.
  Existing module-validation APIs and source-profile contracts remain compatible.

- Value14 codec groundwork adds catalog-checked Function tag12 and a Rust JSON
  graph encoder with function omission/null rules and bounded capture validation.
  Schema14 retains existing JSON input types; current source profiles/host imports
  stay unchanged pending function-aggregate integration.

- Allocation-free managed closure/environment/binding envelope views prepare
  versioned function-value transport, independently checked against emitted heap
  objects. Optional v30 capture catalogs add typed value/GC-mask validation,
  result-pair checks and bounded recursive closure inspection. Function-valued
  aggregate admission remains pending.

- Opt-in v30 adds optional-record deletion, primitive delete keys and array key
  handling, preserving canonical absence, order, aliases/GC and checked required
  fields. Object.entries returns fresh dense key/value pairs for closed records
  and typed arrays, preserving sparse presence and shared child identities. It
  reuses v29 layouts and advertises 33 source profiles. Object.keys/values/entries
  also enumerate String UTF16 units and primitive unions with guest-only helpers;
  keys/values evaluate ignored arguments before snapshots under v30. Object.is
  adds SameValue comparison, retaining NaN, signed-zero and reference identity
  semantics through rooted once-only argument evaluation. Checked Object.assign
  adds ordered record/array/String copies, shared identities, partial-write errors
  and preallocated field inference for fresh record literals. Existing objects
  retain declared storage; general dynamic properties and descriptors remain gaps.

- Opt-in v29 adds BigInt graphs, sparse arrays, optional records, primitive-union
  arithmetic/conversions, compound assignments and numeric updates, owned BigInt exceptions and recoverable JSON TypeError.
  ABI28/value13/schema13/channel4 preserve old profiles and exact roots; array
  searches use BigInt value equality and malformed graphs/resources stay fatal.
  The reviewed inventory then pinned v30 with 78 cases (67 supported, 11 known gaps),
  after promoting the unchanged coercive-width gap; primitive width/radix
  coercions preserve ToIndex/ToBigInt order and recoverable error classes.
  Primitive update ToNumeric preserves converted postfix values, NaN/presence
  semantics and checked numeric stores without widening mutable storage types.
  Primitive compound conversion handles String concatenation and numeric rules,
  bounded wide-union dispatch, correct mixed-numeric failure types and metered
  conversions, preserving once-only reads/RHS effects and checked stores.
  Ordinary primitive arithmetic, exponentiation, shifts and unary +/−/~ reuse
  conversion with scalar fast paths; unary plus retains native validation and
  a Number result domain while rejecting BigInt with TypeError.
  All eight primitive equality/ordering operators preserve exact BigInt, UTF16
  order, nullish/Boolean conversion and NaN semantics; native transport/fuel
  checks and scalar fast paths are covered by a 2048-comparison Node oracle.

- Opt-in v28 connects BigInt scalars to rooted binary leaves and pure ABI27
  arithmetic/conversion/comparison/width operations with guest fuel accounting,
  recoverable semantic errors and fatal malformed/resource errors. Value/schema12
  stay frozen; BigInt aggregate/union/throw/JSON and broader coercive built-ins
  remain pending. The reviewed corpus pins v28 with 46 cases (34 supported,
  12 known unsupported), including an explicit BigInt-array gap.

- Internal `varta-bigint` mathematical foundation provides immutable integers,
  parsing, arithmetic, shifts, bitwise operations, Number/radix conversions and
  signed/unsigned widths with caller-supplied bit/work budgets. It is now
  consumed by the v28 scalar profile. The
  binary leaf transport has canonical block/payload validation, budgeted
  import/export and a generated-GC fixture. Large shifts charge destination work;
  malformed binary transport never maps to a recoverable SyntaxError.

- Opt-in v27 infers one admitted union for uncontextual array elements, flattening
  existing unions and preserving source-order effects, exact Number bits, shallow
  reference identity and roots. Empty/all-hole peers use typed array evidence;
  explicit contexts, array invariance, shape/Function/opaque restrictions and
  production budgets persist. Shared ABI26/value12/schema12 requires rebuilt
  companions with the exact v27 profile list. The reviewed compatibility corpus
  closes its inference gap and grows to 40 cases (28 passes, 12 known unsupported).

- Developer-only `varta-conformance` pins an ECMAScript 2025/v27 checked-program
  inventory with source/dependency digests, isolated default-budget workers,
  bounded protocols and explicit pass/gap/regression/infrastructure/untested
  reporting. CI checks the reviewed baseline; strict mode exposes remaining gaps.
  The suite then recorded 28 passes, 12 known unsupported cases and 7 untested
  feature rows. Full Test262/script support remains planned CORE-15 work.

- Opt-in v26 adds checked Array.reduceRight with seeded/unseeded accumulator
  inference, descending live sparse traversal, recoverable empty-fold TypeError,
  callback/seed/ignored-argument ordering and rooted intermediate graphs. Existing
  reduce/profile admission remains unchanged; no new host imports, dependencies,
  ABI/layout versions or production budgets. Rebuild both companions for v26.

- Opt-in v25 adds checked Array.findIndex/findLast/findLastIndex with live sparse
  reads, forward/reverse snapshot traversal, cached matched values, receiver and
  ignored-argument evaluation, rooted callbacks and unchanged capability/fatal
  boundaries. Existing profiles reject these methods. The callback-before-thisArg
  lowering fix also prevents stale refinement reads in existing callback methods.
  Shared ABI26/value12/schema12 and production budgets remain unchanged; rebuild
  both native companions for exact v25 profile negotiation.

- V24 specializes stable callback parameters at proven calls, propagating symbolic
  inputs through forwarding, const aliases and captures. Direct/nested parameter
  replacements retain unknown reentry. Default/rest/receiver argument mapping,
  borrowed guard/loop effects and shared proof-depth limits preserve existing
  call/root/status contracts without ABI/layout/production-budget changes.

- V24 bounds local-cell effects using immutable function provenance and transitive
  closure summaries. Known calls preserve unrelated captured facts; module calls
  account for supplied callback proofs. Argument effects, unknown/self/parameter
  calls and mutable/returned functions retain conservative invalidation. Checked
  non-reentrant member intrinsics preserve binding facts without promising
  unchanged heap contents. No ABI/layout/production-budget or runtime dependency change.

- V24 tracks reachable break/continue facts by loop, switch and label target.
  Updates/do-while tests join their actual inputs; normal loop/switch/label exits
  combine false tests and matching breaks. Finally writes transform pending exits,
  and overriding returns/throws/branches cancel old completions. Repeated-part
  analysis after one-time for initializers fixes stale second-test assumptions.
  Physical control routing, ABI26/value12/schema12 and production limits stay intact.

- V24 infers local union types after successful stores, including destructuring,
  compound/update and logical assignments. Normal if/ternary branches join their
  resulting types; early return/throw paths do not erase surviving facts. Skipped
  defaults/RHS, switch entries/fallthrough, labels, loop continues and exceptional
  prefixes cannot leak a later store's fact. Declared storage, prior profiles,
  ABI26/value12/schema12 and execution budgets are unchanged.

- V24 allows guarded writes through per-binding revision tracking, branch joins,
  loop effect analysis and conservative invalidation after possible user calls.
  Mutable captures retain declared cell types; const captures inherit stable
  narrowed types through nested closures. Compiler-only swc_ecma_visit29.0.0 adds
  bounded AST traversal. Runtime distribution, ABI26/value12/schema12, capabilities
  and production limits remain unchanged; older profiles keep readonly guards.

- Opt-in `checked-f64-utf16-v24` adds truth-based direct-local narrowing, compound
  guards and &&/||/?? RHS refinement, including while/for bodies and for updates.
  False branches retain potentially false Number/String/Boolean members. Scoped
  refinements use the write/capture analysis above; v23 keeps
  its earlier source admission. ABI26/value12/schema12, imports and limits are
  shared unchanged; native negotiation adds v24 to the supported profile list.

- V23 integrates guest ToBoolean into conditions, loops, ternaries, !, logical
  assignment tests and existing predicate callbacks. Operand-valued &&/||/?? cache
  the original left value, narrow selected branch types and preserve identity,
  signed zero and short-circuit effects without native truth/nullish imports.
  Guest roots, fatal limits and older profiles retain their contracts; no ABI,
  dependency or resource-limit change.

- V23 adds direct Boolean conversion over admitted ordinary values and coercing
  global isFinite/isNaN over primitives/unions. Exact guest ToBoolean uses no host
  call or allocation; numeric predicates reuse existing Number conversion only
  for non-Number arguments. Cached effects/roots, ordinary binding precedence,
  status and fatal limits are retained, with no ABI/dependency/resource changes.

- Opt-in `checked-f64-utf16-v23` / ABI26 adds direct global and Number.parseInt /
  parseFloat calls on primitives/unions. Longest-prefix grammar, exact leading
  whitespace, signed Int32 radix conversion, hexadecimal detection, signed zero
  and bounded nearest/even integer rounding preserve effects, roots/status and
  fatal budgets. Each present input uses one capability-free host call; no result
  buffer, dependency, value/schema12 or resource/container limit changes.

- Opt-in `checked-f64-utf16-v22` / ABI25 adds direct Number.toFixed,
  toExponential and toPrecision with bounded exact rounding, primitive precision
  conversion, method-specific RangeError order and owned UTF-16 results. Direct
  Number.valueOf preserves the original bits and ignored effects without a host
  call. Value/schema12, status, resource limits and container formats are retained.

- Opt-in `checked-f64-utf16-v21` / ABI24 adds direct Number.toString for bases2–36,
  cached primitive radix arguments and catchable RangeError in the specified order.
  Bounded exact intervals choose shortest nondecimal text using direct runtime
  num-bigint0.4.8 (already locked via the compiler); decimal text reuses ryu-js.
  Owned results, roots/status, fatal budgets and source-free packaging retain
  value/schema12 and existing resource/container boundaries.
- Opt-in `checked-f64-utf16-v20` / ABI23 adds primitive Number(...) conversion
  and whole numeric String parsing: exact whitespace/grammar, decimal/exponent and
  binary/octal/hexadecimal prefixes, nearest/even rounding, signed zero and NaN/
  infinity rules. Cached effects/roots, status/buffers, fatal limits and source-free
  packaging are retained, with unchanged value/schema12 and no dependency change.
- Opt-in `checked-f64-utf16-v19` / ABI22 adds primitive String(...) conversion,
  including admitted unions, absent/ignored argument rules, canonical Number text
  and exact UTF-16. Checked cells transfer through the existing result buffer into
  owned strings; status, roots, fatal limits and lexical bindings are retained.
  Source-free packaging negotiates v19/22; value/schema12 and limits are unchanged.
- Opt-in `checked-f64-utf16-v18` / ABI21 adds canonical ECMAScript Number output
  for scalar and union logs: shortest decimal/scientific notation, zero and
  nonfinite rules. Existing ryu-js1.0.3 formats numbers; old profiles retain their
  text. Exact registry/signature gates, budgets/status, roots and source-free
  native packaging are covered, with unchanged JSON/value/schema/limits.
- Opt-in `checked-f64-utf16-v17` / ABI20 adds direct Math.random with fresh OS
  entropy, exact 53-bit samples in [0,1), ignored argument effects and fatal
  entropy/budget failures. Runtime-only pinned getrandom 0.3.4 supplies portable
  entropy; no seed/cache is persisted. Value/schema/container versions and limits
  stay unchanged; native packaging negotiates v17/20.
- Opt-in `checked-f64-utf16-v16` / ABI19 adds nineteen unary native Math methods
  and atan2 through a shared typed pure registry. Required zero/NaN/infinity/domain
  rules precede platform-approximated finite math; one-call/deadline accounting,
  status, helper purity and old-profile contracts are preserved. Value/schema12,
  container formats and limits are unchanged; native packaging negotiates v16/19.
- Checked variadic v15 `Math.hypot` uses exact power-of-two scaling, compensated
  squares/sums and root refinement in guest code. All arguments execute before
  Infinity/NaN selection; extreme values, subnormals, near-overflow high-precision
  goldens, GC/pairs and artifact/permission paths are tested without new ABI/budgets.
- Checked v15 `Math.f16round` rounds binary64 directly to binary16 ties-to-even
  using guest instructions. Signed zero, subnormals, overflow and halfway neighbors
  are verified against an independent bit oracle without binary32 double rounding,
  new imports, ABI versions, dependencies or increased execution limits.
- Checked v15 `Math.pow(base, exponent)` shares the existing exponentiation
  import, Number edge cases, finite approximation and one-call/deadline budgets.
  Argument order, host-free-helper rejection and operator/method agreement are tested.
- All eight Math numeric constants and eight Number constants in v15, with exact
  binary64 values and dot/literal-key reads. Lexical shadows retain ordinary
  properties; built-in writes have a read-only diagnostic. Guest constants add
  no host imports, allocation, ABI changes or limits.
- Direct v15 `Number.isFinite/isNaN/isInteger/isSafeInteger` predicates accept
  admitted values and unnarrowed unions without coercion. Non-Numbers/missing
  arguments return false; ignored extra arguments retain all effects and errors.
  Guest-only execution preserves status, pairs/roots and existing ABI/budgets.
- Checked `Math.min/max`, `sign`, `fround`, `clz32` and `imul` in v15. Variadic
  extrema preserve argument order and NaN/signed-zero rules; binary32 rounding
  and exact modulo-32-bit conversions/products use guest instructions. No new
  host import, capability, ABI version or runtime budget.
- Direct checked `Math.abs`, `floor`, `ceil`, `trunc`, `round` and `sqrt` calls
  in v15, each with one narrowed Number argument. Guest instructions preserve
  signed-zero/nonfinite rules and rounding toward positive infinity on ties,
  without new imports, permissions, helper exports, ABI versions or limits.
- Opt-in `checked-f64-utf16-v15` / host ABI v18 adds Number `**` and `**=` through
  the pure typed `number_power` import. Explicit NaN/zero/Infinity/odd-integer
  rules precede implementation-approximated finite power, preserving status and
  one-call/deadline budgets. Value/schema v12 and container formats are retained;
  older profiles reject the new operators/import and native negotiation includes v15.
- Checked `&=`, `|=`, `^=`, `<<=`, `>>=` and `>>>=` in v12+ for mutable variables,
  static/dynamic record fields and numeric arrays, plus length in v14. Retain
  old values before RHS effects, validate actual Number operands, reuse full-range
  bitwise conversion and store computed results through existing checked references.
- Number-only `&`, `|`, `^`, `~`, `<<`, `>>` and `>>>` in v12+, with full-range
  ToInt32/ToUint32 semantics, signed/unsigned Number results and masked shifts.
  Conversion is inline, host-free and nontrapping, with exhaustive exponent/
  seeded-payload checks and unchanged union-narrowing/profile/resource boundaries.
- Checked prefix/postfix `++` and `--` in v12+ for mutable variables, static/dynamic
  record fields and numeric arrays, plus length in v14. Require an actual Number,
  store through one prepared reference, then return the exact old/new value;
  preserve partial effects, captured loop cells, roots and fatal boundaries.
- Checked `&&=`, `||=` and `??=` in v12+ for mutable variables, static/dynamic
  record fields and numeric arrays, plus length in v14. RHS evaluation and checked
  stores run only in the selected branch, retaining original values/references,
  presence, result types and rooted partial-chain/GC/finalizer behavior.
- Dynamic record `+=`, `-=`, `*=`, `/=` and `%=` in v12+, retaining the original
  receiver/key and old value across RHS effects, with checked computed results,
  exact selected-field stores and existing read-union/absence/prototype boundaries.
- Checked `+=`, `-=`, `*=`, `/=` and `%=` in v12+ for locals, static fields
  and numeric array references, with cached old values, matching Number/String
  guards, computed results, rooted nested effects and existing v14 length checks.
- Simple local, dot-field and numeric-array assignment expressions in v12+, with
  shared prepared references, original RHS types/values, full result/function
  ownership and v14 length-expression checks.
- Computed record assignment expressions/chains in v12+, returning the original
  RHS type/value with prepared references, static literal checks, rooted temporary
  floors and preserved inner writes on outer failure.
- Typed computed String assignment statements on closed records in v12+, with
  prepared receiver/key roots, exact selected-field checks, Error family guards,
  optional presence/order and catchable failures after RHS effects.
- Typed computed String reads on closed records in v12+, with inferred field/Undefined
  unions, presence-aware UTF-16 lookup, preserved payload identity and rooted operand
  effects. Unsupported inherited lookups raise TypeError; prototype access stays closed.
- Dynamic String own keys for sparse arrays in v13+ and Error families in v12+,
  preserving canonical indices/length, holes versus explicit Undefined and own
  message presence, argument effects, exact roots and older profile boundaries.

- Checked TypeScript-to-Wasm compilation, local module loading, source diagnostics
  and explicit, versioned language profiles.
- Opt-in binary64 numbers, UTF-16 strings, typed unions, optional fields, bounded
  arrays, checked JSON, recoverable exceptions, typed functions and lexical closures.
- Array callbacks, optional/default/rest parameters, spread calls, and shared
  object/array destructuring for parameters, local declarations and assignments.
- Expression-valued destructuring assignments and checked record-field/array-element
  targets, including defaults and rest. V14 adds indexed extension and writable length.
- Intrinsic array for-of in v12+, with binding/assignment patterns, per-iteration
  lexical captures, live mutation, sparse Undefined visits and finally cleanup.
- Checked do-while in v12+, with trailing-condition continue, finalizer routing,
  capture/root cleanup and guaranteed-first-body completion/return inference.
- Statement labels and named break/continue in v12+, including loop aliases,
  exact finalizer/scope routing and target-aware completion inference.
- Object.keys for closed records in v12+, with presence-aware JS key ordering,
  fresh dense UTF-16 string-array results, nullable checks and rooted snapshots.
- Object.values for closed records in v12+, with inferred homogeneous/union
  arrays, shared own-property ordering, rooted payload preparation and shallow identity.
- Object.keys/Object.values for bounded arrays in v12+, including sparse own-index
  enumeration, exact element types and rooted host-free decimal key creation.
- Catch object binding patterns in v12+ for name/message, with aliases/defaults,
  TDZ, rooted initialization and existing exception/finalizer routing.
- String.slice/substring in v12+, with bounded Number/Undefined normalization,
  exact UTF-16 ranges, owned result storage and preserved Array.slice dispatch.
- String.includes/startsWith/endsWith in v12+, with exact UTF-16 matching,
  bounded positions, allocation-free helpers and unchanged Array.includes rules.
- String.indexOf/lastIndexOf in v12+, with UTF-16 code-unit indices,
  method-specific position normalization and allocation-free helpers.
- Array.lastIndexOf, Array.reverse, and Array.fill in resizable-array profiles,
  with tagged-value matching/bounds, in-place mutation, sparse handling and
  ownership preservation.
- String.trim/trimStart/trimEnd in v12+, with exact UTF-16 whitespace
  removal, surrogate/NUL preservation, and owned result storage.
- Dynamic String keys for Object.hasOwn on closed records in v12+, with exact
  UTF-16 decision ordering, rooted argument evaluation and presence-only lookup.
- Capability-checked host APIs, independent host policy ceilings, programmatic
  operation results, and bounded execution with managed guest memory.
- Checked synchronous HTTP/1 handlers in the default language profile.
- Primitive tool discovery and checked named JSON invocation.
- Portable Wasm artifacts and native executables with an independent execution-only
  runtime companion, without shipping the TypeScript compiler.
- Contributor and security policies, an MIT license, and GitHub issue/PR templates.

### Changed

- Unhandled-rejection reports (v34 and v35 scheduler) hold bounded host
  memory: each reason is copied out of guest memory only up to 256 UTF-16 code
  units per string field and rendered to at most 2048 bytes, at most 16
  outstanding reports keep a rendered reason and every other report keeps one
  tracking bit. The termination diagnostic marks a cut reason with
  `[reason truncated]` and names an earliest remaining report whose reason was
  not kept as `<reason not retained>`; `SchedulerError::UnhandledRejection`
  carries a `RejectionReason`. Guest-observable behaviour and the termination
  itself are unchanged.

- The module graph admits 256 unique modules (was 64) and 2048 import and
  re-export declarations (was 256) in every profile. The active-path depth, the
  per-file and aggregate source byte limits and the user-function limit are
  unchanged. The pilot package has 65 modules and about 370 relative
  declarations, so its entry graph exceeded the earlier limits.

- Source complexity limits bound recursion instead of width: the per-file token
  count is removed (byte limits remain), and list commas in `[...]`/`{...}` and
  top-level union/intersection members of a type alias no longer count toward
  the 256-token chain limit. Nesting, expression chains and call arguments keep
  their existing limits.
- In v31 a module-local type named like an Error family (for example an API
  `Error` model) shadows the built-in type name in annotations, as in TypeScript.

- Documentation is restructured: a short README orientation page, a
  [language guide](docs/language-guide.md), per-profile
  [implementation rules](docs/profile-rules.md), a [history](docs/history.md) of
  completed increments and a rules-only `CLAUDE.md`. Contract text moved
  verbatim. The development plan records the research thesis and the uarp-sdk
  pilot. Version and limit tables in `docs/architecture.md` and the profile indexes
  are generated from the registries and checked by a workspace test.

### Fixed

- V38 `charAt` and string index reads of a non-ASCII unit copy the unit into a
  one-unit string in the guest instead of calling the linked
  `string_char_at16`, which copied the whole string per call (about 670 fuel
  on a 4-unit string, 23,600 on a 1002-unit one, more than v37's host call).
  A non-ASCII read now costs about 100 fuel at any length; a Markdown-like
  block parser of one chat reply fell from 80,042 to 59,276 fuel.
- `varta dev --profile-fuel` profiles programs with checked JSON, tools or
  HTTP: the host-free value, tool and HTTP ABI helpers and their call graph
  stay uninstrumented (validation rejected their probes as host imports), and
  their fuel counts toward the caller, so the profile still equals the
  ordinary run's fuel.
- Dispatch Array.lastIndexOf to reverse search instead of Array.indexOf. Omitted
  positions start at the final element; explicit undefined/NaN select zero,
  positive infinity clamps to the final index and positions below -length miss.
- Generate valid sparse Array.reverse helpers by retaining the i64 array handle
  for bitmap lookup; keep payloads, presence and identity together in v13/v14.
- Preserve U+0085 in trim/trimStart/trimEnd: it is not ECMAScript WhiteSpace or a
  LineTerminator. Tests cover all 25 admitted whitespace/line-terminator units.
- Restore v13 rejection of writable length/deletion and fatal indexed extension
  checks and add source-free regression coverage.

### Compatibility and limits

- The default profile retains its checked i32/UTF-8 semantics; newer language
  features require their documented opt-in profiles.
- Native packages target the build machine's OS and CPU. CI covers Linux and
  macOS; Windows and cross-target packaging remain unverified.
- VARTA3 packages contain portable Wasm. Legacy VARTA1/VARTA2 and native AOT
  payloads are rejected and must be rebuilt.
- Full ECMAScript/TypeScript and Node/npm compatibility, asynchronous execution,
  MCP bridges and artifact signing remain planned.

See the [language guide](docs/language-guide.md) for supported features and the
[development plan](docs/development-plan.md) for remaining work.
