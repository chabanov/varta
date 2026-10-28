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
Direct-local guards refine branch reads; earlier profiles reject writes under refinement so loops
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
Object.hasOwn Number/String keys distinguish holes from explicit Undefined; reads,
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
real grants. No layout/import/helper/ABI change. Primitive boxing, strings/custom
iterables, array-as-object patterns and broader keys/prototypes remain unsupported.
The narrower catch object pattern contract is specified below.
V12+ local const/let patterns reuse the shared binding mechanism. Predeclare all
block leaves in TDZ, infer or contextually check one RHS, initialize in source
order and preserve const/let mutability, shared capture cells, shadowing and roots.
For let clones captured leaves before the first test and before later updates;
initializer closures keep their original cells. Const has no per-iteration
rebinding. Preserve continue/finally update routing and break/return/throw cleanup.
Shared pattern limits also bound each lexical declaration (64 depth/1024 nodes);
parameter limits keep aliases. Actual effects remain visible to helper validation
and real grants/status. No ABI/layout/helper/import change. Destructuring requires
an initializer; var/ambient, top-level, catch and for-in stay unsupported.
V12+ destructuring assignment statements update initialized mutable locals/
parameters/captured cells. Validate shared pattern limits and target mutability/
initialization/guards without declaring names. Root one RHS, traverse live source
and write in order with explicit Assign mode/Set; never reinitialize captured
cells. Repeated targets/default reads are allowed; prior writes survive later
throws. Preserve exact/union type checks and pure Undefined narrowing with owned
TypeError before the failing write. Defaults use target context; opaque pairs/
functions stay outside aggregate tags. Retain existing roots/rest/status/permissions/
helper purity/fatal limits. Parenthesized statements and ordinary for init/update
are admitted; expression values/chains and member targets are supported below.
Dynamic record keys, catch and for-in remain unsupported. No ABI/helper/import/layout change.
V12+ destructuring assignment values retain the actual contextual RHS type and
original identity/fields/holes. Save one RHS in a hidden rooted scope, traverse
Assign mode and publish it only on success. Propagate result context and allow
nested chains/arguments/returns/callbacks/admitted expression positions. Embedded
statement cleanup restores a saved temporary floor, preserving older operands;
nested floors compose and handler scopes cover RHS evaluation. Load the result
before clearing its hidden root without allocation; use normal expression/return
retention afterward. Visitors must traverse both RHS/body for layouts/effects.
Traversal cannot contain source returns/finalizers; lifted functions remain
separate. Preserve partial writes, exceptions/fatal limits, cells/pairs/status/
actual grants and all bounds. No ABI/import/helper/layout change.
V12+ assignment patterns admit checked record-field/Number array-element targets,
including default/nested/rest leaves. Initialization patterns still bind names.
Prepare receiver/key once before source GetV/iterator step/default; rest prepares
before copy, nested leaves only after acquiring their containing source. Root
prepared references across allocation/reassignment and commit through ordinary
setters against saved receiver/key and current backing. Do not re-lower references
after defaults. Const children are mutable; target types/default context/Undefined
checks and optional presence/order remain invariant. V12/v13 preserve indexed
bounds, v14 adds extension/length targets and existing RangeError/fatal policies.
References/defaults after a throw are skipped, prior writes survive, successful
expressions return their original RHS. Actual receiver/key/default effects stay
visible to helper-purity validation and grants/host ceilings. Preserve pattern/
source/frame/resource bounds, GC/pairs/floors and old profiles. No ABI/helper change.
V12+ intrinsic array for-of supports const/let binding heads and checked existing
identifier/member/pattern assignments. Predeclare lexical head TDZ before one
rooted RHS; reject incompatible sources and throw owned TypeError for nullable
array absence before head/body. Test live length/backing; v12 reads dense element
types, v13+ visits holes as Undefined. Save current value before assignment head
references/defaults. Reuse Scope/For IR with per-step current/leaf scopes; Init
creates fresh cells for both captured let/const each iteration. Assignments reuse
existing cells/setters. Preserve break/continue/switch/finally/return/throw cleanup,
GC/pairs/callback inference, status, actual grants, all bounds and old profiles.
Source reassignment cannot redirect iteration; growth/shrink/deletion remain live.
No new ABI/import/helper/layout or iterator objects/hooks. For-in/await-of, var/
using, strings/custom iterables remain unsupported. Empty sources need
type context; head type annotations remain a parser error.
V12+ do-while owns body/test IR and runs the body first. Emit break block, loop
header and continue block so continue reaches the trailing condition and break/
return/throw skip it after applicable finalizers. Clear condition temporaries
before the repeat branch. Lower body scope before the condition; captured body
bindings initialize fresh cells while outer bindings stay shared. Completion
analysis propagates body returns/throws and permits normal exit only for normal/
break/continue body paths; callback inference must visit do bodies and recognize
guaranteed first returns. Preserve finalizer overrides, GC/pairs/saved returns,
truthiness, actual body/test effects, grants/status and all fatal budgets. Earlier
profiles remain rejected; labels are supported below. No ABI/helper/import/layout change.
V12+ statement labels have unique per-function control IDs, active duplicate/
unknown/continue-kind checks and the existing nesting budget. Reset state across
functions/wrappers; names do not declare bindings. Label chains immediately around
loops share iteration targets; named break also admits non-loop statements. Emit
unbreakable label wrapper blocks and bind continues to actual loop targets; take
pending names before source for/for-of initializers so generated rest loops cannot
steal them. Route resolved block indices through existing finalizers/cleanup;
unlabeled branches remain nearest-loop/switch. Retain named branch sets through
inner loops/switches in completion analysis, consume only matching IDs and preserve
finalizer overrides. Never mistake an outer jump for a guaranteed return. Traverse
label bodies for layouts/effects/callbacks. Preserve captures/GC/pairs/status/grants,
fatal limits and all older profiles; no ABI/import/helper/layout change.
V12+ Object.keys accepts one unshadowed closed record or nullable single-record
union; reject unsupported sources/spreads/generics/arity/old profiles. Root input
once and throw owned TypeError for nullish union absence. Enumerate present index
keys numerically then other names by insertion, preserving explicit Undefined and
all 32 bits; never serialize/read payloads. Use a typed internal value block with
saved temporary floor, registered input/output roots before RHS and success loading
before cleanup. Visitors traverse its body for layouts/helpers/effects; callback
inference uses the resolved type and excludes internal non-return statements.
Fresh dense String arrays use UTF-16 literals, existing capacity/layout/helpers;
compact keys, zero removed payloads/bits then publish both lengths. No source
aliasing or resurrected slots. Preserve caller operands, GC/pairs/finalizers,
status/actual grants and all bounds; no ABI/import/helper/layout change.
V12+ Object.values shares typed record enumeration with Object.keys. Derive the
homogeneous result element through existing union interning; preserve exact field
kinds, optional Undefined, unique record/array union limits and no opaque fields.
Empty records return Undefined[]. Outer nullability throws, not an element widening.
Prepare each field once into rooted cached slots before output allocation; entries
in compaction are literals/locals only, with no calls/boxing/user code. Keep cache,
source/output and earlier operand/pair roots across allocations. Order by presence
and JS keys, pack numeric bits exactly and retain shallow reference identity. Clear
spare payloads/bits then publish both lengths with no allocation in between. Generic
RecordEnumerate collectors/effect visitors traverse entries; no source-return sites
enter internal value blocks. Preserve GC/finalizers/status/grants, all bounds and
old profiles. No ABI/import/helper-export/layout change.
V12+ Object.keys/Object.values also accept bounded arrays or nullable single-array
unions; mixed record/array inputs remain rejected. Keys are decimal String[] in
ascending own-index order, omitting length. Values preserve the exact invariant
element type; sparse holes are skipped without widening and explicit Undefined is
present. ArrayEnumerate selects private host-free helpers. Snapshot length/backing
once, reserve zeroed geometric capacity with live length zero, root backing before
cell allocation and root cell before decimal-key allocations. Publish payload,
presence and both lengths without allocating between them; GC sees only canonical
completed prefixes. Preserve child masks/numeric bits, shallow identity and zero
spare slots/bits. Format indices directly as bounded UTF-16 decimal, never through
host output/JSON/coercion. Save/restore helper temporary roots on success, preserving
earlier operands/pairs/finalizers and bounded repeated calls. Keep effect/layout
visitors, nullable errors, exact grants/status and all budgets/old profiles intact.
No host import, helper export or ABI/layout change.
V12+ catch object patterns expose static name/message keys with identifier leaves,
aliases/defaults and empty patterns over the complete primitive/Error channel.
Reject arrays/rest/other keys/annotations/dynamic keys/aggregate throws and older
profiles; do not infer a narrower thrown type from a try body. Predeclare all
BoundNames in TDZ before defaults, initialize in source order, preserve mutability
and fresh capture cells, and resolve defaults before handler-body declarations.
Transfer the original union to a hidden exact root before channel clear, include
it in handler cleanup scopes, then reject nullish values before defaults/body.
Non-null primitives yield Undefined; Errors yield existing family-aware fields.
Use pure internal IsError tag tests to guard Error payload access and existing
String|Undefined widening/defaults. Binding failures escape the inactive original
handler and route through outer handlers/finalizers. Traverse all defaults/layouts/
effects and preserve callback completion, GC/partial bindings/returns/pairs/status/
grants/fatal limits. No new host operation/helper export or ABI/layout change.
V12+ String.slice/substring accept narrowed String receivers and zero to two
Number/Undefined bounds only. Evaluate receiver/start/end once in order; retain
prior operands across nested effects, exceptions and allocations. Lower shared
slice receivers once before choosing array/string dispatch; preserve existing
Array.slice and older profiles. Typed StringRange selects private guest helpers.
NaN -> zero, truncate fractions, default start zero/end length, clamp before
integer conversion. Slice resolves negatives relative to length and empties
reversed ranges; substring clamps negatives to zero and swaps reversed bounds.
Copy exact UTF-16 units/NULs/lone surrogates into one owned nonempty result; never
return an interior GC pointer. Empty results are canonical after argument effects.
Keep collector roots, physical pairs/returns/finalizers, full effect visitors,
status/grants and all byte/memory/fuel/deadline limits; bulk-copy exhaustion is
fatal. Reject coercion/borrowed/computed/generic/spread/extra args and nullable
receivers without narrowing. No host operation/helper export or ABI/layout change.
V12+ String.includes/startsWith/endsWith require narrowed String receivers and
String needles, with optional Number/Undefined positions. Evaluate/root receiver,
needle and position once in order. Includes shares dispatch with Array.includes:
lower its receiver once and preserve array SameValueZero/holes/relative bounds and
old profiles. Default positions to zero except endsWith uses length. NaN -> zero,
truncate fractions and clamp 0..UTF-16 length before integer conversion; negatives
are not relative. Empty needles match after all effects. Reject candidate ranges
before subtraction and compare exact UTF-16 units, never unaligned bytes. Private
StringSearch helpers allocate/call nothing and return canonical booleans; keep
full operand/effect visitors, GC/pairs/saved results/finalizers/status/grants.
Initial includes uses bounded candidate scanning (quadratic worst case), with
existing fuel/deadlines fatal on adversarial repetition; prefix/suffix are linear
in needle length. No coercion/regex/omitted needle/borrowed/computed/generic/spread/
extra arguments or nullable receivers without narrowing. Existing limits and ABI/
heap/helper exports/host operations remain unchanged.
V12+ Object.hasOwn(record,key) accepts dynamic narrowed String keys for narrowed
closed records. Evaluate/root record then key once and preserve key effects before
presence reads, original receiver identity, explicit Undefined versus absence,
empty-shape effects, unknown/inherited names and no payload/prototype/JSON access.
String literal lookup may return false for unrepresentable names or __proto__
without enabling prototype initializers. Numeric literals and old profiles retain
their rules; reject coercion/dynamic non-String or union keys, nullable receivers
without narrowing, symbols, generic/spread calls
and shadowed Object. Lower through a rooted Boolean value block and a balanced
decision tree sorted by UTF-16 units, not UTF-8 metadata; preserve original slots
including bit 31. Use existing comparisons/presence loads and full value-block
visitors. Preserve earlier operands/pairs, GC/returns/finalizers/status/grants,
static-data and all execution/type/frame/root limits. No host operation/helper
export or managed/schema/ABI layout change.
V12+ Error own keys also admit dynamic narrowed Strings on Error families/only-Error
unions; root source/key, compare message then mask own-message independently of
class bits. Literal Strings keep direct reads/false for unknown/unrepresentable
names; name is inherited. V11 and earlier retain literal-only Error keys. V13+
array String own keys recognize length or canonical ASCII decimal indices, no
leading zeros/sign/fraction/exponent/coercion. Reject overlong names before scanning
at most the maximum bounded index's digit width, then reuse ArrayPresent bounds/
bitmap. Keep explicit Undefined versus hole, current length, original identity and
key mutation/throws/GC. Numeric array checks and older profile boundaries persist.
Visit both operands in collection/effects; private helpers stay allocation/host-free.
No ABI/layout/import/helper-export change.

