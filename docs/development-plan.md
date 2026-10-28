# Varta development plan

## Development checkpoint — 2026-10-28

Stage: experimental compiler/runtime core, with K1–K3 implemented in their initial
checked profiles and K4 language/value work in progress. This is a development
snapshot, not a production release or full Deno/Bun/ECMAScript compatibility.
Engine work remains the priority; applications and MCP adapters stay separate.

Completed since baseline commit `526f9ab`:

| Core increment | Implemented boundary |
| --- | --- |
| Object.values for records | Exact/admitted union result types, present-own ordering, shallow identity and rooted cached payloads |
| Object.keys/Object.values for arrays | Ascending own indices, dense results, skipped holes and exact value element types |
| Catch object bindings | Static name/message keys, aliases/defaults, TDZ, fresh captures and cleanup through outer handlers/finalizers |
| String.slice/substring | Optional Number/Undefined bounds, exact UTF-16 units, owned result strings and unchanged Array.slice |
| String.includes/startsWith/endsWith | Required String needle, bounded optional positions, allocation-free unit comparison and unchanged Array.includes |
| Array.findIndex/findLast/findLastIndex (v25) | Typed forward/reverse predicate search, live sparse reads, snapshot bounds and rooted cached matched values |
| Array.reduceRight (v26) | Reverse folds with typed seeded/unseeded accumulators, live sparse presence and rooted values |
| Array literal inference (v27) | One admitted element union, rooted once-only initializers, typed peer empties and unchanged mutable-array invariance |
| BigInt scalars (v28) | Canonical managed leaves, exact arithmetic/conversions/comparisons, recoverable semantic errors and typed width/radix methods; aggregate/union/throw/JSON integration pending |
| BigInt graphs (v29) | Value/schema13, exact reference masks, primitive-union arithmetic, native JSON TypeError and owned BigInt exceptions; BigInt compound assignments and numeric updates; primitive width/radix, update, compound and ordinary arithmetic/unary coercion and six-kind comparison/equality; object ToPrimitive pending |
| Object operations (v30) | Optional-record deletion and dense Object.entries snapshots for closed records/typed arrays; String/primitive own-index enumeration, Object.is/SameValue and checked Object.assign with fresh-record shape preparation; descriptors and general objects remain pending |
| Function codec groundwork | Common value14 tag12, catalog-required function cells, typed graph checks and JSON omission/null rules; source profile and versioned host operations remain pending |
| Checked compatibility inventory | Pinned v30/ECMAScript 2025 sources, isolated executor, explicit supported/gap/untested counts and reviewed-baseline CI gate |

Earlier additive operations require v12+; sparse/mutation rules remain profile-specific.
The default remains `checked-i32-utf8-v1`. The latest opt-in profile is
`checked-f64-utf16-v30`, using host ABI v28 with value ABI v13 and schema v13.
It adds optional-record deletion and Object.entries for closed records/typed arrays,
inheriting v29 BigInt graphs/primitive unions, JSON TypeError and BigInt exceptions,
v28 checked BigInt scalars and v27 uncontextual array element-union
inference, inheriting v26 Array.reduceRight and v25 Array.findIndex/findLast/findLastIndex and v24
truth-based local narrowing, post-store/targeted-exit joins, proven call effects
and stable callback-input specialization,
write/capture-aware invalidation and short-circuit
RHS guards, inheriting v23
global and Number.parseInt/parseFloat prefix parsers, Boolean conversion and
coercing global isFinite/isNaN, guest truth conversion/control flow and
operand-valued logical operators, inheriting v22
Number.toFixed/toExponential/toPrecision and guest-only valueOf, plus
v21 checked Number.toString in bases2–36 and v20 primitive Number/numeric
String parsing, v19 String conversion and v18 canonical Number output,
v17 OS-backed Math.random, twenty
v16 native Math imports and
v15 Number exponentiation, with no new permission/helper export/value/schema/
container version or limit change.
Fourteen checked Math intrinsics execute as guest instructions in the same profile,
with strict Number arguments and no additional host imports or version change.
Math.pow shares the existing versioned power host operation and its budgets.
Number predicates accept admitted values/unnarrowed unions without coercion,
retaining missing/ignored-argument semantics and the same guest-only boundary.
The source/artifact paths and public engine API are covered by regression tests.

The v28 BigInt scalar increment uses host ABI27 with unchanged value/schema12.
It connects parsed literals, rooted locals/parameters/returns/closures and exact
arithmetic/bitwise/shift operations to the binary mathematical core. BigInt(),
String/Number/Boolean/typeof, primitive comparisons, Number/Undefined-radix
toString, valueOf and explicit Number-width/BigInt-value asIntN/asUintN are covered.
Declared semantic errors are recoverable; malformed transport and resource limits
stay fatal. Structured BigInt tags, arrays/records/unions/throws, JSON behavior,
compound updates and complete coercive/method forms remain pending. CORE-01/07
remain Partial; scalar execution does not close the BigInt work stream.

The v29 increment admits BigInt tag11 only under value13. Managed records, sparse
arrays and reference cells retain exact child masks, including mathematical zero.
Primitive-union arithmetic dispatches cached values without replaying source
expressions; mixed BigInt/Number pairs throw TypeError. Versioned observation and
conversion imports validate complete BigInt blocks and account native work.
JSON validates the bounded graph before a recoverable BigInt TypeError, keeping
malformed metadata/cycles/resource failures fatal. JSON tokens never create a
BigInt; checked unions select a compatible JSON-kind member. Exception channel4
owns uncaught binary payloads and preserves BigInt through catch/finally/GC.
V28 keeps its scalar-only ABI27/value12 boundary. Primitive width/radix coercion
and primitive arithmetic/unary/update/compound coercion are supported in v29
within checked source/storage types.
Object ToPrimitive remains outstanding.
All CORE work streams retain their previous completion state.

The completed v29 comparison/equality block passed 1179 runtime tests at the broad
checkpoint, then seven focused comparison tests after preserving scalar-nullish
fast paths and the UTF16 iterator cleanup, plus a strengthened direct-operand GC
case. Common/compiler suites pass (55/36 tests). Strict common/compiler/runtime
Clippy, the cross-module API, rebuilt runtime companion and source-free CLI/native
cases pass. Current conformance reports 78 cases: 67 supported, 11 known gaps,
seven untested feature rows, with no regressions or infrastructure failures.
The full pre-commit gate is run once for this completed code batch; a local pass
does not prove cross-platform or complete ECMAScript readiness.

V28 scoped verification includes **1117 runtime unit tests** at the broad
checkpoint, followed by **15 targeted BigInt tests** after the final lexical
shadowing test/fix; **99 compiler/common/numeric tests**, **24 conformance runner
tests**, the dependency-boundary test, public virtual-module BigInt execution and
the CLI/source-free Wasm/native artifact BigInt case passed. Workspace all-target
Clippy and formatting passed. The v28 baseline and strict reports agree on
**46 cases: 34 supported, 12 known unsupported, 7 untested feature rows**; strict
exit 1 remains expected. This is scoped batch evidence, not a new full workspace,
full CLI repeat, cross-platform run or BigInt-completeness claim.

Last full checkpoint, before binary transport work, on macOS arm64 with Rust
1.98.0 and locked dependencies: **1582 workspace
tests passed**, followed by **268 CLI tests passed** in a separate repeat run.
Formatting, the core-only library check, runtime-companion build, workspace/all-target
build and Clippy with warnings denied also passed. This records local verification;
it does not claim a new cross-platform CI or release qualification.

Current boundaries remain explicit: 32 record fields, 1024 array elements and a
1 MiB encoded string ceiling, alongside source/type/frame/root/memory/fuel/deadline
budgets. Catch patterns do not admit arrays/rest/other properties or aggregate
throws. String search rejects coercion/regex/omitted needles; includes currently
uses candidate scanning with quadratic worst-case cost, bounded by existing fuel.
Maximum-string stress tests use explicit test fuel without changing runtime defaults.

Completed: **String.indexOf/lastIndexOf** and **Array.lastIndexOf**, including
UTF-16/code-unit index results and their method-specific position rules.
Checked compiler support and allocation-free helpers are available in the
relevant resizable/UTF-16 profiles.

Review follow-up restores the method and compatibility boundaries: Array.lastIndexOf
uses reverse dispatch, distinguishes an omitted position from explicit Undefined,
and clamps/rejects bounds before conversion; Array.reverse uses the complete i64
handle for sparse bitmap access. Trimming excludes U+0085 while retaining the full
ECMAScript WhiteSpace/LineTerminator set. Reinstated v13 regression checks continue
to reject writable length/deletion and trap indexed extension. New tests reproduce
the original failures, verify dense/sparse profiles and execute dependency modules
in source-free Wasm/native artifacts. No ABI, capability or resource-limit change.

Completed: **dynamic String own keys for closed records** in v12+, with exact
UTF-16 lookup, presence-aware results and rooted source/key evaluation.

Completed: **dynamic String own keys for arrays (v13+) and Error families (v12+)**.
Array keys preserve canonical index/length and sparse-presence rules; Error keys
read only own message presence. Key effects, original identity, roots and earlier
profile boundaries are covered without an ABI change.

Completed: **typed computed-key reads on closed records** in v12+. Narrowed String
keys return the admitted union of field types and Undefined through presence-aware
UTF-16 lookup. Key effects run once on the retained original receiver; scalar bits,
reference identity and GC/exception/finalizer lifetimes are preserved. Unsupported
inherited names throw TypeError unless declared own; no prototype access or coercion.
Literal exact types and older-profile rules remain unchanged, with no ABI change.

Completed: **typed computed-key writes on closed records** in v12+. Simple assignment
statements prepare source/key before RHS effects, then check full selected-field
types before storing. Preserve shapes, invariant arrays, Error families, optional
presence/order and root/exception/GC lifetimes. Unknown keys and mismatched values
throw TypeError after RHS success; no new properties, coercion or ABI change.

Completed: **computed record assignment expressions** in v12+, with original RHS
types/values, static computed-literal checks and prepared references through chains.
Saved temporary floors retain earlier operands/pairs and original results through
GC, calls and finalizers. Inner failure skips outer stores; outer failure retains
successful inner writes. No ABI/import/helper/layout change.

Completed: **ordinary local, dot-property and array assignment expressions** in
v12+, sharing prepared references/storage with destructuring. Preserve original
RHS types, local function/capture identity, full result pairs and mixed chains.
Numeric expressions validate after RHS against current backing/presence; v14 adds
checked length expressions, while earlier profile/mutability/type/budget rules persist.

Completed: **checked compound arithmetic assignments** (`+=`, `-=`, `*=`, `/=`, `%=`) in v12+
for locals, static record fields and numeric arrays, plus length in v14. Prepared
references cache old values before RHS, then validate matching Number/String
operands, compute and store with existing type/bounds/ownership rules. Preserve
RHS effects, checked absence errors, partial chains, GC/pairs and fatal limits.

Completed: **dynamic record compound references** in v12+, retaining receiver/key
identity and old-value snapshots across RHS effects. Reuse read-union admission,
strict arithmetic kinds and full selected-field checks, with explicit inherited/
missing timing, partial chains and rooted GC/finalizer lifetimes.

Completed: **short-circuit assignment operators** (`&&=`, `||=`, `??=`) in v12+,
retaining prepared references and evaluating/storing RHS only in the selected
branch. Preserve skipped values/presence, original RHS types/results, current
storage/profile rules, function captures, partial chains and rooted GC/finalizers.

Completed: **checked prefix/postfix increment and decrement** in v12+, preserving
prepared references and actual Number checks, then returning old/new values only
after successful stores. Captured/fresh loop cells, partial effects, exact numeric
bits, optional/sparse absence, length errors and rooted GC/finalizers are covered.

Completed: **checked Number bitwise and shift operations** in v12+, with explicit
full-range ToInt32/ToUint32 conversions, masked counts and signed/unsigned Number
results. Inline host-free conversion preserves source order, ordinary narrowing,
GC/pairs/status and profile/resource boundaries; exhaustive numeric checks pass.

Completed: **bitwise compound assignments** in v12+, retaining prepared references
and old values before RHS, strict runtime Number guards, computed results and
post-RHS selected-field/current-storage checks. Partial chains, absence timing,
GC/pairs/finalizers, unsigned results and fatal/profile boundaries are covered.

Completed: **checked Number exponentiation** (`**`, `**=`) in the opt-in v15/ABI18
profile, with explicit signed-zero/nonfinite/odd-integer rules and bounded pure
native power. Ordinary finite results are implementation-approximated; registry/
profile/signature checks, host-call/deadline budgets and old-profile gates persist.

Completed: **checked Math.abs/floor/ceil/trunc/round/sqrt** in v15, with one
narrowed Number argument, exact rounding/signed-zero rules and guest-only bounded
execution. All-exponent numeric checks, argument effects/roots, profile/type/
shadowing gates and source-free artifact paths are covered without an ABI change.

Completed: **Math.min/max, sign, fround, clz32 and imul** in v15, with explicit
arity, exact integer conversions/products, binary32 rounding and NaN/zero rules.
Variadic argument order, exceptions, roots and pure guest execution are covered.

Completed: **Number.isFinite/isNaN/isInteger/isSafeInteger** in v15, accepting
admitted values and unnarrowed unions without coercion. Missing/non-Numbers return
false; ignored extra arguments retain effects/errors. Exact numeric boundaries,
union tags, pairs/roots/finalizers and artifact/permission paths are covered.

Completed: **all Math and Number numeric constants** in v15: exact binary64 values,
dot/literal-key reads, ordinary lexical shadowing and read-only write diagnostics,
with no import/allocation/ABI change. Source-free and public-API paths are covered.

Completed: **Math.pow** in v15 as a two-Number intrinsic over the existing power
host operation, with operator/method edge-case bits, one-call accounting, argument
order and helper-purity checks.

Completed: **Math.f16round** in v15, with direct binary64-to-binary16 ties-to-even
rounding through exact guest rescaling. Every binary16 encoding and finite midpoint
neighborhood, both signs, all f64 exponents and a binary32 double-rounding example
are checked against an independent integer-significand oracle. No new host/ABI/limits.

Completed: **Math.hypot** in v15, with all-argument Number snapshots, exact
power-of-two scaling, compensated products/sums and root refinement in guest code.
Infinity wins over NaN after all argument effects; empty/all-zero calls return +0.
Exponent-wide comparisons, exact scaled triples/subnormals, high-precision overflow
goldens, compensation, roots/pairs/finalizers and source-free/grant paths are covered.
Finite results remain implementation-approximated; no new ABI/import/budget.

Completed: **nineteen unary native Math methods and atan2** in v16/ABI19,
with shared typed registry names/signatures, exact required edge cases and bounded
platform-approximated finite math. One-call/deadline/status rules, inherited values,
helper purity, hostile metadata/signatures, GC/pairs and source-free/public APIs are
covered. Value/schema12, default/older profiles and resource limits remain unchanged.

Completed: **Math.random** in v17/ABI20, using a fresh OS word per call and exact
53-bit sampling in [0,1). No shared PRNG/seed/cache is persisted; ignored argument
effects, entropy failures, limits, status, roots and source-free artifacts are
covered. The function remains separate from the deterministic native registry.

Completed: **ECMAScript decimal Number output** in v18/ABI21 for scalar and union
logging. Shortest decimal/scientific text and zero/nonfinite rules use the existing
pinned formatter; old profiles retain their text. Exponent/threshold/minimality,
registry/budget/GC/status and source-free paths are covered.

Completed: **primitive String(...) conversion** in v19/ABI22 for all admitted
primitives/unions, absent/ignored-argument rules, exact UTF-16 and owned results.
Status, roots, buffers, limits and old bindings/profiles retain their contracts.

Completed: **primitive Number(...) conversion** in v20/ABI23, providing the numeric
argument foundation for Number.prototype.toString. Strict numeric String grammar,
exact whitespace, decimal/prefixed rounding, signed zero/nonfinite/empty rules and
ignored effects/roots/status/budgets are covered without new limits or dependencies.

Completed: **checked direct Number.prototype.toString** in v21/ABI24, bases2–36,
primitive radix/default/ignored argument rules and catchable RangeError before
nonfinite cases. Exact midpoint intervals select shortest native text; decimal
uses the existing formatter. Bounds, independent rational checks, roots/status/
limits and source-free paths are covered; num-bigint0.4.8 gains direct runtime use.

Completed: **Number.toFixed/toExponential/toPrecision and valueOf** in v22/ABI25.
Exact bounded decimal rounding, missing/Undefined arguments, method-specific
RangeError ordering, guest-only identity and ignored effects are covered alongside
roots/status/budgets, independent Decimal goldens and source-free/public paths.

Completed: **Number.parseInt/parseFloat and their global forms** in v23/ABI26.
Primitive String/radix conversion, exact longest-prefix grammar/whitespace, Int32
radix wrapping and bounded nearest/even integer rounding preserve status/buffers,
roots/pairs and fatal budgets. Independent integer goldens and source-free/public
paths cover this boundary.

Completed: **Boolean conversion and coercing global isFinite/isNaN** in v23,
without an ABI change. Guest-only Boolean accepts ordinary values/unions including
references; numeric predicates reuse primitive Number conversion after cached
argument effects. Numeric exponent, roots/pairs/status/budget, helper-purity and
source-free/public paths are covered.

Completed: **guest ToBoolean across conditions, loops, ternaries, ! and existing
predicate callbacks**, plus operand-valued &&/||/?? and logical assignment tests.
Cached operands, selected-branch type narrowing, numeric bits/reference identity,
roots/pairs/status/limits and source-free/public paths are covered without ABI changes.

