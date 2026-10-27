# CLAUDE.md — Varta Agent Guide

Guide for AI agents and contributors working in this repository. Follow it exactly; when it conflicts with personal preference, this file wins.

## 1. What Varta is

Varta targets general-purpose cross-platform TS/JS applications, with optional
explicit AI tool interfaces. The current implementation is a compiler and secure runtime
for a deliberately small source subset:

- TS source is parsed (SWC) and lowered through resolved typed IR to validated WebAssembly (`varta-compiler`);
- the Wasm module runs in Wasmtime with capability-checked host functions (`varta-runtime`);
- privileged operations (fs, net, env, time, process) exist **only** as host imports and are denied by default;
- the production artifact is a single native launcher with portable Wasm, a strict capability manifest and a fixed versioned EOF footer (`varta build --single`).

Primitive Tool Contract v1 and offline discovery are implemented: entry-module
`varta.tool` declarations generate bounded metadata and checked handler exports.
`inspect` reads schemas without instantiating guest code. Keep metadata and
physical signatures consistent, recompute transitive effects from Wasm, and
preserve the copy/retain string ownership contract described in `docs/architecture.md`.

Named JSON invocation is implemented in the shared runtime/project path and the
thin `invoke` CLI. Validate every parameter before instantiation, use one fresh
bounded store for argument helpers and the handler, never call `_start`, check
canonical boolean results and all guest string ranges/UTF-8, then copy results
before dropping the instance. Keep duplicate-key rejection, encoded JSON byte
limits and typed invocation errors separate from recoverable host statuses.