V12+ computed record reads require a narrowed closed record and String key. Join
all field types with Undefined under existing exact union limits; reject multiple
record shapes/array types without erasure. Root receiver then key once, initialize
Undefined after key success, dispatch the shared balanced UTF-16 tree and test
presence before the selected payload read/widen. Keep exact numeric bits and
reference identity; observe key mutations on the original receiver, never a later
source-binding reassignment. Unknown/absent non-inherited fields return Undefined;
unshadowed inherited names raise owned TypeError after key effects rather than
fabricating missing values. Share the inherited-name table with static rejection;
declared required own fields win. Preserve literal paths/exact types, older profiles,
Array numeric access, union allocations, GC/earlier operands/pairs/saved returns,
callback inference, effect/layout visitors, catch/finally cleanup/status/grants and
all fatal bounds. Reject coercion/non-String/union keys, nullable sources without
narrowing, dynamic pattern targets/deletion and unrepresentable unions. No host
operation/import/helper export or ABI/schema/layout change.

V12+ simple computed record writes evaluate/root source, narrowed String key and
RHS once in that order. Capture receiver/key before RHS effects, then lookup/check/
store after success; keep earlier mutations even when the requested store fails.
Reject impossible RHS types at their span, functions/result pairs, non-String/
unnarrowed union keys and nullish receivers without narrowing. Contextualize
aggregate literals only for one unambiguous corresponding type, never a merged
field union. Per leaf intersect full static types, preserving record shapes,
invariant arrays and exact Error families. Partial union checks use host-free
IsUnionMember tags plus exact Error class bits after matching the Error tag; only
then unbox/retype/widen and SetField. Unknown/inherited non-own names and bad field
values throw TypeError after RHS effects, with no requested payload/presence/order
mutation. Keep own inherited-looking fields, optional Undefined presence/order,
all 32 bits and literal/legacy/Array rules. Scope roots retain all inputs/child
graphs through GC, exceptions, saved returns/pairs and finalizers; visitors traverse
member-check operands and the full statement body. Computed expression values are
specified below, as are dynamic-key arithmetic compounds and Number updates. Reject
dynamic pattern targets/deletion/property creation/coercion/prototypes. Preserve status/grants and
all fatal bounds; no host operation/import/helper export or ABI/schema/layout change.

V12+ computed record assignment expressions reuse ValueBlock for dynamic keys and
literal String/Number keys, returning the cached original RHS only after success.
Keep literal static field/type/unknown-key errors and dynamic field checks; require
narrowed records/String keys. Contextualize aggregates through target rules, never
pre-widen the cached RHS or narrow its result from a runtime field choice. Widen/
unbox only the storage copy; preserve original union members, exact scalar bits,
Error families and aggregate identity/types. Prepare outer source/key before inner
references and final RHS; store inner to outer. Inner failure skips outer work;
outer failure keeps inner stores. Save/restore temporary floors before operands so
internal statements cannot clear earlier caller operands/pairs. Root all prepared
inputs/children and original outputs through nested GC/calls/returns/finalizers;
load results before cleanup without allocation. Statements use the same dynamic
value path and discard normally. Internal bodies contain no source returns, and
existing value-block visitors/inference must see complete effects/layouts and the
original result type. Support standard enclosing expression positions, including
receivers/keys and chains with destructuring expressions. Ordinary simple values
and dynamic-key arithmetic compounds/Number updates are specified below; Error
mutation and older profiles stay rejected.
Keep presence/order/literal/legacy rules, status/grants and fatal bounds; no host
operation/import/helper export, emitter helper or ABI/schema/layout change.

