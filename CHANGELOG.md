# Changelog

## Unreleased

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The
workspace uses package version `0.1.0`; these entries describe the development
snapshot and do not announce a published release. Counts quoted in an entry
describe the state when it was written. Current status is in the
[development plan](docs/development-plan.md#status) and current conformance counts
in [conformance/README.md](conformance/README.md).

### Added

- Opt-in v36 (pilot batch: checked declared fields, function fields, async
  generators and timers, in development) is registered. It inherits every v35
  feature, shares value ABI/layout 16 with v35 and declares its own host ABI 32
  (`TIMERS_VERSION`) without a new import yet; the launcher advertises host ABI
  32. A v36 module relabelled as v35 fails at its host ABI section, and
  runtimes that do not list v36 reject its declared profile name before any
  guest effect. Its `checked_declared_fields`, `function_fields`,
  `async_generators` and `timers` selectors admit nothing yet.

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