Closed records/null and bounded JSON.stringify are implemented in the current
i32/UTF-8 profile. Record fields use structural shape indices and preserve object
identity, source evaluation order and JS key enumeration order. Dense homogeneous
arrays support typed literals, indexing/replacement and length; they currently have
fixed lengths and trap on out-of-range indices. Structured-value ABI v2 adds arrays;
v1 scalars/records retain their meaning. `varta.json.parse<T>` compiles a closed
schema and decodes strictly before guest allocation. It rejects duplicate/unknown/
missing fields and unsupported numbers, normalizes record enumeration to schema
order, and creates owned values through validated host-free guest allocation/root
helpers. Preserve the schema/input/traversal budgets, exact roots, fuel/deadline
checks and system status. Tool Contract v1 signatures remain primitive.
The first HTTP/1 profile is implemented: one explicit entry-module serve
registration, exact net.listen IP:port grants, typed request/response handlers,
fresh bounded instances and an independent listener lifetime. Keep Hyper/Tokio
framing, header/body/connection bounds, host-policy conjunction before bind,
request trap isolation and owned shutdown. HTTP registration does not publish AI
tools. Stream runtime events through tooling rather than printing from libraries.
Runtime-only packaging is implemented: `varta-launcher` builds the execution-only
`varta-runtime` companion; `varta-artifact` owns portable/container I/O. Single builds
verify a private snapshot of the sibling/explicit trusted native launcher, check
identity/target/versions and validate payload without instantiation before writing
outputs. Keep probe byte/deadline bounds and unchanged container v1 layout. No
compiler/project/CLI dependencies may enter runtime distribution. New modules
declare `checked-i32-utf8-v1` in bounded strict `varta.language` metadata. Runtime
checks it before native compilation/instantiation; unknown/duplicate profiles and
invalid/duplicate host ABI sections fail. Legacy ABI v1 modules retain the same
semantics; raw Wasm without metadata is unspecified. Preserve that distinction in
inspection. Launcher protocol v2 negotiates supported profiles. The opt-in
`checked-f64-utf8-v1` profile uses host ABI v2 with f64 Number slots,
(f64,i32) captured Number results and existing rooted UTF-8 strings. Keep the typed
host signature registry, floating remainder edge cases, truthiness/short-circuit
evaluation and exact finite-i32 process-exit checks. Never reinterpret numbers as
roots or legacy aggregate slots. Value ABI v3 adds Float64 tag 6 in existing
eight-byte record/array slots; use bit reinterpretation, not numeric conversion.
Numeric fields/elements are never reference edges. Schema v2 and separate
json_stringify_v3/json_parse_v3 imports enforce the binary64 boundary; legacy
descriptors/schemas/imports retain their interpretation. Parsing validates finite
numbers and the complete closed schema before guest allocation and returns a full
i64 payload; serialization uses pinned ryu-js for ECMAScript number text, -0 -> 0
and non-finite -> null. Exact array indices are checked before conversion/access.
Tool/HTTP contracts still require future adapters; default i32 features remain.
The further opt-in `checked-f64-utf8-v2` profile uses host ABI v3/value ABI v4:
Undefined tag 7 has a canonical zero payload and is distinct from Null. Void
call expressions, `void` and explicit undefined values preserve effects once;
declared void functions still have no Wasm result. Root JSON undefined returns
undefined, object fields are omitted and array entries emit null. Validate even
omitted fields, count them against traversal budgets and reject nonzero payloads.
JSON v4 imports/schema v3 are exclusive to this profile. Strict typed JSON cannot
target undefined; no implicit null conversion or missing-property/union support
is supplied. Preserve lexical shadowing/TDZ, exact roots and all older profiles.
The UTF-16 profile `checked-f64-utf16-v1` uses host ABI v4/value ABI v5/schema v4.
Guest string handles retain byte lengths, with even UTF-16LE payloads including
lone surrogates. Keep code-unit length/charAt/charCodeAt, concatenation, lexical
comparison, exact roots and immediate host-result snapshots. Resource names must
convert losslessly and reject lone units/NUL before policy or OS access. New
encoding-specific imports retain the same capability IDs; old string imports
cannot be used under the new profile. JSON v5 escapes lone units and schema v4
parsing preserves them before allocating. Closed key/schema metadata stays UTF-8;
surrogate keys, dynamic unions and string bracket access remain unsupported.
Preserve both stored/converted byte limits, pure-operation system status and
all older profiles. Diagnostic replacement is display-only, never resource conversion.
The primitive-union profile `checked-f64-utf16-v2` uses host ABI v5/value ABI v6/
schema v5. Immutable 16-byte cells hold a primitive tag/count/payload; only String
members set the child-reference mask. Keep cell roots across calls/allocations,
never trace numeric bits, and validate canonical cells at every new host boundary.
Direct-local guards refine branch reads; reject writes under refinement so loops
cannot retain stale assumptions. Preserve source order, true short-circuit operand
values, strict Number/string equality and nullish-only loose equality. JSON v6
validates cells even when omitting undefined fields, counts logical values and
returns real String|Undefined at a union root. Strict parsing still rejects union
targets/fields. No object/array members, casts, any/unknown or uninitialized bindings
are admitted. Existing profiles, exact resource grants and pure-operation status
remain unchanged. Next expand this checked value foundation without claiming full JS.
The missing-value profile `checked-f64-utf16-v3` uses host ABI v6/value ABI v7/
schema v6. Every record appends presence/order metadata after its unchanged typed
slots. Optional primitive fields distinguish absence from explicit undefined;
newly present assignments append only after RHS success and overwrites retain
insertion position. Object.hasOwn is a pure record/literal-key intrinsic. Reject
optional inherited names, prototype reads, dynamic keys and reference-array reads
requiring unsupported unions. Numeric primitive/string indexing returns true
undefined for invalid/out-of-range numbers, without truncation; writes remain
bounded. Keep base/index roots across nested calls, exact String/Number payloads,
TDZ and undefined-capable let/return initialization. JSON v7 validates complete
presence/order/required/absent metadata and every omitted slot before publishing
output. Required typed parsing creates fully present trailers; optional/union
schemas remain rejected. Do not trace trailer words or change any older profile.
The reference-union profile `checked-f64-utf16-v4` uses host ABI v7/value ABI v8/
schema v7. Tag 9 holds immutable cells retaining Record/Array identity; tag 8 and
old imports remain primitive-only. At most one record shape and one array element
type per union are admitted. ReferenceBox performs bounded shallow validation;
JSON traversals retain complete cycle/depth/node/byte checks. Trace the actual
cell child bit (String/Record/Array), never numeric payload bits. Contextual literals,
optional references and nullable nested array reads preserve exact roots and aliases.
Typeof object includes Null/Record/Array; Array.isArray and null/undefined guards
narrow direct locals conservatively. Do not allow ambiguous shape casts or writes
under refinement. Versioned v2 value imports preserve status/permissions, and
JSON parse v8/schema v7 bind this profile; serializer v8 is also used by its
compatible successor. Strict JSON parsing in v4 remains
non-union/non-optional. No engine-to-tooling dependency or domain state is introduced.
The checked-JSON profile `checked-f64-utf16-v5` uses host ABI v8/value ABI v8/
schema v8 and pure json_parse_v9; serialization retains json_stringify_v8. Closed
primitive/reference unions select one member by JSON kind, never shape guessing
or coercion. Optional fields must admit undefined; absence clears presence and
stores canonical undefined, while required union fields remain required. JSON
never supplies undefined. Validate full input/schema/descriptors (including exact
optional flags) before guest allocation. Preserve backward DAG/unique member
constraints, legacy schema ceilings, input/traversal budgets, lone UTF-16 units,
finite Number bits, schema-normalized present-key order and zero unused order
entries. Parsed cells retain each actual reference child across further allocation;
keep allocator/retainer signatures and call graphs host-free. Older profiles keep
strict non-union/non-optional parsing. General JSON.parse/revivers, broader shape
unions and new-profile Tool/HTTP adapters remain pending.
The resizable-array profile `checked-f64-utf16-v6` uses host ABI v9/value ABI v9/
schema v9, v3 value operations and json_stringify_v9/json_parse_v10. Stable 16-byte
Array cells (heap flag 32) own one backing edge; typed backing blocks (flag 8)
carry live length and packed capacity. Grow geometrically within 1024 elements,
keep spare/removed slots zero, and validate kind/count/capacity/masks at host
boundaries. Push evaluates all arguments once before selecting the current target
length/backing, and completes allocation before publishing changes. Pop roots
returned children before boxing allocation. GC traces cell->backing->live slots,
never numeric bits or stale spare slots; resolve indexed backing after operand
side effects. Ten internal helpers include append/slice/search; pop and at are inline. Required JSON
helpers stay host-free, status/permissions/budgets persist, and old profiles retain
inline fixed arrays. Indexed writes still replace existing slots; writable length,
holes/indexed extension, spreads, constructors/iterators, broader shapes and
new-profile Tool/HTTP adapters remain pending.
The v6 profile also admits at(index?) and slice(start?,end?) over the same v9
representation, without new host imports/capabilities. Accept Number/Undefined
bounds and their union only; preserve argument effects once before selecting the
current length/backing. Normalize NaN/fractions/infinities and negative indices
before bounded integer conversion; keep bracket indexing semantics unchanged.
Slice always returns fresh storage/identity with shared child references, copies
only selected canonical slots and retains its backing before cell allocation.
Keep actual reference masks, zero spare slots, exact roots and pure system status.
Earlier profiles do not gain these methods; further array/prototype/coercion APIs
remain pending. Additive Wasm compiler operations do not reinterpret the ABI.
V6 search intrinsics includes/indexOf accept any checked needle and optional
Number/Undefined starting bounds. Unwrap actual tags before comparison: includes
uses SameValueZero, indexOf strict equality, UTF-16 strings use content and aggregate
references use underlying identity. Never coerce kinds or turn numeric bits into
roots/pointers. Preserve receiver/needle/start effects once before reading current
storage, and root detached needles through later argument allocations. The search
helper/comparator allocate nothing, preserve status and require no capability.
All ten helper dependencies remain typed/resolved; ABI/layout v9 is unchanged.
The primitive-exception profile `checked-f64-utf16-v7` uses host ABI v10 while
retaining value/schema v9 and existing host operations. Explicit primitive throw
and try/catch cross named calls via mutable pending/value globals and a reserved
8-byte root at the shadow-stack base. Root the cell before unwinding; callers
check pending before observing placeholder results or later effects. Restore the
handler's temporary cursor, clear abandoned scopes, root the catch binding and
then clear the channel. Resolve exception branches against all emitted controls,
including expression-level blocks. Preserve return/break/continue, guard rules,
Number bits, UTF-16 and system status. Require canonical initial/final channel
states, typed exports, value metadata and sufficient initial memory; forbid native
Wasm start before initialization. Uncaught values are owned typed errors with
bounded escaped diagnostics. Fatal budgets, checked JSON/index traps and authorized
exit bypass guest catch/finally. Finally uses one emitted body per region, rooted
saved exceptions/typed return pairs and explicit completion routing. Preserve
inside-out execution, finalizer overrides, lexical roots and loop/switch targets;
its own handlers are inactive during finalization. Error/aggregate throws and new-profile
Tool/HTTP adapters remain pending; no native EH feature/dependency is added.
The Error-object profile `checked-f64-utf16-v8` uses host ABI v11/value v10/schema
v10. Nominal Error tag 10/heap flag 64 owns one UTF-16 message edge, with exact
16-byte layout, own-message flag/count/mask and canonical absent zero. Constructors
accept only string/undefined, once; preserve identity, inherited default name,
non-enumerable message/presence, toString and instanceof guard narrowing. Error
fields are immutable in this increment. Catch admits Error plus primitives; root
cells/returns across calls/finalizers/collection and copy full owned uncaught data.
Require immutable exception.version=2 before effects; v7 rejects this extra export.
Use v4 value operations, value_is_error, json_stringify_v10 and json_parse_v11;
JSON schema v10 retains closed rules and cannot construct Error. Select vector
reader versions explicitly; earlier profiles must reject new tags/imports/layouts.
No new capability or fatal-to-catch conversion. Mutation/causes/native families,
subclasses/prototypes/stack traces and newer Tool/HTTP adapters remain pending.
The native-family profile `checked-f64-utf16-v9` uses host ABI v12/value v11/schema
v11. Error tag/layout retain their size, but the first word encodes own-message
bit 0 and a checked ErrorClass above it: Error/TypeError/RangeError/SyntaxError.
V10 readers retain class-0-only behavior. Keep source Error as the closed union
of all four classes, specific annotations/guards nominal, sibling types distinct,
name/toString dynamic and Object.hasOwn masked to presence only. Use v5 value
imports, checked value_is_error_class and json_stringify_v11/json_parse_v12.
Require exception.version_v3=3 in v9 and reject v8's selector (and vice versa),
checking its value before effects. Native uncaught errors own the class and full
UTF-16 message. Preserve roots/identity through calls, aggregates and finalizers.
This adds explicit constructors/throws, not automatic conversion of checked JSON,
index/resource traps or authorized exit. Other families/custom subclasses,
mutation/causes/prototypes/stack traces and new-profile Tool/HTTP adapters remain pending.
The recoverable-JSON profile `checked-f64-utf16-v10` uses host ABI v13 with unchanged
value/schema v11, Error channel/layouts and v5 value operations. Only json_parse_v13
turns declared input syntax/type failures into owned SyntaxError/TypeError cells.
Check complete grammar and whole-input budgets before schema matching; retain
typed limit state (including synthetic optional nodes), never classify by messages.
Keep malformed transport/schema/ABI/helpers, all budgets and authorized exit fatal.
Construct errors through validated guest helpers, recheck descriptors/channel and
deadline before publication, root/set value before pending and check pending at the
call site before result unpacking/effects. Never overwrite a pending channel or
expose partially decoded values. Messages are fixed, status/permissions persist,
and earlier profile/import semantics stay frozen. General JSON.parse, serializers,
other API errors and newer Tool/HTTP adapters remain further work.
String.repeat is additive in the v9 native-family profile and successors. Accept
Number/Undefined counts only; evaluate/root receiver before count exactly once.
Normalize NaN/undefined to zero and truncate fractions; negative normalized counts
and infinities use the existing RangeError channel even for empty receivers.
Use two shared activation-local i32 copy slots only after operand evaluation;
nested calls/repeats must not overwrite live outer operands. Check result bytes
before integer conversion/allocation, allocate once and bulk-copy doubling without
calls during copying. Preserve UTF-16/NULs, exact roots, status and all fatal
budgets. No new host import/helper/ABI version; earlier profiles reject repeat.
The function-value profile `checked-f64-utf16-v11` uses host ABI v14 with unchanged
value/schema v11 and preceding imports/channel. Exact invariant function types,
locals/arguments/returns/calls and capture-free arrows use 16-byte managed function
objects (heap flag 128, no child edges): signature/reserved0/code-index/reserved0.
Named references have lazy permanent canonical roots after the exception root;
subtract that prefix from frame capacity. Each arrow evaluation allocates fresh
identity. Root ordinary locals/operands/returns, never hidden scratch or numeric
metadata. Check object/block bounds, flags and semantic signatures before dispatch.
Use closed direct calls to compatible non-main user functions; no table/indirect
call exception may weaken JSON helper host-free analysis. Check pending before
result use. Bound semantic types (128), lifted functions (256), physical parameters
(32) and emitted body bytes (65536). Preserve status/grants/fatal limits, module
alias identity and source evaluation order. Captures, function expressions,
function-valued aggregates/unions/JSON/throws, generic/async/optional/rest/default
parameters and function properties remain unsupported. Design traced environments
and mutable binding lifetimes before enabling closures.
The lexical-closure profile `checked-f64-utf16-v12` uses host ABI v15 and unchanged
value/schema v11. V12 function bodies grow to 24 bytes (signature/fields2/code/
reserved0/env handle); flag 128 now traces bit1's environment edge. Environments
(flag 256) trace up to 32 shared binding-cell handles; binding cells (flag 512) trace
only actual reference payload bit0, never numeric metadata/status. Preserve full
VartaResult pairs, exact masks, canonical lengths and semantic cell types. Box only
captured slots; root incoming parameter payloads before boxing and capture cells
before allocation. All sibling/escaped/transitive closures share live cells;
separate activations and each loop let iteration own distinct cells. Rebind loop
cells before update after continue/finally, never on break/return/throw. Captured
catch cells must be rooted before clearing the exception channel. Scope exits
clear roots without erasing escaped cells. Trace and collect cyclic closure graphs.
Only lifted v12 arrows add a hidden environment parameter within 32 physical slots.
Validate environments/cells before user effects, preserve direct dispatch and
JSON helper purity, capability checks and fatal budgets. Captures before binding
initialization and under active refinements are unsupported. Reject narrowing of
mutable captured unions even when capture is discovered after the guard, so loop
back edges/callback aliases cannot invalidate types. Broader functions, TDZ/
ReferenceError, function-valued aggregates/JSON and newer Tool/HTTP adapters remain
pending. V11 layout/collector semantics stay frozen.
Array map/filter/forEach/find are additive compiler operations in v12, without
new imports/helpers/ABI. Emit ordinary typed wrappers and FunctionCall dispatch;
wrappers count toward function/body budgets. Evaluate receiver/callback once,
snapshot initial length, read current backing at each turn and root pre-callback
elements/results across mutation/collection. Filter/forEach skip removed slots;
find visits them as Undefined and requires element|Undefined callback values. Use
exact 0..3 prefixes of value/index/receiver. Inline arrows infer parameters and
expression results; unannotated callback blocks infer typed return sites and
closed unions, including bare/fallthrough Undefined (see below). Map/filter return fresh arrays; filter/find preserve pre-call
values and aliases, forEach discards results. Void map results become Undefined.
Preserve checked truthiness, empty-array callback validation, exceptions/finally,
status, grants, helper purity and fatal budgets. Map traps fatally on a missing
visited index until sparse array presence is implemented; never silently compact
or fill holes. Typed thisArg is implemented below; borrowed/prototype APIs remain pending.
The sparse-array profile checked-f64-utf16-v13 uses host ABI v16/value v12/schema
v12. Stable vector cells retain their layout; backing bytes append a 128-byte
presence bitmap after geometric capacity slots, with allocated|array|sparse flags
(1|8|1024). Require exact packed lengths/kinds/counts/masks, zero absent/spare
payloads and zero bits beyond length. Locate the bitmap by packed backing length,
never recycled allocation size. GC traces only live payloads; bitmap bytes are not
edges. Keep old v9-v11 vector readers dense and version-bound. New v6 value imports,
value_is_error_class_v2 and json_stringify_v12/json_parse_v14 select the new readers.
JSON counts/render holes as null and parses fully present arrays; whole-input/schema
validation and recoverable syntax/type errors retain their existing boundaries.
Map allocates its final-length output once and fills present input indices only;
filter/forEach skip holes, find visits Undefined. Preserve snapshot values/aliases,
live backing lookup, exact roots and pending exceptions. Sparse literals and
Object.hasOwn Number/literal keys distinguish holes from explicit Undefined; reads,
at, slice, pop/push, search and indexed filling preserve bits. Growth copies the
bitmap before publication; clear removed payload/bit before count reduction and
root a popped reference before boxing. New presence helpers are host-free and
bounded; v13 allows 12 helpers, older profiles retain 10. Keep capabilities/status
and fatal budgets intact. Delete, extension, writable length, constructors and
prototypes remain unsupported. No source arrays may enter the exception channel.
The array-mutation profile checked-f64-utf16-v14 uses host ABI v17 with unchanged
value/schema v12 and preceding imports/layouts. Indexed writes evaluate/root base,
key and RHS once before numeric-domain checks and resize. Preserve original base/
key across RHS reassignment, resolve current storage afterward and publish only
after RHS success. Length supports Number and literal ["length"]; invalid uint32
numbers raise RangeError (Invalid array length), -0 is zero, valid lengths above
1024 remain fatal resource violations. ArrayResize is the thirteenth host-free
helper: allocate/zero/copy before growth publication; shrink clears removed values
and bits before both counts; preserve zero spare data and aliases. Caller roots
retain base and reference RHS across allocation. Array-only delete preserves
length, clears a present payload/bit without allocation and returns true for
absence; numeric/literal keys only. Strict deletion of length raises TypeError
(Cannot delete array length). No coercive/general property writes or deletion.
Preserve old profile semantics/ceilings, fatal budgets, helper purity and grants.
Array.reduce/some/every are additive in v12 and successors without ABI/import/
helper changes. Evaluate receiver/callback/seed once in source order before the
length snapshot; skip live holes/removals and preserve pre-call value roots.
Some/every use checked truthiness with immediate canonical Boolean short circuits
and false/true for empty arrays. Reduce uses exact accumulator types from a seed
or the element type, a 0..4 (accumulator,value,index,array) parameter prefix, and
contextual arrow returns. A distinct initialization flag selects the first present
entry without a callback when no seed exists; explicit Undefined is a seed. Empty/
all-hole unseeded arrays throw owned TypeError (Reduce of empty array with no
initial value), never read an uninitialized accumulator. Keep seed/current/old
accumulator roots across allocations and update only after callback success;
retain multi-slot result and function accumulators. Preserve existing function/
helper purity checks, status/grants and all fatal resource/exit boundaries.
Callback block inference is additive in v12 and successors: lower an unannotated
Array callback body once with deferred return checking, then collect only its own
statement returns, join checked types and widen each return. Nested arrows do not
contribute; analysis is conservative over all sites/finalizer overrides. Bare or
possible normal exits add Undefined when there are value returns; otherwise retain
void. Defer finally saved-return locals until the inferred result is known and
scope/root their exact types, including physical pairs. Explicit annotations and
ordinary arrow defaults stay unchanged. Seeded reduce uses an annotated first
callback parameter (or return annotation), then a resolved function signature,
then surrounding aggregate literal context; otherwise infer the seed normally.
Resolve/lower each operand once, recursively context nested literals, reject exact
type mismatches and unsupported unions. A wider outer scalar result alone must
not widen a scalar accumulator. Keep source order, limits, helper purity, status,
grants and all exception/fatal boundaries. No ABI/import/helper changes.
Anonymous function expressions and explicit typed this are additive in v12+.
Use common lifting/return inference/captures with a new ordinary-function this
boundary; never inherit outer this into an ordinary expression. Parser this_param
is separate from positional params. Expected function contexts can type this;
otherwise its read requires annotation. Keep receiver mode in semantic function
types and dispatch target matching, even for equal physical Wasm signatures.
A typed receiver is the first ordinary rooted physical parameter, with the
existing environment last; count both against slot limits. Arrow this remains
lexical via capture cells. Bare calls and reduce supply Undefined; fn.call and
six Array callback methods supply a checked explicit receiver. Evaluate callee/
receiver/arguments once in order; ignored thisArg expressions still execute, and
helper/effect traversal must include them. Preserve full-width discards, exact
roots through GC/finalizers, status/grants and fatal limits. Named expressions
and declaration this are implemented below. Implicit method/thisArg inference, constructors, arguments,
bind/apply and wider function syntax remain unsupported. No ABI/import/helper or
managed layout change. Do not silently erase unsupported receiver contracts.
Named expressions in v12+ require an annotated/contextual result and create a
private immutable self binding behind parameter/body scope. Logical function types
exclude the private self parameter; dispatch passes the actual callee handle after
source parameters and before environment only to named-expression targets. Root
it on entry; validate function metadata/signature/exact code/environment identity
before allocations/user effects. Nested closures capture it through ordinary
cells; preserve alias identity, scope shadowing, physical result pairs, GC and
exception/finalizer paths. Count self/receiver/environment toward slot bounds.
No new cycle/layout/helper/import/ABI is required. Outer-variable forward capture
and recursive initialization remain unsupported. Named declarations now retain
a typed receiver in shared signatures, logical references and physical parameters;
bare calls supply checked Undefined and explicit call/Array paths use checked this.
Canonical identity/import aliases and escaped lexical this stay intact. Keep main
and legacy Tool/HTTP entry contracts receiver-free; old profiles reject declaration
this. Preserve helper purity, actual grants and fatal recursion/resource limits.
Optional/default/rest parameters are additive in v12+. Track exact optional/rest
modes in logical function types/dispatch. A default before a later required slot
is required Undefined-capable; optional ? cannot precede a required slot. Normalize
arguments once: check/widen fixed values, pad optional tails with Undefined, pack
a fresh bounded dense rest array. All supplied arguments precede default effects.
Separate incoming ABI slots from source body bindings for extended signatures;
root inputs before allocation, initialize bindings left to right, then clear only
incoming roots after transfer. Defaults trigger only on Undefined, see initialized
prior parameters/this/self/outer bindings, and reject own/later parameter reads or
captures before initialization. Body declarations stay outside initializer scope.
Keep managed capture cells, result-pair widths, GC/finalizers/exception paths and
fatal limits. Array callbacks use actual implicit arguments, checked optional/rest
modes and missing-tail defaults; reduce's result preserves accumulator context.
Optional/default Function or physical-result unions remain unsupported.
V12+ checked spread calls snapshot each narrowed array at its source
position before later operands, preserving child identity and mapping holes to
Undefined. Root callee/receiver/segments and expand into typed private slots/rest;
keep Function/result pairs outside aggregates. Validate shared flattened cap 1024
with a fatal trap, and dynamic arity/types with owned TypeError before defaults/
body. Preserve explicit call receiver order, exact dispatch and existing physical
slot/function/body/resource bounds. Pure guest Undefined tests avoid per-element
host calls; actual effects stay visible to helper validation and real grants.
String/general iterables, spread thisArg, host/builtin/Array method spreads and
aggregate spread literals remain unsupported. Legacy Tool/HTTP entries and main
keep their contracts. No host operation, heap layout, helper or ABI change;
preserve grants and status.
V12+ parameter patterns retain aggregate call signatures and initialize recursive
closed record/array leaves in source order. Collect all BoundNames in TDZ first;
reject duplicates, own/later reads/captures and dynamic computed keys. Support
static aliases/literals, nested defaults, elisions and final rest; object rest ends
in an identifier. Root source/default temporaries, release them only after leaf
transfer and retain normal shared cells/self/this/result pairs. Nullish aggregate
reads throw owned TypeError before later bindings/body; resource traps/exit stay
fatal. Array iteration reads live backing/length, remembers exhaustion and creates
dense rest with explicit Undefined for sparse holes. Object rest shallow-copies
remaining fields, then remaps live presence/order without allocation/user code;
keep all 32 bits and zero unused order words. Pattern depth/nodes use shared
64/1024 limits alongside parser/body/frame/resource budgets. Pure tag/metadata
operations preserve status; actual effects stay visible to helper analysis and
real grants. No layout/import/helper/ABI change. Local declaration/assignment/
catch patterns, primitive boxing, strings/custom iterables, array-as-object
patterns and broader keys/prototypes remain unsupported.
Artifact I/O checks manifest/code ABI agreement. Next implement the remaining
value/ABI design under K4; a further breaking profile requires a new host ABI so old runtimes fail
even if they ignore its custom section. Do not claim full ECMAScript support from
a profile name or silently reinterpret old tool/HTTP/JSON boundaries.
A developer must eventually be able
to implement a JSON server in TS/JS using Varta APIs without writing a separate
host wrapper. Generic JSON/network/event-loop primitives belong to the engine;
routes, payload schemas and business rules belong to applications. Implement the
actual compiler/runtime mechanisms before creating a separate application project.
General async support is not automatically a prerequisite for a bounded host-driven
server loop with checked named handlers. API/ABI details must be specified first.