V12+ simple assignment values share prepared references/setters with destructuring
for mutable initialized local/parameter bindings, static record fields and numeric
array elements. Resolve locals/captures once; cache/root receiver/raw index before
RHS, never an interior backing pointer. After RHS success validate/store against
the original reference/current backing. Keep const/TDZ/guard-refinement and complete
invariant type checks. Cache original RHS values/types in ValueBlock; use non-union
context for normal aggregate/arrow inference and only aggregate-literal context
for target unions. Widen storage copies only. Preserve raw bits/identity, local
function signatures/cells and both slots of VartaResult (including String roots).
Indices validate after RHS: finite integral 0..1023, -0 -> zero; bad indices stay
fatal. V12/v13 require current in-range indices, v14 may extend with holes/current
bitmap/backing. V14 length/literal length expressions retain owned RangeError for
invalid uint32 and fatal >1024 limits, returning original Number/-0. Older profiles
keep their length rules. Shared floors/root transfer and cleanup preserve earlier
operands/pairs, mixed partial chains, GC/escaped functions, callbacks and finalizers.
All layout/effect visitors must traverse prepared bodies. Error/result fields,
unknown record fields and coercive/String array indices remain unsupported.
Arithmetic compounds are specified below. Keep status/grants/fatal bounds and ABI/import/helper exports.

V12+ +=, -=, *=, /= and %= reuse prepared local/static-field/numeric-array references;
v14 also admits length; dynamic record keys are specified next. Reject compound destructuring,
Error/result mutation and old
profiles. Read/cache the old value after receiver/key effects but before RHS.
Do not re-read after RHS changes the same target/backing; use current storage only
for the final store. MaybeIndex snapshots sparse/missing/invalid numeric reads as
Undefined. Evaluate RHS before operand validation, so missing/invalid old values
retain RHS effects or its original exception. Static successful cases are
Number/Number for all five ops and String/String for +=; reject statically
impossible pairs, otherwise check exact union tags with pure IsUnionMember before
unboxing/Binary computation. No coercion or ordinary binary-union rule change.
Cache/return computed results, widening storage copies only. Operand TypeError
must not store, create fields or convert resource traps into catchable errors.
Valid results retain current array-write and length checks, numeric bits/UTF-16
String ownership, status/grants and all fatal bounds. Nested value-block roots and
floors retain prepared sources/keys, old/RHS/result graphs, earlier pairs and saved
returns through GC/partial chains/calls/finalizers. Visit every operand/storage/
failure branch for effects/layouts; callback inference uses computed result types.
Reuse existing remainder/concat/setters; no new host operation, emitter helper,
helper export or ABI/schema/layout change.

V12+ dynamic record arithmetic compounds require narrowed record/String operands
and the existing computed-read union admission (all fields plus Undefined, at most
one record shape/array element type; no functions/results). Keep literal static
checks. Root/cache receiver and exact UTF-16 key once, then read/cache the selected
old value before RHS. Key effects precede the read; RHS cannot redirect the saved
reference or replace its old value. Unknown ordinary keys/absent fields read
Undefined and fail arithmetic after RHS effects; undeclared inherited names fail
during the read before RHS, while declared own names work. Preserve original RHS
exceptions and mutations, never create a property or coerce. Reuse arithmetic
guards/result typing and full selected-field write checks, widening only storage
copies after success. Nested blocks/floors retain original receiver/key/old/RHS/
result graphs, earlier pairs and saved returns through GC/calls/finalizers/partial
chains. Visitors must see all read, operand and storage branches, including host
effects in source/key/RHS. Preserve status/grants/ceilings and all existing
compilation/value/fatal bounds. No host/import/helper export or ABI/layout change.

V12+ &&=, ||= and ??= share prepared references for mutable initialized locals/
parameters, static/dynamic record fields and numeric arrays; v14 admits length.
Cache receiver/key and old value once before the branch. &&= selects truthy, ||=
selects falsy and ??= selects only Null/Undefined. Records/arrays/functions/Errors
are truthy; primitive/union truth follows the existing exact checked rules.
Evaluate RHS and perform the checked store only in the selected branch. A skip
returns the old value without write validation or payload/presence/order changes;
selected success returns the original RHS, widening only storage copies. Result
types join RHS with possible skipped members: &&= excludes always-truthy kinds,
||=/??= exclude Null/Undefined. Never narrow partially truthy scalar kinds or RHS
types from runtime choices. Static targets require full RHS compatibility even
when skipped; dynamic record leaves retain complete checked field types and read
union restrictions. Unknown ordinary keys skip &&= as Undefined but selected
writes fail after RHS; inherited non-own names fail at the read before RHS.
Array MaybeIndex captures missing/invalid reads; skip writes entirely, otherwise
validate against current post-RHS backing/presence/length with existing profile/
fatal rules. Length setters keep RangeError/budget behavior. Prepared reference
identity, old values and original RHS bits/UTF-16/graphs/functions survive nested
chains, GC/calls/callbacks/returns/finalizers through existing blocks/floors/roots.
Keep original exceptions/effects and completed inner stores. Const/TDZ/guarded
writes, result-pair targets, Error/result fields, coercive keys, logical patterns
and older profiles stay rejected. Function bindings retain signature/arrow
context/capture identity. All body visitors must include the selected RHS/store
even if runtime skips; helper validation must see every source/key/RHS host effect.
Use existing pure truth/nullish tests and checked setters; preserve status/grants/
ceilings and compilation/value/fatal bounds. No new host operation/import, emitter
helper, helper export or ABI/schema/layout version.

V12+ prefix/postfix ++/-- require an actual Number in mutable initialized locals/
parameters, static/dynamic record fields or numeric array elements; v14 adds length.
Parenthesized simple targets work. Prepare/root receiver/key once, read once,
validate Number before unboxing/computation/storage, and compute old +/- 1 with
existing binary64 Binary IR. Static types without Number fail at the target span;
union/non-Number or missing values throw owned TypeError without a requested store.
Never coerce String/Boolean/null/Undefined/references/Errors or turn resource traps
into catchable operand failures. Dynamic keys retain read-union/UTF-16 rules and
full checked field stores; inherited non-own names fail at read and no properties
are created. Array MaybeIndex absence/holes/invalid numbers fail before any write
attempt, including in v14; no indexed extension from an update. Final storage uses
the prepared original reference/current storage and existing presence/length/fatal
checks. Only after success, prefix returns the new Number and postfix the cached
old Number, preserving -0/NaN/infinity/large-number bits. Failed stores expose
neither result and skip later work, retaining original exceptions/earlier effects
and completed updates. Const/TDZ/guarded bindings, nonnumeric/function/result-pair
targets, Error/result mutation, coercive keys and old profiles stay rejected.
Captured bindings/fresh loop cells and callback result inference keep existing
rules. Nested blocks/floors/root transfer retain inputs/old union graphs/earlier
operands/pairs/saved returns through GC/calls/finalizers. Effect/layout visitors see
all source/key/read/check/store branches; pure helper controls must pass while
host effects in either source/key fail validation. Preserve status/grants/ceilings
and all compilation/value/fatal bounds. No new host operation/import, emitter
helper, helper export or ABI/schema/layout version.

V12+ &, |, ^, ~, <<, >> and >>> require already narrowed Float64 Number operands.
Reject other types/unnarrowed unions at their operand span and older profiles;
bitwise compounds are specified next. Do not add ToNumber coercion or weaken ordinary union rules.
Use Binary IR and shared numeric operator classification; lower ~ as xor with -1.
Evaluate both binary operands once in source order before conversion. ToUint32
truncates finite fractions/modulo 2^32; nonfinite/either zero becomes positive
zero. Reinterpret the binary64 exponent/significand for bounded shifts, masking
fraction bits and restoring the sign modulo 2^32; do not use saturating/trapping
float casts or host remainder. Magnitudes below one and exponents >=84 yield zero;
subnormal/NaN/infinity cases take the zero branch. Apply i32 operations, with Wasm
shift-count low-five-bit masking and signed conversion back to f64 except >>>'s
unsigned conversion. No negative-zero result or new integer value type.
Reuse declared scratch locals only after all user operand evaluation, with no
calls/allocations in conversion; never trace numeric bits as pointers or clobber
an earlier/nested operand, graph or physical pair. Existing visitors must see
both operands and unary source effects. Preserve original exceptions, status,
grants/ceilings and fatal bounds. Exhaustive exponent/seeded-payload tests compare
an independent truncation/remainder oracle in fresh bounded stores, plus runtime
Wasm validation, nested/GC/helper-purity/source-range/module/artifact/denial tests.
No new host operation/import, emitter helper export, schema or ABI/layout version;
existing compilation/code/root/memory/fuel/deadline limits stay active.