Completed: **truth-based direct-local narrowing and RHS guards** in opt-in v24,
sharing ABI26/value12/schema12 with v23. False primitive possibilities, scoped
compound/loop facts, write/capture invalidation and redundant guards
have regression coverage alongside metadata, roots/status and source-free paths.
V23 keeps its earlier truth-branch mutation admission.

Completed: **write-aware refinement invalidation and capture dataflow** in v24.
Per-binding revisions preserve alternative branch input and discard written facts
at joins; loop/test/callback effects prevent stale narrowed reads. Mutable captures
retain declared cell types and const proofs propagate through nested closures.
Compiler-only swc_ecma_visit29.0.0 adds bounded effect traversal; runtime dependency,
ABI, capability and limit boundaries are unchanged. Assignment/loop/capture/GC/
status/diagnostic and source-free/public paths have regression coverage.

Completed: **post-store local type inference and normal branch joins** in v24.
Stores preserve observed types before widening into declared storage; if/ternary
joins combine normally completing paths, including subunions and surviving early
return/throw guards. Logical targets use selected result types. Skipped RHS/default
stores, loop continues, switch direct entries/fallthrough, named breaks and
exception prefixes cannot leak facts. No ABI/layout/production budget changes.

Completed: **completion-targeted loop/switch/label flow** in v24. Reachable breaks
and continues carry types to their actual targets; updates/do-while tests join
normal/continue inputs, and loop exits combine false tests with matching breaks.
Finalizers transform or cancel pending exit facts in unwind order. One-time for
initializers precede repeated-only mutation analysis, fixing stale second-test
assumptions. GC, nested targets, overrides, diagnostics and source-free/public
execution have regression coverage with unchanged ABI/layout/runtime budgets.

Completed: **proven call effects for local cells** in v24. Immutable function
provenance and transitive closure summaries preserve unaffected captured facts;
module calls account for supplied callback proofs. Unknown/mutable/returned/self/
parameter targets stay conservative. Argument writes, guards/loops, shadowing, GC,
denial status and source-free/public paths are covered without runtime/ABI changes.
This does not admit function-valued aggregates or prove property immutability.

Completed: **stable callback-input specialization** in v24 closure summaries.
Symbolic callback positions bind actual arguments through forwarding, const copies,
read-only captures and default/rest/receiver prologues. Direct/nested parameter
replacement retains unknown reentry. Separate committed writes from local flow
invalidation; shared-depth AST guard/loop effects, GC, diagnostics and source-free/
public paths preserve ABI/layout/resource contracts. Higher-order opaque inputs
and point-sensitive mutable/returned provenance remain conservative.