Broader Application Contracts, shared application state, artifact trust and MCP
remain planned. MCP must not displace basic application-core capabilities.
Ordinary source exports must not automatically become
AI tools; external callers enter the same authorized command/query path as UI/API
callers, and application state must outlive agent connections.

Keep Varta application-agnostic. Domain entities, schemas, commands, validation
rules and application-specific permissions belong to separate TS/JS applications,
not to engine crates or built-in CLI profiles. The document editor is a standalone
reference application, not an engine feature. Core state/lifecycle/transaction
APIs must work with application-defined contracts; do not add document/title/
rename models or fixed application roles to varta-common, runtime or project.
Tests may use domain examples as fixtures without turning them into public APIs.

The target is a self-contained JS/TS runtime in the Deno/Bun category, retaining
the TS/JS → Wasm → Varta execution path. The current small subset is a development
stage, not a permanent product limit. General JS compatibility is not implemented.
Grow language, GC, scheduler, standard APIs and module/package support through an
explicit ECMAScript compatibility profile. Record the i32/UTF-8 MVP deviations;
do not freeze them into the future JS value model or silently change old artifacts.
Preserve versioned ABI/contract behavior, correct standard semantics and clear
unsupported-feature diagnostics while implementing each increment. A working
HTTP example alone is not evidence of a complete runtime.