V12+ &=, |=, ^=, <<=, >>= and >>>= reuse numeric compound preparation/storage for
mutable initialized locals/parameters, static/dynamic record fields and numeric
arrays; v14 admits length. Cache receiver/key and old value once before RHS.
Evaluate RHS once, then require actual Number tags for both cached operands before
unboxing/conversion/computation. Static types without a possible Number pair fail
at target/RHS spans; closed unions otherwise throw owned TypeError after RHS on
incompatible kinds. Do not coerce or change plain bitwise narrowing rules. Return
the computed Float64 Number, widening only the storage copy; preserve unsigned
>>> results and positive zero using existing inline ToInt32/ToUint32 conversion.
Dynamic keys retain read-union/UTF-16/inherited timing and full selected-field
checks. Missing/unknown/invalid numeric reads fail after RHS without attempted
writes or property creation, retaining original RHS exceptions/effects. Valid
results retain current storage/profile/length/fatal bounds, including v14 regrowth
after RHS and RangeError/non-uint32 versus fatal >1024 length results. Nested
chains retain completed inner writes; roots/floors retain original source/key/
old/RHS graphs, earlier pairs and saved returns through GC/calls/finalizers.
Keep const/TDZ/guarded/function/result/Error-field/coercive-key/old-profile rules.
Visitors must see all source/key/RHS/check/store effects; pure helper controls
pass while any operand host effect remains detectable. Preserve status/grants/
ceilings and all budgets. No new IR kind, host operation/import, helper export or
ABI/schema/layout version.

The Number-power profile checked-f64-utf16-v15 selects host ABI v18 and inherits
all v14 features/value ABI/schema v12/import selectors. Default remains i32; old
profiles keep rejecting `**`/`**=`. New pure FloatPower/number_power takes (f64,f64)
and returns f64, minimum ABI18, with no capability/result buffer/status reset.
Compiler plain ** requires narrowed Number operands at their spans; compound **=
uses prepared references and Number-pair guards after RHS, preserving original
old/RHS graphs, partial stores, current backing/length rules and computed Number
results. Keep standard exponentiation associativity/unary-base syntax via parsing.
Runtime Number::exponentiate checks exponent NaN, either exponent zero, base NaN,
signed infinity/zero with odd integral exponent signs, infinite exponent |base|
rules and negative finite/nonintegral domain before abs(base).powf(exponent).
Restore negative signs only for odd integral powers. Preserve signed underflow,
NaN**0=1, 1**NaN=NaN and (±1)**(±Infinity)=NaN. Finite ordinary power is platform
implementation-approximated, not cross-platform bit-identical. Do not add BigInt,
ToNumber coercion or custom exponent loops proportional to exponent.
Charge one existing host operation, check host/deadline before and after native
math without a second charge, preserve status/permissions and fatal budgets.
Shared registry names/signatures/minimum ABI must drive compiler/runtime/artifact
validation. New language/ABI18 metadata is mandatory even without an active power
import; downgrade/profile/signature/unknown-import mismatches fail before effects.
V15 originally negotiated maximum18/value12; current protocol v2 also advertises
v16/v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27/v28/v29 with maximum28/value12. Packaging
requires a matching rebuilt trusted companion. Keep old artifact execution and
value/schema/container layouts. Host-free JSON allocation/retain helper validation
still rejects any reachable math host call. Test special-case bits/parity/subnormals,
finite sqrt accuracy, one-call accounting, narrowed types/old-profile rejection,
prepared effects/GC/pairs/finalizers, inherited sparse/JSON/closure behavior,
hostile metadata, source ranges, modules/artifacts, grants/status and fatal limits.

V15 also admits direct unshadowed Math.abs/floor/ceil/trunc/round/sqrt with exactly
one narrowed Number argument, via typed Math IR and guest instructions. Recurse
through arguments in collection/effect visitors; preserve effects/exceptions,
earlier roots/pairs and saved returns through GC/finalizers. Reject old profiles,
coercion/unnarrowed unions, missing/extra/spread/generic args, shadowing including
captures/imports/TDZ, computed calls, aliases and other methods.
Use f64 abs/floor/ceil/trunc/sqrt; preserve nonfinite/signed-zero rules. Round uses
cached x and nearest(x), adding one only for x-nearest(x)==0.5; this fixes lower-even
ties toward +Infinity without floor(x+.5) errors. Scratch is used after argument
completion so nested calls cannot corrupt it. No new host import, allowance,
permission, helper export, dependency, ABI/layout/container version or limit.
Math.pow takes two narrowed Numbers and uses the existing FloatPower import and
its special-case/approximation/status/one-call/deadline rules. Collection must
register that operation when traversing MathIntrinsic::Pow; host-free JSON helper
validation must reject it. No new import name/profile/ABI/capability is added.
Typed Math IR now holds a mode and argument vector. Sign/fround/clz32 take one
narrowed Number, imul exactly two and min/max zero or more. Sign preserves NaN/±0;
fround uses f64->f32->f64 ties-to-even; clz32 reuses exact ToUint32 then i32.clz.
Imul evaluates both inputs before scratch use, converts to Uint32, multiplies i32
and converts the signed result to f64; never multiply f64 first. Min/max fold from
+Infinity/-Infinity using Wasm min/max, propagate NaN and choose -0/+0 for ties.
Keep the accumulator on the Wasm stack across argument calls/GC, evaluate every
argument after NaN, and preserve abrupt completion. No new host calls or budgets.
Pure JSON helper trees may use these guest operations; argument host effects must
still fail helper validation. Existing tool/HTTP binary64 adapter gates persist.
Test exact finite bits, all exponents/representative mantissas, half neighbors,
special values, nesting/narrowing, roots/pairs/finalizers, argument permissions/
status/fatal limits, source ranges, modules and source-free Wasm/native artifacts.

V15 Math.f16round is a guest-only unary Number intrinsic. Round f64 directly to
binary16 ties-to-even, not through f32. Cache the fully evaluated argument; for
abs(x)<2^-14 emit nearest(x*2^24)*2^-24, otherwise for abs(x)<65520 extract exponent
e and emit nearest(x*2^(10-e))*2^(e-10). Construct scale powers exactly from bounded
exponent bits. The other branch x*Infinity propagates NaN and gives signed overflow/
infinity. Preserve signed-zero underflow. No host/helper/ABI/limit/local-type change.
Test independent integer rounding over every binary16 encoding/midpoint/neighbor,
all f64 exponents, double-rounding counterexamples, nesting/GC/pairs, pure helpers,
type/arity/old-profile gates and source-free/permission/error paths.

V15 Math.hypot takes zero or more narrowed Numbers, cached once as absolute values
in hidden f64 locals before any special-value selection. Infinity wins over NaN;
otherwise NaN propagates and empty/all-zero calls return +0. Lower to existing
ValueBlock statements and unique scalar work slots, preserving temporary floors,
earlier pairs/roots and exception/finalizer behavior. Internal BinadeScale Math IR
clears significand bits and clamps to MIN_NORMAL; never expose it as a source API.
Scale by that exact power of two, use Dekker split square errors and compensated
summation, refine the root with its squared-product residual, and rescale. Do not
square original inputs or use max*sqrt(sum) rounding near overflow. No host, heap
array, helper export, dependency, ABI or limit change. Keep finite approximation
explicit. Verify exponents/representative bits, exact scaled triples/subnormals,
compensated small terms, high-precision overflow goldens (native libm can be wrong),
all-argument effects after Inf/NaN, pure helpers, GC/pairs and artifact/grant paths.

