# Varta development history

Completed increment logs, earlier status checkpoints and superseded direction text,
moved verbatim from the development plan and README on 2026-10-28. These records
describe the state at the time each entry was written; counts, versions and "next"
statements in them are historical. Current status is in the
[development plan](development-plan.md#status); current contracts are in
[architecture.md](architecture.md).

## Superseded direction text (2026-10-28)

The project direction was restated as a [research thesis](development-plan.md#research-thesis).
The earlier wording follows.

### Former development-plan runtime target paragraph

The product target is a self-contained JS/TS runtime in the same application
category as Deno/Bun, retaining Varta's TS/JS → Wasm → controlled-runtime path.
The current deliberately small checked subset is an implementation stage, not
the permanent product boundary. A JSON server is the first vertical acceptance
scenario, not the definition of a complete runtime.

### Former development-plan active priority

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

### Former README platform direction

Varta targets typed, contract-driven TS/JS applications for desktop, server and
robot environments. The intended architecture compiles supported source into
portable Wasm and exposes explicitly selected application operations to external
AI agents through MCP. UI, API and agent callers share the application's command
and query layer, business rules and state; connecting another agent does not
create another application state.

The target is a self-contained JS/TS runtime in the Deno/Bun category. The current
small language subset is an implementation stage, not the product's final scope.
Language semantics, memory management, scheduling, standard APIs, modules/packages
and distribution all belong to the core roadmap. Full compatibility is not yet
implemented; the current i32 numeric profile in particular is not standard JS
Number behavior. See the [runtime target](development-plan.md#runtime-target-and-compatibility).

New modules explicitly declare their selected language profile (default `checked-i32-utf8-v1`).
`inspect` reports that profile and whether it was declared or inferred from an
older host ABI v1 artifact. Unknown profiles fail before execution. Generic Wasm
without language/ABI metadata reports an unspecified source language. An opt-in
binary64 profile with typed aggregates and JSON is implemented below. A further
opt-in profile adds distinct `undefined`; `checked-f64-utf16-v1` adds UTF-16 strings.
`checked-f64-utf16-v2` adds closed primitive unions and runtime type checks.
`checked-f64-utf16-v3` adds presence-aware optional fields and missing primitive reads.
`checked-f64-utf16-v4` extends unions to closed records/arrays and adds Array.isArray.
`checked-f64-utf16-v5` adds strict JSON parsing for optional fields and unions.
`checked-f64-utf16-v6` adds bounded resizable arrays with push/pop, at/slice and includes/indexOf.
`checked-f64-utf16-v7` adds explicit primitive throw/try-catch across calls.
`checked-f64-utf16-v8` adds nominal Error objects with checked messages and guards.
`checked-f64-utf16-v9` adds typed TypeError, RangeError and SyntaxError families.
`checked-f64-utf16-v10` makes checked JSON syntax/type failures catchable.
`checked-f64-utf16-v11` adds typed function values and capture-free arrows.
`checked-f64-utf16-v12` adds escaped closures over initialized lexical bindings,
plus map/filter/forEach/find/reduce/some/every as additive compiler operations.
`checked-f64-utf16-v13` adds sparse array presence and hole-preserving map results.
`checked-f64-utf16-v14` adds bounded indexed extension, delete and writable length.
`checked-f64-utf16-v15` adds checked Number exponentiation (`**`, `**=`), using
host ABI v18 with the existing value/schema v12 layouts. It also admits the
documented direct Math intrinsics and non-coercing Number predicates below.
`checked-f64-utf16-v16` adds the twenty deterministic native Math operations
described below, with host ABI v19 and the same value/schema v12 layouts.
`checked-f64-utf16-v17` adds Math.random through host ABI v20, inheriting v16
without changing the value/schema v12 layouts.
`checked-f64-utf16-v18` adds ECMAScript decimal Number formatting in scalar and
union logging through host ABI v21, retaining value/schema v12.
`checked-f64-utf16-v19` adds primitive String(...) conversion through ABI v22
with the same value/schema v12 layouts.
`checked-f64-utf16-v20` adds primitive Number(...) conversion and checked numeric
String parsing through ABI v23, retaining value/schema v12.
`checked-f64-utf16-v21` adds checked direct Number.toString calls in bases 2–36
through ABI v24 with value/schema v12.
`checked-f64-utf16-v22` adds direct Number.toFixed/toExponential/toPrecision
and guest-only Number.valueOf through ABI v25 with value/schema v12.
`checked-f64-utf16-v23` adds global and Number.parseInt/parseFloat calls through
ABI v26, retaining value/schema v12. It also admits direct Boolean(...) and
coercing global isFinite/isNaN as described below.
`checked-f64-utf16-v24` adds truth-based local guards and short-circuit RHS
refinement, sharing host ABI26 / value/schema12 with v23.
`checked-f64-utf16-v25` adds checked Array.findIndex/findLast/findLastIndex,
sharing host ABI26 / value/schema12 and inheriting v24. Rebuild both native
companions for the expanded protocol2 profile list.
`checked-f64-utf16-v26` adds checked Array.reduceRight with optional typed
initial values, inheriting v25 with the same host ABI26 / value/schema12.
`checked-f64-utf16-v27` adds uncontextual array element-union inference and
peer typing for empty nested arrays, retaining host ABI26 / value/schema12.
General dynamic objects and external adapters remain pending. The
[migration contract](development-plan.md#k4--ecmascript-value-profile-and-language-expansion)
preserves the existing artifact and tool semantics.

Primitive Tool Contract v1, offline discovery and checked JSON invocation are
implemented. A broader Application Contract and a long-lived application
service, MCP bridges, package signatures and desktop/database/hardware adapters
remain planned. Closed typed objects, `null` and bounded `JSON.stringify` are
implemented. Dense typed arrays and `varta.json.parse<T>` are also implemented.
Inbound HTTP/1 now supports checked synchronous handlers with isolated request
instances. General dynamic JSON parsing and broader web APIs remain pending;
`net.fetch`/`net.get` are outbound clients.
The [active delivery sequence](development-plan.md#delivery-sequence) now
has delivered the first structured values/JSON, HTTP and independent runtime
packaging profiles. Broader language semantics and standard APIs remain core work;
generic application state and MCP follow those foundations.
A developer-written external server wrapper is an integration option, not the
acceptance target for an HTTP server on Varta.

Portable Wasm requires a compatible host; native launchers target a supported
OS/CPU. Source-free compilation does not guarantee that business logic cannot be
analyzed or modified. Authenticity requires verification by a trusted loader,
while concrete actions always remain subject to host and caller permissions.

### Former README introduction

Varta compiles a small, checked TypeScript subset to WebAssembly and runs it in a
capability-checked Wasmtime host. System access is denied unless the exact
operation and resource are declared. Applications can be distributed as a single
native executable containing portable Wasm and its runtime.

Status: MVP / active development. The [architecture contract](architecture.md)
describes the implemented language, trust boundaries, ABI and limits. The
[development plan](development-plan.md) tracks audit fixes and next milestones.
The [2026-10-28 checkpoint](development-plan.md#status)
records completed core increments, verified test counts, remaining limits and the
next steps for checked record access.

### Former architecture introduction

This document describes the implemented MVP. Future work is tracked separately in
[development-plan.md](development-plan.md). Varta compiles a deliberately small
TypeScript subset to portable WebAssembly and executes it in a capability-checked
Wasmtime host. It does not provide general JavaScript, Node or npm compatibility.
The [2026-10-28 development checkpoint](development-plan.md#status)
records the current delivery state and local verification; the contracts below
remain the source of truth for individual features and profile boundaries.

## Status checkpoints

### Development checkpoint — 2026-10-28

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
| Function codec groundwork | Common value14 tag12, catalog-required function cells, typed graph checks, JSON omission/null rules and immutable per-module/Store catalog ownership; source profile and versioned host operations remain pending |
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
array APIs remain unfinished. See the [predicate-search contract](architecture.md#checked-f64-utf16-v25--predicate-array-searches).

Completed: **Array.reduceRight** in opt-in v26, sharing ABI26/value12/schema12.
The fold visits present indices in descending order, uses an explicit typed seed
or the rightmost present element, and throws recoverable TypeError for unseeded
empty/all-hole arrays. Live mutation/presence, source/callback/seed/ignored-argument
ordering, accumulator inference/identity, callback roots and fatal/resource/capability
boundaries have regression coverage. V25 and earlier reject the new method;
existing reduce admission stays unchanged. See the
[reverse-fold contract](architecture.md#checked-f64-utf16-v26--reverse-array-folds).

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
[inference contract](architecture.md#checked-f64-utf16-v27--array-literal-inference).

Next architectural block: the
[suspension foundation](development-plan.md#k44--suspension-scheduling-and-async-planned)
(expression normalization, continuation frames over closure cells, exception/GC
invariants and a closed resume dispatcher), after the prepared-catalog increment
is committed. Generators, Promise jobs and async/await build on it in that order.
Priorities are checked against the [uarp-sdk pilot](development-plan.md#pilot-acceptance-package-uarp-sdk);
its async-independent steps M1–M3 can proceed alongside that block.

Other open work: **higher-order callback-argument binding, property/exception effects, initializer
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

## Pilot measurements

Superseded uarp-sdk pilot measurements, newest first; the current one is in the
[development plan](development-plan.md#pilot-acceptance-package-uarp-sdk).

**Verified behavior with v32, second wave (2026-10-29).** The same probe on the
integration branch with all nine v32 selectors (default exports, interface
inheritance, generics and Error subclasses added): each of the 65 source files of
the pinned tarball was compiled with `varta check` under `checked-f64-utf16-v32`
as the only `export *` source of a probe entry, so the file and all its
dependencies form the graph; production budgets were unchanged. The first
diagnostic per file:

| First diagnostic | Files |
| --- | --- |
| Compiles and validates: `generated/meta.ts`, `generated/models.ts`, `core/json.ts` | 3 |
| `core/errors.ts:121` `unsupported type reference Headers` (reached through `core/resource.ts`, `core/transport.ts`, `core/sse.ts` and most resources) | 36 |
| `core/pagination.ts:27` `unsupported async or generator generic function` (`autoPaginate<T>`, an `async function*`, reached by paginated resources) | 23 |
| `core/util.ts:21` `unsupported type reference AbortSignal` (`sleep`) | 1 |
| `generated/resources/index.ts`, `index.ts`: module graph exceeds the import-declaration limit | 2 |

Compared with the first wave, `CursorPage<T>` and the uninstantiated generic
`pick<T, K extends keyof T>` now pass, so pagination stops at the async generator
(K4.4) and `core/util.ts` at Web types; `index.ts` passes its `export default` and
stops at the graph limits (65 modules and about 370 relative import/re-export
declarations against 64 modules and 256 declarations). With `Headers` stubbed,
`core/errors.ts` still stops first at the index signature in `ProblemDocument`,
so its `class … extends Error` hierarchy (now in the v32 subset, except
`unknown`-typed causes) is not yet reached in the file itself. Instantiating `pick` would next need `Record<string, unknown>`,
`unknown`, an `as` assertion and computed access with a `keyof` key, all still
rejected.

## Completed milestones and follow-ups

### Completed milestone: architectural hardening

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
### Completed follow-up: host policy ceilings

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
### Completed follow-up: source diagnostics

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
### Completed follow-up: local modules

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
### Completed follow-up: retained string values

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
### Completed follow-up: programmatic host errors

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
### Completed follow-up: string comparison and concatenation

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
### Completed follow-up: string memory reclamation

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
### Completed follow-up: engine and tooling boundaries

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

## Increment logs

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
### Prepared function-catalog ownership increment

The preceding source snapshot was committed and pushed as `bebe8d4` after the
full seven-command verification gate passed 1746 tests with unchanged sources.
The reviewed compatibility inventory remains 78 cases: 67 supported, 11 known
gaps, plus seven untested feature rows; this is not complete compatibility.

The next runtime increment retains an immutable verified capture catalog beside
its validated module and transfers shared ownership into each execution Store.
Startup, tools and HTTP requests use this prepared path. One instantiation
attempt per execution prevents replacing a live module's schema, including after
a failure. Failed instantiation clears the owner. Legacy backend validation APIs
and optional-catalog admission remain available. Capture proofs on the sealed
common catalog retain graph/work checks without revalidating immutable schemas.
Source profile31, host ABI29, function-valued aggregates/member invocation and
end-to-end value14 source/artifact/conformance checks are still outstanding.
CORE completion states remain unchanged; this increment closes runtime metadata
ownership groundwork, not the full function transport requirement.

Verification for prepared ownership: the new common sealed-catalog test and
three runtime ownership tests pass (the prepared-name filter also ran nine
existing regressions). Eight tool-invocation tests, six HTTP tests and the final
58-test function filter pass, including real emitted capture graphs validated
through the Store's catalog and v30 artifacts with optional metadata removed.
The cross-module public API test and rebuilt runtime companion/source-free CLI
artifact test pass. Strict common/runtime all-target Clippy passed; final test
fixture additions passed compilation and focused behavior checks. Formatting and
whitespace checks pass. No new full-workspace gate or value14 source compatibility
pass is claimed for this increment.