## 2. Repository layout

```text
Cargo.toml               # workspace only; crate metadata lives in crates/*/
crates/
  varta-common/          # engine contracts, ABI, capability model and shared types
  varta-compiler/        # explicit TS module graph (SWC) -> Wasm; no runtime dependency
  varta-runtime/         # Wasmtime host: budgets, capability enforcement, host APIs
  varta-artifact/        # portable artifact/container I/O; common + wasmparser, no VM/compiler
  varta-launcher/        # execution-only `varta-runtime` binary; no source tooling
  varta-project/         # tooling: project config/loader, check/dev/build, verified packaging
  varta-cli/             # the `varta` binary; thin orchestration, no domain logic
  varta-common/src/varta.d.ts # one owner for embedded host declarations
docs/
  architecture.md        # system spec — update when ABI/capabilities change
  development-plan.md    # phases and exit criteria
.github/workflows/       # CI mirrors the commands below
```

Placement rules:

- Engine crates are `varta-common`, `varta-compiler` and `varta-runtime`; artifact
  and launcher crates supply the independent distribution layer. They must
  not depend on project tooling/CLI, including through test dependencies. Runtime
  production dependencies must not include the compiler. Artifact/launcher
  production dependencies must not include compiler/SWC/project/CLI/clap. Project configuration
  (`VartaConfig`, `CapabilitySpec`) belongs to `varta-project`, not common. CLI
  production code depends on `varta-project`; backend details stay in runtime.
  The boundary regression test and the CI core-only build enforce this direction.