V15 Number.isFinite/isNaN/isInteger/isSafeInteger accept any admitted value and
unnarrowed unions without coercion. Missing/non-Number first arguments return
false; all ignored extra arguments still evaluate once in source order. Preserve
their exceptions, grants/status, pair widths, GC and fatal limits. Check union
Float64 tags before loading payload bits. Finite is abs(x)<Infinity; NaN is x!=x;
integer adds trunc(x)==x, safe integer uses abs(x)<=9007199254740991 and integrality.
Compute the immutable first Number/type result before ignored args and keep its
canonical Boolean on the Wasm stack; later scratch/binding changes cannot corrupt
it. No local type refinement. Traverse every typed NumberPredicate IR argument.
Reject old profiles, shadowing, computed/borrowed/generic/spread calls and other
Number methods. No host/import/helper/ABI/layout/permission/budget change.
Test all exponents, safe bounds, non-Number/union tags, missing/extras/throwing
arguments, pairs/GC/finalizers, helper purity, source-free modules/artifacts and
permission/status/fatal boundaries; keep existing tool/HTTP adapter restrictions.

V15 also admits eight Math and eight Number numeric constant properties through
unshadowed namespace identifiers (parentheses allowed), dot or literal String-key
reads (parentheses allowed). Use exact f64 values; Number.MIN_VALUE is bits 1,
safe bounds ±9007199254740991. Fold to existing Float64 IR, never host calls or
allocations. Lexical records/captures/functions/TDZ named Math/Number take precedence
and retain ordinary property behavior. All write/update/delete/pattern paths use
the shared member-base check for a read-only type diagnostic at the member span.
Reject old-profile built-ins, dynamic namespace keys, aliases/values/descriptors
and unknown properties; preserve real shadowed reads/writes in all profiles.
Test exact finite bits/special values, both syntaxes, shadow/TDZ, every write path,
host-free helpers/budgets/status, dependency ranges and source-free/public APIs.
Existing numeric logging text is retained; JSON uses the pinned ECMAScript formatter.

V24 closure summaries now include symbolic callback inputs (owner/index) as well
as writes/unknown. Register Function ABI slots before initialization and remap them
to source bindings after extended parameter prologues; explicit Function this is
physical input0. Const parameter copies inherit origins through captures. Validate
called inputs against committed direct writes and nested captured writes across
the completed lexical body; a replacement anywhere falls back to unknown. Merely
capturing a parameter is not a store. This relies on closed binding cells/no dynamic
eval or external environment mutation; revisit that proof as facilities expand.
Keep direct_writes separate from writes revisions: symbolic calls invalidate facts
locally but do not invent captured writes in a summary. Propagate call_parameters
through known callee summaries, binding own positions to actual arguments and
retaining external lexical origins until their defining invocation. Unproved or
higher-order callback arguments remain conservative. Module declarations still
bound all supplied callbacks; no declaration-order-dependent summary lookup.
Borrowed AST effects use corresponding .call/receiver positions and inline callback
effects; share64 proof depth through nested traversal, never reset that budget.
Keep source complexity rejection, mutable/returned/self fallback, argument ordering,
physical slots/cells, exact roots, GC, status/permissions and fatal limits intact.
See stable-callback-parameter architecture contract. Precise higher-order argument
binding, point-sensitive mutable provenance and property/alias effects remain open.

V24 now tracks compiler-only call provenance/effects. call_effects stores a lifted
closure's external binding writes, symbolic inputs and unknown-reentry flag; call_writes propagates
transitive effects even when the intermediary captures only a function. Exclude
callee-local writes from caller summaries. known_calls uses immutable binding
identities for const FunctionRefs/aliases/conditional choices and inherits them in
children; mutable/returned/self functions and unproved parameter inputs stay unknown.
Module declarations have no caller environment in the admitted model, so caller
cell effects come only from supplied function values. Classify actual bound arguments;
opaque callbacks use unknown, not an empty effect. Function-valued aggregates remain
unsupported. Revisit this proof when mutable globals, prototypes/hooks, dynamic
coercion, callback retention/reentry or suspension enter the model.
Apply effects after arguments, never resurrect argument-invalidated facts. Use the
same summaries for borrowed AST loop/test effects where lexical identity is known;
shadows remain conservative. .call and stable spread targets share provenance.
Native Number/String/Error and callback-free array member intrinsics do not reenter
binding cells; array/record contents may mutate and need separate future property
invalidation. Memoize visited callback-reachability types; existing64 proof depth
falls back to unknown. No new dependency or physical IR/ABI/cell representation.
Preserve exact roots, cached callee/args, status/pending, GC and fatal budgets.
See proven-call-effects architecture contract. Higher-order callback-argument binding, returned/mutable-function provenance
and property/alias tracking remain unfinished.

V24 now supports guarded writes using per-slot writes revisions and flow snapshots.
Increment revisions for committed stores (all assignment families/pattern heads)
and conservative loop/call effect invalidation, not reference preparation. Evaluate RHS under current facts, retain the observed type
before widening, and replace target facts with a strict subset of its declared
union. Physical slots/captures keep declared types. flow_alternate restores
mutually exclusive branch input; flow_join unions normal-path facts using the
reachability/target analysis. Missing facts mean the full declared upper bound.
flow_restore filters saved facts by unchanged revisions for optional RHS/defaults.
Never resurrect a written slot. Logical targets take the selected result type;
other RHS/default effects remain conditional. Switch tests lower before bodies;
direct entries join fallthrough. Track reachable break/continue snapshots by loop/switch/label target, joining
normal fallthrough, false tests and matching exits. Literal true/omitted for tests
have no false edge; literal false bodies have no backedge. For initializers run
once before repeated-only effect analysis. Do not let their facts leak into a
second test after mutation. For-of caches its RHS before repeated head/body effects
and retains an empty/exhausted edge. Finally transforms only its own pending exit
region by actual writes plus guaranteed normal facts; no normal completion cancels
old exits but keeps new overriding branches. Nested finalizers transform in unwind
order. Never consume an outer named continue in an inner loop/switch. Unreachable
source is still checked but contributes no exits. Handler/finalizer prefix input,
loop fixed points/first-iteration splitting, initializer and call/property inference
remain unfinished. See completion-targeted architecture contract.
flow_effects scans borrowed AST parts for potential writes/calls with lexical
shadows before repeated code; literal truth analysis is bounded at64;
for body lowers before update in24, old profiles retain prior order. Guard tests
block their own effect targets. Proven calls invalidate matching identities;
unknown calls invalidate mutable cells. The native whitelist is valid only for
current non-reentrant checked APIs, revisit when callbacks/hooks
expand. Mutable captures use full declared types/fresh checks; immutable const proofs
propagate via binding identities/outer_narrows and existing cells, including transitive
captures. No physical layout change. Keep old23-and-earlier readonly guards. The
compiler-only pinned swc_ecma_visit29.0.0 handles AST traversal with64 depth/fact-drop
fallback and no runtime dependency/limit change. Preserve saved args/returns/pairs,
roots/status/pending/fatal budgets. Next improve exception/call/property precision.
See write-aware refinement/capture architecture contract.

The truth-guard profile checked-f64-utf16-v24 inherits v23 with shared ABI26/
value12/schema12. Add no host registry/ABI constant; launcher current maximum28
retains a new supported profile entry. Rebuild/verify both companions; unknown profiles
and missing language/mismatched ABI fail, while compatible26/profile23 bytecode
cannot prove source provenance. Direct local Guard::Truth(yes) matches potential
truth classes, not a complemented filter: true excludes Null/Undefined, false
excludes always-true refs; keep Number/String/Boolean in both. Nullish guard matches
Null/Undefined. Parens/! recurse; && true combines both true guards, || false both
false; opposite compound outcomes stay conservative. RHS && uses true left, ||
false left, ?? nullish left; restore refinement map after lowering even on error.
While/for bodies use true test facts; body/continue effects filter update facts.
Normal if/ternary joins combine facts; loops join false tests/matching breaks and
continue-to-update/test inputs. Optional RHS exits filter changed incoming facts. Do-while
first body unguarded. Skip redundant/empty filters without new capture marks in24.
Write/capture invalidation now prevents stale assumptions; v23 truth mutation
admission stays unchanged. Refine exit/call/property effect precision next. Keep exact roots, boxed member checks,
pairs/status/pending/saved returns, fatal budgets and source-free contracts. See
truth-guard/shared-ABI26 architecture section.