Completed: **Array.findIndex/findLast/findLastIndex** in opt-in v25, sharing
ABI26/value12/schema12. Typed callbacks receive live values (including Undefined
for holes), indices and the original array. Reverse searches, snapshot bounds,
mutation/deletion/truncation, cached matched values, thisArg and ignored argument
effects preserve roots, capabilities, system status and fatal limits. Existing
profiles reject the new methods. Generic array-like/prototype calls and other
array APIs remain unfinished. See the [predicate-search contract](architecture.md#predicate-array-search-profile-v25-with-shared-host-abi-v26).

Completed: **Array.reduceRight** in opt-in v26, sharing ABI26/value12/schema12.
The fold visits present indices in descending order, uses an explicit typed seed
or the rightmost present element, and throws recoverable TypeError for unseeded
empty/all-hole arrays. Live mutation/presence, source/callback/seed/ignored-argument
ordering, accumulator inference/identity, callback roots and fatal/resource/capability
boundaries have regression coverage. V25 and earlier reject the new method;
existing reduce admission stays unchanged. See the
[reverse-fold contract](architecture.md#reverse-array-fold-profile-v26-with-shared-host-abi-v26).

Completed: **initial checked compatibility inventory and isolated runner**.
The pinned ECMAScript 2025/v30 corpus has 40 feature rows and 78 original checked
programs: 67 supported, 11 known unsupported, with 7 feature rows untested. Source
SHA-256 checks, complete corpus listing, strict metadata, default runtime budgets,
empty authority and per-process deadline/output bounds preserve explicit results.
Baseline CI distinguishes expected gaps from conformance passes; strict mode
currently fails. The suite exposed uncontextual array union inference as a
separate CORE-01/02 gap; v27 closes that case and adds four positive cases. This advances CORE-15 to Partial and does not close it;
upstream Test262 execution and broader coverage remain required. See the
[runner contract](../conformance/README.md).

Completed: **uncontextual array literal inference** in opt-in v27, sharing
ABI26/value12/schema12. Source-order expressions are lowered once, admitted union
members flatten into one final element type, and existing boxing/rooting preserves
Number bits, reference identity, holes and captured-cell effects. Empty/all-hole
array peers can use typed array evidence without side-effect reanalysis; existing
aggregate contexts and explicit annotations remain strict. Conflicting record/
array kinds, Function/opaque elements and untyped empty graphs remain unsupported.
The old profile still rejects the formerly unsupported `[1, undefined]` case;
the v27 compatibility baseline now records it as supported. Source/type/root/fuel
and function-body limits remain unchanged. See the
[inference contract](architecture.md#array-literal-inference-profile-v27-with-shared-host-abi-v26).

Next: **higher-order callback-argument binding, property/exception effects, initializer
inference and loop fixed points**, then
the broader object/prototype and conversion foundation. Remaining
array APIs remain tracked under CORE-02; existing filter/find/some/every
callbacks use the new truth path. General
object-to-primitive String/Number conversion follows the object/prototype foundation.
General Math namespace objects, coercion and borrowed methods remain separate language work.
Remaining work also includes generalized record reads/writes/coercion, constructors and prototypes,
aggregate exception design, scheduler/promise jobs, standard/system API expansion
and package compatibility. Wider dynamic values remain explicit profile work.
Shared application state, MCP, signing and external adapters remain later
milestones. Detailed completed contracts and sequencing follow.

## Product direction

Varta targets general-purpose, typed TS/JS applications compiled to portable
Wasm and shipped with a controlled execution environment. A developer should be
able to implement a JSON HTTP server in TS/JS using Varta's own runtime APIs,
without writing a separate Rust/Node/Python server wrapper for each application.
Network I/O belongs in the runtime; routes, payload schemas and business rules
belong in the TS/JS application. Explicit AI tools and MCP are optional interfaces
on top of a capable application core, not prerequisites for ordinary applications.

This is the target direction, not a claim that these platform features already
exist. The current implementation is the compiler/runtime foundation described
in [architecture.md](architecture.md): a checked TypeScript subset, local modules,
strings and reclamation, captured host results, capability policies, native
packaging, primitive Tool Contract v1, offline discovery and checked JSON tool
invocation. Closed records, bounded resizable/sparse typed arrays in opt-in profiles,
`null`/`undefined`, graph tracing and bounded
`JSON.stringify`, `varta.json.parse<T>` and the first HTTP/1 service profile are
implemented. A shared-state application service, broader Application Contracts,
MCP, package signatures,
GUI/database/hardware adapters and full JS/npm compatibility are not implemented
yet. HTTP listeners are independent of the existing outbound client.

### Runtime target and compatibility

The product target is a self-contained JS/TS runtime in the same application
category as Deno/Bun, retaining Varta's TS/JS → Wasm → controlled-runtime path.
The current deliberately small checked subset is an implementation stage, not
the permanent product boundary. A JSON server is the first vertical acceptance
scenario, not the definition of a complete runtime.

The complete core roadmap must cover:

| Area | Target | Current gap |
| --- | --- | --- |
| Language semantics | ECMAScript values, numbers, strings, objects/arrays, functions/closures, errors and control flow | Default `number` is i32; opt-in binary64 supports closed aggregates and JSON. Opt-in UTF-16 supports lone units and character methods; closed primitive unions, typed functions and initialized lexical captures are implemented; dynamic objects/coercion and broader function semantics remain pending |
| Memory | Bounded, correct ownership and tracing for full application value graphs | Collector traces closed records, resizable/sparse arrays, reference unions, Error/closure graphs and strings; general dynamic object graphs remain pending |
| Scheduling | Event loop, promises, microtasks, async/await, timers, cancellation and shutdown | Synchronous bounded invocations only |
| Standard APIs | JSON, URL, encodings/binary data, Request/Response/Headers, streams and abort signals | Most standard APIs are absent |
| System APIs | Capability-checked filesystem, environment, processes and inbound/outbound networking | Existing bounded operations are a small partial API |
| Modules/packages | ES module semantics, a documented resolution/loading policy and tested package compatibility | Explicit local named-function modules only |
| Distribution | Independent runtime, portable guest artifacts and target-specific native packaging | Runtime-only companion implemented; cross-target packaging, signing and platform release distribution remain pending |
| Verification | A pinned conformance target, public-API compatibility cases, negative/security tests and resource/performance measurements | Current regression tests prove the MVP contract, not Deno/Bun or ECMAScript completeness |

Do not treat unsupported JS as solved by providing an application-specific host
wrapper, and do not claim full compatibility because a few server examples run.
Track each missing semantic/API explicitly. Package/API compatibility and broader
language semantics continue after the first HTTP milestone.

Before extending the value representation, specify the intended JS semantics and
version/profile migration: binary64 Number values (including -0/NaN/infinities),
UTF-16 string behavior including lone surrogates, null versus undefined,
coercion/equality, property ordering and aliasing. Existing ABI/contract v1 i32
artifacts must retain their documented interpretation or be rejected by version;
never reinterpret them silently as the future JS-compatible profile. Preserve
clear diagnostics for features not yet implemented.

Primary references: [ECMAScript language values](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html),
[Deno HTTP server](https://docs.deno.com/runtime/fundamentals/http_server/) and
[Bun HTTP server](https://bun.sh/docs/runtime/http/server). These define semantic
and developer-experience references, not a claim of current Varta support.

### Core completion ledger

Completion is measured against the full core target above, not the number of
commits, regression tests or implemented checked profiles. The entries below are
work streams, not equally weighted percentage points. A work stream can close only
when its explicit feature inventory, executable positive/negative cases and
documented compatibility bounds pass. A global 100% claim additionally requires
the conformance, platform and independent security acceptance work; it cannot be
inferred from the current regression suite.

| ID | Work stream | Current state | Remaining acceptance work |
| --- | --- | --- | --- |
| CORE-01 | Number, String and primitive semantics | Partial | Remaining BigInt/object ToPrimitive and inferred storage generality (v28 scalars; v29 graphs, primitive-union arithmetic, throws and JSON integration), Symbol, Symbol/object comparison and remaining conversion operations, locale formatting and general object coercion (canonical logging v18; primitive conversions v19/v20; checked toString/radices v21; precision/valueOf v22; prefix parsers, Boolean/global predicates and guest truth/control flow v23; scoped truth guards v24), remaining operators and primitive built-ins against the pinned ECMAScript target |
| CORE-02 | Objects, arrays and value graphs | Partial | Dynamic properties, property descriptors, prototypes, constructors/classes and their tracing/identity semantics |
| CORE-03 | Functions and control flow | Partial | Complete call/receiver/binding behavior, remaining iteration and generator semantics; retain existing closure/exception guarantees |
| CORE-04 | Exceptions | Partial | General thrown values and complete error families/propagation, with explicit roots and unwinding rules |
| CORE-05 | Standard collections and binary values | Planned | Map/Set, typed arrays, ArrayBuffer/DataView and other target built-ins with ownership/resource contracts |
| CORE-06 | Math and remaining language built-ins | Partial | Finish the explicit Math inventory; cover Date, RegExp and remaining selected standard objects |
| CORE-07 | Garbage collection and resource lifecycle | Partial | Extend exact tracing to every admitted value kind; verify retained roots, cancellation and leak-free long-lived lifetimes |
| CORE-08 | Event loop and promise jobs | Planned | Specify and implement bounded task/microtask queues, Promise resolution/rejection and ordering |
| CORE-09 | Async language and cancellation | Planned | Async/await, timers, cancellation/abort and owned shutdown, integrated with scheduling and roots |
| CORE-10 | JSON and Web APIs | Partial | General JSON parsing plus the target URL/encoding/Request/Response/Headers/stream APIs |
| CORE-11 | System APIs | Partial | Complete the declared filesystem/process/network surface with exact permissions, portable errors and bounded I/O |
| CORE-12 | ES modules and packages | Partial | Module initialization/linking/loading semantics, explicit package-resolution policy and tested package targets |
| CORE-13 | ABI and capability enforcement | Implemented for current profiles | Retain fail-closed version/signature/resource checks for every newly admitted boundary |
| CORE-14 | Distribution | Partial | Supported target matrix, Windows/cross-target validation, release delivery and package authenticity |
| CORE-15 | Conformance | Partial | Pinned checked-case inventory and isolated runner are implemented; expand edition-wide cases, script/module/harness execution for upstream Test262, historical profiles and independent compatibility verification |
| CORE-16 | Performance and production readiness | Partial | Reproducible startup/memory/throughput/leak measurements, sustained workloads, fuzzing and independent security review |

The immediate implementation order remains CORE-01/02/03/06 foundation work,
followed by CORE-07/08/09 scheduling and lifetime integration and CORE-10/11/12 API
and package expansion. CORE-13/15/16 verification accompanies every increment.
Application state, MCP and domain adapters remain separate deferred work streams;
they do not substitute for missing engine semantics.

CORE-06's Math inventory is pinned to the
[ECMAScript 2025 Math object](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-math-object):

| Math surface | Current state |
| --- | --- |
| abs, ceil, clz32, floor, fround, f16round, hypot, imul, max, min, pow, round, sign, sqrt, trunc | Checked direct Number calls implemented in v15 |
| acos, acosh, asin, asinh, atan, atanh, atan2, cbrt, cos, cosh, exp, expm1, log, log1p, log10, log2, sin, sinh, tan, tanh | Checked direct Number calls implemented in v16/ABI19 |
| random | Fresh OS entropy and exact [0,1) samples implemented in v17/ABI20; ignored arguments preserve effects |
| E, LN10, LN2, LOG10E, LOG2E, PI, SQRT1_2, SQRT2 | Exact v15 numeric reads; descriptors/general namespace objects remain planned |
| General argument conversion/arity, spread, first-class methods and ordinary Math object properties | Planned beyond the current direct-call admission rules |

Counting method names does not establish complete Math semantics. Every remaining
entry needs its edge cases, argument/exception behavior, resource contract and
source-free runtime tests before CORE-06 can close.

CORE-01's Number surface is pinned to ECMAScript 2025:

| Number surface | Current state |
| --- | --- |
| Number(...) | Primitive/union conversion in v20; wrappers and object ToPrimitive pending |
| Boolean(...) and global isFinite/isNaN | V23 ordinary-value ToBoolean and primitive numeric coercion; wrappers/object ToPrimitive pending |
| Eight numeric constants; isFinite/isNaN/isInteger/isSafeInteger | Checked v15 reads/predicates |
| Number.prototype.toString | Narrowed direct calls, bases2–36 and primitive radix semantics in v21 |
| toFixed, toExponential, toPrecision, valueOf | Direct narrowed Number calls in v22, primitive precision conversion, exact rounding and owned results; valueOf guest-only |
| parseInt/parseFloat and global aliases | Direct primitive/union calls in v23, prefix grammar, Int32 radices and bounded exact integer rounding |
| toLocaleString | Planned with an explicit locale/Intl contract |
| Wrappers, ordinary prototype objects/descriptors, first-class methods | Planned under object/function foundation work |

These checked methods do not close CORE-01 or establish general Number-object
compatibility; all remaining rows and full conversion/equality/value targets still apply.

### Architecture commitments

- Keep the engine application-agnostic. Application entities, state schemas,
  commands, domain validation and domain permissions are defined by TS/JS
  applications. The engine supplies generic execution, state ownership,
  transactions, versions, limits and authorization mechanisms. Reference examples
  must not become built-in document/editor/robot profiles in engine crates.
- Define the supported TS/JS language profile explicitly. Grow it through concrete
  application requirements; unsupported dynamic behavior continues to fail clearly.
- Keep source compilation separate from interface publication. An ordinary source
  export does not automatically expose an AI tool. Developers explicitly choose
  which operations form the public tool surface.
- Distinguish **Application Contract** (commands, queries, data, errors, state,
  revisions and lifecycle) from **Tool Contract** (the AI-exposed subset, names,
  descriptions, schemas, required effects and limits). Both share one definition
  of the operation rather than duplicating business logic per transport.
- Preserve runtime boundary checks. External/AI arguments and results require
  bounded validation even when their TS definitions were checked at build time.
  Schemas describe data; host policy separately authorizes concrete effects.
- Treat artifacts, callers and AI-generated arguments as separate trust inputs.
  Effective authority must satisfy package requests, host ceilings and caller
  permissions. Discovery and tool annotations never grant execution rights.
- Ship public contract metadata together with executable code. Validate its link
  to real handlers. Signing must bind code, contracts and requested permissions;
  a recomputable checksum alone does not establish a trusted publisher.
- Keep the portability promise precise: portable Wasm plus compatible host
  interfaces, with native launchers built per supported OS/CPU. Do not promise
  one native executable for every platform or hard real-time behavior.
- Avoid including source implementations in release tool catalogs. Names, schemas
  and descriptions are intentionally public. Wasm remains analyzable/editable;
  compilation does not guarantee secrecy of business logic, and signature checks
  require a trusted verifier. Credentials belong to trusted host-side providers.

### Current repository scope

Core development comes first. The engine consists of `varta-common`,
`varta-compiler` and `varta-runtime`. Project loading/configuration, scaffolding,
packaging and CLI presentation are supporting development tools. They consume
engine APIs and must not become engine dependencies.

No separate application project is being built in this repository. The scenarios
below are deferred consumers of the core, not work items for built-in domain
models. First design and verify the generic mechanisms; only then build a separate
TS/JS project using those real APIs. Test source fixtures remain necessary for
compiler/runtime regression coverage.

### Active priority: a usable application core

The immediate priority is engine capability, not an editor example or MCP.
The active order is **K1 structured values/JSON → K2 server lifecycle/inbound
HTTP → K3 runtime packaging and end-to-end validation**, now implemented in their
initial checked profiles. K4.1 and K4.2's binary64/aggregate/JSON increments are
implemented; separate undefined-capable and UTF-16 profiles are also available.
K4.2 adds primitive unions and checked optional/missing values, extends unions to references and continues with broader dynamic values and external adapters. JavaScript semantic
compatibility is a cross-cutting requirement, followed by the wider language,
scheduler, standard-library and module/package work below. Generic application
state and AI integration remain later work. Existing C1/C2.1 achievements below are preserved;
they do not dictate the priority of the remaining core work.

A native TS/JS program serving a real JSON object is the acceptance scenario for
these generic capabilities. It is first exercised through compiler/runtime/CLI
regression fixtures, not a domain-specific server implementation inside Varta.
Only after the APIs actually work should a separate application project use them.
An external wrapper around `varta invoke`, or a guest returning a string that a
separate host parses as JSON, does not satisfy this acceptance scenario.

### Deferred external validation scenarios

| Profile | Example contract operations | Required behavior |
| --- | --- | --- |
| Desktop editor | Inspect a document, rename or patch it, update selection | Connect to the existing application instance; preserve revisions, history/undo and UI consistency |
| Data service | Find records, change an allowed business status | Enforce identity/data scopes, transactions, bounded queries and stable errors |
| Robot software | Read telemetry, start a mission, request a stop | Validate state/command limits; define duplicate, timeout and disconnect behavior; keep the local safety controller independent of AI transport |

A **headless document application** was selected as a possible later shared-state
demonstration: a title, revision and small command/query interface, controlled by
two test clients and then an external MCP client. It is deferred behind the active
core priorities and is not the next implementation task. It exercises shared state and conflicting changes
without requiring a GUI toolkit, database service or physical hardware. This is
a future external validation scenario, not a commitment to a GUI framework.
It must be built as a separate TS/JS application on public Varta APIs, with its
own source, configuration and domain rules. Choosing this demonstration does not
authorize adding document models, title rules, rename commands or application
roles to `varta-common`, `varta-runtime`, `varta-project` or the core CLI.

## Completed milestone: architectural hardening

The first milestone is a checked, bounded single-file TypeScript subset with
capability-controlled execution and one-file native packaging. The implemented
contract is in [architecture.md](architecture.md).

The October 2026 architecture audit is addressed as follows:

| Audit finding | Implemented change | Regression evidence |
| --- | --- | --- |
| Untrusted native deserialization | Native AOT entry points removed; portable Wasm only | Rejection through runtime and all artifact commands |
| Filesystem boundary and check/open race | Component-aware scopes and pinned no-follow descriptors | Allowed file, sibling, traversal, final/intermediate/internal symlink cases |
| Malformed manifest grants expand rights | One strict schema and operation-specific grant types | Wrong/missing fields and unknown operations rejected |
| HTTP request injection | Parsed authorized URLs and bounded HTTP library | CR/LF denied before I/O, normalization and authority tests |
| Unbounded resources | Store memory limits, fuel, epoch deadline, I/O/output/call budgets | Infinite loop, guest memory.grow, output and oversized file tests |
| Function type order mismatch | Signature interning shared with emission | Mixed-arity functions execute correct results |
| Silent semantic changes | Lexical scopes, typed IR, structured branch labels | Shadowing, nested switch, default/fallthrough, continue and top-level-effect rejection |
| Artifact validation misses invalid code | Full Wasm/ABI validation before publish and every artifact command | Invalid operand stack and missing ABI globals rejected |
| Divergent check/build/dev | Shared project pipeline and deterministic internal subset typing | Identical errors for all three commands |
| Stale string length and sentinel errors | Reusable result buffer, cleared result globals, status API | Success then denial, structured status, bounded output |
| Container marker collisions | Fixed versioned EOF footer with checked lengths | Marker literals survive native packaging; overflow lengths rejected |
| CLI reports success after failure | Central command error/exit handling | Run, inspect and validate return code 1 on bad input |

Additional hardening: source complexity is bounded before recursive parsing;
syntax errors return typed diagnostics instead of a dropped-diagnostic panic;
workspace code forbids unsafe; operation/signature metadata has one owner;
filesystem devices are not readable; proxies and redirects are disabled;
unsupported compat and HTTPS configurations fail explicitly.

Quality gates for a completed code batch (see the
[scoped verification map](verification.md) for the development loop):

```sh
python3 scripts/verify.py full
```

This runs formatting, isolated core/companion builds, strict workspace Clippy,
all-target build, workspace tests (including CLI once) and the reviewed conformance
baseline. Local and feature checks accompany intermediate implementation work;
launcher/packaging changes additionally repeat the CLI suite. Documentation-only
and verification-tooling changes use the checks specified in the verification map.

The CI workflow runs the gates on Linux and macOS. Tests are hermetic and use
synthetic sources, process-local environment values and temporary files; they do
not need a network service or external TypeScript checker.

## Completed follow-up: host policy ceilings

`run`, `dev`, `inspect`, `validate` and packaged launchers accept an explicit
`--policy` file. Runtime authorization requires both package and host filters;
empty ceilings deny all privileges and malformed files never fall back. File
access uses pinned host directory roots, including when a package supplies a
symlink as its own root. Inspection reports the two scopes without approximating
wildcard intersection. Host policies are not persisted into artifacts.

Regression coverage includes environment/path narrowing in both directions,
operation independence, scoped network authorization without network I/O, boolean
permissions, malformed/missing files, deny-all packaged execution and symlink-root
escapes. The public runtime and project services expose the same constrained-run
contract as the CLI.

## Completed follow-up: source diagnostics

The shared compiler source context preserves real entry filenames and precise
semantic ranges through expression/function IR. Human diagnostics show bounded
excerpts and carets; `check --json` exposes typed categories, UTF-8 byte offsets
and Unicode line/column ranges. Nested expression errors retain their original
locations. Virtual source paths are supported without file I/O, and naming a
source does not change its generated bytecode.

Regression coverage includes expected/actual type errors, nested calls, unsupported
annotations, Unicode/CRLF/BOM coordinates, parser/resource-limit errors, bounded
large-line excerpts, static-data overflow, config-relative entry paths and
consistent diagnostics across check/build/dev. Valid named-source modules are
instantiated and executed through Wasmtime as well.

## Completed follow-up: local modules

Named function imports/exports and aliases now link through an explicit,
deterministic dependency graph. Functions retain file-local scopes and source
diagnostics, imported calls retain their types, and dependencies are embedded
into portable/native builds. `check --json` exposes the graph for inspection.
Compiler embeddings supply a loader; project source loading stays within the
configuration directory through pinned no-follow descriptors. Cycles and
unsupported initialization/import forms fail explicitly. Module count, imports,
depth, total/per-file bytes and global function count are bounded.

Regression coverage includes shared dependencies, alias forwarding, recursive
library functions, private name isolation, imported type errors and static-data
diagnostics, deterministic bytecode, graph budgets, missing/private exports,
path escapes, symlinks, and execution after deleting sources. Imported host calls
obey both application grants and host ceilings. ABI and packaging are unchanged.

## Completed follow-up: retained string values

Strings now work in inferred/annotated local bindings, assignments, parameters,
explicitly annotated returns and dynamic host arguments across module boundaries.
Literal slices are static; host results are copied immediately into an immutable,
bounded execution arena. Saved values survive host-buffer reuse, nested calls and
reassignment. Full Wasm value types determine signature interning and local slots.
The raw host ABI, capability model and artifact versions remain unchanged.

Regression coverage includes Unicode/empty values, recursive and mixed-signature
functions, scope/control-flow slots, cross-module returns, native and Wasm runs,
multiple host arguments, dynamic environment/filesystem permission denials,
host ceilings, fuel exhaustion, and bounded heap behavior with additional
test fuel. String operators and reclamation are covered by the follow-ups below;
implicit string coercion remains unsupported.

## Completed follow-up: programmatic host errors

`varta.result.capture` now executes a direct privileged host call once and retains
an immutable typed `VartaResult<T>` containing `.ok`, `.status` and `.value`.
Values and failures stay paired across later host calls, aliases, reassignment,
recursive calls and module boundaries. String payloads keep their safe snapshot
ownership; the result itself uses a bounded two-value Wasm representation with no
record allocation. Function annotations, parameter limits, physical local slots
and multi-value returns are checked consistently. Shared editor declarations
document the API. Existing host imports, policies and artifacts remain compatible.

Regression coverage includes empty/false success, all five status codes, retained
Unicode payloads, nested call evaluation, readonly fields and type errors,
parameter budgets, identical check/build/dev rejection, Wasm/native execution,
host ceilings, denied network/process calls and authorized process exit. Resource
traps remain execution failures; capture does not introduce general exceptions.

## Completed follow-up: string comparison and concatenation

String `+` now creates immutable concatenated values, with empty-operand sharing,
left-to-right single evaluation and bounded allocation. Equality compares content;
ordering uses UTF-16 code units without normalization or locale collation.
Both operands must be strings. Host snapshots and concatenations share the same
checked arena; each string is bounded to 1 MiB of UTF-8 bytes. Comparisons require
no allocation, and pure string helpers do not affect saved or live host status.
Helper types/indices are resolved conditionally, including with the maximum user
function count. Host imports, capabilities, ABI and packaging are unchanged.

Regression coverage includes all eight comparisons across 484 pairs of empty,
NUL, ASCII, combining and BMP/supplementary Unicode samples against UTF-16 order;
nested/cross-module concatenation, retained host/captured values, operand evaluation
order, native/Wasm execution, diagnostic ranges, constructed resource denials,
per-value limits, bounded heap behavior, growth failures and comparison fuel exhaustion.

## Completed follow-up: string memory reclamation

Owned strings now use a bounded, nonmoving mark/sweep heap with block reuse,
splitting and adjacent-free coalescing. Collection runs before memory growth;
dead trailing blocks can lower the logical heap end. Committed Wasm
pages stay available for reuse until the instance is dropped. Static data and the
host result buffer remain separate. Host signatures, capabilities and artifacts
stay compatible.

Compiler-managed roots preserve bindings, aliases, string result payloads,
expression operands and call arguments. Scope exit, overwrite, `break`, `continue`
and return remove roots at the appropriate boundary. A fixed 64 KiB root stack
bounds frames and temporaries; root overflow, all-live heap exhaustion and failed
growth remain controlled execution failures. Collection and bookkeeping consume
guest fuel. Bindings are conservatively retained to scope exit; compaction and
last-use optimization are not implemented.

Regressions cover copying more bytes than fit in the heap with bounded committed
memory, nested expression/function transfers, recursive captured results,
middle-block aliases, free-block splitting/coalescing, loop/switch exits, static
and host buffer isolation, per-value limits, live-set exhaustion, root capacity,
fuel, and real CLI/Wasm/native behavior with unchanged permission denials.

## Completed follow-up: engine and tooling boundaries

- Core contracts no longer contain project path/output configuration.
  `VartaConfig` and `CapabilitySpec` now belong to `varta-project`.
- Runtime inspection returns backend-independent metadata; project tooling no
  longer imports Wasmtime, and CLI has no direct runtime dependency.
- Removed the production empty-module test helper and unsupported port-list
  permission types. Unsupported grant shapes continue to fail closed.
- Removed cancelled application-model files and empty abandoned directories.
  No domain models or application profiles were added to engine production code.
- A Cargo dependency-graph regression guards layer direction; a public API
  integration test uses the engine without project tooling or a CLI process.
  CI separately builds core libraries.

The repository contains engine libraries, their development tooling, tests,
documentation and CI. `target` is ignored build output. Native packages now use
the independent `varta-runtime` companion; dependency guards cover artifact I/O
and launcher crates as well as engine libraries.

## Delivery sequence

Core language/runtime capabilities now take precedence over the remaining
contract-platform work. Initial K1–K3 and K4.1 profiles below are implemented;
K4.2's number/aggregate/JSON increments are implemented; remaining value work is active.
C1 and C2.1 are completed
foundations; the unimplemented portions of C2–C5 are deferred. Work-package names
are not implemented commands or a frozen public API.

### K1 — Structured guest values and JSON (checked profile implemented)

- Specify a checked data profile before emission: typed record literals, named
  field access, nesting, optional/null semantics and collection limits. Begin
  with the closed record shapes needed to construct ordinary JSON responses;
  broader arrays and dynamic JSON are separate incremental extensions.
- Define representation, aliasing and ownership for records and contained strings.
  Extend roots/tracing or choose a validated bounded representation explicitly;
  tracing must retain nested objects/strings through aliases and field writes.
- Implement guest-visible JSON serialization with correct escaping, UTF-8 and
  encoded-byte/depth limits. A record must serialize as a JSON object, not as a
  quoted JSON string or the tool protocol's `{value: ...}` envelope.
- Add schema-checked JSON parsing incrementally before accepting structured
  request bodies. Define duplicate/unknown/missing fields and errors explicitly.
  JSON number handling must agree with the supported numeric profile; never
  silently round or truncate unsupported external values into i32.
- Preserve identical check/build/dev acceptance and test actual Wasm behavior,
  malformed input, escaping, retained nested values and resource exhaustion.

Implemented first increment: closed structurally typed records containing i32,
boolean, UTF-8 string, null and nested record fields can be constructed, passed,
returned, aliased, mutated and serialized in compiled Wasm. Object identity and
source evaluation/key enumeration order are preserved. The collector traces
record edges, keeps live strings/objects in place and reuses dead storage.
`JSON.stringify` uses a versioned descriptor ABI with checked memory, escaping,
cycle/depth/node/output limits and no added system authority. Compiler negatives,
hostile ABI data, repeated collection, effect catalogs and CLI dev/Wasm/native/
named invocation exercise the same path. No application model was added.

Implemented second increment: homogeneous dense arrays support `T[]`/`Array<T>`,
contextually typed empty literals, numeric indexing/replacement, `.length`,
nesting, identity and parameter/result transfer. The collector traces every array
reference, including beyond 32 elements. JSON checks array headers, kinds, ranges,
cycles and shared traversal/output budgets. The profile is capped at 1024 elements
per fixed-length array and 128 distinct array element types. Out-of-range indices
trap; sparse/resizable arrays remain pending. Undefined values are available in
the separate K4 v2 binary64 profile. Structured-value ABI
v2 adds arrays while the runtime continues to accept v1 records/scalars without
reinterpretation. Host ABI, artifact and primitive tool versions are unchanged.

Implemented third increment: `varta.json.parse<T>(text)` compiles a bounded closed
schema, checks complete JSON input and creates ordinary owned values using the
same guest allocator/root stack. It rejects duplicate/unknown/missing fields,
wrong types, unsupported number forms and budget violations. Parsed record order
is normalized by schema; this is a checked Varta extension, not the dynamic
ECMAScript JSON.parse API. Invalid input traps and named dispatch reports
`execution_failed`; system status is preserved. Runtime validation checks helper
signatures, transitive host-free call graphs and schema/descriptor consistency,
including modules without a tool catalog. Source-free Wasm/native artifacts use
the same parser and existing permissions. Host/value/container versions remain
unchanged; parsing has its own schema format version 1 and new registered import.

K1's initial checked data path is implemented. K2 below now provides its first
HTTP request/response, handler and lifetime profile.
`undefined`, optional/dynamic properties, general JSON.parse/revivers, recoverable
parse-result APIs, prototypes/hooks and full JS Number/UTF-16 semantics remain
broader language/runtime work. Tool Contract v1 signatures remain primitive;
structured wire contracts need their own explicit version.

### K2 — Generic server lifecycle and inbound HTTP (first profile implemented)

- Specify bootstrap/handler registration, application lifetime, request lifetime,
  shutdown and ownership. A long-lived listener must not be treated as one guest
  call that consumes the current five-second execution deadline forever.
- Start with checked named synchronous handlers if that is sufficient. A host-driven
  event loop can dispatch bounded guest calls without first promising general
  JS promises, closures or arbitrary async semantics. Freeze the callback/request/
  response ABI and error behavior before implementation.
- Provide generic guest-facing server primitives and typed request/response data.
  The developer writes routes and payload construction in TS/JS. Core code does
  not contain application endpoints or example response schemas.
- HTTP handler registration is independent of `varta.tool` and MCP. Ordinary
  server handlers need no AI-tool declaration and are not published to agents
  automatically. Reuse checked marshalling/execution mechanisms without making
  the tool catalog the mandatory entry point for all applications.
- Introduce an actual listener capability with validated address/port scopes and
  package/host conjunction before binding a socket. Outbound grants must never
  authorize listening. Do not revive an unused port-list placeholder as authority.
- Use a maintained HTTP implementation rather than hand-written framing. Bound
  connections, headers, bodies, responses, pending work, per-request fuel/memory/
  deadlines and aggregate work. Define disconnect, malformed request and trap
  isolation, backpressure and descriptor cleanup.
- Separate normal HTTP response bodies from tool invocation/MCP envelopes. Test
  real loopback requests with hermetic listeners and verify denied binds before
  effect. Keep HTTPS support explicit and separate from an initial HTTP profile.

Exit: a compiled TS fixture starts a permitted listener through Varta's API,
returns a real JSON object with a JSON content type, handles repeated requests,
isolates failed handlers and shuts down cleanly. No application-specific host
wrapper or MCP connection is required.

Implemented: `varta.http.serve({hostname,port}, handler)` is an entry-module
compile-time declaration, with typed `VartaHttpRequest` / `VartaHttpResponse` and
no required tool publication. The compiler generates an isolated string adapter
using real checked JSON/record mechanisms. Versioned HTTP metadata advertises the
handler, exact bind endpoint, effects and fixed limits. Its feature import makes
older runtimes fail explicitly. Runtime validation recomputes effects and checks
host-free transfer helpers before execution.

A permitted listener starts after bounded bootstrap. It outlives bootstrap's
five-second deadline; each request receives a fresh bounded store. Hyper/Tokio
own framing/timers. The first profile serializes connections, closes after one
reply, caps headers/body/envelopes, rejects unsupported framing/response headers,
isolates handler failures and supports owned graceful shutdown. Readiness/logs
stream through runtime events and the thin CLI. Loopback tests cover JSON bodies,
headers, repeated requests, slow input, malformed framing, fuel traps, scope
conjunction before bind, shutdown, and source-free Wasm/native artifacts.

Remaining extensions: concurrent/streaming requests, persistent guest state,
standard Request/Response classes, async JS, keep-alive and TLS/HTTP2. Authentication
and domain routes are application responsibilities. The checked HTTP acceptance
path and K3 runtime-only packaging are implemented. No separate application
project has been added.

### K3 — Runtime-only packaging and independent core validation (implemented)

- `varta-artifact` owns portable Wasm/manifest and fixed-footer container I/O,
  independent of VM and source tooling. The project re-export remains available;
  I/O errors now use shared `VartaError`. Existing VARTA3 layouts are preserved.
- `varta-launcher` builds `varta-runtime`, with no production dependency on the
  TS compiler, SWC, project tooling or development CLI/parser. It executes artifacts,
  invokes checked tools, inspects metadata and enforces optional host ceilings.
- `build --single` requires a sibling companion or explicit trusted `--launcher`.
  A private snapshot negotiates strict version/ABI/native-target compatibility
  and validates the payload without executing guest code before outputs are written.
  Probe output/time budgets, rejection of packaged candidates and preservation
  of existing output on preflight failure have regression coverage.
- Core dependency/public-API tests remain; CI builds the launcher independently
  and runs packaging regressions on Linux/macOS. Local validation is on macOS;
  Windows, cross-target packaging, release installers and signatures remain pending.
- K2 regression compiles/packages the HTTP fixture, removes source/configuration,
  deploys only the native executable in another directory and serves JSON with
  owned shutdown. Both standalone runtime and native self-execution are covered.
  Tool invocation/inspection also work source-free through the independent launcher.

This completes the first core HTTP/distribution acceptance path, not general
JS/TS compatibility. The user's current priority remains the core; no separate
application project has been added.

### K4 — ECMAScript value profile and language expansion

#### K4.1 — Profile negotiation and migration contract (implemented)

New compiler output declares `checked-i32-utf8-v1` in strict bounded
`varta.language` metadata. Runtime validation rejects unknown/malformed/duplicate
profiles before native compilation or execution; inspection distinguishes declared,
legacy ABI v1 and unspecified raw Wasm. Older ABI v1 artifacts preserve current
semantics. The runtime companion's protocol v2 reports supported profiles, which
packaging checks on its private snapshot. The physical ABIs/container do not change.
Regression tests cover both profile origins, existing arithmetic/string/identity
behavior, traps, unsupported source values, profile corruption and offline discovery
through runtime, CLI and native artifacts. This is a compatibility foundation;
K4.2 below adds opt-in binary64 with checked aggregates/JSON and a further
undefined-capable profile. A further UTF-16 profile is implemented; dynamic object behavior remains pending.

#### Target value semantics (compatibility target)

The pinned reference is **ECMAScript 2025, ECMA-262 16th edition**. The proposed
`ecmascript-2025-v1` name is reserved for the future value-semantics profile; it is
deliberately absent from the supported profile registry. It does not mean all of
ECMAScript 2025 or web/Node/npm APIs are available. Publish a feature/conformance
matrix as each part becomes executable; do not infer support from the profile name.

| Value area | Required future behavior |
| --- | --- |
| Number | IEEE-754 binary64, preserving signed zero, NaN and infinities; floating-point arithmetic, remainder and division; no i32 wrapping for ordinary Number operations |
| String | UTF-16 code-unit values, including lone surrogates; code-unit length/indexing and lexical comparison; no implicit Unicode normalization |
| Null/undefined | Distinct values/tags; void calls and missing properties can produce undefined; neither is integer zero |
| Identity | Object/array aliases preserve identity through mutation and tracing; primitives compare by their defined value rules |

These are target requirements, derived from the pinned
[ECMAScript 2025 value types](https://tc39.es/ecma262/2025/multipage/ecmascript-data-types-and-values.html).
Internal storage may use f64 or an explicitly specified tagged representation, but
must not lose signed zero or conflate NaN payload bits with heap references. The
collector traces actual reference tags, never arbitrary number/string bits. Object
property slots and array elements must preserve the same value distinctions.

Strict equality must return false for different value types and NaN, and treat
the signed zeroes as equal. Object equality uses identity. `Object.is`/SameValue
(v30) equates NaNs and distinguishes signed zeros; SameValueZero keeps NaNs
equal while joining signed zeros. Loose equality, relational operations,
truthiness and string/number conversion must follow the specified conversion order,
including short-circuit operand values and observable left-to-right effects.
Accepted mixed-type expressions must not reuse today's same-type comparisons.
See the pinned [abstract operations](https://tc39.es/ecma262/2025/multipage/abstract-operations.html).
Until an operation exists, the compiler must reject it explicitly. TypeScript
annotations may restrict admission but must not change the meaning of an accepted
JavaScript value/operation.

Standard JSON must preserve the specified distinctions: non-finite numbers become
JSON null, negative zero serializes as zero, undefined object fields are omitted
and undefined array elements become null; top-level stringify may return undefined.
Surrogates need well-formed escaping. Generic parsing needs binary64 numbers and
ordinary JSON object behavior. See the pinned
[JSON algorithms](https://tc39.es/ecma262/2025/multipage/structured-data.html#sec-json-object).
The existing `varta.json.parse<T>` is a separate strict schema API: duplicate/unknown
field rejection, integer-only schemas and bounds remain versioned contracts. Do
not silently turn it into unrestricted `JSON.parse` or weaken AI input validation.
Numeric-index property ordering, insertion order of other string keys, symbols,
holes and out-of-range array reads need their standard semantics when admitted;
the current closed-record/fixed-array restrictions are not the final JS model.
The property-order reference is
[OrdinaryOwnPropertyKeys](https://tc39.es/ecma262/2025/multipage/ordinary-and-exotic-objects-behaviours.html#sec-ordinaryownpropertykeys).

#### K4.2 — Value/ABI design and implementation (in progress)

The first scalar increment is implemented as **`checked-f64-utf8-v1` / host ABI v2**.
Project `languageProfile` and explicit compiler profile APIs select it for a whole
source graph; existing APIs/configurations default to the old profile. Number
parameters/locals/results use separate f64 slots, captured Number results use
`(f64,i32)`, and UTF-8 string handles retain exact roots. There is no NaN boxing or
dynamic Number heap layout. Host ABI v2 adds typed numeric logging, remainder and
exact checked process-exit imports; old pointer/i32 imports retain their meanings.
Artifact readers now validate manifest/code ABI agreement using the existing pinned
wasmparser dependency, without adding a VM/compiler dependency to distribution I/O.

Tests cover fractional/large/subnormal values, signed zero, NaN/infinity,
division/remainder, comparisons/truthiness, short-circuit effects, switch/loops,
function/module slots, captured status and collection with live strings. Public
engine APIs, dev, portable Wasm, independent runtime and native packages execute
the new path. Downgraded/missing metadata, incorrect signatures, legacy aggregate
metadata and manifest mismatches fail before execution. Permission denials and
host ceilings remain enforced. This is not full ECMAScript compatibility.

The aggregate/JSON increment is also implemented: **value ABI v3 / schema v2**,
with Float64 tag 6, raw numeric bits in eight-byte slots, reference-only tracing
and exact checked array indices. Separate JSON v3 imports bind the layout and
return full i64 parse payloads; allocator/retainer signatures and host-free effect
checks remain mandatory. Strict finite decimal/exponent parsing uses the existing
serde_json parser with round-trip conversion enabled. ECMAScript numeric text
uses exact-pinned ryu-js 1.0.3; -0 serializes to 0 and non-finite values to null.
Mixed metadata, descriptor/schema disagreement and unimplemented tool/HTTP
publication fail explicitly. Legacy i32 schemas/layouts retain their semantics.

Runtime/public-API/dev/native tests cover mutation/aliasing, signed zero, IEEE
rounding, formatting thresholds, partial-input rejection before allocation,
collection of nested reference graphs, incorrect indices, helper effects and
permission/host-ceiling preservation. Inspection reports the actual value ABI;
companion negotiation advertises ABI maxima and probes the concrete payload.

The undefined increment is implemented in **`checked-f64-utf8-v2` / host ABI v3 /
value ABI v4 / schema v3**. Undefined tag 7 has a canonical zero payload and is
separate from null. Void expressions/calls, explicit undefined annotations,
parameters/returns, typed typeof and nullish comparisons preserve effect order.
JSON v4 omits undefined object fields, emits null for array entries and returns
actual undefined at the root. Canonicality and traversal limits also apply to
omitted fields. Typed JSON parsing cannot target undefined or missing fields.
New imports/metadata prevent old runtimes from reinterpreting the representation;
all previous profiles remain available. Tests cover lexical shadowing/TDZ, void
captures/status, GC, malformed payloads/versions, public APIs and source-free native
packages under package/host authority.

The UTF-16 increment is implemented in **`checked-f64-utf16-v1` / host ABI v4 /
value ABI v5 / schema v4**. Guest strings store UTF-16LE byte slices, preserving
lone surrogates through literals, concatenation, calls, aggregates and tracing.
Length/charAt/charCodeAt and unsigned code-unit comparisons use the pinned String
semantics for admitted numeric arguments. Separate string/JSON imports prevent
legacy UTF-8 reinterpretation. JSON escapes lone units and strict parsing retains
them before guest allocation. Resource names reject lone units and NUL before OS
effects; well-formed UTF-8 host results convert exactly to owned UTF-16. Physical
and converted byte limits remain bounded. Record keys/schema metadata retain
UTF-8; surrogate keys and general string bracket access are not yet admitted.
Tests round-trip every u16 and cover effect order, GC pressure, real system I/O,
permissions/host ceilings, malformed encodings/versions, public engine APIs and
source-free native execution. No application project is introduced.

The primitive-union increment is implemented in **`checked-f64-utf16-v2` / host
ABI v5 / value ABI v6 / schema v5**. Closed primitive annotations use immutable,
canonical tagged cells with exact string-only child tracing. Initialization,
reassignment, function boundaries, record fields and explicitly typed array
elements preserve actual types and full Number bits. Direct-local typeof/null/
undefined guards narrow branches; writes under refinement are rejected to keep
loop assumptions sound. Mixed primitive ternaries, truthiness, strict equality,
nullish-only loose equality and short-circuit union operators execute in source
order. JSON handles dynamic undefined fields/entries/roots; typed parsing remains
explicitly non-union. Version/import/cell validation, GC pressure, malformed
payloads, module/public APIs, source-free execution and unchanged system authority
are tested. Older profiles retain their contracts.

The missing-value increment is implemented in **`checked-f64-utf16-v3` / host
ABI v6 / value ABI v7 / schema v6**. Closed optional primitive fields retain
separate presence and insertion-order metadata; explicit undefined stays an own
property. Object.hasOwn, statically absent record reads, nullable numeric reads of
primitive arrays/UTF-16 strings and undefined-capable let/return initialization
use real values and exact roots. Inherited optional names and reference-element
reads are rejected until their semantics are supported. Assignment appends newly
present declared fields only after RHS success. JSON validates required/absent
slots and full trailers, preserves enumeration and counts omitted slots against
budgets. Strict parsing remains non-optional/non-union and creates fully present
required records in the new layout. Tests cover 32-field masks, malformed metadata,
source order, declaration/return rules, collection pressure, permissions, public
module APIs and native source-free artifacts. Earlier profiles remain frozen.

The reference-union increment is implemented in **`checked-f64-utf16-v4` / host
ABI v7 / value ABI v8 / schema v7**. One record shape and one array element type
may join primitives in a checked union. Tag 9/typed v2 value imports validate
canonical reference cells and shallow managed layouts without reinterpreting
primitive tag 8 or old imports. Identity, aliases and exact tracing survive
boxing, optional fields and nullable object/array-element reads. typeof object
retains Null/Record/Array, while Array.isArray and null/undefined guards recover
known types in branches. Contextual literals/function returns remain supported.
JSON recursively serializes references, rejects cycles and keeps omission/budget
rules; strict parsing remains non-union/non-optional. Tests cover malformed cells,
source order, guards, GC pressure, independent/public/native execution and system
authority. Existing profiles retain their contracts.

The checked-JSON increment is implemented in **`checked-f64-utf16-v5` / host
ABI v8 / value ABI v8 / schema v8**. `json_parse_v9` supports optional fields and
closed primitive/reference unions while retaining `json_stringify_v8`. A union
selects its unique JSON-kind member without coercion; missing optional fields
retain clear presence bits and canonical undefined slots. Required fields still
must be present even when admitting undefined. Strict backward schemas, unique
member kinds and descriptor optional flags are checked before guest allocation.
Parsing preserves finite binary64 bits and lone UTF-16 units, exact cell/child
roots and alias identity. Duplicate/unknown keys, invalid members and resource
budgets remain enforced. Tests cover malformed metadata/helpers, no partial
allocation, collection, module APIs, source-free artifacts and host ceilings.
Earlier profiles remain frozen; no application logic or new capability is added.

The resizable-array increment is implemented in **`checked-f64-utf16-v6` / host
ABI v9 / value ABI v9 / schema v9**. Stable array identity owns replaceable,
geometrically grown dense storage; push evaluates all arguments before appending,
and pop returns the actual removed value or undefined while clearing its slot.
Aliases and parsed arrays share changes across records/functions/modules. GC
traces the identity's sole backing edge and every live reference element, including
arrays beyond 32 slots; returned children survive collection after removal.
Versioned v3 value operations/JSON imports validate cell/storage agreement and
canonical spare slots. The current limit remains 1024; indexed writes are bounded
replacements, with length assignment/holes/indexed extension still deferred.
Tests cover effects, union/reference values, malformed layouts/versions, resource
bounds, GC, public APIs, source-free packages and host ceilings. No application
logic or new capability is added; older profiles keep inline fixed arrays.

The same v6 profile now also implements `at` and `slice` as additive compiler
operations over ABI/layout v9. Number/Undefined bounds retain optional union
values and source-order effects; fractions truncate, NaN maps to zero and negative/
infinite/outside bounds follow the checked dense-array algorithms without integer
conversion traps. At returns the original element or Undefined. Every slice has
fresh storage/identity and shares nested object/array references. Selected slots
and actual reference masks are copied, spare slots are zero, and the new backing
is retained before identity allocation can collect. Tests cover edge cases,
mutating arguments, GC, the complete helper dependency set, pure status, public module
APIs, source-free execution and host ceilings. No import, capability or versioned
representation changed; earlier profiles retain their admitted behavior.

The same v6 profile now implements `includes`/`indexOf` over ABI/layout v9.
A shared allocation-free search helper compares actual kinds/payloads, UTF-16
contents and underlying reference identity. Includes uses SameValueZero; indexOf
uses strict equality. Needle/Number-or-Undefined starting bounds preserve source
order and exact roots through later allocations, including detached pop results.
Tests cover numeric edge cases, mixed values, optional bounds, GC, all ten helper
dependencies, public APIs, source-free artifacts and permission/status preservation.
No capability, import or representation changed; earlier profiles are unchanged.

Next specify and implement wider dynamic values and external adapters. Broader
shape unions, sparse collections, further array operations, prototype values and
general coercion still need explicit semantics and layouts. Tool/HTTP publication
in the newer profiles remains pending;
no legacy field/schema/handler is reinterpreted.
The remaining rollout must satisfy these rules:

1. Existing host ABI v1, structured-value v1/v2, Tool Contract v1 and HTTP v1 stay
   frozen. Old artifacts either execute through their existing paths or fail by
   version; never infer binary64/UTF-16 from the newest compiler version.
2. A further semantics-breaking profile requires a new host ABI (ABI v2 belongs
   to binary64/UTF-8 v1, ABI v3 to its undefined increment, ABI v4 to UTF-16, ABI v5 to primitive unions, ABI v6 to missing values, ABI v7 to reference unions, ABI v8 to checked optional/union JSON, ABI v9 to resizable arrays, ABI v10 to primitive exceptions, ABI v11 to Error objects, ABI v12 to native error families and ABI v13 to recoverable checked JSON; further breaking layouts need a new version)
   and explicit language metadata. A future runtime must validate the profile/ABI
   combination, reject missing metadata, and reject incompatible value layouts.
   Earlier runtimes must reject that version even if they ignore new custom-section names.
3. Keep tool v1's exact i32/boolean/UTF-8 JSON contract. A new-profile handler needs
   an explicitly checked adapter or a new tool/invocation contract; never round,
   truncate or coerce external arguments/results to fit the old slots. HTTP v1's
   transfer layout also needs an adapter or a new HTTP contract. Strict integer
   port/status/process parameters must be checked before any OS operation.
4. Host filesystem/URL/environment strings need an explicit conversion policy for
   lone surrogates and NUL. Reject unrepresentable/security-invalid inputs before
   effect; do not silently replace characters that change resource identity.
5. Add executable conformance cases for fractional/large numbers, signed zero,
   NaN/infinity, division/remainder, UTF-16/lone surrogates, undefined/null, equality,
   coercion, property order and JSON. Exercise compiler/runtime/public API/native
   execution, collection pressure and permission denials in both old/new profiles.

After that increment, expand closures/errors, scheduling, standard APIs and
module/package compatibility from the roadmap above. Application-state/MCP work
remains deferred behind core capabilities; no application project is required.

#### K4.3 — Wider language and asynchronous runtime (in progress)

The first explicit guest-exception increment is implemented in
**`checked-f64-utf16-v7` / host ABI v10 / value ABI v9 / schema v9**. Primitive
throw and try/catch work across named calls/modules, with typed guardable catch
bindings, rethrow, lexical scopes and ordinary return/break/continue. The reserved
root/global channel preserves thrown cells across frame unwinds; pending checks
stop unfinished expressions before later effects. Runtime initialization and
uncaught boundaries validate channel state and return owned typed values with
bounded diagnostics. Resource traps, checked JSON/index failures and authorized
exit remain fatal; system statuses retain their existing meaning. Tests cover
primitive bits/UTF-16, nested control flow, GC/repeated catches, malformed channels,
public APIs, source-free artifacts and host ceilings.

Finally/abrupt-completion handling is implemented additively in the same profile:
one finalizer per region, exact saved return/exception roots, nested completion
ordering and override semantics for return/throw/break/continue. Fatal traps and
authorized exit bypass finalization. Completion analysis checks overridden returns;
tests include collection, captured-result pairs and source-free execution.

The first generic Error value model is implemented in
**`checked-f64-utf16-v8` / host ABI v11 / value ABI v10 / schema v10**: nominal
identity, checked string/undefined constructors, immutable name/message reads,
toString, instanceof narrowing and exact ownership through calls/aggregates/catches/
finalizers. Message presence/non-enumerability follow the admitted standard rules.
Uncaught errors own full UTF-16 messages; malformed objects/channel versions and
old observers fail closed. This remains a deliberate subset: writable properties,
cause/options, error families, subclasses, prototype behavior and stack traces
need further implementation. JSON parsing cannot synthesize Error objects.

The next error increment is implemented in **`checked-f64-utf16-v9` / host ABI v12 /
value ABI v11 / schema v11**: TypeError, RangeError and SyntaxError constructors,
nominal family annotations/unions, the Error base type, family-specific instanceof
narrowing and correct names/toString/owned diagnostics. A versioned header class
and immutable exception selector keep earlier readers/profiles frozen. Identity,
roots, presence and status survive calls, aggregates, rethrow, finally and GC.
These are explicit guest exceptions; checked JSON/index failures remain fatal.

The first recoverable API error increment is implemented in
**`checked-f64-utf16-v10` / host ABI v13 / value/schema v11**. Checked JSON syntax
failures become SyntaxError and closed-schema mismatches become TypeError through
the versioned json_parse_v13 import. Whole-input grammar/resource validation and
typed limit tracking separate input mistakes from fatal budgets without matching
error strings. Error construction is owned/rooted, publication checks the empty
channel and untrusted helpers, and the compiler checks pending before observing
results or later effects. Tests cover Unicode, schema/type/numeric failures,
limits/corruption, GC/repeated recovery, host ceilings and source-free execution.
Older profiles keep fatal parsing; system status and authority remain unchanged.

String.repeat is implemented additively in the native-family profile (v9) and
successors: Number/Undefined count normalization, exact UTF-16 repetition and
recoverable RangeError for negative normalized counts/infinities. One bounded
result allocation, bulk-copy doubling, rooted operands and activation-local
scratch preserve effects/ownership without new imports, helpers or ABI versions.
String/memory/fuel/deadline limits remain fatal. Tests include GC, nested calls,
module/public APIs, status/host ceilings and source-free artifacts.

Recoverable JSON.stringify cycle failures depend on admitting cyclic structures
in the source value model. Current closed type DAGs cannot create them from TS;
raw malformed/cyclic Wasm fixtures continue to fail fatally. Specify recursive/
dynamic types and their validation before treating this as a source capability.

Next expand standard APIs and recoverable language/API errors without
weakening fatal budgets or capability checks. General JSON.parse, serializer
exceptions and remaining native families, writable
properties, causes/options, custom subclasses and stack traces need further work.
Typed function values and capture-free arrows are implemented in
**`checked-f64-utf16-v11` / host ABI v14 / unchanged value/schema v11**. Exact
function types support locals/reassignment, arguments, returns and calls; expected
types supply arrow parameter context. Named references have stable identity across
module aliases; each arrow evaluation creates a fresh managed object. Closed direct
Wasm dispatch checks semantic signatures, preserves evaluation order, roots and
pending exceptions, and keeps JSON helper host-effect analysis unchanged. Explicit
function/type/body bounds prevent unbounded dispatcher expansion. Tests cover GC,
malformed cells, hidden host effects, public APIs, grants and source-free execution.
Captures, function-valued aggregates/unions/JSON and wider function syntax remain
unsupported; no application logic or asynchronous machinery enters this increment.

Lexical closures over initialized bindings are implemented in
**`checked-f64-utf16-v12` / host ABI v15 / unchanged value/schema v11**. Captured
parameters/locals/catch bindings use shared managed cells; nested closures forward
cells and escaping closures retain their complete graph. Mutations remain visible
across sibling closures while separate activations stay independent. For-loop let
bindings are copied before update, including continue/finally paths; body bindings
are allocated anew on entry. Exact edge masks preserve non-reference bits, host
result pairs and GC of unreachable cycles. Entry guards validate function,
environment and binding metadata before user effects. JSON helper call-graph
purity, capability checks and fatal resource limits remain unchanged.

Captures before binding initialization, mutable captured union refinement and
captures under an active refinement are rejected. Later TDZ/ReferenceError and
alias-aware refinement work must specify those semantics before admission.
Function expressions and private self bindings are now implemented below.
Outer-binding recursive initialization, function-valued aggregates/JSON and
broader function syntax remain pending. Tests exercise nested/escaped mutable
closures, per-iteration bindings, roots/cycles, exceptions/finally, malformed memory,
limits, helper effects and source-free/public APIs.

The first Array callback increment is implemented additively in v12: map/filter/
forEach/find use ordinary typed function dispatch and capturing closures. Receiver
and callback evaluate once; traversal snapshots length and reads the current
backing at each turn. Filter/forEach skip removed slots; find admits Undefined in
its callback value and visits deleted indices. Predicates preserve pre-call values
and reference identity. Map supports checked value results/void-to-undefined;
filter/find apply checked truthiness; forEach discards results. Typed wrappers share
existing frame roots, exception routing, effect validation and resource bounds.
Tests cover mutations/regrowth, empty/1024-element arrays, errors/limits, malformed
callbacks, collection, permissions and source-free/public APIs. No ABI/import or
permission changes are required.

The sparse-array increment is implemented in **checked-f64-utf16-v13 / host ABI
v16 / value ABI v12 / schema v12**. A bounded presence bitmap follows typed backing
slots; holes and spare data are canonical zero, and bitmap bytes are never traced
as child references. Sparse literals, hasOwn, reads/at, filling existing slots,
slice, pop/push, includes/indexOf and callback traversal preserve presence. Map
allocates its final-length result once and fills only visited indices, retaining
holes after callback shrinkage. Filter/forEach skip holes; find visits Undefined.
Checked JSON renders holes as null and parses fully present arrays. Versioned v6
value imports, json_stringify_v12/json_parse_v14 and value_is_error_class_v2 bind
the new representation; old profiles retain their contracts and v12 map trap.
Tests cover bitmap boundaries/growth, 1024-length maps, GC, malformed data, version
substitution, helper purity, public/source-free APIs and capability ceilings.

Bounded delete, indexed extension and writable length are implemented in
**checked-f64-utf16-v14 / host ABI v17 / unchanged value/schema v12**. Indexed
writes retain the original receiver/key and evaluate RHS before numeric write
checks/growth; extension creates holes. Length shrink clears references and bits,
while growth allocates/copies before publishing or reuses zero spare storage.
Invalid numeric uint32 lengths raise owned RangeError, valid lengths above the
1024 resource ceiling remain fatal, and deleting nonconfigurable length raises
TypeError. Array-only numeric/literal delete preserves length and returns true
for absence. The thirteenth bounded host-free helper preserves canonical sparse
layout, exact roots, helper purity, status and authority. Tests include independent
mutation models, operand order/errors, callbacks, GC, boundary lengths/indices,
profile compatibility and source-free/public APIs.

Reduce/some/every are implemented additively in v12 and successors with unchanged
ABI/imports/helpers. Typed wrappers snapshot length after receiver/callback/seed
effects, skip live missing entries and preserve prior element/accumulator roots.
Some/every short-circuit through checked truthiness and return false/true for
empty/all-hole arrays. Reduce supports exact typed accumulators, an optional seed
(including explicit Undefined), first-present initialization without a seed, and
an owned TypeError for an empty/all-hole array without a seed. Inline reducers
contextually type parameters and returns; checked unions, aggregate/function and
physical-result accumulators retain their ownership. Tests cover sparse mutation,
operand order, short circuits, errors/fatal limits, malformed callbacks, GC,
1024-element traversals at default fuel, helper purity, permissions/host ceilings
and public/source-free execution.

Contextual callback return and reduce seed inference are implemented additively
in v12 and successors. Unannotated inline Array callback blocks infer actual typed
return sites and supported closed unions, including bare/fallthrough Undefined;
ordinary arrows retain prior defaults. Bodies lower once and nested arrow returns
remain separate. Finally saved returns receive exact inferred slot/root types
before emission, preserving aggregate/union/multi-slot ownership through GC.
Reduce context comes from explicit callback annotations/function signatures, then
surrounding aggregate result positions, enabling empty/nested arrays and optional
record seeds. Wider outer scalar results never widen a numeric seed. Exact
signatures, ambiguous-type rejection, operand order, authority and all budgets
remain intact. Tests cover these rules, errors/diagnostics, GC/finalizers, helper
purity, modules, host ceilings and source-free/public engine use.

Anonymous function expressions, typed this/thisArg and checked function.call are
implemented additively in v12 and successors. Function expressions share capture,
return-inference and direct-dispatch mechanisms. Explicit this is separate from
positional parameters, exact in function types and rooted through normal physical
parameters/cells; receiver mode discriminates even identical Wasm signatures.
Six Array callback methods accept optional thisArg with once-only source order
before snapshot length; reduce keeps its seed/Undefined receiver semantics.
Arrows preserve lexical this through escaped/transitive captures. Calls preserve
strict primitive/null/undefined values and ignored receiver effects. Tests cover
GC/finalizers, sparse mutation, errors/fatal limits, malformed targets, slot/fuel
bounds, authority/helper purity and source-free/public execution. No ABI/import/
layout/helper change is needed.

Named-expression self bindings and declaration receivers are implemented
additively in v12+. Named expressions require an annotated/contextual result and
own a private immutable name, behind the parameter/body scope. A hidden rooted
self parameter carries the actual callee identity into each activation; entry
checks validate signature/code/environment before effects. Escaped closures retain
it through existing cells, while alias reassignment and shadowing stay separate.
Declaration this is included in exact types/physical parameters and canonical
imported references; direct bare calls supply checked Undefined, call/Array paths
supply explicit receivers. Entry main and legacy tool/HTTP contracts stay
receiver-free. Tests cover recursion/scopes, GC, pairs/slot limits, errors/fatal
recursion, forged self inputs, helper purity/permissions and source-free/public
APIs. No heap/import/helper/ABI change is introduced.

Optional/default/rest parameters are implemented additively in v12+. One call
normalizer checks arity/types, supplies missing Undefined and builds fresh bounded
rest arrays. Exact function modes participate in dispatch even for equal physical
signatures. Extended callees separate rooted incoming ABI slots from sequential
source-binding initialization: defaults run after supplied argument effects and
only for Undefined, see prior initialized parameters/this/private self, and cannot
read later parameters or body declarations. Incoming roots clear after transfer;
normal capture cells/GC/finalizer/result-pair ownership remain intact. Array
callbacks receive their real implicit arguments, optional tails and checked rest;
reduce preserves the exact accumulator type. Tests cover these semantics, scopes,
GC, errors/fatal limits, mode substitution, helper purity, permissions and source-
free/module/public execution. No layout/helper/import/ABI change is required.

Bounded spread-call arguments are implemented additively in v12+. Narrowed array
segments and explicit values work with declarations, closures and explicit
Function.call receivers. Shallow snapshots preserve source evaluation order and
child identity; sparse holes become Undefined. A rooted wrapper validates the
1024 flattened-argument cap (fatal), dynamic arity/types (owned TypeError), pads
optional slots and builds a fresh dense rest array before defaults/body code.
Explicit Function/physical-result pairs retain their widths outside aggregates.
Pure guest Undefined checks avoid per-element host-call budget use. Existing
function/physical-slot/body/resource limits, helper purity, actual grants/status
and profile/ABI boundaries remain intact. Tests cover v12/v13/v14 default-fuel
limits, ordering, holes/defaults, exceptions, GC/pairs, forged dispatch, diagnostics,
virtual modules and source-free artifacts with host ceilings/authorized exit.
No layout/helper/import/ABI change is required.

Parameter destructuring is implemented additively in v12+: closed record/array
patterns, nesting/renaming/static literal keys, top/nested defaults, elisions,
object/array rest and destructured array rest parameters. Incoming aggregate
signatures stay exact; all BoundNames start in TDZ and leaves initialize in source
order with ordinary shared capture cells. Nullable nested aggregates throw owned
TypeError before later defaults/body code. Array reads observe live mutation and
retain iterator exhaustion; rest is fresh/dense and maps sparse holes to Undefined.
Object rest preserves source presence/enumeration order and shallow child identity.
Pure guest tests/metadata copies retain status, exact roots, GC, physical pairs,
helper purity and actual permissions. Shared pattern budgets (64 depth/1024 nodes)
join existing resource limits. Tests cover full-width presence, default-fuel rest/
callbacks, errors/fatal limits, scopes/closures/this/self, virtual modules and
source-free native/Wasm execution. No ABI/import/helper/layout change is required.

Local const/let destructuring is implemented additively in v12+ through the shared
binding mechanism. Blocks and ordinary for initializers predeclare all leaves in
TDZ, infer or contextually check one RHS evaluation, then initialize in source
order. Const/let keep mutability and managed closure cells; nested scopes restore
shadowed bindings. For let now separates the initializer from the first iteration,
as well as rebinding before later updates; const retains its loop environment.
Pattern semantics, exact roots, GC/physical pairs, recoverable/fatal paths, status,
helper purity and actual grants remain intact. Shared 64/1024 pattern budgets also
bound each lexical declaration. Tests cover scopes/mutability/captures, iteration/
finally, bounds/default-fuel copies, dependency diagnostics, virtual modules and
source-free artifacts with real grants/host ceilings. No ABI/helper/layout change.

Checked destructuring assignment statements are implemented in v12+. Existing
initialized mutable local/parameter/captured targets are validated without new
bindings; RHS evaluates once and nested source-order writes retain existing cells.
Defaults see earlier writes/prior values, duplicate targets are allowed and runtime
errors preserve successful writes before skipping later work. Exact/union types
and checked Undefined narrowing protect invariant target types; defaults use target
context. Shared traversal/root/rest/exception rules, bounds, actual grants/status
and helper purity remain intact. Tests cover closures, partial failures, GC/pairs,
1024-value default budgets, diagnostics, virtual modules and source-free artifacts.
No ABI/import/helper/layout change is introduced.

Expression-valued destructuring assignment is implemented additively in v12+.
Save one rooted RHS and return the original value/type only after traversal;
identity, unselected fields and sparse holes remain intact. Results work in nested
chains, arguments, returns, callbacks and supported expression positions. Scoped
root floors protect earlier operands across embedded traversal statements, while
normal handlers/finalizers retain cleanup and partial-write semantics. Collection/
effect visitors see both RHS and body. Tests cover result context, earlier/later
operand GC, physical pairs, fatal limits/default budgets, helper purity, virtual
modules and source-free artifacts with real grants/status. No ABI/helper change.

Checked record-field/array-element destructuring targets are implemented in v12+,
including nested/default/rest targets. Prepared receivers/indices evaluate once
before corresponding source reads/defaults or rest copying; hidden roots preserve
identity through allocation, reassignment and array resizing. Existing setters
retain types/presence/order and profile-specific bounds; length targets require
v14. Tests cover order/throws, GC/pairs, current backing, fatal limits, legacy gates,
helper purity, virtual modules and source-free artifacts with grants/ceilings and
dependency diagnostics. No ABI/helper/layout change.

Intrinsic array for-of is implemented in v12+. One rooted source is traversed
through live length/backing; v13+ holes yield Undefined. Lexical const/let heads
receive fresh capture cells per iteration, while assignment identifiers/members/
patterns retain checked writes. Iterator values precede reference/default effects;
TDZ, patterns, GC/pairs, branch/finalizer cleanup and status/grants retain their
contracts. Nullable array absence is recoverable TypeError; bounds/fuel/exit stay
fatal. Tests cover 1024-element default budgets, virtual modules and source-free
artifacts with grant denial, host ceilings and dependency diagnostics. No ABI,
helper, import or layout change. General iterables and async iteration remain open.

Checked do-while is implemented in v12+. Dedicated typed IR runs the body first,
routes continue through finalizers to the trailing condition and skips that test
for break/return/throw. Body scopes/captures, condition-root cleanup and actual
effect visitors retain ordinary semantics. Completion/callback inference recognizes
guaranteed first-body returns/throws without inventing normal exits; possible
break/continue paths still permit fallthrough. Tests cover ordering, overrides,
truthiness, GC/pairs, bounded condition roots, profile gates, virtual modules and
source-free artifacts with grants/ceilings and dependency diagnostics. No ABI,
helper, import or layout change; existing resource/exit failures remain fatal.

Checked statement labels and named break/continue are implemented in v12+. Unique
function-local IDs resolve active targets; iteration aliases bind to actual loop
continues before generated initializer/rest loops. Non-loop statements allow
named break. Existing finalizer routing and exact scope cleanup preserve captures,
GC, saved returns/pairs and fatal limits. Completion analysis propagates named
paths through inner loops until their own label, keeping callback/function return
inference sound. Tests cover loops/blocks/switches/aliases, invalid targets/depth,
legacy gates, virtual modules and source-free artifacts with grants/ceilings and
dependency diagnostics. No ABI/helper/import/layout change.

Object.keys is implemented in v12+ for closed records and nullable single-record
unions. One rooted argument yields a fresh dense String array of present own keys:
index names sort numerically, other names follow insertion order, and explicit
Undefined differs from absence. Pure metadata enumeration avoids payload reads;
compaction zeros tail payloads/bits before publishing both lengths. Internal value
blocks preserve caller roots/floors and exception/return lifetimes, with full
layout/effect traversal. Tests cover 32-field/default limits, Unicode/order/JSON/
rest, GC/pairs/callbacks, profile gates, virtual modules and source-free artifacts
with grants/ceilings and dependency diagnostics. No ABI/import/helper/layout change.

Object.values is implemented in v12+ for closed records and nullable single-record
unions. Shared key/value enumeration derives exact or admitted union array elements
from field types, preserving presence/JS order, numeric bits and shallow identity.
Typed cached payloads are rooted before output allocation; compaction reads only
literals/locals and clears spare values/bits before publishing lengths. Empty
records yield Undefined[]; unrepresentable unions fail without casts. Tests cover
32-field default limits, JSON/rest/callbacks, GC/pairs/cached child graphs, profiles,
virtual modules and source-free artifacts with grants/ceilings and diagnostics.
No ABI/import/helper-export/layout change; keys retain their existing behavior.

Object.keys/Object.values now also accept bounded arrays and nullable single-array
unions in v12+. Private guest helpers produce ascending decimal keys and exact
invariant values while skipping holes and retaining explicit Undefined. Fresh
zeroed backing and rooted incremental prefixes preserve GC/child graphs, numeric
bits, shallow identity and later growth. Helper roots are bounded/restored; no
host/import/helper-export/layout/ABI change. Tests cover all element kinds, 1024
indices, repeated GC/roots, virtual modules and source-free artifacts with grants,
host ceilings, status and fatal exit.

Checked catch object patterns are implemented in v12+ for static name/message
identifier bindings, aliases/defaults and empty patterns. The full primitive/Error
channel remains unchanged: non-null primitives supply Undefined, nullish inputs
throw TypeError before binding effects, and Errors retain existing fields. All
names are predeclared under TDZ, initialized left to right and own fresh capture
cells; hidden original-value roots and partial leaves clear on every exit. Binding
failures reach outer handlers/finalizers. Tests cover defaults/effects, GC/pairs,
returns/branches/callback inference, modules, source-free artifacts and diagnostics.
Array/rest/other-key patterns and aggregate throws remain explicit future work;
no ABI/import/helper-export/layout change.

String.slice/substring are implemented in v12+ for narrowed strings with optional
Number/Undefined bounds. They preserve code-unit UTF-16/NULs/lone surrogates,
source-order effects and owned result storage; floating-point normalization/clamp
precedes bounded integer conversion. Pure private helpers retain ordinary GC,
physical pairs/saved returns/finalizers/status/grants and fatal byte/memory/fuel
limits. Shared slice dispatch lowers receivers once and keeps array/legacy rules.
Tests cover numeric edges, nested/throwing bounds, repeated GC, maximum byte
limits with test fuel, virtual modules and source-free artifacts with host ceilings
and diagnostics. No ABI/import/helper-export/layout change.

String.includes/startsWith/endsWith are implemented in v12+ with required String
needles and optional Number/Undefined positions. Exact UTF-16 unit comparisons
preserve lone surrogates/NULs/case; normalized clamping precedes integer conversion.
Private guest helpers allocate/call nothing and return canonical booleans. Source
order, exception/GC/pair/finalizer lifetimes and shared Array.includes dispatch
retain existing contracts. Candidate scanning is bounded by existing fuel/deadline
limits, including adversarial-input regression tests. Maximum byte, module and
source-free artifact tests preserve grants/host ceilings/status/diagnostics.
No ABI/import/helper-export/layout change.

String.indexOf/lastIndexOf and array reverse search/mutation increments are
implemented as recorded in the checkpoint follow-up above.

V12+ dynamic String own keys for closed records are implemented. A rooted value
block evaluates source/key once, then a UTF-16-ordered balanced decision tree reads
existing presence bits without payload/prototype access. Literal String keys keep
a direct path, including false for unrepresentable names/__proto__. Existing numeric
literal and older-profile rules remain unchanged. Tests cover exact
Unicode ordering, all 32 bits/default limits, argument mutation/throws, GC/pairs/
finalizers/callbacks, effects/grants/status, modules and source-free artifacts.
No ABI/import/helper-export/layout change.

Dynamic String own keys now cover arrays in v13+ and Error families/only-Error
unions in v12+. Allocation-free bounded array parsing reuses current-length/bitmap
checks; length remains own, holes differ from explicit Undefined. Error lookup
masks only own-message after exact key matching. Tests cover bitmap edges/max index,
canonical/unknown/max-byte names, argument mutation/order/throws/fatal limits,
GC/pairs/finalizers, older profiles, modules, source-free artifacts and grants/status.
No ABI/import/helper-export/layout change.

Typed computed-key reads on closed records are implemented in v12+ as specified in
the [computed read contract](architecture.md#computed-record-read-contract). Tests
cover admitted result unions/optional presence, exact UTF-16 ordering, all slots,
numeric bits/reference identity, source/key mutation and throws, GC/pairs/finalizers,
helper purity, bounds/profiles, virtual modules and source-free artifacts.
No ABI/import/helper-export/layout change.

Typed computed-key assignment statements are implemented in v12+ as specified in
the [computed write contract](architecture.md#computed-record-write-contract).
Prepared source/key/RHS roots, full field/member checks and pure Error-family guards
preserve exact payloads, optional presence/order and existing limits. Regressions
cover modules/source-free artifacts, diagnostics, effects/grants/status, failure
order, invariant types, all slots, GC/saved pairs/finalizers and fatal budgets.
No ABI/import/helper-export/layout change.

Computed record assignment values/chains are implemented in v12+ under the
[assignment value contract](architecture.md#computed-record-assignment-value-contract).
Both static computed literals and dynamic String keys preserve field rules and
original RHS types/identity, with value-block floors/roots across nested effects,
GC and finalizers. Tests cover all slots, callbacks/destructuring composition,
partial chains, source-free artifacts, diagnostics and permissions/fatal exits.
No ABI/import/helper-export/layout change.

Ordinary simple assignment values are implemented in v12+ under the
[simple value contract](architecture.md#simple-assignment-value-contract). Shared
prepared references/setters retain original RHS types/results, local functions and
physical pairs, current array storage/sparse rules and v14 checked length values.
Regressions cover effects/partial chains, invariance, fatal indices/length bounds,
capture/GC/finalizer cleanup, modules/artifacts, source ranges and grants/status.
No ABI/import/helper-export/layout change.

Checked arithmetic compound values/statements are implemented under the
[compound contract](architecture.md#arithmetic-compound-assignment-contract).
Regressions cover old-value ordering, matching Number/String union cases, absence/
RHS exceptions, current sparse storage/length bounds, original result bits, GC/
saved pairs/returns, artifacts/modules, source diagnostics and permission/status
boundaries. No ABI/import/helper-export/layout change.

Dynamic record arithmetic compounds are implemented under the
[dynamic compound contract](architecture.md#dynamic-record-compound-assignment-contract).
Regressions cover retained receiver/key/old-value identity, inherited versus missing
read timing, strict union cases, complete field types, exact UTF-16 names/all field
bits, partial chains, callbacks/GC/pairs/returns/finalizers, source diagnostics,
virtual modules and source-free artifacts with operation/host-policy boundaries.
Computed reads retain their existing union restrictions and all compilation/value/
fatal budgets; no ABI/import/helper-export/layout change.

Logical assignments are implemented under the
[logical contract](architecture.md#logical-assignment-contract). Regressions cover
truth/nullish edges, branch-only RHS/store effects, precise skipped-member result
types, optional presence/order, prepared identity, dynamic field failures, sparse/
current storage/length rules, function context/identity, GC/pairs/returns/finalizers,
fatal skips and exhaustion, module/artifact execution and permission boundaries.
No ABI/import/helper-export/layout change; existing limits remain active.

Number updates are implemented under the
[update contract](architecture.md#number-update-contract). Regressions cover
prefix/postfix Number results, -0/NaN/infinity/rounding, source/key order, optional/
sparse/missing/inherited failures, length checks, partial chains, captured/fresh
loop cells, GC/pairs/returns/finalizers, helper purity, precise source ranges,
module/artifact execution, status/grants/host ceilings and fatal exhaustion/exit.
No ABI/import/helper-export/layout or resource-limit change.

Number bitwise/shift operations are implemented under the
[bitwise contract](architecture.md#number-bitwise-contract). Regressions cover
all binary64 exponent ranges and seeded payloads against an independent numeric
oracle, all operators/shift masks, source order, narrowed unions/callbacks, roots/
pairs/GC/finalizers, helper purity, source ranges, module/source-free execution and
operation/host-policy/fatal boundaries. No ABI/import/helper-export/layout change.

Bitwise compound assignments are implemented under the
[bitwise compound contract](architecture.md#bitwise-compound-assignment-contract).
Regressions cover all six operators, Number guards/results, original receiver/key/
old snapshots, RHS effects/exceptions, exact UTF-16 names/all field bits, partial
chains, sparse/current storage/length rules, GC/pairs/saved returns/finalizers,
helper purity, dependency ranges, modules/source-free artifacts and permission/
status/fatal boundaries. No new IR/ABI/import/helper-export/layout or limit change.

Number exponentiation is implemented in **checked-f64-utf16-v15 / host ABI18 /
value and schema12** under the [power contract](architecture.md#number-power-profile-and-host-abi-v18).
The pure typed import preserves system status and uses one host-operation budget
unit with deadline checks before/after native math. Explicit Number edge cases
precede platform-approximated finite pow; BigInt/coercion remain deferred.
Tests cover exact special-case bits/parity, independent sqrt accuracy, budgets,
prepared compounds/GC/pairs/finalizers, inherited sparse/JSON/closure contracts,
old-profile and hostile metadata/signature rejection, virtual modules, source-free
artifacts, source ranges and operation/host-policy/fatal boundaries. Native
negotiation advertises v15/18; value/schema/container formats and limits persist.

Direct Math.abs/floor/ceil/trunc/round/sqrt are implemented in v15 under the
[checked Math contract](architecture.md#checked-math-intrinsic-subset). One narrowed
Number input, exact zero/nonfinite/rounding rules and guest-only execution preserve
all argument effects/roots/pairs/finalizers, host status and budgets. All-exponent
numeric checks and source-free artifacts cover this subset; shadowed/computed/
borrowed/coercive calls and older profiles are rejected. No ABI/import/permission/
helper-export/layout/limit change.

Math.min/max, sign, fround, clz32 and imul are also implemented in v15, including
empty/variadic extrema, exact Uint32 conversion, signed low-bit products and
binary32 ties-to-even rounding. Tests cover all-exponent conversions, binary32
halfway/overflow/subnormal boundaries and all-argument NaN/exception order.

Number.isFinite/isNaN/isInteger/isSafeInteger are implemented in v15 under the
[predicate contract](architecture.md#number-predicate-intrinsics): all admitted
types/unnarrowed unions, non-coercing false for non-Numbers/missing input, and
effectful ignored arguments. Guest-only Boolean results preserve ABI/budgets,
roots/pairs/finalizers and artifact/capability boundaries.

All sixteen Math/Number numeric constants are implemented in v15 under the
[constant contract](architecture.md#numeric-constant-properties), with exact bits,
dot/literal-key reads, lexical precedence and complete read-only write-path checks.
Namespace objects/aliases and descriptors remain future work; existing Number
logging text is retained while JSON uses the pinned ECMAScript formatter.

Math.pow is implemented in the same profile through the existing FloatPower
operation, preserving exact special-case behavior, finite approximation, source
order, host budgets/status and helper-purity rejection.

Math.f16round is implemented under the [direct binary16 contract](architecture.md#direct-binary16-rounding).
Exact power-of-two rescaling avoids f32 double rounding, preserves special values
and covers all half encodings/midpoint neighborhoods with an independent bit oracle.

Math.hypot is implemented under the [scaled norm contract](architecture.md#scaled-variadic-mathhypot).
Power-of-two normalization and compensated products/sums/root refinement avoid
premature magnitude loss without new host calls or raised limits. Sampled native
comparisons and independent decimal overflow goldens cover numerical behavior;
all-argument effects, roots and source-free/public APIs retain their contracts.

The twenty deterministic native Math operations are implemented under the
[native Math contract](architecture.md#native-math-profile-and-host-abi-v19), with
explicit edge cases, platform approximation, typed ABI19 and inherited value/schema12.
Registry/metadata, budgets/status, helper purity, all exponents/domain invariants,
argument lifetimes, public modules and source-free packaging have regression coverage.

Math.random is implemented under the [random contract](architecture.md#random-profile-and-host-abi-v20),
with fresh OS entropy, exact 53-bit sampling, fatal failure/limit behavior and
ignored-argument/root/status guarantees. No seed or shared PRNG is persisted.
Profile/registry/signature checks, injected fault/edge tests, independent stores
and source-free portable/native/public engine paths cover this boundary.

ECMAScript decimal Number output is implemented under the
[Number output contract](architecture.md#number-output-profile-and-host-abi-v21),
with versioned scalar/union logs, shortest canonical decimal/scientific formatting,
old-profile compatibility and unchanged value/schema12. Edge/exponent/minimality,
hostile-import/helper checks, budgets/status/GC and source-free/public APIs are covered.

Primitive String conversion is implemented under the
[primitive String contract](architecture.md#primitive-string-profile-and-host-abi-v22),
with cached first values, ignored argument effects, exact UTF-16 and owned results
through existing value12 cells/result buffers. Profile/registry/hostile-cell/helper
checks, maximum-string/GC/status tests and source-free/public paths cover it.

Primitive Number conversion and numeric String parsing are implemented under the
[primitive Number contract](architecture.md#primitive-number-profile-and-host-abi-v23).
Checked cells, exact grammar/whitespace/rounding and whole-string NaN rules preserve
effects, signed zero, roots/pairs, status/buffers and fatal limits. Independent
integer goldens, exponent-wide checks, maximum strings and source-free/public API
paths cover the foundation needed for radix argument conversion.

Checked Number.toString is implemented under the
[radix contract](architecture.md#number-radix-profile-and-host-abi-v24), with all
bases2–36, cached/coerced primitive radix arguments, correct RangeError order and
owned UTF-16. Exact bounded intervals choose shortest text; independent rational
round-trip/minimality tests, runtime budgets/roots/status and source-free/public
paths cover this method. Wrappers and ordinary prototype objects are still pending.

Number precision and valueOf are implemented under the
[precision contract](architecture.md#number-precision-profile-and-host-abi-v25),
with bounded exact rounding, conversion/default/range ordering, owned results and
guest-only identity. Independent Decimal goldens, exponent-wide bounds, raw ABI/
helper checks, lifetimes/status/budgets and source-free/public paths cover them.

Numeric prefix parsers are implemented under the
[parsing contract](architecture.md#numeric-prefix-profile-and-host-abi-v26),
with primitive inputs/radices, prefix grammar, exact integer rounding, Int32 wrapping
and ordinary NaN failures. Independent integer goldens, exponent/Unicode/bounds,
malformed cells, helpers and source-free/public paths cover the boundary.

Boolean and global numeric predicates are implemented under the
[conversion contract](architecture.md#boolean-and-global-numeric-predicates-v23).
Ordinary-value ToBoolean is guest-only, numeric coercion reuses existing imports;
root/status/budget, exponent, helper and source-free/public checks cover it.

Guest truth/control flow and logical operators are implemented under the
[control-flow contract](architecture.md#guest-truth-conversion-and-logical-operators-v23).
Exact tests, operand-valued short circuiting, selected-branch type narrowing,
roots/status/budgets and source-free/public paths have regression coverage.

Truth-based local/RHS guards are implemented under the
[v24 contract](architecture.md#truth-guard-profile-v24-with-shared-host-abi-v26),
sharing ABI26 and existing values while versioning stricter source refinement rules.
Tests cover scoped branches/loops, false primitive classes, mutation/capture safety,
metadata, numeric bits/roots/status and source-free/public API execution.

Write-aware invalidation/captures are implemented under the
[write-flow contract](architecture.md#write-aware-refinements-and-captures-v24).
Per-binding revisions, bounded AST loop/test effects and typed captured cells retain
root/status/limit/legacy boundaries; conservative inference remains explicit.

Post-store inference and normal/targeted-exit joins are implemented under the
[completion-flow contract](architecture.md#completion-targeted-local-flow-v24).
Optional effects, repeated-body invalidation and exceptional entry remain conservative.
Known call effects are implemented under the
[call-flow contract](architecture.md#proven-call-effects-for-local-cells-v24).
Stable callback inputs are implemented under the
[parameter-flow contract](architecture.md#stable-callback-parameter-effects-v24).
Next implement higher-order callback-argument binding, property/exception effects, initializer inference,
loop fixed points and the
remaining inventory. Generalized
coercion, aggregate exception design and constructors/prototypes
remain language work. Scheduler/promise jobs follow retained-root, cancellation,
queue and handle lifetime design.


- Grow functions/closures, dynamic object/array behavior, exceptions and the
  corresponding memory model under the explicit compatibility profile.
- Implement event-loop and promise-job semantics, timers, async/await, cancellation
  and handle lifetime with bounded queues and correct retained roots.
- Expand standard JSON/Web/binary APIs on that foundation. The initial synchronous
  HTTP handler profile is a delivery increment, not the final language/API ceiling.

### C1 — Tool Contract v1 and offline discovery (completed)

Entry-module `varta.tool({ name, description }, handler)` declarations now
publish explicitly exported functions. Checked primitive signatures supply named
parameters and results; metadata does not duplicate handwritten schemas.
Ordinary exports and dependency modules cannot silently publish tools.

- Shared strict types, limits and generated JSON schemas live in `varta-common`.
- Compiler lowering resolves the actual handler (including imported/re-exported
  bindings), derives transitive system requirements and emits a sorted catalog
  plus typed Wasm exports.
- Runtime validation binds metadata to actual Wasm export signatures and recomputes
  transitive effects. Malformed, duplicate, oversized, incompatible and inconsistent
  catalogs fail before execution. Indirect calls and Wasm start functions are
  outside tool invocation ABI v1.
- Project inspection returns public schemas, effects, limits and existing package
  permissions. It compiles/validates portable code without instantiating or running
  it. The CLI only presents that metadata.
- Contract v1, invocation ABI v1, host ABI v1, manifest schema v1 and native
  container format v1 are distinct. Existing no-tool applications keep working;
  tool metadata is inside the Wasm payload, so packaging requires no new footer.
- String copy/retain exports establish fresh-instance invocation ownership and are
  tested across input-buffer reuse and collection. C2.1 below now supplies the JSON
  dispatcher. `domainErrors: []` leaves typed application errors for later C2 work.

Exit verified: compile a primitive string tool, inspect the same catalog in Wasm
and native packages after deleting sources, reject invalid catalogs/signatures/
effects, preserve permission denials and demonstrate that inspection does not run
privileged application code. No LLM connection or MCP server is implemented.

The complete implemented syntax, limits and ownership rules are in the
[tool contract](architecture.md#tool-contract-v1-and-offline-discovery).

### C2 — Validated invocation and generic application state (partly complete, deferred)

The typed invocation path is implemented. Generic state ownership, transactional
updates and shared application lifetime remain deferred until the active language,
JSON, server-lifecycle and packaging work is established.

#### C2.1 — Checked named JSON invocation (completed)

- Shared runtime/project APIs and `varta invoke <artifact> <name> --params '<JSON>'`
  dispatch only published tools, without calling `main` or executing native launchers.
- A bounded closed-object parser rejects duplicate/unknown/missing parameters,
  wrong types, invalid UTF-8 and non-i32 input before instantiation. Integers use
  exact plain integer tokens; decimal/exponent and negative-zero forms are rejected.
- All argument helpers and the selected handler share a fresh store, fuel,
  memory, deadline and host-call budgets. String copy/retain results are checked;
  strings returned to the host are bounds/UTF-8 checked and copied immediately.
- Canonical booleans and the encoded result envelope are validated. JSON output
  counting avoids oversized temporary serialization buffers. Logs remain separate.
- Stable invocation codes distinguish request, catalog/code, policy, result,
  execution and guest-exit failures. Recoverable host statuses and package/host
  resource authorization keep their previous semantics.
- Tests cover real Wasm/native invocation without sources, no startup/private
  dispatch, strict JSON, primitive boundaries, buffer reuse/collection, hostile
  result values, escaped-output limits, traps, recovery and permission denials.

This completes the dispatcher slice, not the live application milestone. The
[implemented invocation contract](architecture.md#checked-json-invocation) describes
its bounds. It has no persistent state, caller identities, remote listener,
rollback, automatic retry or application-wide queue/accounting yet.

#### Remaining C2 work (after K1–K3)

- Design the generic application-state contract and ownership/transaction ABI
  before implementation: application-defined state types, bounded host storage,
  guest access, validated changes and atomic commit. The engine must not know
  document fields, command names or domain rules. Revision preconditions belong
  to generic request metadata rather than reserved application parameter names.
- Add typed application/domain errors on top of the stable invocation codes.
  Keep them separate from host statuses, protocol errors and traps; revision
  conflicts must not become host I/O errors.
- Define application identity, startup/shutdown, state ownership, request identity,
  ordering, revision checks, cancellation and retry/idempotency semantics.
- Validate a proposed initial model: a long-lived trusted application service owns
  state and serializes mutations; fresh bounded Wasm invocations implement commands.
  The application's lifetime is independent of any agent or MCP connection.
  Decide this model explicitly before adding persistent guest memory or concurrency.
- Preserve the checked string-transfer boundary when introducing persistent
  host-owned state. State must never retain guest pointers after a call ends.
- Keep one command/query path for UI/API/MCP callers. Apply authorization and state
  preconditions at that path, then commit a mutation atomically. A stale revision
  yields a typed conflict; transport failure must not trigger an automatic write retry.
- Enforce per-call budgets and bounded application-level queues/accounting so that
  reconnecting or making repeated calls cannot reset all resource controls.

Exit: generic core tests demonstrate shared state across client connections,
revision conflicts, authorization and atomic rejection of malformed/trapping
commands. A separately built TS/JS document application then demonstrates reading
and renaming through those public APIs with two clients. Its title schema,
commands and validation live in the application. State survives client disconnects;
disk persistence and a GUI are separate requirements, not implied by this milestone.

### C3 — Artifact provenance and trusted loading

Deliver package verification before third-party distribution or treating tool
metadata as publisher-authenticated.

- Define trust roots, signed bytes, contract/code/permission binding and signature
  verification before invocation. Cover versions and payload lengths as well as
  code; changing the catalog or manifest must invalidate authenticity.
- Define publisher identity, key rotation/revocation, update and rollback rules.
  Keep signing credentials outside shipped artifacts and guest memory.
- Separate package authenticity from caller authorization and native OS signing.
  A valid signature cannot expand host grants, and an unsigned developer build
  must not silently acquire trusted-package status.
- Use explicitly trusted local development builds for earlier prototypes. Preserve
  safe portable Wasm validation and do not restore arbitrary native deserialization.

Exit: code, contract and permission tampering is rejected by a trusted loader;
trusted/unknown/revoked publishers are distinguishable, and host ceilings still
restrict a correctly signed application. AOT remains a later, separate design.

### C4 — Local MCP bridge to the running application

Deliver external-agent discovery and control of the C2 application through MCP.

- Pin the supported MCP revision(s) and transcript compatibility tests. Implement
  deterministic, bounded `tools/list` and validated `tools/call` from the existing
  catalog/dispatcher, with protocol metadata and errors appropriate to that revision.
- Start with stdio. Separate protocol output from application logs and enforce
  request size, queue, response size, timeout and cancellation limits.
- For an already-running desktop application, the stdio bridge connects through
  an authenticated local channel to a specific existing instance. Launching another
  bridge must not silently create a second document or divergent application state.
- Bind callers to the intended application/contract identity, expose the permitted
  catalog, and authorize every concrete invocation. Tool descriptions/annotations
  remain untrusted data and do not override host or caller policy.
- Test disconnect/reconnect, multiple agents, duplicate requests and changing
  document revisions. State belongs to the application, not the MCP session.

Exit: an external MCP client lists and invokes published tools from a separately
compiled TS/JS application through the common command path, including generic
state/revision behavior when supported. No application command names or state
fields are built into the bridge. No source code, embedded model or AI-provider SDK is needed
inside the Wasm module. No remote listener is enabled by this local milestone.

### C5 — Generic adapters and remote access (deferred)

- Define versioned host-adapter boundaries, permissions, ownership, event handling
  and resource budgets before adding optional platform integrations.
- Add authenticated remote MCP with HTTPS, explicit listener configuration and
  network deployment tests after local generic dispatch works.
- Establish a supported OS/CPU/runtime matrix and packaging tests. Evaluate small
  devices against actual runtime/memory constraints rather than assuming support.
- Keep GUI behavior, database business operations, document history and robot
  missions/safety workflows in separate applications or integrations. They are not
  built-in engine models. Future external projects validate generic adapter APIs
  only after the required core mechanisms have been implemented.

Desktop/server/robot support must not be claimed merely because Wasm and MCP run
on a development host. Selecting or implementing a reference application is not
part of current core development.

### K5 — Modules, standard system APIs and package compatibility

- Grow ES module resolution/loading under explicit host authority and resource
  budgets; track implemented import/export and initialization semantics.
- Expand filesystem/process/network APIs as generic runtime facilities with
  operation/resource-specific permission checks and portable error behavior.
- Evaluate real package compatibility against an explicit target. Separate loader,
  language and Node/Web API gaps; do not promise npm compatibility from syntax alone.

### K6 — Conformance and runtime readiness

- Pin a supported ECMAScript edition/profile and build a compatibility matrix
  with a maintained conformance-suite strategy plus Varta-specific runtime tests.
- Exercise ordinary server/CLI/library programs through actual public APIs,
  independent of MCP and domain-specific host implementations.
- Track correctness, malformed-input/security behavior, leak-free lifecycle,
  startup/memory/throughput and supported platforms before calling the runtime ready.

### Supporting language and runtime work

- K1 provides the initial checked values/JSON path. Continue resizable/dynamic
  collections, enums and numeric-profile expansion through explicit semantics/ownership
  contracts. Full JS/npm compatibility remains a separate compatibility effort.
- Preserve tracing for strings, records and arrays as richer value graphs are
  introduced; new value kinds need explicit collector and ownership contracts.
- Async/events require cancellation, task/handle lifetime, retained roots, scheduling
  and queue budgets before exposing promises or long-running guest operations.
- Continue model/differential tests, fuzzing and performance measurements of the
  compiler, collector, boundary schemas and artifact parser throughout these phases.

Every implementation milestone retains identical shared check/build/dev acceptance,
versioned documentation, positive and denial tests, bounded malformed-input handling,
Wasmtime validation/behavior coverage and the existing quality gates. Planned
features are described as planned until their exit criteria actually pass.

Protocol references for planning: [MCP tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
and [MCP transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports).
Choose and test exact compatibility targets when implementing C4; the catalog
and application core remain independent of transport/protocol revisions.

## Later milestones

- HTTPS deployment and asynchronous execution follow the C5 and supporting-work
  contracts above; neither is supplied by the initial synchronous HTTP handler profile.
- Authenticated AOT: define who trusts the signer, which bytes are authenticated,
  and how engine version, target and CPU requirements are pinned. Never restore
  arbitrary native deserialization behind a filename or format marker.
- Package compatibility: only after module/type/memory semantics are stable;
  evaluate packages against a documented subset rather than promising Node parity.
- Production: fuzz schemas/containers/Wasm boundaries, expand platform coverage,
  isolate compilation and blocking host operations where hard OS-level resource
  isolation is required, and obtain an independent security review.

### BigInt compound assignment increment (v29)

BigInt targets and admitted primitive unions reuse checked scalar/union arithmetic
for compound assignments. Prepared receivers, keys and old values precede RHS
effects; failed operations do not store. Five new runtime cases cover all eleven
signed arithmetic/bitwise operators, unsigned-shift TypeError, division RangeError,
array-backing changes, sparse/optional fields, union dispatch and v28 rejection.
The public API and source-free artifact cases also exercise an array compound store.
At that checkpoint, full coercive built-ins and increment/decrement remained
pending; the numeric-update increment below advances the latter. CORE-01 stays Partial.

Scoped verification of this increment: 51 compound-assignment runtime tests and
19 BigInt graph/ABI/GC tests pass (overlapping selections), plus one cross-module
public API test and one CLI test covering development/module execution and
source-free Wasm/native artifacts; the launcher build passes. Strict compiler/runtime
all-target Clippy, formatting and diff checks pass. The reviewed conformance run reports 53/41/12 total/pass/known gaps,
no unexpected support, regressions or infrastructure failures, and seven untested
feature rows. These focused results do not replace the pre-commit full gate.

### BigInt numeric updates (v29)

Prefix/postfix increment and decrement preserve exact BigInt values and return
the old/new numeric value after a successful checked store. Number|BigInt unions
select the corresponding 1 or 1n unit after caching the input once. Prepared
receivers/keys, sparse and optional presence, mutable backing resolution and
rooted old/new results retain the existing update contract. V28 still rejects
BigInt updates. At this checkpoint primitive ToNumeric coercion remained pending;
the later primitive-update increment below adds it within checked storage types.
CORE-01 remains Partial, with coercive built-ins and broader primitive semantics
still outstanding.

Scoped verification of numeric updates: 23 update-related runtime tests and 25
BigInt graph/ABI/GC tests pass (overlapping selections), including six new numeric
update cases. The cross-module public API case and CLI development/source-free
Wasm/native case pass with both prefix and postfix array updates. Strict
compiler/runtime all-target Clippy, formatting and diff checks pass. The reviewed
corpus at that checkpoint had 54 cases: 42 supported, 12 known unsupported, zero
regressions, unexpected support or infrastructure failures, and seven untested
feature rows. Inventory and lock digests matched the files at that checkpoint.
This is scoped evidence; the
full pre-commit gate has not been repeated for this increment.

### BigInt primitive width/radix coercion (v29)

ABI28 adds pure bigint_width_v2 and bigint_to_string_v2 checked imports accepting
canonical v13 primitive cells. Source arguments, including ignored extras, are
evaluated once before conversions. Width methods apply ToIndex before ToBigInt:
String/Boolean values are admitted, Number values throw TypeError even at width
zero, malformed integer strings throw SyntaxError, and invalid widths throw
RangeError first. Radix defaults only for undefined; other primitives undergo
implicit ToNumber, so BigInt radix throws TypeError and out-of-range/NaN radix
throws RangeError. Both transports are validated before a recoverable failure;
malformed payloads/resources stay fatal, fuel is charged and pure status preserved.
V28 retains its Number-width/BigInt-value and Number/Undefined-radix admission.
At this checkpoint object coercion, Symbol and primitive update coercion remained
pending; primitive updates are implemented in the later increment below.

The unchanged coercive-width corpus gap was observed as unexpected support with
exact expected output before promotion. A new radix/error case extends the
inventory; broad ECMAScript compatibility and CORE-01 completion remain unproven.

Verification of primitive BigInt methods: 1149 runtime unit tests, 55 common
and 36 compiler tests pass, including five new source-coercion tests and the
new malformed-transport ABI test (31 focused BigInt graph cases). Three primitive
conversion/error scenarios also match normalized Node.js v24.20.0 output exactly.
The cross-module API and CLI development/source-free Wasm/native cases pass with
coercive widths and radices; the updated runtime companion builds. Strict
common/compiler/runtime all-target Clippy, formatting and diff checks pass.
Baseline and strict JSON reports are identical: 55 total, 44 supported, 11 known
unsupported, zero unexpected support/regressions/infrastructure errors and seven
untested feature rows. Strict mode exits 1 as expected; inventory and lock digests
match the current files. No new full workspace or cross-platform gate is claimed.

### Primitive update ToNumeric (v29)

Increment/decrement now converts String/Boolean/Null/Undefined to Number and
retains BigInt exactly, including the converted numeric old value for postfix
expressions. Each source read occurs once; metered PrimitiveNumberV2 handles
non-numeric primitives. Number-capable optional fields and sparse array entries
can become NaN with successful stores publishing presence/order only afterward.
Declared mutable storage types remain invariant: computed numeric union results
are checked before storing, and postfix old values are narrowed only after a
successful store proves their numeric kind. Missing BigInt-only entries still
throw TypeError without mutation; explicit String-only storage cannot accept a
Number update. Dynamic record guards preserve each selected field type.
V28 and older profiles retain noncoercive updates. Object ToPrimitive and general
inferred storage widening remain pending; CORE-01/02/03 stay Partial.

Verification of primitive update ToNumeric: seven new runtime tests pass,
covering all admitted primitive kinds, refined stores, sparse/present fields,
dynamic record field invariants, BigInt-only storage and v28/object rejection,
and fatal native parsing fuel exhaustion. The affected update selection passed
34 tests before the additional one-test fuel run; 31 BigInt graph/ABI/GC tests
and 36 compiler tests also pass. Cross-module public API and CLI source-free
Wasm/native cases exercise primitive updates through the updated module helper.
Strict compiler/runtime all-target Clippy, formatting and diff checks pass.
Two pinned conformance cases match normalized Node.js v24.20.0 output. Baseline
and strict reports match exactly: 57 total, 46 supported, 11 known unsupported,
zero unexpected support/regressions/infrastructure failures, seven untested rows;
strict exits 1 as expected and current inventory/lock hashes match. No new full
workspace/runtime/CLI or cross-platform pass is claimed.

### Primitive compound assignment coercion (v29)

All arithmetic/bitwise compound assignments now cache the old value and RHS
before primitive conversion. Addition chooses concatenation when an actual
operand is String; other cases normalize to Number/BigInt, retaining exact BigInt
and mixed-numeric TypeError. Six-member primitive operands normalize once before
a maximum two-by-two numeric dispatch, preserving existing body/root limits.
Both native String/Number conversions use the metered v2 imports. Always-throwing
mixed pairs do not invent a BigInt result type, so Number targets can propagate
the intended TypeError instead of failing static assignment admission.
A shared checked result guard narrows only successful values compatible with
declared storage. Dynamic record fields retain their individual types, and
failed stores preserve RHS effects without publishing a new target value.
Number-capable holes can become NaN, while a String RHS can concatenate an
undefined sparse read into a valid String result. Typed array/field invariance,
source order, UTF16/roots, resource ceilings and v28 rejection are preserved.
Ordinary unary/binary coercion, object ToPrimitive, Symbol and general inferred
storage widening remain outstanding; CORE-01/02/03 stay Partial.

Verification of compound primitive coercion: all 1165 runtime unit tests and
36 compiler tests pass, including nine new cases covering primitive operators,
six-member operands, checked sparse/dynamic stores, mixed-numeric failures,
GC across RHS replacement and fatal native conversion fuel exhaustion. The
cross-module API and CLI development/source-free Wasm/native cases pass with
coercive multiplication and String/BigInt concatenation. Strict compiler/runtime
all-target Clippy, formatting and diff checks pass. Three new pinned corpus cases
match normalized Node.js v24.20.0. Baseline and strict reports are identical:
60 total, 49 supported, 11 known unsupported, no unexpected support/regressions/
infrastructure errors, seven untested rows; strict exits 1 as expected. Current
inventory and lock hashes match. This affected-runtime pass replaces the older
1149-test runtime checkpoint for current lowering; full workspace/CLI repetition
and cross-platform verification have not been performed for this increment.

### Ordinary primitive arithmetic and unary operators (v29)

Ordinary +, -, *, /, %, ** and bitwise/shift expressions share the cached
primitive arithmetic engine with compound assignments. Addition chooses actual
String concatenation; otherwise operands become Number/BigInt, with checked
mixed-numeric TypeError and exact BigInt operations. Scalar Number and String
paths avoid redundant native conversions. Six-member primitive unions normalize
before bounded numeric dispatch, preserving source order and current limits.

Unary +, - and ~ admit primitive conversion. Minus/not retain BigInt; unary plus
uses the Number result domain and throws TypeError for BigInt. The native
BigInt unary operation still validates canonical leaves and meters work before
that recoverable error, so malformed/resource failures bypass guest handlers.
GC, sparse reads, signed zero/NaN/Infinity and once-only source effects are covered.
V28 and older source admission stays frozen. Comparison/equality semantics,
object ToPrimitive, Symbol and general inferred storage widening remain pending;
CORE-01/02/03 stay Partial.

Verification of ordinary primitive arithmetic/unary: eight new runtime cases
pass, including canonical BigInt unary-plus corruption bypassing catch/finally,
wide unions, once-only effects and GC after RHS replacement. Affected selections
pass 72 primitive-related, 50 BigInt-related and seven update-coercion tests
(overlapping), plus 36 compiler tests. The cross-module API and CLI development/
source-free Wasm/native cases cover arithmetic and unary conversion, including
Number-domain BigInt TypeError. Strict compiler/runtime all-target Clippy,
formatting and diff checks pass. Three new pinned cases match normalized
Node.js v24.20.0. Baseline and strict JSON reports agree: 63 total, 52 supported,
11 known unsupported, zero unexpected support/regressions/infrastructure errors,
seven untested rows. Strict exits 1 as expected; current inventory and lock hashes
match. The older 1165-test runtime pass remains historical evidence; no new
full runtime/workspace/CLI repeat or cross-platform pass is claimed.

### Primitive comparison/equality block (v29)

All eight equality/ordering operators support the six admitted primitive kinds.
Strict equality retains type distinctions, NaN inequality and signed-zero equality.
Abstract equality implements nullish pairs, Boolean conversion, String/Number
conversion and exact BigInt/String/Number rules. Ordering compares two Strings by
UTF16 units, treats NaN/invalid BigInt strings as unordered and never rounds BigInt
through Number. Source operands run once before canonical cells are boxed/rooted.
ABI28 pure primitive_equal/primitive_compare imports validate both transports,
charge native fuel and keep malformed/resource failures fatal. Scalar Number,
String, Boolean equality, BigInt and nullish paths retain suitable guest/existing
fast paths; a 1500-iteration numeric/nullish loop stays below host-call ceilings.

Six new runtime cases include a pinned Node.js v24.20.0 oracle of 2048 comparisons,
precision/infinity/UTF16 edge cases, source order, direct-operand GC, scalar-loop
imports and malformed/fuel failures bypassing handlers. Three pinned corpus cases
also match Node.js. V28 admission remains frozen; objects, Symbol and broader
mutable storage remain explicit unfinished CORE-01/02/03 work.

### Closed property deletion (v30)

V30 inherits v29 ABI28/value13/schema13/channel4 and adds optional-record deletion
and primitive computed delete keys for records/arrays. Base/key run once before
mutation. Deletion installs canonical undefined, compacts live order entries,
zeros the unused trailer and preserves static trace masks. Reinsertion appends
new string-key order; numeric enumeration stays numeric-first. Escaped values
retain identity through GC while deleted edges can be reclaimed. Absent/unknown
own keys return true; required checked record fields and array length throw
TypeError before mutation. Object/Symbol ToPropertyKey, arbitrary properties,
descriptors and general required-field deletion remain unfinished CORE-02 work.
V29 and older source admission remains frozen; native launcher protocol2 now
advertises 33 profiles. No ABI/layout/resource ceiling was raised.

Verification: seven focused runtime tests, 55 existing common tests plus one new
profile test, 36 compiler tests, strict common/compiler/runtime all-target Clippy,
one cross-module public API test and one CLI development/source-free Wasm/native
test pass. The runtime companion rebuild passes. Tests cover field31, order,
primitive keys, aliases and repeated large allocations/deletions beyond heap
capacity. Three new pinned cases match Node.js v24.20.0. Current inventory is
v30 with 78 cases: 67 supported, 11 known gaps, zero regressions/infrastructure
errors and seven untested feature rows. The previous full workspace gate (1678
tests, commit 040756c) is historical evidence, not a full pass for this increment.

### Object.entries snapshots (v30)

Closed records and typed arrays now produce dense mutable key/value pair arrays
in own-key order. Absent fields and holes are skipped; explicit undefined remains.
Pairs are fresh and nested objects retain identity. Source/ignored arguments run
once before snapshots; nullish/missing sources throw TypeError and primitive
Number/Boolean/BigInt wrappers return empty arrays. Pair types use the existing
String/value union; positional tuple typing, generic objects, Symbol and mixed aggregate source
shapes remain unfinished CORE-02/07 work. The String enumeration increment below
extends this operation to String inputs and primitive unions.
Seven focused runtime tests pass, including a 1024-element snapshot and collection
beyond heap capacity with explicitly increased test-only instruction budgets.
The cross-module public API and CLI development/source-free Wasm/native fixtures
pass with deletion followed by enumeration. Strict common/compiler/runtime
all-target Clippy passes. Two new pinned cases match Node.js v24.20.0. Baseline
and strict corpus reports agree: 78 total, 67 passes, 11 known gaps, no unexpected
support, regressions or infrastructure failures; seven feature rows remain
untested. Strict mode exits 1 for these declared gaps. Full workspace verification
remains pending for this uncommitted batch; the CORE work streams remain Partial.

### String own-property enumeration (v30)

Object.keys/values/entries now accept String, nullable String and unions of the
six admitted primitive kinds. String properties enumerate UTF16 units, retaining
surrogate pairs as two values and preserving lone surrogates. Primitive wrappers
without own enumerable properties produce empty arrays; nullish/missing inputs
throw TypeError. V30 keys/values evaluate ignored arguments before snapshots.
Guest-only slicing/array helpers avoid native calls per unit. More than 1024
units raises recoverable RangeError; ABI/layouts and production budgets remain
unchanged. Older-profile call rules and String rejection stay frozen.

Seven new targeted tests cover UTF16, primitive unions, ignored-argument order,
nullable errors, capacity and repeated collection. Capacity and collection stress
fixtures explicitly increase test-only fuel. Two pinned cases match Node.js
v24.20.0. The combined object-operation filter passes 53 runtime tests, and the
cross-module public API and CLI development/source-free Wasm/native fixture pass.
Strict common/compiler/runtime all-target Clippy and formatting checks pass.
The reviewed corpus reports 78 total, 67 passes and 11 known gaps, with no
unexpected support, regressions or infrastructure failures; seven feature rows
remain untested. Strict mode reports the same counts and exits 1 for declared
incomplete coverage. Seven property-deletion regression tests also pass after
the enumeration changes. General object conversion, mixed aggregate sources,
Symbol, descriptors and positional tuple inference remain unfinished. CORE-02/07
remain Partial; no full workspace or production readiness claim is made.

### Object.is / SameValue (v30)

Object.is now compares admitted primitives and reference identities without
coercion, including union dispatch across different checked aggregate shapes.
NaNs are equal, signed zeros distinct; Strings compare UTF16 contents and BigInts
retain exact mathematical equality. Argument expressions run once left-to-right,
missing/void arguments become Undefined, and ignored extras execute before the
comparison. Cached operands remain rooted across later allocations/collection.
Numeric comparisons are guest-only and no ABI/resource ceiling changes.

Seven focused tests pass, including a pinned Node.js v24.20.0 matrix of 324
primitive pairs, reference/error aliases, distinct shapes, ignored arguments,
collection stress and a 1500-iteration numeric loop at default budgets. The
collection-only fixture explicitly increases test fuel without changing memory
limits. Two original corpus cases match Node.js. Symbol, unimplemented kinds,
first-class intrinsic values and native result-pair object identity remain
unfinished; native results require explicit fields. CORE-01/02/03 remain Partial.

Object.is integration verification: cross-module public API, CLI development,
source-free Wasm/native execution, strict common/compiler/runtime all-target
Clippy and formatting checks pass. The reviewed corpus contains 40 feature rows
and 78 cases: 67 pass, 11 remain known unsupported, zero unexpected support,
regressions or infrastructure failures; seven feature rows remain untested.
This is scoped evidence for the increment, not a full workspace or readiness gate.

### Checked Object.assign (v30)

Record/array targets copy own enumerable record, array and UTF16 String properties
in source/key order, preserving target identity and shared children. All argument
effects precede target validation; per-source snapshots follow earlier copies so
later aliases observe prior writes. Holes are skipped, explicit Undefined copied,
and nullish sources ignored. Primitive/function/Error sources without enumerable
own fields are inert. Missing/nullish targets throw TypeError. Array length uses
Number validation; semantic failures retain earlier writes and checked field
storage remains sound. Numeric array copies require no per-index native call.

Plain fresh record literal targets reserve known record-source fields in their
initial descriptor; new fields are optional and existing literal field types may
widen to an admitted union. No escaped target changes layout or reference mask.
Record sources read fields separately, admitting unrelated nested shapes without
erased unions. General property creation, primitive target boxing, descriptors,
getters/setters, Symbol and unrestricted shape-changing storage remain unfinished
CORE-02/03/06 work. Indexed sources require declared target fields; unknown keys
retain existing checked-write guards. This is an explicit checked increment,
not general JavaScript Object.assign completion.

Fifteen focused tests pass, covering own order/presence, fresh shapes, nested
identity, record/array/source unions, argument order, partial errors and aliases.
A 1024-element array copy fits default production budgets. The collection stress
fixture alone raises test fuel and keeps the production memory limit. Three new
pinned cases match Node.js v24.20.0. ABI/layouts, the 33-profile list and all
production ceilings remain unchanged; CORE-02/03/06/07 remain Partial.

Checked Object.assign integration verification passes: cross-module public API,
CLI development and source-free Wasm/native execution, strict common/compiler/
runtime all-target Clippy, formatting and whitespace checks. The reviewed corpus
contains 40 feature rows and 78 cases: 67 passes, 11 known unsupported, no
unexpected support, regressions or infrastructure failures; seven feature rows
remain untested. This increment has scoped verification; the full workspace
pre-commit gate and wider readiness criteria remain outstanding.

### Managed function transport foundation (in progress)

Function-valued records/arrays/unions still require a versioned public value tag,
updated schema/native transport and receiver-preserving property calls. Current
v30 source admission and value13 stay frozen. The existing closure representation
has a distinct opaque heap flag, signature/code slots and an optional environment;
its binding type IDs are compiler-private and are not public ValueKind tags.

New allocation-free common views validate physical closure/environment/binding
envelopes before future structured-value admission. They check handle extents,
alignment, allocated heap flags, reserved words, signature/code budget ceilings,
exact trace masks, capture counts up to 32 and one/two-slot binding widths.
Duplicate captured cells and recursive function payloads remain legal; opaque
binding payloads are not recursively interpreted. Matching a binding envelope to
compiler-provided type ID/width/root metadata is explicit. These checks do not
prove code/signature membership in a module or validate a captured value's type;
those remain required at the future typed transport boundary.

Five malformed/boundary common tests pass. Two runtime tests compare these views
with real compiler-emitted mutable captures, String captures, capture-free
closures and captured native result pairs, and retain v30 rejection of function
aggregates. No public function value tag, new profile, host import, ABI/layout,
resource limit or language admission has changed. Function-valued properties,
method calls, JSON function rules and descriptors remain unfinished CORE-02/03/07
work; this foundation does not close any readiness stream.

Envelope-foundation verification also passes strict common/compiler/runtime
all-target Clippy, formatting and whitespace checks. Corpus inventory remains
78 cases / 67 supported / 11 known gaps with seven untested feature rows; no
source behavior changed and no new compatibility pass is claimed for this step.

### Typed private capture catalog (transport preparation)

V30 modules with function signatures now optionally carry a versioned
`varta.functions` catalog outside mutable guest memory. It maps private binding
IDs to exact value schemas/result widths and maps closure code/signature IDs to
capture layouts. The compiler derives it from the same IR tables used by checked
dispatch and closure prologues. Older V30 artifacts without the optional catalog
remain accepted; earlier profiles do not admit the new metadata. Preflight rejects
duplicate, malformed or incompatible catalog sections before execution.

Typed closure inspection validates captured primitive bits, String extents,
BigInt leaves, Error classes, records, sparse arrays and union member selection.
Record checks include field names, types, optionality and exact compiler slot
identity, not merely a structurally valid descriptor. Result pairs retain exact
width/root metadata and bounded status values. Private function captures check
catalog code/signature membership; shared bindings and recursive closures remain
legal through memoization. A work allowance plus existing 4096-node/64-depth
bounds prevents unbounded traversal. Catalog type DAGs enforce their depth bound
even when serialized bottom-up. No helper invokes guest code or grants authority;
Wasm checked dispatch remains responsible for actual invocation.

Ten common function/catalog tests and the 51-test runtime function filter pass.
After strengthening record slot checks, four focused runtime integration/security
tests and ten common tests pass again. These cover real emitted closures, nested
structured captures, native result pairs, recursive function payloads, corrupted
Boolean values/descriptors, and preflight metadata rejection. The public
cross-module and artifact fixtures now include an escaped numeric closure.

Value ABI13, schema13, host ABI28 and the 33-profile list are unchanged. A public
function value tag, native transport integration, function-valued aggregate source
admission and receiver-preserving property calls remain outstanding. The catalog
is preparation for that versioned increment, not a claim that function-valued
properties or accessor descriptors are complete. CORE-02/03/07 remain Partial.

Typed-catalog batch verification: strict common/compiler/runtime all-target
Clippy, cross-module public API, rebuilt runtime companion and CLI development /
source-free Wasm/native fixture pass. Formatting/whitespace checks pass. The
reviewed corpus still reports 78 cases: 67 supported, 11 known unsupported,
zero unexpected support/regressions/infrastructure failures, seven untested
feature rows. No new language compatibility pass or full workspace readiness
claim is made; public function-value admission remains the next required stage.

### Function value14 codec (not yet source-admitted)

Common value ABI14 adds Function tag12 with an exact reference mask. V13 and
older reject this tag. Record/presence, sparse-vector, Error and BigInt layouts
inherit their prior physical forms under the new explicit version. Function
reference cells require an immutable owner catalog and bounded typed closure
validation; context-free function reads fail. Catalogs allow function fields,
array elements and one function signature per reference union only under v14.
Private function captures in v13 catalogs retain their existing behavior.

The runtime exposes a Rust value14 graph encoder without granting invocation
rights. Ordinary function properties are omitted, array function values become
null, and a top-level function produces Undefined. Validation occurs before
omission and shares a caller-owned function work budget. Private capture cycles
through records or arrays remain legal and do not become observable JSON cycles.
Callable toJSON hooks are explicitly rejected until guest-side dispatch exists;
this encoder never silently treats such a hook as an ordinary omitted property.
Schema14 maps existing JSON input types to value14 layouts, but JSON tokens and
schemas cannot fabricate callable values. Existing schema/resource ceilings stay.

Five initial v14 data-level tests pass; final tests also cover toJSON rejection
and the public Rust codec on a real emitted scalar-closure snapshot after explicit
owner-side transcoding. All 67 common tests and 85 runtime JSON-filter tests pass.
The whole workspace compiles. This is codec groundwork, not new TS admission:
all 33 source profiles retain their current contracts (latest v30/value13/ABI28).
A new source profile, versioned host imports/context wiring, member invocation,
function-valued aggregates and their end-to-end source/artifact/conformance checks
remain outstanding. No additional language compatibility pass is claimed, and
CORE-02/03/07 remain Partial.

Final value14 codec verification: seven focused data/Rust-API tests and four
legacy typed-closure/catalog tests pass. Strict common/compiler/runtime all-target
Clippy passes, followed by final runtime all-target Clippy after the public API
fixture; formatting and whitespace checks pass. Node.js independently matches
ordinary function omission/null/Undefined and both private-cycle JSON outcomes.
No source/artifact/conformance pass is claimed for value14 host operations, which
are not registered yet; current language admission stays unchanged.