- New crates go in `crates/<name>/` and are added to workspace `members` and, if user-facing, `default-members`.
- Domain logic (parsing, emission, capability matching, host I/O, artifact packaging and validation) never lives in `varta-cli`. The CLI calls varta-project and prints results.
- Shared types and errors belong in `varta-common`. Compiler and runtime must not re-declare them.
- A `lib.rs` that grows past a few hundred lines is split into modules by concern (e.g. `host.rs`, `expr.rs`, `glob.rs`). Module-private helpers stay private; the crate API surface stays small and documented.
- Unit tests live in `#[cfg(test)] mod tests` at the bottom of the module they test; cross-crate behavior goes in `crates/*/tests/`.

## 3. Commands and quality gates

All three gates must pass before every commit, and they are run frequently during work — not only at the end:

```bash
cargo fmt --all -- --check              # formatting; plain `cargo fmt --all` fixes
cargo check -p varta-common -p varta-compiler -p varta-runtime -p varta-artifact --lib --locked
cargo build -p varta-launcher --bin varta-runtime --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets --locked
cargo test --workspace
cargo test -p varta --test smoke        # CLI end-to-end tests
```

CI runs exactly this set. If clippy fails, fix the code, not the lint; `cargo clippy --fix --allow-dirty` may apply the mechanical part, but review the result as code. Report failures honestly with output; never claim tests pass without running them.