V23 control flow now uses truth()->ToBoolean for every ordinary value; unary !,
loops/ternaries, logical assignments and existing array predicates share it. Reject
opaque pairs without fields. guest_logical intercepts &&/||/?? before legacy union
handling: cache left once in rooted ValueBlock, pure ToBoolean or IsNullish test,
selected branch only. Remove impossible saved-left members (OR/nullish exclude
Null/Undefined, AND excludes always-true refs), narrow payload inside selected
branch, join RHS only if types differ. Known literal or uniform type outcomes preserve left
effects while choosing branch; selected Fn values avoid unsupported Fn unions.
Keep full Number bits/ref identity and roots/input/output/pairs/saved returns through
GC/finalizers. Selected boxing may allocate; tests themselves no host/allocation.
Old profiles/artifact imports unchanged, no ABI26/value12/schema12 change. Source
both operands still checked; shape/Fn-union/opaque constraints remain. JSON helpers
allow guest control flow, reject reachable RHS hosts. V24 adds direct truth guards and RHS
type narrowing; v25 adds checked Array.findIndex/findLast/findLastIndex.
See guest truth/control-flow architecture contract.

V23 also admits direct Boolean/isFinite/isNaN via primitive_builtins flag, with
unchanged ABI26/value12/schema12, no registry or launcher change. Normal global
bindings/closures/TDZ win. ValueBlock caches first input before all ignored effects,
including full pairs. Boolean accepts ordinary admitted values/unions, rejects
opaque pairs without explicit fields. Missing/Undefined/Null/false/zero/NaN/empty
String false; all references true without ToPrimitive. Typed ToBoolean IR emits
canonical guest i32: F64abs>0, packed String length, direct refs1, union actual
Float/String/Bool payloads or nullish/ref tags. No host/allocation/allowance; keep
collect/tools visitors recursive. V23 condition/logical/unary integration now shares guest ToBoolean. Global numeric predicates require primitive-only first types, convert
non-F64 via existing PrimitiveNumber (1host), then NumberPredicate; numeric/missing
guest-only, missingNaN. Root cached refs/cells across extras, retain pairs/status/
pending/saved returns/finalizers and fatal guest/host limits. JSON helpers allow
pure Boolean, reject native coercion. No wrappers/borrowed/computed/generic/spread
built-ins. See Boolean/global predicate architecture contract.

The numeric-prefix profile checked-f64-utf16-v23 selects ABI26/value12/schema12,
inheriting v22. Direct parseInt/parseFloat and Number static forms cache admitted
primitive inputs/radix before all extras. Global normal bindings/TDZ win; Number
requires unshadowed namespace spelling. Host NumberParseInt(i64,i64)->f64 and
NumberParseFloat(i64)->f64 require26, no cap/result globals/buffer. PrimitiveBox
validates cells; input ToString before radix ToNumber/Int32. String borrowed exact
UTF16, Number canonical (-0 ->0), primitive words. Missing input guestNaN/no import,
missing integer radix boxedUndefined ->0. Int32 truncate/mod2^32, nonfinite0.
Leading exact whitespace only. Integer radix0/default10/optional0x, explicit16
optional0x, other2..36 plain digits; longest prefix/no digits or invalidbaseNaN.
Exact BigUint nearest/even, <=1024 value bits then early infinity; scratch<=1030bits.
Float signed Infinity or decimal longest prefix, incomplete exponent rollback,
grammar checked before Rust parse, scratch<=half1MiB. Preserve -0 from text,
status/pending/buffers/roots/pairs/saved returns/finalizers and fatal limits. Tick1,
post-check, host-free JSON helpers reject imports. No wrapper/object ToPrimitive,
first-class built-ins/computed/spread/generic forms. Mandatory26/profile23 metadata;
protocol2 maximum28/value12 includes v23, rebuild both companions. See parser contract.

The Number-precision profile checked-f64-utf16-v22 selects ABI25/value12/schema12,
inheriting v21. Direct narrowed Number toFixed/toExponential/toPrecision cache
receiver and precision before all ignored effects, then primitive Number conversion
and truncate/NaN0. Undefined/missing fixed0, exponential/precision shortest.
Fixed/exponential0..100, precision1..100; Fixed validates BEFORE nonfinite receiver,
others convert then return nonfinite BEFORE range checking. Box/widen RangeError
through exception_type. New NumberToFixed/Exponential/Precision registry imports
(f64,f64)->i32 require25, no cap, existing result globals+owned Snapshot. Raw host
precision canonical integer/Inf, -1 shortest sentinel only for Exp/Precision;
NaN/fraction fatal. Fixed pinned ryu-js, significant digits exact bounded BigUint
ratio decimal rounding ties upward, carry then notation. Bound324 normalizations,
<2500-bit scratch/123ASCII bytes. Tick once, check after; preserve status/pending,
roots/pairs/saved returns/finalizers and fatal limits; JSON helpers reject calls.
valueOf guest-only cached Number identity with all ignored effects, exact -0 and
zero host charges. No wrapper/general prototype/borrowed/computed/generic/spread
calls. Mandatory matching25 metadata even unused; protocol2 maximum28/value12
includes v22/v23, rebuild both companions. See Number precision architecture contract.

The Number-radix profile checked-f64-utf16-v21 selects ABI24/value12/schema12,
inheriting v20. NumberToString / number_to_string (f64,f64)->i32, minimum24,
no capability, returns_string/needs_result_globals true. Direct narrowed Number
receivers enter number_to_string via existing toString dispatch; Error path stays.
ValueBlock caches receiver/radix before ignored extras; Undefined default10,
other primitives Number-convert then truncate/NaN0. Outside2..36 throw RangeError
BEFORE zero/nonfinite special cases; box Error via exception_type before S::Throw.
Preserve receiver/arg throws, roots/GC, pairs/status and saved returns/finalizers.
Native decimal uses canonical_display; other bases exact BigUint midpoint bounds
scaled by2^1075, even endpoints closed/odd open, shortest precision/nearest-even.
BigUint0.4.8 was locked via compiler, now direct runtime dependency. Bound1074
normalizations/55precision steps/<2500-bit scratch/1077text bytes; no new limits.
Tick once, post-transfer check, existing UTF16 result buffer+owned Snapshot; radix
coercion may add existing Number host call. Raw invalid host radix/limits fatal,
language RangeError catchable. Status/pending preserved, JSON helpers reject calls.
No wrappers/general prototypes/borrowing/computed/spread/generic calls or other
Number methods. Mandatory ABI24 even unused; protocol2 maximum28/value12 includes
v21 and requires both rebuilt companions. See Number radix architecture contract.

The primitive-Number profile checked-f64-utf16-v20 selects ABI23/value12/schema12,
inheriting v19. PrimitiveNumber / primitive_to_number (i64)->f64, minimum23, no
capability/results/buffer. PrimitiveBox validates cells; Number inputs retain
values/zero sign, Null0, Boolean0/1, UndefinedNaN, no args guest+0/no import. Shared
primitive_conversion Lower prepares/caches first before ignored extras, then calls
String/Number host as selected; retain all roots/effects/pairs/finalizers/TDZ.
Numeric String parser trims exact ECMAScript whitespace, checks entire ASCII
syntax before Rust decimal conversion and supports unsigned 0x/0b/0o prefixes.
Power-of-two prefixes retain 53 bits+guard/sticky and round once nearest/even;
scan invalid suffixes even after overflow. No arbitrary-precision dependency or
partial parse. Strings stay at1MiB encoded, temporary ASCII<=half that bound.
Tick once, pre/post budget/deadline, invalid strings normal NaN; invalid cells/
limits fatal. Preserve result globals/buffers/status/pending channels. Host-free
helpers reject reachability. New Number/wrappers/object coercion and Number's
text/radix method remain pending. Mandatory ABI23/profile even unused; protocol2
maximum28/value12 includes v20 and requires both rebuilt native binaries. No
layout/container/helper/dependency/resource-limit change. See primitive Number contract.

The primitive-String profile checked-f64-utf16-v19 selects ABI22/value12/schema12,
inheriting v18. PrimitiveString / primitive_to_string (i64)->i32, minimum22, no
capability, returns_string/needs_result_globals true. Direct String calls follow
ordinary local/declaration dispatch before builtin recognition. First args admit
only primitive members; no args returns guest empty String. Present args use a
ValueBlock: cache first, evaluate/discard all extras, widen through one shared
five-primitive union, invoke host, Snapshot owned result. Keep roots/hidden slots,
argument throws, earlier operands/pairs/saved returns/finalizers and old TDZ rules.
Host uses PrimitiveBox::read (existing cell format), canonical number display,
true/false/null/undefined text or exact copied UTF-16 bytes. Copy before result
buffer transfer; no lossy Unicode/NUL conversion. Tick once, post-transfer check,
retain status/pending channels; invalid cells/limits stay fatal. No new limits,
helpers/dependency/layout/capability. Host-free JSON helpers reject reachability.
Mandatory profile/ABI22 even unused; protocol2 maximum28/value12 includes v19 and
both native companions must be rebuilt. Objects/reference unions, wrappers,
Number.prototype.toString/radices/general coercion remain pending, not silently emulated.

