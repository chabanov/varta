# Profile implementation rules

Implementation rules for each language profile and additive feature, moved verbatim
from the former `CLAUDE.md` §1 and its appended profile sections. Read the relevant
section before changing a feature, together with its normative contract in
[architecture.md](architecture.md). Version maxima, profile counts and conformance
counts quoted below describe the state when each increment landed; current values
are in the [version matrix](architecture.md#version-matrix).

## Core and profiles v1–v24 (former CLAUDE.md §1)


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

### Structured values, JSON and HTTP

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

### Runtime-only packaging, language metadata and binary64

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

### checked-f64-utf8-v2

The further opt-in `checked-f64-utf8-v2` profile uses host ABI v3/value ABI v4:
Undefined tag 7 has a canonical zero payload and is distinct from Null. Void
call expressions, `void` and explicit undefined values preserve effects once;
declared void functions still have no Wasm result. Root JSON undefined returns
undefined, object fields are omitted and array entries emit null. Validate even
omitted fields, count them against traversal budgets and reject nonzero payloads.
JSON v4 imports/schema v3 are exclusive to this profile. Strict typed JSON cannot
target undefined; no implicit null conversion or missing-property/union support
is supplied. Preserve lexical shadowing/TDZ, exact roots and all older profiles.

### checked-f64-utf16-v1

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

### checked-f64-utf16-v2

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

### checked-f64-utf16-v3

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

### checked-f64-utf16-v4

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

### checked-f64-utf16-v5

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

### checked-f64-utf16-v6

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

### checked-f64-utf16-v7

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

### checked-f64-utf16-v8

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

### checked-f64-utf16-v9

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

### checked-f64-utf16-v10

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

### String.repeat (v9 and later)

String.repeat is additive in the v9 native-family profile and successors. Accept
Number/Undefined counts only; evaluate/root receiver before count exactly once.
Normalize NaN/undefined to zero and truncate fractions; negative normalized counts
and infinities use the existing RangeError channel even for empty receivers.
Use two shared activation-local i32 copy slots only after operand evaluation;
nested calls/repeats must not overwrite live outer operands. Check result bytes
before integer conversion/allocation, allocate once and bulk-copy doubling without
calls during copying. Preserve UTF-16/NULs, exact roots, status and all fatal
budgets. No new host import/helper/ABI version; earlier profiles reject repeat.

### checked-f64-utf16-v11

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

### checked-f64-utf16-v12

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

### Array callbacks (v12 and later)

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

### checked-f64-utf16-v13

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

### checked-f64-utf16-v14

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

### Additive v12+ contracts

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

### checked-f64-utf16-v15

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

### checked-f64-utf16-v24 flow analysis

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

### checked-f64-utf16-v24

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

### checked-f64-utf16-v23

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

### checked-f64-utf16-v22

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

### checked-f64-utf16-v21

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

### checked-f64-utf16-v20

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

### checked-f64-utf16-v19

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

### checked-f64-utf16-v18

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

### checked-f64-utf16-v17

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

### checked-f64-utf16-v16

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

### Direction and engine boundaries

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
69 cases: 58 supported, 11 known unsupported, 7 untested feature rows. Its original
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

V30 property_deletion inherits v29 layouts/hosts and adds optional-record deletion
and primitive delete keys. Cache the receiver before key conversion. Install a
fresh canonical undefined value before clearing presence; compact order in-place
and zero the unused entry. Preserve static trace masks so absent union cells are
still traced; never mutate an escaped old union cell. Required checked fields and
array length throw before mutation. Reuse exact ArrayStringHasOwn parsing before
numeric deletion; reject object/Symbol key coercion. Keep v29 frozen and retain all
budgets. Rebuild the companion for 33 profiles; no ABI/value/schema bump is needed.

V30 Object.entries caches the source and evaluates ignored arguments before any
snapshot. Reuse enumerate_value for keys/values in the same own-key order, then
build fresh dense pairs. ArrayPush takes an array of items, so wrap each pair in
one outer singleton; passing the pair directly corrupts the outer element type.
Root source, both snapshots and output across all allocations. Pair elements use
an admitted String/value union, not positional tuple typing. Preserve shared
children, skip holes/absent fields, keep explicit undefined, and throw on nullish
inputs. Number/Boolean/BigInt wrappers produce empty arrays. String inputs enumerate UTF16 units under v30;
mixed aggregate shapes remain unsupported. Do not extend v29 admission or raise
production budgets; capacity/GC fixtures explicitly declare test-only fuel.

V30 own-key enumeration accepts String and all-primitive unions. Dispatch Strings
with IsUnionMember before unboxing; reject nullish inputs after ignored-argument
execution. Numeric/Boolean/BigInt wrappers yield empty arrays. Keys/values accept
missing/ignored arguments only under v30. For String, guard UTF16 length <=1024,
then build a dense String[] of empty placeholders (keys) or one-unit slices
(values). Existing ArrayKeys generates decimal keys without native call pressure.
Keep partial outputs and the original source rooted. Preserve lone surrogates;
never iterate Unicode scalar values. Overcapacity throws recoverable RangeError;
capacity/collection fixtures may raise test-only fuel, not production limits.

V30 Object.is implements SameValue with cached operands and per-member union
dispatch. Do not use strict numeric equality: NaNs must compare equal and signed
zeros must differ. NumberSameValue IR accepts only cached numeric locals/unboxes;
emit both-NaN OR equal raw bits. Preserve argument order, missing Undefined,
void-to-Undefined normalization and ignored-argument effects before dispatch.
Compare Strings by guest UTF16 contents, BigInts with existing exact comparison,
and all reference kinds by handle identity without joining incompatible shapes.
Root both cached operands across later and ignored arguments. Native result pairs
need explicit fields; keep historical admission and ABI/resource bounds frozen.

V30 Object.assign caches all arguments before target nullish checks, then copies
sources in sequence. Skip nullish and non-enumerating primitive/function/Error
sources. Dispatch record/array target/source unions without joining incompatible
shapes. Snapshot record keys per source and read each field at its visit; never
require one union across unrelated nested field shapes. Arrays copy present
indices at a per-source length snapshot; String sources use existing UTF16 guest
enumeration. Preserve target identity, copied child identities and previous writes
when a checked field/array-length operation throws. Reuse normal store/root guards.
A plain fresh record literal may reserve record-source fields before its original
allocation: make added fields optional with canonical absent payloads, widen
existing literal field storage to an admitted union, and update only its hidden
local type. Never migrate an escaped target or change its reference mask in-place.
Scope only newly declared scratch slots; redeclaring a prepared source array in
an inner Scope would clear its live handle. Keep record/array limits and v29 frozen.
Primitive target boxing, arbitrary properties, descriptors and Symbol remain gaps.

Use common function ClosureView/EnvironmentView/BindingView only for bounded
physical envelope inspection. They intentionally leave capture payloads opaque;
never treat a binding type_id as a ValueKind or infer module code membership from
budget bounds. Match compiler binding type/width/root metadata before typed use.
Keep one/two-slot native result captures, shared cells and recursive function
payloads legal. These views do not admit function values into v30 aggregates;
future structured transport needs explicit versioning and typed boundary proofs.

V30 closure modules emit optional varta.functions metadata from binding_types and
function_targets; keep those logical code/signature/capture IDs synchronized.
Never obtain a trusted capture schema from mutable guest memory. Typed inspection
checks private IDs, widths, reference masks, status domains, exact record slots,
array elements and union members. Memoize recursive functions only after catalog
code/signature checks, and retain work/node/depth budgets. Type DAG height checks
must work for bottom-up tables too. Catalogs do not grant call authority or admit
function values into public value13 aggregates. Reject malformed/duplicate or
wrong-profile catalogs; accept older v30 artifacts where the optional section is
absent. Future native function transport must retain the checked call boundary.

Value14 recognizes Function tag12 with reference tracing; never reinterpret this
tag in v13. Function cells need owner catalogs, not context-free ReferenceBox
reads. V14 catalogs may include function fields/arrays/reference unions, while
v13 permits only existing private function captures. The Rust graph encoder maps
validated functions to JSON omission/null/Undefined and decrements a shared work
allowance. Preserve legal private capture cycles; malformed functions must fail
even when serialization would omit them. Reject callable toJSON until guest-side
hooks are implemented. Schema14 cannot fabricate functions. This codec does not
change the 33 admitted profiles; new host imports and source gating are pending.


Prepared module capture ownership is part of the runtime foundation. Keep
`PreparedModule` construction private to validation, with immutable verified
catalog ownership paired to its backend module. Startup, invocation and fresh
HTTP instances must use `instantiate_prepared`; raw module validation remains a
compatibility/inspection API. Install the catalog before instantiation, clear it
on failure, and forbid replacing it after the execution's first attempt. Never
recover trusted metadata from guest memory or a global module registry. Sealed
catalog proofs may skip schema revalidation, but must retain all graph/work and
payload checks. This does not admit value14 source operations or call authority.

## Named types profile v31

Inherit v30 under checked-f64-utf16-v31 with shared ABI28/value13/schema13 and
the v30 exception/JSON selectors; rebuild both native companions for the exact
profile list. `named_types()` admits v31 only; `property_deletion()` and every
inherited selector include it. Named types are compile-time only: no host import,
value tag, layout, helper or catalog entry changes, and artifacts carry no names.

The type namespace lives in `ValueTypes.named`, keyed by (module order index,
name). `named::link_module` records local interface/alias declarations and type
imports for each module in dependency order before that module's function
signatures are checked, then computes its type exports. Imports follow
`Declaration::Imported` chains to the defining entry; resolution lowers the
declaration body once in its defining module (switching `named.current`) and
memoizes the interned type. Keep the resolving stack: any cycle fails with
`recursive type declaration`, never by unrolling. Built-ins (`Array`,
`VartaResult`, HTTP types, Error families) are matched before named lookup,
except that a declared or imported type of the same name as an Error family
shadows that type name (TypeScript module scoping); `Error` values and
`instanceof` stay built-in. Interfaces reuse the closed
type-literal rules; reject generics, `extends`, duplicates and unknown names
with distinct diagnostics.

The module scanner receives the profile. Only v31 admits `import type`,
`{ type T }`, `export type { … }` and records `export interface/type` as type
exports; earlier profiles keep their exact messages and silently ignored
declaration bodies. Function linking skips type-only and type-resolved bindings;
a missing function binding that is also not a type keeps the existing error.
The project loader maps `./x.js` to `./x.ts` only for v31 and never reads `.js`
files; all confinement, no-follow and size rules are unchanged.

Structural subtyping between distinct record shapes, generics, literal types,
module constants and re-exports are separate pilot-driven work.

## Source complexity limits

The lexical pre-parse check bounds recursion, not width: nesting 64 and 256
consecutive chain tokens. Only separators that are flat siblings in every grammar
position reset the run: commas in `[...]`/`{...}` and top-level `|`/`&` in a
`type` alias outside angle syntax (SWC collects union/intersection members in a
loop). Expression `|`, call arguments and anything after `<` keep counting. An
alias context ends at `;`, an enclosing close or a line break that does not
continue the union. The former 65,536-token per-file count was removed by
explicit decision (2026-10-28); byte limits bound total size. Never make a
separator a reset unless its AST is a flat list in every context. SWC's lexer
returns `}` after a template substitution and leaves the template text to the
parser: the check must call `rescan_template_token` at the `}` that closes a
`${` delimiter (as the parser does), or template text and later doc comments
with backticks are counted as one long code run.

## Pilot batch profile v32

Inherit v31 under checked-f64-utf16-v32 with shared ABI28/value13/schema13 and
the v31 JSON selectors; `error_subclasses` selects exception channel v5 (see
[its rules](#error-subclasses-error_subclasses)). Each pilot work stream has exactly one selector
(`module_constants`, `literal_types`, `classes`, `module_reexports`,
`template_literals`, `default_exports`, `interface_extends`, `generics`,
`error_subclasses`); gate every new admission on its own selector, keep v31 and
earlier profiles frozen, and add rules for the stream under its own heading here.
The streams compose; the rules for their interactions are in
[cross-stream integration](#cross-stream-integration-v32).

### Module constants (`module_constants()`)

`declarations::collect` skips top-level non-ambient `const` only under the
selector and names `let`/`var`; earlier profiles keep `unsupported top-level
declaration`. `link::link_values` visits `graph.order` after all function
signatures: it resolves the module's value imports through the value export
tables, calls `module_constants::link_module` with the imported constants, and
then links the module's value export table, so constants import, export and
re-export through the same rules as functions. `check` validates the whole module first
(identifier bindings, initializer presence, duplicates against functions and
imports, the pure initializer form, and temporal-dead-zone order), so chunking
never changes diagnostics. Keep the initializer subset call-free: admitting calls,
member reads with effects or anything that can observe capabilities needs a new
evaluation-order design and initialization on tool/HTTP entries first.

Number/boolean/null/undefined constants fold to literals (`fold`); never put a
non-handle value in a root slot, because the collector treats words at or above
the heap base as block pointers. Handle-typed constants get a stored index:
`S::InitModuleConstant` publishes the evaluated local, `E::ModuleConstant` reads
it, and emit reserves `Program.constants` permanent slots after the exception and
canonical function roots, starting the root cursor beyond them. Evaluate each
stored initializer exactly once; never re-evaluate or inline a stored value at a
use site, which would break identity and shared mutation. Initializers are chunked
by AST weight (`INITIALIZER_WEIGHT`) to stay under the function body budget, are
called at the start of `main` in dependency order, and are excluded from
function-value dispatch targets.

Functions bind only the constants they mention (`referenced`) as hidden immutable
locals in a scope inserted at `scopes[0]`, after `reserve_parameters`: parameter
slots must remain the leading Wasm locals, and parameters/locals must shadow
constants. Arrows reach constants through ordinary immutable captures.

`as const` is stripped only by `const_assertion` for a whole declaration
initializer and marks `Binding::readonly`. Every member-write gate must call
`reject_readonly_write`: `write_member_base`, array `push`/`pop`/`reverse`/`fill`
and the `Object.assign` target. A new mutating operation must add the same check.
Readonly is binding-level intent, not a type: do not intern readonly array or
record types without a separate profile decision.

### Literal types (`literal_types()`)

`Type::Literal(LiteralBase, id)` indexes `ValueTypes::literals`, interned by base
and value set. Annotation interning fails at `MAX_LITERAL_MEMBERS` and
`MAX_LITERAL_TABLE_MEMBERS`; guard refinements use the same tables but skip the
refinement instead of failing. `unions::intern` normalizes literal members
(`literals::normalize`) and records an erased twin of each literal-bearing union.
Gate every admission on `literal_types()`; v31 must keep its exact behavior.

Literal types never reach emission. `literal_erasure::erase_program` erases every
table and IR type and merges function types that coincide after erasure (signature
dispatch and the capture catalog match signatures structurally). Its IR match is
exhaustive on purpose: a new `Expression` or `Statement` variant must erase its
children there. Records, arrays and unions are not merged; duplicates are inert.

Reads erase: `expr_contextual` without an expected type returns the base (or the
erased union twin). Use `expr_preserving` only where a literal type must survive:
inferred bindings and assignment right-hand sides before `literals::conform`.
Assignability goes through `unions::widen` (`literals::conform`), union member
checks through `literals::member_of` and `arguments::accepts` through
`literals::assignable`. A new write path must check against the place's declared
type (field, element, parameter or binding), never against an erased read. Only
`E::String`, `E::Float64`, `E::Negative(E::Float64)` and Boolean `E::Integer`
constants count as literal values; anything else needs a literal type.

Literal guards (`literals::guard_operand`, `Guard::Literal`) may refine non-union
locals. Such a refined read retags the local instead of unboxing it (expression and
prepared-target reads), and `flow_fact` uses `literal_flow_fact`, which also keeps
facts for non-union declarations. `either_guard` joins the states of `a || b`; a
join that cannot intern keeps the unrefined state. `varta.json.parse<T>` rejects
any target containing a literal set (`literals::contains`); admitting it requires a
schema version that validates membership before allocation.

### Classes (`classes()`)

Class instances are records with a private `RecordShape.class` shape: inherited
slots first, never interned with structural shapes. Identity is the canonical
descriptor of the most-derived class; never add a hidden field, heap flag or
value tag for it, and keep construction in canonical slot order so the allocated
layout key equals `canonical_layout`. `Type::Class` must stay distinct from
`Type::Record`: record operations are admitted for classes only by deliberate
decision (subclass instances carry more slots and a different presence trailer
than the static shape). Widen subclasses to ancestors only by retyping; a union
holds at most one record-or-class member, and union cells keep the record tag.

Classes register in the v31 type namespace (`named::Declaration::Class`) during
`link_module` (`export class` is a scanned `Export`, so classes join the type
export tables and re-exports), are laid out per module after its function signatures (parents
first, cycle-checked) and allocate initializer/method indices in that order;
dispatchers are allocated after all modules and their bodies are pushed after the
last module, before lifted closures. Function vector positions must equal the
allocated indices. Every overridden root method uses one dispatcher shared by
all descendants; never inline per-call dispatch or call an override directly
through an ancestor-typed receiver.

Placeholders must stay unobservable: keep the syntactic constructor discipline
(field-only `this`, top-level definite assignment, single top-level `super`, no
escape/capture/method calls/returns/`new.target` other than the
`error_subclasses` `new.target.name`, `this`-free initializers and
parameter defaults) unless a flow analysis proves the same property. Readonly
writes are allowed only in the declaring class's initializer through `this`;
private/protected checks use `classes.context`. Class calls are user code: keep
`flow::native` false for names declared by any class method and treat class
`new` as an unknown call in guard effects. Captured class bindings use the
catalog's `class` leaf. Static methods are receiver-free direct calls resolved
against the named class (no dispatch). `#private`, static fields and setters
need their own design; keep them rejected with construct-specific diagnostics.
Error subclasses follow [their own rules](#error-subclasses-error_subclasses).

### Module re-exports (`module_reexports()`)

`module_reexports()` gates the scanner forms only; earlier profiles keep the
exact `re-export or type-only export` / `module export form` messages. The scanner
records `export { … } from` as an `Export` with `from: Some(target)` and
`export * from` as a `Unit.stars` entry, and adds each source as a `Dependency`
with no bindings, so cycles, ordering, metadata and limits come from the existing
graph walk. Never bind a re-exported name in the re-exporter's function scope or
type namespace.

Export tables live in `lower/exports.rs`: `Table<V>` holds bound names plus the
set of ambiguous star names, and `exports::link` applies the shared rules
(explicit exports first with duplicate detection, then stars filtered by
`default` and every explicit exported name, merged by binding identity). Each
binding namespace implements `exports::Namespace`: values (`link.rs` `Values`,
binding `Value::Function` or `Value::Constant`, identity = function slot or
`Constant::id`, a graph-unique declaration counter) and named types
(`named.rs`, identity = defining declaration after following
`Declaration::Imported`; classes are type declarations). Type tables are linked
per module before function signatures; value tables in `link_values` after
all signatures and after each module's constants, so the value namespace
decides cross-namespace "missing" errors. Import resolution in
every namespace must treat `Lookup::Ambiguous` as `exports::ambiguous`.

Extension point: a new value kind becomes a `link::Value` variant with its own
identity, so functions, constants and it stay one namespace (a function and a
constant of one name from different stars are ambiguous); never give a value
kind its own table. A new type-like kind registers in `named` declarations.
Local `export` declarations of a new kind must be recorded as scanned `Export`s
under its selector, as `export const` and `export class` are.

Namespace re-exports, `export {} from`, `export type *` and attributed forms
stay rejected; namespace objects need their own contract. `default` names follow
[default exports](#default-exports-default_exports).

### Template literals (`template_literals()`)

`template_literals()` gates `ast::Expr::Tpl` and the tagged-template diagnostic in
`expr_inner`, plus the `string` inference for an unannotated template parameter
default; without it both keep the exact `unsupported expression` / annotation
errors. `lower/templates.rs` lowers a template to a left fold of
`E::Binary(Add)` string concatenations, so it inherits the `+` operand roots,
source order and 1 MiB value limit; never add a separate join helper without an
ownership/budget contract. Lower each substitution with `self.expr` and convert it
before lowering the next one. `string` passes through; every other admitted
primitive goes through `primitive_convert` (the shared body of `String(x)`, which
caches the value in a rooted ValueBlock and snapshots the host result), so
Number text stays the v18 canonical display and BigInt uses `bigint_text`. Check
members before conversion and reject any non-primitive member at the
substitution span; adding ToPrimitive (classes, records, arrays, Errors) is a
separate decision. Cooked parts reuse the string-literal WTF-8 to UTF-16 encoding
(lone surrogates exact); empty parts emit nothing; a missing cooked value is an
invalid program. Tagged templates and `String.raw` stay rejected. No host import,
layout or budget change; `encodeURIComponent` remains a separate builtin.

### Default exports (`default_exports()`)

`default_exports()` gates the scanner only. `modules/defaults.rs` rewrites every
`export default` item in place before the scan: declarations become the plain
top-level function/class/interface (anonymous ones named `defaults::ANONYMOUS`,
ECMAScript's unspellable `*default*`), a binding identifier becomes an empty
statement, and any other expression becomes `const *default* = expr`. Each
records an `Export` with `exported: DEFAULT_EXPORT`, so every later phase sees
ordinary declarations and the export tables see an ordinary name; never add a
default-specific path to linking, classes or constants. `import x from` is an
`ImportBinding` with `imported: "default"`; `name(.., defaults)` admits `default`
only as an imported, exported or re-exported name, never as the local name of a
local list. Keep the identifier rule syntactic (`bound`: a top-level declaration
or import of that name) and the temporal-dead-zone check for later `const`/class
declarations. `exports::link` already filters `default` from stars; one default
per module is checked in the rewrite, before bindings could collide. Earlier
profiles never reach the rewrite and keep `module export form`,
`default or namespace import` and `string or default export name`. Namespace
imports keep a distinct `namespace import` diagnostic under the selector.

### Interface inheritance and merging (`interface_extends()`)

Under `interface_extends()`, `named::link_module` defers every interface
(after the generic-declaration check) into `interfaces::Merged` (per module, by
name) instead of rejecting
`extends`/duplicates, then `declare_interface` registers each name after the
item loop. A single declaration without `extends` stays `Declaration::Local`
with its v31 type literal (byte-identical behavior); anything else becomes
`Declaration::Interface`, resolved lazily by `Interface::resolve` inside
`named::resolve`, so the resolving stack catches cycles through bases. Bases are
lowered through `keyword_type` of a synthesized `TsTypeRef` (with the written
type arguments) in the declaring module. A generic interface keeps its
`TsTypeParamDecl` in `Interface::params`: `plain()` interfaces become a generic
`Declaration::Local`, and `named::instantiate` binds the parameters of a
`Declaration::Interface` with `resolve_parameters` and runs `Interface::resolve`
inside `generics::with_frame`, so own members and `extends A<T>` see `T`.
Merging generic interfaces is rejected (`declaration merging of generic
interface X`) because identical type parameter lists are not compared.
`named::generic_declaration` returns the own-member literal plus each base
reference (`Interface::generic_patterns`) as inference patterns; never infer
from a partially resolved shape. Only `Type::Record` bases are
admitted; class bases stay rejected because class types are nominal and have
methods. Own fields come from `records::type_fields` (no interning of partial
shapes, which would consume `MAX_RECORD_SHAPES`), redeclarations are checked with
`arguments::accepts` (base type expected) plus the optional-to-required rule,
and only the combined shape is interned. An interface name taken by a class gets
the `declaration merging of interface X with class X` diagnostic; implementing
it needs constructor initialization of merged fields (the pilot's
`Object.assign(this, …)`), which the class definite-initialization discipline
forbids. The scanner records one export for merged `export interface`
declarations.

### Generics (`generics()`)

Monomorphization only: never emit a generic body or a type parameter. Generic
interfaces/aliases store their `TsTypeParamDecl` in `named::Declaration::Local`;
`named::instantiate` lowers type arguments in the referencing module and frame,
then `generics::bind` (defaults, constraints) and the body run in the defining
module, memoized by (declaration key, argument types) in `NamedTypes.resolved`.
Keep the `resolving` check keyed by declaration alone, so recursion through any
instantiation fails. Type parameters live in `Generics.frames`; only the top
frame is visible (`generics::parameter`, the first check in `keyword_type`), and
every named resolution pushes its own frame, so a referenced declaration never
sees the caller's parameters.

Generic functions get no slot in `link::lower`; `generics::declare` registers
them (stripped of type parameters) with a module binding. They join the value
namespace as `link::Value::Generic`, so imports, exports and star re-exports use
the shared rules; names must not collide with functions, constants or classes.
`Lower::generic_call` (before the `generic call` rejection in `expr::call`)
lowers every argument once and in source order, never reordering or re-lowering
an argument for inference: lowering allocates locals and lifted functions and
updates flow facts. `generics::instantiate` allocates an index from
`next_function` on first use of an argument list and queues the body;
`generics::bodies` lowers the queue (including instances requested by instances)
after class dispatch bodies, with the instance frame and `depth` set, through
`link::body` and the defining module's scope. Generic names make `flow::native`
false; a generic call is an unknown call for flow effects. Generic functions as
values need a function-value instance per type; keep them rejected.

`keyof`, indexed access, intersections and the built-in generic references live
in `type_operators`. A built-in maps onto an existing closed type or is rejected
with its reason; `Readonly`/`ReadonlyArray`/`readonly T[]` erase readonly, which
stays binding-level (`reject_readonly_write` does not see it). Admitting
`Record<string, V>`, `Promise<T>`, generic classes/methods/arrows or
inference through context-sensitive arguments needs its own design.

### Error subclasses (`error_subclasses()`)

`lower/classes/error_classes.rs` owns the stream. `layout::parent_class` maps an
`extends` of a v9 family to `error_classes::root`, which creates the family's
synthetic root class on first use (the Error root first; other roots extend it).
Roots have `module = NO_MODULE`, `State::Ready`, no `order` entries and no type
namespace entry: layout and body lowering must never visit them, and `Error`
annotations, `new Error` and native `instanceof` must keep resolving to the v9
natives. The Error root's fields are `message`, `name`, `cause` in that order;
read slots through `root_slot`, never by literal index. `cause` and the root
initializer's options record use `exceptions::exception_union`, which adds the
Error root to the catch union only once a root exists, so programs without Error
classes keep their exact catch union, exports and code.

Never allocate or call a root initializer: `super_call` delegates to
`error_super_call`, which evaluates `message` and `options` once into rooted
hidden locals (non-handle types are not rooted) and then writes the three slots
through `this`. Keep the message type check (`string`/`undefined`, no coercion)
and options restricted to `undefined`/one record member; a cause must widen to
the catch union. `new.target.name` is admitted only by `error_classes::new_target_name`
in `ThisUse` (`new_target` is false for field initializers; closures fail) and
lowered by `new_target_member` as a descriptor chain over the descendants of the
constructor's class; never read a hidden class-name field or add one.

`Classes::is_error` decides Error-ness (descends from the Error root). Hooks
that must stay consistent: `throw_statement` admission, `instance_of_error`
(`error_instance_test` when `has_error_instances`), `error_to_string` and
`class_method_call` (`toString` cannot be declared, so an Error class always gets
`class_error_text`), `result_field` (`error_union_read` for a mixed native/class
union), `caught_property` (class branch before the native IsError branch),
`refine_error_family` for `instanceof Error`/families (keep possible members on
the positive edge, drop only certain ones on the negative edge) and
`refine_error_downcast` in `refine_class` (Error classes only; other classes keep
the classes-stream rule that downcasts do not narrow). `unions::intern` joins
Error classes to their nearest common ancestor under the selector so every union
keeps at most one class member; never apply it to non-Error classes without a
classes-stream decision. `JSON.stringify` rejects values that `reaches_instance`
because message/cause enumerability is not modelled.

The channel selector lives in `LanguageProfile::exception_channel`
(`CLASS_ERROR_VERSION = 5`); the runtime's `initial`, `finish` and the JSON
parse `ready` check must know every selector. `finish` decodes Record cells only
under v5 through `error_instance`, which reads the already-validated descriptor
and requires non-optional String `name`/`message` fields. `ThrownValue::ErrorInstance`
owns both texts; display follows `Error.prototype.toString` with escaped, bounded
text. Pending: the `unknown` type for `cause`, an `Error` annotation that admits
Error classes, own-property presence of `message`/`cause` (and with it
`Object.hasOwn`/JSON), `stack`, and new-profile Tool/HTTP adapters.

### Cross-stream integration (v32)

- Constants and functions share `link::Value`; constant imports resolve in
  `link_values` before `module_constants::link_module`, and a declared constant
  may not take the name of an imported function (checked after the module's
  constants exist).
- Class initializer and method bodies receive the module's constant scope
  through `classes::bodies`, exactly like `link::body` for functions.
- Literal erasure is exhaustive over IR forms: `E::ModuleConstant`,
  `S::InitModuleConstant` and `E::IsClass` erase their children there. Class
  record shapes erase with the record table; class construction picks the
  placeholder of `ty.erased()`, so a literal field gets its base slot.
- Assignability combines `literals::assignable`/`member_of` with
  `classes::upcast`/`union_upcast` in `arguments::accepts` and `unions::widen`;
  literal conformance runs before the class upcast.
- The constant initializer subset stays call-free, so `new` (and therefore class
  instances) stays outside it. Template literals are also still rejected there
  (`unsupported template literal in a module constant initializer`); they are a
  candidate for the subset, like `+`, but need their own decision.
- Template substitutions treat literal-typed values as their base primitive;
  class instances need ToPrimitive and stay rejected.
- Default exports rewrite into ordinary declarations before the scan, so a
  default-exported generic function is registered by `generics::declare` and
  linked as `link::Value::Generic` under the name `default`. A default absent
  from both the value and type export tables reports `module m.ts has no
  default export` in `link::other_namespace`. Never add a default-specific path
  to generic linking.
- Generic interfaces with bases resolve through `Declaration::Interface` (see
  [interface inheritance](#interface-inheritance-and-merging-interface_extends));
  the generics memo key `(declaration, arguments)` and the `resolving` stack
  keyed by declaration apply unchanged.
- Generic functions may be constrained by and instantiated over Error classes;
  constraints use `arguments::accepts`, so an Error-class argument satisfies an
  ancestor-class constraint but not the native `Error` annotation. Generic
  classes, including generic Error subclasses, stay `unsupported generic class`.
- `exceptions::exception_union` exists once: the catch union, the Error-root
  member added only once a root exists, and the frame exception completion type
  read by `lower::suspension::prepare` after all bodies (including generic
  instances) are lowered. Do not compute it before class layout.
- No second-wave stream adds an IR variant; `suspension/walk.rs`,
  `normalize.rs` and literal erasure stay exhaustive over the shared forms. New
  v32 value blocks (Error `super(...)`, `instanceof` on Error unions, Error
  `toString`, class `new`) are not on the `construct.rs` allowlist and are
  rejected by construct name; methods and constructors are rejected as
  `a method`.

## Suspension foundation (K4.4 block 1)

Contract:
[architecture.md](architecture.md#suspension-foundation-k44-block-1).

- `ir::Expression::Suspend` comes from v33 `yield` and from the test-only hook
  `lower::suspension::hook` (`__varta_suspend(value[, witness])`). Never
  produce it from other source syntax without its own profile selector and
  the runtime admission rules for `varta_resume`, the frame flag and frame
  catalog version 2.
- `lower::suspension::prepare` runs on IR after lowering and literal erasure.
  `walk.rs` is exhaustive on purpose: a new IR form must state its children
  there. `normalize.rs` must state whether it splits the form, hoists its
  operands left to right or rejects it with a construct name. Never reorder
  effects; hoist only operands that are evaluated before the suspending one.
- Split a lowering value block only when its construct is on the
  `construct.rs` allowlist (assignments, updates, `&&`/`||`/`??`/`?:`). To add
  a construct, add an evaluation-order test that resumes in Wasmtime.
- States are numbered in statement order and guards depend on contiguous
  ranges. Resumption must re-enter structured code (never jump into it), so
  `try` handlers, labels and root frames are rebuilt by the normal emitter.
- `emit::frames` owns frame layout, the creator, resume bodies and the
  dispatcher. Use `varta_common::function::frame` constants for every offset,
  flag, kind, status, completion and outcome. Keep the dispatcher closed: direct
  calls by layout id, no tables. Keep its checks before any effect.
- Frames use their own budgets (`MAX_FRAME_BINDINGS`, `MAX_SUSPENSION_STATES`,
  `MAX_LIVE_FRAMES`), never `MAX_CAPTURES`. Captured locals keep their closure
  cell in the frame. Restoration validates every cell before use.
- The exception channel is empty at every suspension and dispatcher entry.
  Uncaught exceptions become the frame completion; traps stay fatal.
- Modules without suspension points must not change: no frame flag in the
  collector, catalog version 1, no `varta_resume` export.

## Profile v33 (in development)

Inherit v32 under checked-f64-utf16-v33 with value13/schema13, exception channel
v5 and the v32 JSON selectors, but its own host ABI
(`abi::RESUMABLE_FRAMES_VERSION`); never map another profile to that ABI. Two
selectors gate the K4.4 streams: `resumable_frames` (runtime admission of
frames, frame catalog v2 in preflight and the `varta_resume` export) and
`generators` (`function*`, `yield`, `next`/`throw`/`return`). Both are
implemented (below). Gate every admission on its own selector, keep v32 and
earlier profiles frozen (no `varta_resume`, catalog v1 only in preflight, the
existing `function*` diagnostics), and add each stream's rules under its own
heading here. A new value tag needs a new value ABI.

### Runtime admission (`resumable_frames`)

Contract: [architecture.md](architecture.md#runtime-admission-resumable_frames).

- One rule set, `varta_common::function::admission`, serves runtime preflight
  (`varta-runtime` `frames.rs`) and artifact validation (`varta-artifact`
  `frames.rs`). Each crate only extracts a `FrameSurface` with its own parser;
  never add a frame rule to one of them alone. Errors stay typed
  (`FrameAdmissionError`, `RuntimeError::Frames`).
- Read catalogs through `admission::read_catalog(bytes, profile)`. Keep
  `FunctionCatalog::from_bytes` and `VerifiedFunctionCatalog::from_bytes`
  version 1 only; select the version 2 reader by `resumable_frames()`, never by
  a host ABI number.
- Keep the dispatcher closed: if the emitter changes `varta_resume`, keep its
  exact signature, one resume body per layout in layout order, direct calls only
  and no host import calls, or change the shared rules and their tests in the
  same commit. Never export a resume body.
- The runtime must not call `varta_resume` until a scheduler with its own root,
  generation and budget contract exists. Do not classify the dispatcher as a
  tool, value ABI or HTTP helper, and do not relax the host-free helper checks
  for it.
- The `suspension-test-hook` compiler feature is test-only: enable it only in
  `[dev-dependencies]` (runtime, launcher). It is inert unless a test enables
  the hook on its thread, and frame test exports are emitted only then (or in
  compiler unit tests).

### Synchronous generators (`generators`)

Contract: [architecture.md](architecture.md#synchronous-generators-generators).

- Source admission lives in `lower::generators` (annotations, `yield`, protocol
  methods, `for...of`) and is gated on `profile.generators()` only: the
  signature check, generic declarations, `Generator`/`IteratorResult`
  annotations, the `yield` expression arm and the `next`/`return`/`throw`
  method arm. V32 and earlier keep `unsupported async, generator, generic or
  decorated function`.
- `ir::Function::generator` marks a `function*`. Until `lower::suspension`
  builds its frame, `result` is the generator type that calls see; `prepare`
  swaps in `TReturn`. A generator is always suspendable, even without a
  `yield`. Calls to it create the generator object; function references stay
  rejected (`generator function as a value`).
- Generators have no value tag. Keep `Type::Generator` out of
  `value_kind()`, arrays, records and unions (`generators::opaque`), JSON,
  logging, `typeof`, equality and tool/HTTP signatures. Admitting any of these
  needs a value tag, a new value ABI, the version matrix and launcher update.
- Every resumption goes through the step function of the generator type
  (`emit::frames::step_function`): state checks before any effect, closed
  direct calls by layout id and resume code. Lowered protocol code decides on
  the `GeneratorOutcome`; read a completion cell only directly after the step.
  Never resume a generator frame through `varta_resume`; its creator releases
  the live-table slot, and its dispatcher branch traps before the body call
  that admission counts.
- Generator catalog rows carry `generator: true` and `FrameInput::Guest`
  inputs; the catalog and `admission.rs` enforce the pairing. Keep inputs of a
  resume body in their own locals and never root a local whose completion kind
  was not delivered.
- `for...of` closing is lowered as try/catch inside try/finally with an
  `active` flag set only while the body owns a yielded value. Do not close on a
  generator's own completion or exception, and keep the body's throw ahead of a
  closing throw. Its `TNext`/`TReturn` must accept `undefined`.
- Suspension in `catch`, `finally` and `switch` case bodies is shared by every
  suspension point: the catch-flag rewrite in `normalize.rs`, the saved
  finalizer completion and the saved case index in `states.rs`. Case tests,
  `do-while` conditions and unlisted value blocks stay rejected by name.
- Generator behavior tests compare against pinned Node output
  (`varta-runtime` `generator_cases`); regenerate expectations only from Node,
  never from Varta output.

## Profile v34 (in development)

Inherit v33 under checked-f64-utf16-v34 with every v33 selector, but its own
value layout (exception channel v6, JSON schema 15), its own host ABI
(`abi::SCHEDULED_FRAMES_VERSION`, the microtask scheduler that drives
`varta_resume`) and its own value ABI (`value::DYNAMIC_VALUE_VERSION`); never
map another profile to either except v35, which inherits them. Eight selectors gate the streams, all
specified below: the value layout (`dynamic_values`), records
(`index_signatures`, `recursive_types`, `wide_records`), unknown
(`unknown_type`, `checked_assertions`) and async (`promises`,
`async_functions`). Gate every admission on its own selector, keep v33 and
earlier profiles frozen, and add each stream's rules under its own heading here.
Cross-stream behaviour (unknown cells holding value15 values, promise records
under value15 descriptors, recursive values in frames) is pinned by
`varta-runtime` `v34_cross_stream_tests`.

### Value layout 15 (`dynamic_values`)

Contract: [architecture.md](architecture.md#value-layout-15-dynamic_values).

- V34 stamps descriptors, record sizes and capture/frame catalogs with
  `LanguageProfile::value_layout`, which equals `value_abi` (value15). Value15
  keeps every value13 tag and layout with its value13 meaning; a new value15
  layout or tag is added only through its own selector and never changes what
  an existing tag means.
- A value-graph reader that accepts value13 accepts value15 by adding
  `DYNAMIC_VALUE_VERSION` next to `BIGINT_VALUE_VERSION`, never next to
  `FUNCTION_VALUE_VERSION` alone: value15 has no value14 function tag. Never
  let a value13 operation read value15 or the reverse.
- Value-reading host operations are bound to a layout. Add a v34 replacement
  through `HostOperation::value13_counterpart` (same signature, capability,
  results and recoverable errors; minimum host ABI
  `abi::SCHEDULED_FRAMES_VERSION`) and map it in
  `LanguageProfile::host_operation` for `dynamic_values()` only. Compiler call
  sites that name a value13 reader directly go through `host_operation`.
- JSON schema 15 belongs to v34; exception channel v6 reads thrown cells as
  value15. Keep the schema-13 grammar and channel-v5 contents unchanged.
- Prove equivalence for every change in this area: the same program under v33
  and v34 must log identical, Node-pinned output (`varta-runtime`
  `value15_tests`), and the conformance inventory must report identically
  under both profiles.

### Wide records (`wide_records`)

Contract: [architecture.md](architecture.md#wide-records-wide_records).

- Read the field ceiling from `ValueTypes::max_record_fields` (value15:
  `value::MAX_WIDE_RECORD_FIELDS`); never raise `value::MAX_RECORD_FIELDS`,
  which stays the frozen value13 ceiling of every older profile.
- Address presence only through `value::presence_bit`, `value::order_offset`
  and `Emitter::present_count`; never assume one presence word, a fixed `+ 4`
  order offset or a `u32` slot mask in compiler or runtime code (`u128` slot
  sets hold any value15 record).
- A record wider than 32 fields has a zero link word and a descriptor-trailing
  edge bitmap (`value::descriptor_bytes`); records with at most 32 fields keep
  the value13 layout byte for byte. Every value15 reader checks the bitmap
  against the field kinds, and the collector traces it for record blocks only.
- Lowerings that emit one leaf per field (lookups, enumeration, rest copies)
  must stay within the per-function body budget for the widest admitted shape;
  prefer a direct leaf for static keys.

### Index signatures (`index_signatures`)

Contract: [architecture.md](architecture.md#index-signatures-index_signatures).

- A dictionary is `Type::Dictionary(id)` with `ValueTypes::dictionaries[id]`;
  physically it is its storage record (`ValueTypes::record_layout`). Emit it
  only through `E::Record` with the `keys`/`values` slots in canonical order;
  never write a dictionary's vectors except through the lowerings in
  `lower::dictionaries`, which keep keys distinct and in own-key order.
- Every value15 reader of the tag goes through `value::Dictionary::read`; add
  `Type::Dictionary` wherever a lowering lists object types (typeof, truth,
  identity, guards, union members, catalogs, schemas) rather than relying on a
  wildcard.
- Keys are narrowed strings: never add ToPropertyKey coercion or prototype
  reads; absent inherited names throw, absent `__proto__` writes throw.
- Keep `value::MAX_DICTIONARY_ENTRIES` equal to the vector element bound and
  enforce it in insertion, literals and checked JSON.

### Recursive types (`recursive_types`)

Contract: [architecture.md](architecture.md#recursive-types-recursive_types).

- Resolve self-references only through `named::recursive` reservations, and
  only for non-generic declarations whose body is an object type, interface,
  index signature or union. Never unroll a declaration or erase a cycle.
- Any compiler walk over interned types (assignability, literal search, JSON
  admission, instance reachability, callback search, schema building) must
  carry a visited set; recursion on `Type` structure without one is a bug.
- Schema 15 may refer forward; capture and frame catalogs must not. Describe
  a back-edge with a shallow node of the same value kind
  (`CaptureType::Unknown` for a union, `CaptureType::Class` for a record) and
  reject a dictionary or array that reaches itself without an intervening
  union or record, instead of emitting a cycle.

### Unknown (`unknown_type`)

Contract: [architecture.md](architecture.md#unknown-unknown_type).

- `Type::Unknown` is a reference-union cell (`Type::is_cell`); never give it a
  value tag, layout or host import of its own. Emitter paths that test or unbox
  cells use `is_cell()`; lowering keeps `is_union()` for closed member sets, so
  every operation on `unknown` is an explicit decision (`lower::unknown`).
- Admit into `unknown` only types with a cell tag (`unknown::admissible`);
  widen through `unions::widen`, which boxes or retags. A union with an
  `unknown` member interns to `unknown`.
- Narrow only by tag-exact positive guards, class descriptor tests and the
  family rule of `instanceof Error`; never refine `unknown` to `object`, an
  array of unknown element type or by assignment. A refinement that unboxes a
  record or class must be proven by a descriptor test, never by the tag alone.
- Keep property access, indexing, calls, arithmetic, `in`, `Object.hasOwn`,
  `Number(x)` and typed JSON parsing rejected on `unknown`. `typeof x ===
  "object"`, `Array.isArray` and `in` do not narrow `unknown` to a dictionary
  (records and class instances answer the same); a checked
  `as Record<string, T>` is the way back.
- Under v34 `unknown` cells use value layout 15 and may hold dictionaries and
  wide records; every reader goes through `LanguageProfile::host_operation`.
  Promises have no value tag and are rejected with their own diagnostic.
- A flow join of paths that all refine an `unknown` binding keeps the joined
  refinement (`flow_fact` admits any narrower admissible type for a declared
  `unknown`); a path without a fact joins to `unknown`.
- Only identifier catch bindings become `unknown`; catch patterns, the frame
  exception binding and the exception channel keep the closed catch union, and
  `throw` of an `unknown` is a checked conversion to it.
- Capture and frame catalogs describe `unknown` as `CaptureType::Unknown`.
- Behaviour tests pin Node 24 output of `varta-runtime/src/unknown_cases`;
  regenerate expectations only from Node, never from Varta output.

### Checked assertions (`checked_assertions`)

Contract: [architecture.md](architecture.md#checked-assertions-checked_assertions).

- `x as T` is an upcast (ordinary widening, no test) or a tested conversion:
  hold the operand in a rooted hidden cell local, run `cell_test`, throw the
  `TypeError` on failure, then unbox or retag. Never emit an unchecked unbox of
  an asserted value.
- Admit a tested assertion only when TypeScript would (comparable members) and
  the target is checkable exactly: primitive and literal tags/values, exact
  record descriptors, class family descriptors, primitive-element array kinds
  and primitive-valued dictionaries (the `Dictionary` tag plus the value
  vector's element kind). Reject other targets with `unsupported type assertion to …` instead of
  approximating them (no element-wise array scans, no structural record match).
- `x!` is free on non-nullable operands and tested otherwise; `as const` keeps
  its v32 path; `<T>x` stays rejected.
- Failure messages quote the asserted type's source text; tests pin them,
  because Node erases assertions and never throws there.

### Promises and async functions (`promises`, `async_functions`)

Contracts: [scheduler](architecture.md#microtask-scheduler-promises),
[promises](architecture.md#promise-objects-promises) and
[async functions](architecture.md#async-functions-async_functions).

- Source admission lives in `lower::promises` (types, record operations,
  synthesized helpers), `lower::promises::methods` (`new Promise`, the statics,
  `then`/`catch`/`finally`) and `lower::promises::async_functions` (`await`
  and the async split). Gate `Promise<T>`, `new Promise`, the statics and the
  promise methods on `promises()`, and `async`/`await` on
  `async_functions()`; v33 and earlier keep `unsupported async, generator,
  generic or decorated function` and the existing `Promise<T> type` rejection.
- Promises have no value tag. Keep `Type::Promise` out of `value_kind()`,
  arrays, records and unions (`promises::opaque`), JSON, logging, `typeof` and
  tool/HTTP signatures. Combinators and promises in aggregates need a value tag
  and therefore the records stream's value15 layouts; until then reject them
  by name.
- Every promise job is a scheduled frame of a synthesized function: register
  it in `Asynchrony::scheduled` so `prepare` marks its machine `scheduled`.
  Never add a second job mechanism (a guest queue, closures run by the host or
  a table call): the runtime runs jobs only through `varta_resume` with
  `Normal` and `undefined`, and every value crosses through guest memory.
- Synthesize helpers through `Lower::synthesize`, which gives each function
  its own locals and lifts resolving closures over one captured promise
  binding; memoize them by kind and types. Build bodies from ordinary IR
  (record fields through `Reinterpret`, `Zero` for unset slots, `Host`
  scheduler calls with `Float64` serials and ids). Only the suspension point
  touches resume tokens; keep the token out of IR.
- Preserve the job counts that make event logs match Node: a reaction is one
  job, adoption of a promise is an adoption job plus its reaction, `await` of
  any value is one job, and `finally` adopts `PromiseResolve(f()).then(...)`.
  Settle through the `Fulfill`/`Reject` helpers only, so a pending check,
  `promise_settle` for registered reactions and rejection tracking happen in
  that order.
- Scheduled suspension points issue the generation before registering the
  token, and their dispatcher branches return without issuing another. Keep
  v33 dispatcher bytes unchanged for modules without scheduled layouts.
- The async split keeps the declaration's index as the wrapper; the body gets
  the promise as its last parameter and rewrites `return` into a labelled
  break, so `finally` blocks run before settlement. Never settle the promise
  before the body completes.
- `await` is an unknown reentry for write-aware refinements
  (`flow_reentry`). `await` inside unreviewed intrinsic value blocks stays
  rejected by construct name (`unsupported await inside <construct>`).
- Scheduler failures are fatal `SchedulerError`s and never rejections; the
  scheduler imports are not charged to the host-call budget but keep their own
  bounds. Keep production budgets unchanged; fit tests into them.
- Behavior tests compare against pinned Node output (`varta-runtime`
  `promise_cases`, `promise_tests`, `async_tests`); regenerate expectations
  only from Node, never from Varta output.

## Profile v35 (in development)

Inherit v34 under checked-f64-utf16-v35 with every v34 selector and the
scheduler. The async stream owns the batch's ABI bumps: v35 declares host ABI
`abi::PROMISE_VALUES_VERSION` and value ABI/layout `value::PROMISE_VALUE_VERSION`
(value16), read by the value16 host operations, exception channel v7 and JSON
schema 16; v34 keeps host ABI 30 and value15. Seven selectors gate the streams:
Web (`web_headers`, `abort_signals`), records (`mixed_index_records`,
`unknown_json`), async (`async_methods`, `promise_values`) and frames
(`frame_liveness`), each specified below. Gate every admission on its own selector, keep v34 and
earlier profiles frozen, and add each stream's rules under its own heading here.

- A stream that adds a host import adds a new `abi` constant and maps only v35
  to it in `LanguageProfile::host_abi` in the same change, with the import's
  minimum ABI, the launcher maximum, the version matrix and the v35 ABI tests.
  A stream that adds a value tag or layout does the same for `value_abi` and
  `value_layout` and binds new readers to the new version; v34 readers stay
  frozen. Bump each ABI once per batch: the first stream owns the bump and the
  others rebase onto it.
- Tests that assert v35 equals v34 (`pilot_v35_inherits_v34_…` in
  `varta-common` `language`, the v35 runtime language test and the v35 CLI
  smoke test) change in the stream that introduces the difference.
- `frame_liveness` changes the frame layout of v35 modules only. V34 and
  earlier modules keep their frame slots and dispatcher bytes; any catalog
  change the runtime reads is admitted for v35 alone.
- `web_headers` and `abort_signals` are guest-implemented engine types with no
  capability and no host I/O; `AbortSignal` has no timers until a later batch.

### Value layout 16 (`promise_values`)

- Value16 is value15 plus the promise tag (`ValueKind::Promise`). Every reader
  that accepts value15 accepts value16 through the same code path: extend a
  version pattern with `PROMISE_VALUE_VERSION` (or use `value::dynamic_layout`,
  `json::dynamic_schema`), never copy a reader. Value15 readers keep rejecting
  value16 stamps and the promise tag, and value16 readers reject value15.
- Each value16 host operation mirrors its value15 counterpart
  (`HostOperation::value15_counterpart`): same signature, result convention,
  capability (none) and budgets, minimum host ABI `PROMISE_VALUES_VERSION`.
  Register it as one more row of the existing per-layout tables; select it only
  through `LanguageProfile::host_operation`, which maps every value15 reader to
  value16 for v35 alone.
- A stream that needs another layout-bound operation or tag under v35 rebases
  onto this layout and adds the value16 variant only; it does not bump the
  value or host ABI again in this batch.

### Async methods and closures (`async_methods`)

- Split every async method, arrow and function expression through
  `AsyncSplit` (`lower::promises::async_functions`): the wrapper keeps the
  source function's index, receiver, self slot, captures and shape; the body
  takes the wrapper's physical parameters plus the promise, which must be the
  slot right after them. Never give the body a receiver or a self slot.
- An async closure's body is an environment function with the closure's
  captures; `Schedule` forwards the wrapper's environment
  (`Emitter::environment_param`) and the frame creator stores captured cells as
  shared slots. Do not copy captured values into the body: mutable captures
  must stay one cell. `suspension::admissible` admits environment functions only
  for v35 scheduled bodies; generator closures and self slots stay rejected.
- Keep v34 frozen: async instance methods, async arrows and function
  expressions, async optional/rest parameters and `Promise<T>` inference in
  generic calls are gated on `async_methods()`; v34 keeps its diagnostics.
- Tests compare against pinned Node logs (`varta-runtime`
  `async_method_tests`, `async_method_cases`); GC tests force a collection
  before every job with the test hook, paying collections from a test-only
  fuel refill, never by raising a production budget.

### Promise values (`promise_values`)

- Promise tags exist only in value16: lowering admits a promise in an
  aggregate through `Type::profile_kind` and emission writes its tag through
  `profile_kind`/`layout_kind`; `Type::value_kind` stays untagged so v34 and
  earlier keep every rejection (arrays, records, unions, dictionaries,
  `unknown`, `typeof`, logging, `JSON.stringify`, the combinators by name).
- At most one promise type per union; `Promise<T>` never admits a `T` that may
  hold a promise. Await and resolve a cell by its tag (`promises::values`), so
  a promise member keeps the one-job PromiseResolve path; never box or copy a
  promise into a new promise.
- A promise inside `unknown` is never adopted silently: every path that would
  adopt one without a static `T` throws the documented `TypeError`.
- Combinators are guest code: one memoized job frame per combinator and
  element type, a private state record and ordinary helpers. Add no host
  import and no scheduler bound; keep the element snapshot and index-order
  starts so job order matches Node.
- The runtime reads a promise only through the value16 readers: its record
  block is validated like a record, and logging locates `state`, `value` and
  `reason` by the shared `scheduler::PROMISE_*_FIELD` names.

### Frame liveness (`frame_liveness`)

Contract: [frame liveness](architecture.md#frame-liveness-frame_liveness).

- `lower::suspension::liveness` computes the saved set of every state on the
  normalized body after `Machine::number` and before the state machine is
  built; `emit::frame_slots` emits liveness segments, per-state saves, the
  `br_table` restoration and the creator. `Suspendable::live` is `Some` only
  under `frame_liveness()`; every other profile takes the unchanged cell path,
  and `frame_liveness_tests` pins their module digests. Never touch the cell
  path's instruction sequence for a liveness need.
- The analysis may over-approximate, never under-approximate. Kill only on a
  statement-level `Init`/`Set` of an unshared local; count every other mention
  (expression-level writes, hidden statement slots, closure captures) as a
  use; never kill a closure-shared local, and count a statement-level `Set`
  of one as a use (it writes through the existing cell; `Init` allocates a
  fresh one). Add the exception continuation
  before every statement and both completion continuations at every point.
  A new IR statement or expression that names a local must be added to the
  liveness visitors (`node_mentions`, `statement_fields`, `operands`,
  `substatements`) and to `Liveness::statement`; an unknown branch target
  falls back to every local.
- A hidden local the state machine reads on the resumption path (today the
  `switch` case index and the finalizer's kept completion) is recorded in
  `Machine::resumed` with the states it covers.
- Saved entries are packed from zero per state; two-slot values take two
  entries and shared locals one (their closure cell). Publish a state's edge
  masks only after its entries are stored, clear them at resumption after the
  values are restored and rooted, and never allocate in between. The creator
  publishes the state-0 masks before allocating shared parameter cells.
- State 0 of an async closure body (`async_methods`) saves its captured
  environment cells after the parameters; the creator stores the loaded cells
  themselves, never copies (`varta-runtime`
  `async_method_tests::async_closure_captures_stay_shared_cells_in_liveness_frames`).
- Restoration validates every touched segment (handle length, flags, kind,
  count, the state's exact mask) and canonical scalar payloads before use;
  keep the default `br_table` target a trap.
- Liveness rows exist only in catalog version 3 and are admitted only for
  v35 (`admission::frame_catalog_version`); test relabelled modules in both
  directions and tampered rows (`admission_tests`, runtime
  `frame_admission_tests`). Existing suspension suites run under v35 too
  (`promise_tests::FRAME_PROFILES`, `generator_tests::run`,
  `suspension_tests::behaviour_holds_with_liveness_frames`).
- Record fuel figures only from `frame_liveness_tests` (deterministic fuel
  under production budgets) and update the table in the contract when the
  frame or scheduler path changes.

### Unhandled-rejection reports (all scheduler profiles)

- Copy reasons only through `Channel::render_bounded` with
  `MAX_REJECTION_REASON_UNITS`, keep at most
  `MAX_RETAINED_REJECTION_REASONS` rendered reasons and one tracking bit per id
  otherwise. Never store a host value whose size follows guest input; never
  change which reports are made or withdrawn, because the bound applies to v34
  as well.

### Mixed index records (`mixed_index_records`)

Contract: [architecture.md](architecture.md#mixed-index-records-mixed_index_records).

- A mixed record is `Type::Dictionary(id)` whose `DictionaryType::declared`
  indexes `ValueTypes::mixed`; its storage, keys and values are those of the
  plain dictionary of the same value type. Intern it only through
  `mixed_records::intern`, which checks TS2411 assignability and checkability
  of every declared field. Never give declared fields a slot, tag or layout.
- Declared reads go through `declared_get`: look the key up, convert with
  `assert_value` (free when the index type is the declared type) and throw for
  a missing required key. Never trust a stored entry because its key is
  declared: dynamic writes, plain-dictionary aliases and deletes reach it.
- Dictionary conversions are retags between types of the same storage
  (`dictionaries::converts`); keep them in `arguments::accepts`,
  `unions::widen` and `unknown::upcast` together. `object` views convert only
  through a checked assertion.
- Under v35 dictionary get/set/delete call the shared function of their value
  type (`lower::dictionary_helpers`); build their bodies only from the inline
  builders (`dictionary_get_inline`, `dictionary_store`,
  `dictionary_delete_inline`) and keep v34 on the inline path so its bytes do
  not change.
- `for…in`, updates and spreads evaluate the receiver and key once and read an
  entry before evaluating the right-hand side. `for…in` snapshots the keys and
  re-checks presence before each visit; spreads start from an empty dictionary
  and apply members in source order.
- Keys convert only from strings and (v35) numbers through `ToPropertyKey`
  (`dictionary_key_value`); never coerce other types.
- Optional class fields are `T | undefined` with an `undefined` initializer.
  A redeclared inherited field keeps the shared slot and the inherited type
  exactly; a non-`declare` redeclaration appends an initializer, and
  `Field::redeclared` lets the redeclaring constructor assign it.
- Behaviour tests pin Node 24 output of `varta-runtime/src/v35_cases`;
  regenerate expectations only from Node, never from Varta output. Checked
  deviations (declared reads after dynamic writes) are pinned separately.

### JSON to unknown (`unknown_json`)

Contract: [architecture.md](architecture.md#json-to-unknown-unknown_json).

- `JSON.parse` and `unknown` parse targets build the JSON value graph from
  existing schema nodes, stamped schema 16 under v35 (`emit::schemas`,
  `json::unknown_graph`), and read them only through the value16 readers
  (`v35_cross_stream_tests`); intern the
  `unknown[]` and `Record<string, unknown>` types while lowering, because the
  emitter only reads `ValueTypes`. Never add a host-side `unknown` node or a
  schema version for it.
- `JSON.parse(text) as T` becomes the checked parse at `T` only when `T` is a
  parse target; otherwise it is the ordinary assertion from `unknown`.
- Dictionary assertion targets are proven by the `Dictionary` tag and the
  exact storage descriptor (`IsClass` over the storage shape), for any value
  type; v34 keeps its element-kind test. A closed source union's array,
  dictionary, record or class member is proven by its tag alone (v35 only).
- `Type::Object(nullable)` is a refinement fact, never an expression type:
  identifier reads of an `object` fact return the `unknown` cell, and only
  `object_value` consumers (`in`, `Object.keys`/`values`/`entries`/`hasOwn`,
  `for…in`) use it, through the checked dictionary conversion. Unions with an
  `object` fact join to `object` (with `null` and `object` views) or `unknown`.
- `in` guards refine to `object` views (`MixedMembers::object`) that declare
  the proven keys as `unknown`; views reject dynamic and undeclared keys and
  never convert implicitly to other dictionaries.

### Web stream (`web_headers`, `abort_signals`)

Contract: [engine Web types](architecture.md#engine-web-types-web_headers-abort_signals).

- Engine Web types are typed TypeScript preludes in
  `crates/varta-compiler/src/prelude/`, registered in `modules::engine`
  (`PRELUDES`, gated by their selector). Implement behavior in the prelude
  source, not as compiler intrinsics, so classes, dispatch, rooting, frames and
  diagnostics stay on the shared pipeline. A new engine type adds a prelude (or
  an export of one) with its selector, never a host import or capability.
- A prelude loads only when a module names one of its exports and does not
  declare or import that name; keep programs that name none byte-identical to
  the same profile's output without preludes (compiler
  `programs_that_do_not_name_web_types_compile_as_without_preludes`, through
  the test-only `engine::without_preludes`; v35 no longer shares v34's ABIs,
  so v34 bytes are no reference). Prelude paths stay
  reserved from host loaders and out of module metadata.
- Write preludes for the production budgets: no per-code-unit `charCodeAt`,
  `%`, union `===`/`!==` against `null`/`undefined` or class identity in loops
  (each is a host call); narrow chains with `instanceof`. Code-unit work uses
  the `varta.web.*` intrinsics (`lower::web_text`, `ir::TextUnits`), which
  exist only in prelude modules. Keep
  `headers_typical_workload_stays_within_production_budgets` passing rather
  than raising a budget.
- Pin every behavior test to Node 24 output (`web_cases`, `web_headers_tests`,
  `abort_signal_tests`); mark Varta-specific results. Validation messages
  match Node's text.
- Members a prelude names but cannot implement are listed in
  `engine::PENDING` with their reason, so programs get an `unsupported`
  diagnostic instead of an unknown-member error. Abort listeners need
  function-valued object fields, a value-layout change: keep them pending
  until a stream owns that layout bump. `AbortSignal.timeout`/`any` stay
  pending in v35; v36 implements `timeout` (see Timers below).
- The fewer-parameter rule for function literals is gated by `web_headers()`;
  earlier profiles keep the exact-count rule.

## Profile v36 (in development)

Inherit v35 under checked-f64-utf16-v36 with every v35 selector, the
scheduler and frame catalog version 3. V36 has its own host ABI
`abi::TIMERS_VERSION` (32) and its own value ABI/layout 17
(`value::FUNCTION_FIELD_VALUE_VERSION`) with the value17 host operations,
exception channel v8 and JSON schema 17; v35 keeps host ABI 31 and value16.
Four selectors gate the streams, all implemented: records
(`checked_declared_fields`), functions (`function_fields`), async
(`async_generators`) and timers (`timers`). Gate every admission on its own
selector, keep v35 and earlier profiles frozen, and add each stream's rules
under its own heading here.

- A v35 rule or binding stated "for v35 only" (value16 readers, liveness
  frames, preludes, mixed records) extends to v36 through the inherited
  selectors; never widen it to v34 or earlier.
- Every host import the batch adds takes `abi::TIMERS_VERSION` as its minimum
  ABI in the `HostOperation` registry; do not add another host ABI constant in
  this batch. Add its `.d.ts`, linker, validation and denial tests in the same
  change.
- Value layout 17 (the function-fields stream's bump) is in place: v36
  stamps value17 everywhere and binds the value17 readers
  (`HostOperation::value16_counterpart`, minimum ABI 32). A reader that accepts
  value16 accepts value17 through the same code path (`value::dynamic_layout`,
  `value::promise_layout`, `json::dynamic_schema`), never by copying it; only
  the function tag differs, admitted by `value::function_layout`. Value16
  readers stay frozen for v35. Prove every change here with `value17_tests`:
  the v35 cases must log identically under v36.
- Value17 function cells are checked through `function::ClosureView`
  envelopes only; never decode captured payloads or invoke guest code from a
  host reader, and never accept the function tag in a value16 or older reader
  other than the catalog-bound value14 codec.
- Timers are scheduled by the runtime, never by guest busy-waiting; keep
  production fuel, deadline and scheduler limits unchanged and pin timer
  ordering against Node output.
- Cross-stream behavior is pinned in `varta-runtime` `v36_cross_stream_tests`
  (Node 24 logs, virtual clock, a collection forced before every job): every
  stream's programs validate under value17 and import only value17 readers;
  timer callbacks and abort listeners held in function fields stay rooted;
  `for await` over generators that sleep on timers; declared reads under
  optional chains. Add a case there when a change spans two streams.
- `AbortSignal.any` lives in the v36 abort prelude
  (`prelude/abort_listeners.ts`) with the listeners; `AbortSignal.timeout` stays
  in its own prelude (`prelude/abort_timeout.ts`), loaded only where called, so
  no other program imports a timer operation or needs `time.timer`. Keep
  `prelude/abort.ts` (v35) byte-identical.
- The conformance inventory pins v36. A case that needs a capability lists it
  in its reviewed `grants` (only `time.timer` is admitted); never grant a
  capability to turn a denial into a pass.

### Function fields (`function_fields`)

Contract: [architecture.md](architecture.md#function-fields-function_fields).

- A function type is a value type only through `Type::profile_kind` /
  `layout_kind` (value17 tags it `ValueKind::Function`); `value_kind` stays
  `None` for functions so every pre-v36 admission check keeps rejecting them.
  Gate each lifted rejection (record, class, dictionary, union, element,
  `unknown`, logging) on `function_fields()` and keep the earlier diagnostic
  text byte for byte for older profiles.
- A union holds at most one function type: guards, `?.()` and contextual
  typing select "the" function member. Never add signature tests to cells;
  the call's checked closed dispatch (signature word, target code, environment)
  is the only proof a function value has its static type.
- Calls through a field go through `function_field_callee` (a side-effect-free
  static peek) before builtin method names; the field is read once and called
  without a receiver. Reject a receiver-typed function at such a call instead
  of passing `undefined` or the object as `this`.
- Optional chains lower the first optional link's object once into a hidden
  `#chain` binding narrowed by a refinement; everything after it is the
  ordinary AST lowered again on that binding. Keep the rewrite AST-only (no
  second lowering of the object) and parentheses as chain boundaries.
- Built-in function values are synthesized once per program
  (`ValueTypes::builtin_functions`) and referenced canonically; add a name to
  `function_fields::BUILTIN_VALUES` only with its own body and Node-pinned
  identity test.
- The v36 abort prelude is a separate source (`prelude/abort_listeners.ts`)
  selected by `Prelude::text`; never edit `prelude/abort.ts` for a v36
  feature. `v35_modules_keep_their_bytes` freezes v35 output (including the v35
  prelude) by digest; a change there is a v35 regression, not a new baseline.
- JSON: a function serializes like `undefined`; reject a reachable
  function-valued `toJSON` field at compile time. Checked parsing never builds
  functions.
- Tests: Node-pinned `function_field_cases` logs, the forced-collection
  driver for closures reached only through containers, descriptor-tag
  tampering, v35 rejection and frozen-digest tests, the public API and CLI
  smoke tests and the v35-pinned conformance gaps.

### Checked declared fields (`checked_declared_fields`)

Contract: [architecture.md](architecture.md#checked-declared-fields-checked_declared_fields).

- All conversions live in `lower::declared_fields`. `convertible` admits a
  declared type of a mixed record indexed by `unknown`; `applies` selects the
  conversion (source `unknown`, a record, array or dictionary member) for
  declared reads, checked parsing (`json::validate_declared`) and assertions
  (`unknown::assert_value`). Other sources and targets keep the v35 tests.
- Return a stored value only when `exact_test` proves its static type. Never
  treat an empty array, an array with holes or an array of union cells as
  exact, and never prove an array of class instances by its elements: another
  alias may hold the same array at another element type. Structural records
  are proven by any layout descriptor of their shape (`IsClass` in v36);
  classes by their family.
- Builds start from the JSON-graph forms only (`Record<string, unknown>`
  storage, arrays of cells or of the target's element kinds). Records accept
  exactly their declared keys, in the dictionary's order, through `SetField`
  so presence and order trailers stay exact. Root every handle local of a
  synthesized function in its value block; built values are published only
  after every child converted.
- Synthesized functions (`Converter::{Exact, Build, Read}`) are keyed by the
  target type in `ValueTypes::converters` and registered before their body is
  lowered, so recursive types recurse. Use `reserve_function` and
  `define_function`; do not inline conversions at each read (the per-function
  body budget).
- Declared reads write a built value back through the shared dictionary `Set`
  helper under the found key; parse validation calls the same read. Do not
  write back from assertions.
- The completions (dictionary binding patterns, `typeof` property-path
  conditionals, `??`/`||` literal fallbacks, `let x: unknown;`, exhaustive
  `switch` returns through `completion::exits_function_through_switches`) are
  gated on this selector; keep their v35 diagnostics.
- Pin behavior to Node 24 output of `varta-runtime/src/v36_cases`; pin the
  checked deviations (`checked_conversions_deviate_from_node`) separately.

### Timers (`timers`)

Contract: [architecture](architecture.md#timers-timers). Lowering lives in
`lower/promises/timers.rs`; the event loop in `varta-runtime` `scheduler.rs`.

- A timer is a scheduled frame of a synthesized function (memoized per
  `Helper::Timer(kind)` and callback/argument types) that awaits an
  engine-internal pending `Promise<void>`. Keep it that way: the frame is what
  roots the callback, captures and arguments, and the runtime must never hold a
  guest reference or call a function value directly. The runtime resumes timer
  frames only through `varta_resume`, like every job.
- Never settle the timer promise record; the runtime only queues its one
  reaction. Fired and cleared timers are told apart by `timer_take`; an
  interval frame loops on `timer_take` and awaits the same promise again, and
  `Scheduler::react` releases a cleared interval's frame at once.
- `timer_start` is the only timer import with a capability (`time.timer`,
  exact boolean grant) and the only one charged to the host-call budget; check
  the grant before the timer exists and keep denial fatal
  (`SchedulerError::TimerDenied`). Clear/take/uncaught stay bookkeeping.
- Loop time is virtual: it starts at 0 and moves only to the due time of the
  timer that fires. Never derive a due time from the wall clock; the runtime
  only waits for the wall clock to catch up. Order is (due bits, sequence).
- A timer due at or after `abi::DEADLINE_SECONDS` must end the execution with
  `SchedulerError::TimerDeadline` without waiting. Unreferenced timers
  (`TIMER_UNREF`, only `AbortSignal.timeout`) never keep the loop alive.
- Check unhandled rejections after every microtask checkpoint, before the
  next timer.
- Runtime tests run on `scheduler::with_virtual_clock` (test-only, never in a
  production build); keep at most a lower-bound assertion on real waits so no
  test depends on timing.
- `AbortSignal.timeout` is the `ABORT_TIMEOUT` prelude, loaded by the member
  trigger `AbortSignal.timeout` and bound under that dotted name, which source
  cannot declare; `varta.web.unrefTimeout` is a prelude-only intrinsic. Programs
  that never name it must not import a timer operation.
- Under `timers()` an arrow expression body in a `void` context discards its
  value (TypeScript's void compatibility); earlier profiles keep "arrow
  expression result mismatch".
- Timers belong to `main` executions only; when async HTTP or tools are
  admitted, give each request isolate its own loop and cancel its pending
  timers when the request ends, rather than sharing a queue.

### Async generators (`async_generators`)

Contract: [architecture.md](architecture.md#async-generators-async_generators).

- Source admission lives in `lower::promises::async_generators` (annotations,
  the split, `yield`/`await`/`return` in bodies, the protocol helpers and every
  `for await` source) and is gated on `profile.async_generators()` only:
  `signature::check`, the annotation names in `keyword_type`, generic
  inference from `AsyncIterable<T>`-like parameters, the `for await` entry in
  `for_of.rs`, the checked-assertion entry in `suspension::construct` and the
  `Number`/`Math` constant parameter default in `parameters::literal`. V35 and
  earlier keep `unsupported async generator function`, the v34 `for await`
  rejection and `unsupported await inside intrinsic expression`.
- An async generator type is a `GeneratorType` with `asynchronous` set; its
  objects are frames of the paired body type (`GeneratorType::body`), reached
  by `Reinterpret`. Intern both through `async_generators::intern`, which also
  interns the state and request records and the result promise. Never give an
  async generator a value tag, a layout or a frame of its own type: no layout
  may name the object type, and synchronous `for...of`, `yield*` and the
  synchronous protocol code must reject it.
- The body's completion cell is the only route from an object to its state
  record. Publish it at the start point, keep it in the body type's step
  function (`step_function(.., keep_completion)`) and in every later
  suspension (`Suspendable::keeps_completion`), and end the body with
  `return state`. Never read a body's `GeneratorValue` as anything but the
  state record, and never clear it.
- Resume a body only through its step function with its own frame handle as
  the input (the start point's input is that handle, saved for the relays):
  synchronously from the `Resume` helper, or from a relay job. The body is a
  generator frame, never a scheduled one: an `await` in it carries
  `Expression::Await::relay` and the normalizer starts the relay right before
  the suspension and publishes the state record instead of the promise
  (`Normalizer::publish`). Keep one relay per pending await; do not add a
  second job mechanism.
- Read every resumption value from the front request; do not deliver `Throw`
  or `Return` completions through the step function (its deliveries only keep
  the frame layout uniform). The helpers mirror the ECMAScript abstract
  operations one to one (Enqueue, Resume, CompleteStep, DrainQueue,
  AwaitReturn, UnwrapYieldResumption); keep their job counts, because the
  pinned Node logs depend on them. Memoize the helpers per async generator
  type through `async_helper`, which reserves the index before synthesizing
  (Drain, AwaitReturn and its job call each other).
- Body `return`s become a store of `TReturn | undefined` and a break to the
  body's exit label (`Exits`), so `finally` blocks run first; every rewritten
  `TryFinally` keeps a pending state-machine `Return` in a fresh local of the
  state record type. A `yield` is a value block at the `yield` span, which the
  construct allowlist splits; it may `throw` and break to the exit label.
- `yield*` (`async_delegate`) forwards requests to an async generator
  delegate through its protocol helpers and passes yielded values on through
  the same CompleteStep and suspension as `yield`, without awaiting them; keep
  its delegate restricted to async generators with the same `TNext` until a
  source with its own pinned Node log is added.
- `for await` over synchronous sources must keep the async-from-sync job
  counts (two per value and per done result, one or two to close, the first-job
  closing of a rejected value); add a source only with its pinned Node log.
- Behaviour tests compare against pinned Node 24 output (`varta-runtime`
  `async_generator_cases`, `async_generator_tests`, which also force a
  collection before every job and record the fuel figures of the contract);
  regenerate expectations only from Node, never from Varta output.

## Profile v37 (in development)

Inherit v36 under checked-f64-utf16-v37 with every v36 selector, value layout
17 and the value17 host operations, exception channel v8, JSON schema 17, the
scheduler, frame catalog version 3, async generators and the timer operations.
V37 has its own host ABI `abi::NET_REQUEST_VERSION` (33) and shares value
ABI/layout 17 (`value::FUNCTION_FIELD_VALUE_VERSION`) with v36; v36 keeps host
ABI 32. Five selectors gate the streams and admit nothing yet: language core
(`language_core`), class generics (`class_generics`), collections and binary
text (`collections`), RegExp (`regexp`) and network requests (`net_request`).
Gate every admission on its own selector, keep v36 and earlier profiles
frozen, and add each stream's rules under its own heading here.

- A v36 rule or binding stated "for v36 only" (value17 readers, timer
  operations, preludes, async generators) extends to v37 through the inherited
  selectors; never widen it to v35 or earlier.
- Every host import the batch adds takes `abi::NET_REQUEST_VERSION` as its
  minimum ABI in the `HostOperation` registry; do not add another host ABI
  constant in this batch. Add its `.d.ts`, linker, validation and denial tests
  in the same change.
- The collections stream owns the batch's value-layout bump (value 18) and
  lands it first as a foundation commit: a new `value` constant,
  `value_abi`/`value_layout` for v37, value18 variants of every layout-bound
  reader (minimum ABI 33) and the launcher value maximum. Value17 readers stay
  frozen for v36. The other streams rebase onto it.
- `net.request` is a new exact capability operation, denied by default and
  checked before any socket is opened; never let `net.fetch`, `net.get` or
  `net.listen` grants authorize it.
- Tests that assert v37 equals v36 apart from its host ABI
  (`pilot_v37_inherits_v36_…` in `varta-common` `language`, the v37 runtime
  language test and the v37 CLI smoke test) change in the stream that
  introduces the difference.

### Class generics (`class_generics`)

Contract: [architecture.md](architecture.md#class-generics-class_generics).

- Monomorphization only, as for [generics](#generics-generics): never emit a
  generic body. Generic methods live in `Class::generic_methods` /
  `generic_statics` (`lower/classes/generic_methods.rs`), get no slot at
  layout and are instantiated per (owner, name, static, arguments) in
  `Classes::method_instances`; their bodies queue in `pending_methods` and are
  lowered by the `generics::bodies` fixpoint with the class context and the
  merged class + method frame. Reuse `Lower::infer`, `instance_arguments` and
  `bind_instance_arguments`; never re-lower an argument for inference, and
  lower the receiver first.
- An overridden generic method dispatches through `generic_dispatchers`
  (root, name, arguments), instantiating every implementation with the same
  arguments when the dispatcher is created and synthesizing its body in
  `generic_dispatch_bodies` after the last body. `super.m<T>()` and statics
  are direct calls.
- Generic class templates (`Class::is_template`) own statics only; instances
  (`Class::template`, `frame`) are created by `instantiate_class` (memoized in
  `Classes::instances` before layout so self-references resolve), laid out at
  once when `Classes::ready` and otherwise at `lay_out_deferred` (start of
  `dispatchers`). Instance members are allocated `UNALLOCATED` and get their
  slots in `materialize` (end of `dispatchers`, and `Lower::class_functions`
  before any member call, `new`, getter or `super` use), which also rewrites
  the inherited entries of descendants; their bodies queue in
  `pending_members`. Never let an instance member index reach emission
  unallocated, and never put instance members in `Classes::order`.
- V37 synthesizes the declared dispatchers late (`late_dispatch_bodies`) so
  instance shapes created by later bodies join the descriptor tests. Keep
  `dispatch_bodies` at its v36 position for every earlier profile
  (`v36_modules_keep_their_bytes`). Instances may not override inherited
  methods (`override_rejected`): a dispatcher decided before the instance
  existed would miss the override.
- Method signatures are function fields (`records::method_signature`); object
  literal methods are function expressions that may not use `this`/`super`
  (`uses_receiver`). Never bind a receiver for a function field call.
- `implements` is a check only (`classes/implements.rs`) after the class (or
  instance) is laid out; never widen a class instance to a record.
- Private names are a pre-layout AST rewrite (`classes/private_names.rs`):
  `#x` becomes a `private` member named `#x`. Keep quoted `'#x'` members and
  computed `obj['#x']` member accesses rejected under the selector, keep
  `reaches_private` gating `JSON.stringify` and `unknown::widen`, and keep a
  subclass from redeclaring an inherited private name. `#x in v` is
  `class_test` of the declaring class.
- Tests: `varta-runtime` `class_generic_tests` with Node 24 logs of
  `class_generic_cases`, the forced-collection driver, v37 diagnostics, v36
  rejections and `v36_modules_keep_their_bytes`, and the pilot function-count
  measurement; the public API cross-module test and the CLI smoke test.