Useful during development:

```bash
cargo check                             # fast type-check, no codegen
cargo run --bin varta -- <command>       # workspace also builds varta-runtime
cargo test <name> -- --nocapture        # one test with stdout visible
cargo test -- --test-threads=1          # when tests share state
```

## 4. Rust conventions

- Edition 2024. Match the style of the surrounding code: naming, comment density, `std::collections::BTreeMap` for deterministic ordering, `slice::from_ref` for single-element slices.
- **No magic numbers or strings.** Named constants, enum-driven tables, or struct-resolved indices — always. The Wasm function-index bug produced by a literal `Call(1)` (fixed after it called the wrong import) is the canonical anti-example of this repository: host-import indices must only ever be resolved through the typed `HostOperation` registry and emission tables.
- **Error handling.** Library crates expose typed errors derived with `thiserror` and return `Result<T, TypedError>` from public APIs — never `anyhow` or `String` in a public signature. The `varta` binary may use `anyhow` with `.context()` for orchestration-level failures. The compiler, runtime and project service expose typed error enums. Never `unwrap()`/`expect()` on fallible external input (files, user config, Wasm memory, network); `.unwrap()` is acceptable only in tests and on provably-infallible cases (say which in a comment). Mark important returns `#[must_use]`.
- **No `println!` in library crates** except when explicitly required by an embedding. Runtime output is captured with a budget and returned to the CLI. Debug output uses `tracing`. Leftover debug prints (e.g. a `DBG_…` line) are release blockers.
- Prefer the standard library and the existing dependency set. Adding a dependency requires justification in the PR description; pin exact versions in `[workspace.dependencies]` and reference them with `.workspace = true`.
- Keep functions focused; extract helpers instead of nesting match arms past three levels. Delete dead code and redundant `let _ =` bindings rather than silencing them.
- Workspace code forbids unsafe. Never deserialize native Wasmtime payloads from package input; only validated portable Wasm is accepted.