The Number-output profile checked-f64-utf16-v18 uses ABI21/value12/schema12 and
inherits v17. HostOperation LogFloatV2 / log_f64_v2 (f64)->() and ValueLogV7 /
value_log_v7 (i64)->() have minimum21, no capability/result globals/buffer. Profile
host_operation remaps only float and union/reference log names; v18 rejects old log
imports, earlier profiles reject the new ones. Other selectors/JSON stay unchanged.
number::canonical_display uses existing pinned ryu-js1.0.3 with both zeroes "0",
NaN/Infinity names, shortest finite decimal and ECMAScript scientific thresholds.
references::register_log retains old cells/kinds/diagnostic JSON; only V7 Float64
uses canonical text. Old imports retain number::display and historical text.
One host charge, bounded<=25-byte numeric text, unchanged output/deadline/status/
result-buffer/pending/GC/pair/finalizer contracts; JSON helpers still reject logs.
This increment adds no String()/Number.prototype.toString, radix/coercion/wrappers
or new dependency/layout/limits.
Mandatory ABI21/profile metadata even unused; launcher maximum28/value12/protocol2
includes v18/v19/v20/v21/v22/v23/v24/v25/v26/v27/v28/v29, rebuild both binaries. See the architecture Number output contract.

The random profile checked-f64-utf16-v17 uses ABI20/value12/schema12 and inherits
v16. Shared MathRandom / number_random has ()->f64, minimum20, no capability/result
globals. RandomSource reads one fresh getrandom0.3.4 OS word per call; high53 bits
scale exactly by2^-53, giving +0 through1-2^-53. No production seed/PRNG/cache or
public override exists; test source injection is cfg(test), store-local only.
Math IR Random collects/resolves this host index and discards all ignored args'
full physical widths after once/in-order evaluation. Preserve earlier roots,
argument throws, saved returns, pairs/status and fatal exit/limits. Host ticks once
before entropy and checks budget/deadline after; entropy failure is fatal with no
fallback or status/result-buffer mutation. JSON helpers still reject any host
reachability. Profile/ABI20 metadata is mandatory even unused; launcher maximum24,
value12/protocol2 includes v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27/v28/v29. No limits/layout/container/tool-adapter changes.
See docs/architecture.md#random-profile-and-host-abi-v20 for lifecycle and tests.

The native-Math profile checked-f64-utf16-v16 selects ABI19 with unchanged
value/schema12 and inherits every v15 feature/JSON/value selector. Default i32 and
old profiles stay frozen. The common NativeMath inventory maps nineteen unary
methods plus atan2 to typed number_<name> imports, minimum ABI19, with no capability
or result globals/buffer. Keep names/signatures/minimum ABI in the shared registry;
collector registers Native Math IR operations and emitter resolves their indices.
Explicit required signed-zero/NaN/infinity/domain/quadrant cases precede finite
platform f64 math, whose ordinary results remain approximated. atan2 args are y,x.
Tick once before math and check budget/deadline after, preserve status, and retain
fatal traps/exit. Host-free JSON helpers reject reachable native imports regardless
of purity. V16 has no Math.random, coercion, spread/generics/computed/borrowed calls or
unnarrowed unions. V16/ABI19 metadata is mandatory without any active import;
launcher maximum28/value12 and v16/v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27/v28/v29 negotiation must accompany both rebuilt binaries.
No new heap/helper/layout/container/dependency/capability/limit. Test every operation,
special bits/domains/atan2 quadrants, finite known values/exponent invariants,
one-call accounting, hostile ABI/signature/profile/name data, helper purity, roots/
pairs/returns/finalizers, argument source ranges and source-free/public APIs.

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

Use scoped verification during development; do not run the full workspace after
individual edits. Implement a coherent batch of related tasks, check changed
behavior and its callers, then run the full gate once for the completed code batch
before committing. See [the verification map](docs/verification.md).

Keep a development batch open across related operator/builtin families rather
than treating each syntax form as a separate delivery. During intermediate work,
run only checks needed for the changed behavior or a concrete failure. Consolidate
Clippy, public API, packaged execution, conformance reports and documentation at
the completed batch boundary; repeat earlier only when a changed boundary or new
failure justifies it. Do not repeatedly close and reverify adjacent increments.

```bash
python3 scripts/verify.py local varta-bigint
python3 scripts/verify.py local varta-runtime --filter array_inference
python3 scripts/verify.py feature array_inference
python3 scripts/verify.py full
```

Local runs check one crate and execute its selected tests. Feature runs cover the
matching runtime unit, public engine API and CLI artifact tests; use direct Cargo
commands when a feature uses different test names. A zero-test selection is not
verification. Full runs formatting, isolated core check, companion build, strict
workspace Clippy, all-target build, workspace tests and the conformance baseline.
Workspace tests already contain CLI smoke tests: repeat those separately only for
launcher/packaging changes or a specific flaky-test investigation. CI runs the full
set on Linux and macOS without the redundant second CLI run.

Changes to shared ABI/layouts, GC/roots, capability enforcement, dependencies or
artifact/distribution contracts need broad affected-boundary tests before a batch
is considered complete; a single feature filter cannot prove those invariants.
Keep production budgets unchanged. On failure, fix and rerun the relevant tests;
run the final full gate after the last code fix, not between every attempted fix.

Record actual scope and results. The helper writes logs and a source fingerprint
under ignored target/verification; it does not skip checks or claim focused tests
prove a workspace pass. Reuse completed checks only while their relevant inputs
remain unchanged. After documentation-only edits, check links/commands/diffs; after
verification-tooling edits, exercise its execution, failure and scope behavior.
Neither requires repeating engine tests if engine/build inputs are unchanged.
Never relabel a previous full pass as verification of newer code. Do not claim
cross-platform success from a local pass. Fix Clippy findings rather than disabling
lints. Cargo dependencies remain locked.

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

## Predicate search profile v25

Inherit all v24 rules under checked-f64-utf16-v25; share ABI26/value12/schema12.
V25+ admits findIndex/findLast/findLastIndex. Both rebuilt native companions
must advertise the exact profile (32 supported profiles under protocol2 with v29).
Use existing rooted iteration/call IR, live MaybeIndex reads for holes, reverse
indices for last searches, cached matched values and -1/Undefined sentinels.
Preserve callback-before-thisArg lowering and cache effective operands before
ignored trailing effects; wrapper length snapshot follows all operand effects.
Callbacks accept element|Undefined and ordinary guest truth; typed receiver and
Function/shape limits persist. Corrupt Fn metadata is fatal even for empty arrays.
Do not raise production fuel/array/source limits for tests. Generic array-like
objects and prototype/borrowed methods remain separate CORE-02 work.
See architecture predicate-search contract and predicate_search regression suites.

## Reverse-fold profile v26

Inherit v25 and share host ABI26/value12/schema12 under checked-f64-utf16-v26.
V26+ admits reduceRight; earlier profiles and existing reduce admission stay
unchanged. Both native companions advertise the exact 30-profile protocol2 list.
Use shared reduces()/reverse() traversal, live present checks, rooted accumulator
and current slots, rightmost implicit seed and recoverable empty-fold TypeError.
Callback metadata must still validate on empty inputs. Evaluate callback before
seed; cache source/callback/seed before ignored effects and snapshot length later.
Retain exact seed/result typing, Undefined this, lexical arrows and all existing
Function/shape/source/fuel limits. Parsed maximum-array fixtures avoid conflating
repeated input construction with fold cost; do not raise production budgets.
Keep granted/denied effects, status, pairs, GC and fatal boundaries across all
artifact/public paths. Generic/prototype/dynamic accumulator support remains
explicit future work. See architecture reverse-fold contract and reduce_right tests.

## Checked conformance tooling

Classify varta-conformance as development tooling; dependencies point to common,
compiler and runtime only. Never add it, sha2 or its CLI to runtime distribution.
The corpus pins ECMAScript 2025/profile v29/checked-ts-main-v1. Keep source hashes,
mandatory .ts manifest listing, strict schema/version/field/ID/path/core bounds and
LF checkout bytes. Preserve original snapshots, fresh workers/stores, empty grants
and environment, default guest limits and bounded stdin/stdout/stderr/deadlines.
Kill/reap workers and join pipe threads on failure. Internal compiler/runtime/
transport failures cannot become supported passes or known-gap suppressions.
Only exact reviewed compile diagnostics match a concrete unsupported baseline;
keep them, unexpected support and untested feature rows separate in every report.
Do not automatically learn baselines or rewrite Test262 into main functions.
Run the baseline gate with workspace tests; --require-supported currently exits 1
for open acceptance work. Source/profile/baseline changes require explicit semantic
review and matching documentation. See conformance/README.md and the CORE ledger.

## Array initializer inference v27

Inherit v26 under checked-f64-utf16-v27 with shared ABI26/value12/schema12;
both rebuilt native companions advertise the exact current profile list. V27 and successors add
uncontextual array unions. Lower effectful elements once, retain source order,
flatten admitted member evidence and intern the final union once. Preserve earlier
aggregate contexts and explicit array typing. Keep context-only record/array
hints distinct from constraints, including calls, ternaries and pattern results. Resolve only literal empty/all-hole
array peers from typed array evidence; never replay an effectful expression to
obtain its type. Keep final IR positions, holes/presence, reference identity,
boxed Number bits, alias invariance and all existing shape/Function/opaque limits.
Outer/unresolved empties still need annotations. Do not raise root/fuel/body/source
limits for wide literals: 1024 elements and 64 KiB function bodies are independent
bounds. Keep stale capture diagnostics, host/status/fatal and GC contracts across
module/public/artifact paths. The reviewed v27 corpus promotes the old gap and adds
four cases (40 total, 28 pass, 12 known unsupported); v26 still rejects it. See
architecture inference contract, array_inference tests and conformance/README.md.

## BigInt mathematical foundation

`varta-bigint` is an engine numeric foundation depending only on common engine
contracts and pinned numeric libraries. Every allocating arithmetic/conversion
operation takes an explicit caller bit/work budget. Keep fatal capacity/work
failures distinct from recoverable semantic errors. V28 consumes it for opaque
rooted scalars via ABI27; structured value tags remain unchanged. Never interpret
that scalar increment as complete aggregate/union/throw/JSON BigInt support.

The binary representation lives in `varta-common::bigint`; do not encode
it as a String or reinterpret numeric digits as roots. Validate the full managed
leaf and canonical sign/count/high digit before importing a magnitude. Keep the
existing byte ceiling, charge large shift destination work before allocation, and
use `Error::recoverable_class` so malformed transport never becomes SyntaxError.
The internal memory fixture is supplemented by v28 source/public/artifact tests.

## BigInt scalar source profile v28

Use ABI27 typed pure BigInt imports and the shared binary selectors. BigInt is
an opaque rooted i64 scalar in this increment: value/schema12 and historical
structured tags remain frozen. Canonical binary literals must be snapshotted into
managed flag2048 leaves; consume SWC's unsigned digits without a decimal roundtrip.
Checked result pairs are pointer/error-class, not privileged status results. Cache
status, snapshot only on success, zero the placeholder on failure, and emit rooted
native Error construction/throw before any subsequent source effect. Pure helpers
remain host-free; do not add capabilities or overwrite result_status. Charge all
native integer/bridge work against remaining guest fuel and check deadlines.

Preserve operand/receiver/ignored-argument ordering, native Type/Range/SyntaxError
and fatal malformed/resource distinctions, shallow roots through closures and
finally, exact Number comparisons and old-profile rejection. Aggregate/union/throw
BigInt, JSON behavior, compound/update syntax and complete coercive method forms
remain explicit follow-up work. The reviewed v28 corpus has 46 cases: 34 supported,
12 known unsupported, 7 untested feature rows. Keep the BigInt-array gap visible.

## BigInt graphs v29

Select ABI28/value13/schema13 and exception channel4. Only v29 admits BigInt tag11
in records, vectors and reference cells; primitive-cell imports/readers stay
frozen. Canonical BigInt handles are reference edges, even for zero, while digit
words never are. Keep versioned observation/conversion/equality hosts, numeric
array search equality and inline union truth. Primitive-union arithmetic caches
source operands once and specializes checked member pairs; never replay an AST
expression. Preserve mixed-numeric TypeError and all body/root/metadata budgets.

JSON stringify v13 returns pointer/error-class; snapshot only on success and
construct/throw a rooted native error before later effects. Validate the whole
bounded graph before returning semantic BigInt TypeError so malformed later slots,
cycles and resources remain fatal. Diagnostic aggregate rendering is separate and
meters BigInt decimal formatting. JSON parse v15/schema13 permits a BigInt type
node but no JSON token constructs that type; empty typed arrays/absent optional
fields and JSON-kind-selected unions retain canonical metadata. Keep parser
helpers host-free and validate schema13 descriptors before publishing channel4
errors. Owned uncaught BigInt payloads retain all bytes; diagnostics stay bounded.

V28 and older source/value/exception selectors remain frozen. The v29 corpus has
66 cases: 55 supported, 11 known unsupported, 7 untested feature rows. Its original
BigInt-array gap was observed as unexpected support and reviewed before promotion;
the unchanged coercive-width gap was later promoted after observed support.

V29 compound assignments reuse checked scalar/union BigInt arithmetic. Cache the
prepared receiver/key and old value before RHS effects; store only after success.
Preserve v28 rejection, sparse/optional presence and current backing resolution.
V29 numeric increment/decrement uses cached numeric members and matching 1/1n
units, roots the old and new results, and publishes only after a successful store.
V29 primitive update ToNumeric converts String/Boolean/Null/Undefined via metered
PrimitiveNumberV2, preserving converted numeric postfix results. Check computed
numeric union members against declared storage before writes; only after success
may a postfix old value be narrowed to the stored numeric kind. Number-capable
holes/optional fields become NaN; BigInt-only missing stores still throw without
mutation. Preserve older noncoercive profiles and selected dynamic-record field
types. Object ToPrimitive and general inferred storage widening remain pending.

V29 primitive width/radix methods use ABI28 BigIntWidthV2/BigIntToStringV2.
Cache receiver and all arguments before conversion, root every boxed argument
before creating the next, and preserve ignored-argument effects. Validate canonical
transports before semantic failures; apply ToIndex then ToBigInt for widths,
reject Number values (unlike BigInt()), and treat only undefined radix as default.
Implicit numeric conversion rejects BigInt. Charge parsing/encoding and preserve
fatal malformed/resource errors and pure result_status. Old v28 imports stay frozen.

V29 compound arithmetic/bitwise assignments cache old value and RHS before
conversion. Addition branches on actual String kind, then uses metered
PrimitiveStringV2; numeric paths reuse primitive ToNumeric once per operand and
dispatch only Number/BigInt pairs. Exclude always-throwing mixed pairs from result
type joins, with an unreachable left-compatible result when no pair succeeds.
Reuse checked_assignment_result for update/compound results before stores,
including dynamic-record aggregate result narrowing and per-field store guards.
Retain RHS effects on failed storage, exact roots and old-profile diagnostics.
Do not increase body, root, memory or fuel limits to admit wider dispatch.
V29 ordinary arithmetic/bitwise/shift expressions share primitive_arithmetic
with compounds. Preserve scalar Number and String fast paths; normalize wide
primitive unions once before numeric dispatch. Primitive unary +, - and ~ use
ToNumeric with a Number result domain for +; BigInt plus still calls the checked
native unary host before throwing, retaining fatal malformed/resource behavior.
Keep ordinary operands/read effects once, exact numeric kinds and older profiles.
Object ToPrimitive and full comparison/equality semantics remain explicit gaps.

V29 comparisons use pure ABI28 PrimitiveEqual(strict,left,right)->i32 and
PrimitiveCompare(left,right)->f64 with canonical value13 primitive cells. Validate
both cells before early strict mismatch; malformed/resources stay fatal. Preserve
exact BigInt/Number comparison, integer-string grammar for BigInt/String and
UTF16 lexical ordering for two Strings. NaN order yields false for all four
relational operators. Cache both source operands before boxing/rooting arguments.
Keep scalar Number/String/equality and known-nullish guest fast paths, so loops
do not consume native call ceilings. The Node oracle fixture covers eight modes
by 256 pairs. Object/Symbol coercion and unrestricted storage remain unfinished.