## 5. Compiler rules (`varta-compiler`)

- The compiler emits Wasm directly with `wasm-encoder` and is the only crate allowed to do so.
- Source graph I/O belongs to a host-supplied loader, never the compiler. Keep deterministic dependency ordering, graph budgets and per-file scopes. Project dependencies must stay within the config directory and use pinned no-follow descriptors; do not replace these with ambient check-then-read paths.
- Every language feature is a deliberate, documented subset decision. Unsupported constructs fail with a clear `unsupported …` error naming the construct — never silently miscompile.
- Static data (string literals) goes into the data section with a deduplicated offset table; the 64 KiB single-page limit is enforced with a proper error.
- String values use immutable packed i64 slices; complete value types determine user signatures and locals. Host ptr/len results must be snapshotted into the bounded execution arena immediately, before any later expression can reuse the host buffer. Never expose a borrowed result buffer as a retained variable/argument/return value. Reclamation or new string operations require an explicit ownership/budget contract.
- String `+` and content comparisons evaluate both operands once in source order. Preserve UTF-16 code-unit ordering over valid UTF-8, including supplementary/BMP boundaries; never compare slice handles as content. Concatenations share the checked heap and 1 MiB per-value limit, and string helpers must not change host status or bypass capability checks on constructed inputs.
- Checked JSON parsing validates the entire input before calling guest helpers. Schema types use bounded backward references; check each static record descriptor against its schema. Allocator/retainer exports must have checked signatures and no reachable host/indirect calls, independently of the presence of a tool catalog. Copy input into owned checked values before callbacks can grow memory; keep every child rooted until the parent is published. Failed parsing must expose no partially built result, and scalar success clears the shared result length.
- Closed objects and dense arrays share the managed heap and exact roots with strings. Trace reference fields through every live record and every reference element of arrays before sweeping (array tracing must not truncate at 32 elements), retain collection kind/reference metadata across collection, and root bases/operands across nested allocation. Keep versioned static descriptors, canonical field slots and runtime JSON bounds checks consistent. Pure serialization preserves host status, requires no capability and still consumes host/output/traversal budgets. No unchecked raw pointer may cross the external JSON boundary.
- The nonmoving string collector relies on exact shadow roots. Protect operands/arguments across nested allocation, update binding roots only after evaluating the RHS, clear lexical roots on every normal/branch exit, and transfer return roots without an intervening allocation. Keep root-stack and heap metadata outside static/host buffers, include their cost in memory/fuel limits, and never reclaim a live alias or move its bytes.
- `VartaResult<T>` is an immutable payload/status pair. Capture only a direct privileged host call, once, and read status before any later host operation. Preserve both physical slots through locals, parameters and returns; compute every local index after type expansion, including switch temporaries. Result field reads must evaluate the base once. Do not turn execution-budget traps or authorized guest exit into recoverable host statuses.
- Wasm type/import/local indices are computed, never written as literals. Host imports are declared conditionally on use; any emitted `Call` must go through the resolved typed operation index.
- Locals: every index the generated code touches (temporaries for `&&`/`||`, switch discriminants, etc.) must be declared. Off-by-one local declarations produce modules that pass our compile step but trap in Wasmtime — always validate.
- Emitted modules must be verified: a compiler change requires a test that instantiates the module with `wasmtime::Module::new` (see `varta-runtime` tests), not just one that checks byte emission.
- Parse and semantic errors must carry the real source identity and innermost available range through the shared source context. Preserve typed category codes and expected/actual types; do not format coordinates into ad-hoc strings or replace a nested diagnostic with an outer span. Project commands use the path-aware compiler APIs. JSON diagnostics use one-based Unicode scalar columns and zero-based UTF-8 offsets, with exclusive range ends.

## 6. Runtime and security rules (`varta-runtime`, `varta-common`)

Security invariants — violating any of these is a bug regardless of feature intent:

1. **Deny by default.** An operation with no matching capability grant fails before any OS call. Empty patterns deny. Omitted namespaces deny.
2. **Check before effect.** Capability validation happens before opening files, sockets, or reading env — the denied path must be observable and tested.
3. **Match the operation, not the namespace.** A grant for `env.has` must not authorize `env.get`; match the full `namespace.operation` id, not a `starts_with` prefix.
4. **Validate all Wasm-derived input.** Every `ptr`/`len` pair from guest memory is bounds-checked, length-capped, and validated for the selected UTF-8/UTF-16 ABI before use. UTF-16 resource names must convert exactly and reject lone surrogates/NUL before any effect. Path inputs reject traversal (`..`), match component-aware scopes, and open through pinned directory descriptors with no-follow at every component. Never reintroduce check-then-open ambient paths.
5. **Explicit grants are the only authority.** Runtime enforcement uses package requests and, when supplied, the separately validated host ceiling. Every concrete resource must satisfy both; empty host grants never trigger fallback. Filesystem authority must be pinned from the host's roots, not package aliases. Nothing is ambient, and no integer from guest memory is ever treated as a capability.

Host-API housekeeping: adding a host function touches, in the same change: the shared `HostOperation` registry and grant schema in varta-common, AST-to-IR lowering, the runtime linker handler, `crates/varta-common/src/varta.d.ts`, and positive/denial/ABI regression tests. Artifact validation uses that same registry.

HTTP-specific invariants: bind only after exact package/host net.listen checks;
port 0 means explicitly authorized ephemeral binding, never a wildcard grant.
Each request owns a fresh store; do not call main again or reset one trapping
instance. Require the varta.http metadata/feature-import pair, validate adapter
signatures and actual effects, and keep copy/retain helpers host-free. Readiness
and logging observers are trusted synchronous sinks and must return promptly.
Keep one bounded active connection/worker, no unbounded task queue, parser/body/
connection deadlines and response framing controlled by the HTTP library. No
business routes, authentication policy or example payloads belong in runtime.

## 7. Testing requirements

- Write the test in the same change as the code it covers — never after the fact.
- Every compiler feature gets: a compile-and-validate test (Wasmtime instantiation) and a behavior test through `varta dev` where practical.
- Every capability rule gets a denial regression test in `crates/varta-cli/tests/smoke.rs` (positive and negative path).
- Smoke tests use temp directories and clean up; never touch the repo tree.
- No test may depend on network, environment variables beyond ones it sets, or execution order.
- Build the runtime companion before standalone CLI smoke tests. Workspace default
  members build both user binaries; launcher integration tests also request the
  runtime binary so a clean workspace test run covers it.

## 8. Documentation

- Changes to the ABI (imports, globals, memory layout, ownership), the capability model, or the config schema require a matching update to `docs/architecture.md` in the same commit.
- New CLI flags update the subcommand doc-comment (visible in `--help`) and the smoke coverage.
- Commit messages: imperative, one line for the what, optional body for the why.

## 9. Things this codebase considers done wrong (do not repeat)

- Literal Wasm import indices in emitted code (see §4).
- Native payload deserialization, marker scanning inside arbitrary file bytes, or implicit true grants.
- Bypassing the shared semantic/type/bytecode pipeline in a CLI command.
- `starts_with("env.")`-style capability matching (see §6.3).
- Debug prints left in host functions.
- New host imports missing from `validate`, the `.d.ts` template, or the denial tests.
- `String`-typed errors in new public APIs.
