// Select languageProfile in project configuration. checked-f64-utf8-v1 uses
// binary64 numbers, host ABI v2 and aggregate/JSON value ABI v3. Tool/HTTP
// adapters, UTF-16 and undefined remain pending for that profile.
// checked-f64-utf8-v2 adds distinct undefined/void-expression semantics using
// host ABI v3/value ABI v4. JSON omits undefined object fields, emits null for
// undefined array entries and returns undefined for an undefined root.
// checked-f64-utf16-v1 adds UTF-16 code-unit strings (including lone surrogates),
// .length, .charAt(number?) and .charCodeAt(number?) with host ABI v4/value ABI v5.
// Resource names reject lone surrogates/NUL before OS access; host UTF-8 text
// converts exactly. JSON preserves lone units using escapes; logs display U+FFFD.
// JSON schema v4 remains strict. Tool/HTTP adapters and dynamic values/keys are pending.
// checked-f64-utf16-v2 adds closed primitive unions, direct-local guard narrowing,
// strict equality/truthiness and union short-circuit operators (host ABI v5/value v6).
// Guarded writes, object/array members, casts, any/unknown and uninitialized locals
// remain unsupported. JSON union roots return string|undefined; schema v5 parsing
// remains non-union. Encoding/resource permissions and pure-operation status persist.
// checked-f64-utf16-v3 adds optional primitive fields, Object.hasOwn(record,literalKey),
// missing primitive-array/string numeric reads and undefined-capable let/returns
// (host ABI v6/value ABI v7/schema v6). Absence differs from explicit undefined.
// Indexed writes remain bounded; reference-array reads, prototype values and
// optional/union typed JSON parsing remain unsupported. Earlier profiles stay frozen.
// checked-f64-utf16-v4 adds record/array reference unions and Array.isArray guards
// (host ABI v7/value ABI v8/schema v7). One record shape and one array element type
// per union; aliases/identity are preserved. Optional reference fields and nullable
// indexed object/array reads are supported. typeof object still includes null.
// Union reference logs use bounded JSON. Strict parsing stays non-union/non-optional;
// broader shapes, resizing and external adapters remain pending.
// checked-f64-utf16-v5 adds strict typed JSON parsing of optional fields and unions
// (host ABI v8/value ABI v8/schema v8, json_parse_v9). Missing optional fields stay
// absent; required fields still must be present even if their union admits undefined.
// JSON-kind selection is unambiguous; duplicates/unknown keys and wrong types trap.
// Serialization retains json_stringify_v8. Shape/array-type union limits persist.
// checked-f64-utf16-v6 adds dense resizable arrays with push(...items):number and
// pop():T|undefined (host ABI v9/value ABI v9/schema v9). Stable identity points to
// geometric backing storage; removed slots are cleared. The bound stays 1024.
// Indexed writes replace existing elements; length assignment, holes, spreads,
// constructors, iterators and methods beyond push/pop/at/slice/includes/indexOf remain unsupported. JSON uses
// json_stringify_v9/json_parse_v10; v3 value imports validate the new representation.
// The same v6 profile adds at(index?):T|undefined and slice(start?,end?):T[].
// Bounds admit number/undefined and their union; fractions truncate, NaN is zero,
// negative bounds are relative to the end and infinities are bounded safely.
// Undefined end uses length. Slice has fresh storage/identity with shared children.
// Bounds/receiver effects occur once before reading the current length/backing.
// These pure compiler operations retain ABI/layout v9; coercive bounds, borrowed
// methods and sparse/prototype behavior remain unsupported.
// V6 also admits includes(value?,fromIndex?) and indexOf(value?,fromIndex?).
// Needles can be any checked value; different kinds never coerce to match.
// Includes finds NaN (SameValueZero), indexOf does not (strict equality).
// UTF-16 strings compare contents; records/arrays compare identity. Starting
// bounds use the numeric/undefined rules above. Argument effects run once even
// for empty arrays; the subsequent allocation-free scan preserves status/authority.
// checked-f64-utf16-v7 adds primitive throw/try-catch across named calls (host ABI
// v10, value/schema v9). Catch binds number|boolean|string|null|undefined; use
// guards before type-specific operations. Nested handlers/rethrow and optional
// bindings and finally are supported. Finalizers preserve/override return, throw,
// break and continue with rooted saved values. Error objects, aggregate throws and catch
// annotations/destructuring remain unsupported. Runtime/resource/JSON/index
// traps and authorized exit bypass catch/finally; host status still uses result/capture.
// Uncaught values leave the runtime as owned typed errors with bounded diagnostics.
// checked-f64-utf16-v8 adds nominal Error (host ABI v11/value v10/schema v10).
// Error(message?) and new Error(message?) accept string|undefined once. name and
// message are immutable in this subset; toString(), instanceof Error narrowing,
// own-message presence and non-enumerable JSON behavior are supported. Error may
// occur in calls, records, arrays/unions and catches. Plain records are not Error.
// Mutation, coercion, cause/options, subclasses/native families, prototypes and
// stack traces are unsupported. Typed JSON cannot construct Error. v4 value ops
// and json_stringify_v10/json_parse_v11 preserve status and fatal boundaries.
// checked-f64-utf16-v9 adds TypeError/RangeError/SyntaxError with host ABI v12,
// value/schema v11, v5 value imports and json_stringify_v11/json_parse_v12.
// Error annotations accept all four supported classes; specific annotations and
// instanceof guards preserve family distinctions. name/toString/diagnostics retain
// the actual class. Presence, non-enumerability, roots and identity are preserved.
// Only explicit constructors/throws gain these families; JSON/index/resource
// failures and authorized exit remain fatal. Other families, custom subclasses,
// property mutation, cause/options and stack traces remain unsupported.
// V9 and successors also support String.repeat(count?:number|undefined). Omitted,
// undefined and NaN produce empty; fractions truncate. Negative normalized counts
// and infinities raise RangeError even for empty strings. UTF-16 units/NULs and
// operand effects/roots are preserved. String/memory/fuel budgets stay fatal.
// This is a pure compiler operation with no ABI/import/helper change; coercive
// counts, borrowed methods and non-string receivers remain unsupported.
// checked-f64-utf16-v10 adds recoverable varta.json.parse<T> failures (host ABI
// v13; value/schema v11 unchanged). Invalid JSON syntax raises SyntaxError and
// strict schema/finite-domain mismatches raise TypeError; messages do not echo
// input. Whole-input/decoded-value budgets, malformed ABI/schema/helpers,
// fuel/deadline/memory/root/output/host-call limits and authorized exit stay fatal.
// json_parse_v13 checks pending before result use; status/grants are unchanged.
// Earlier profiles retain fatal parsing. Dynamic JSON.parse and serializer/
// other API error conversions remain unsupported.
// checked-f64-utf16-v11 adds exact typed function values and capture-free arrows
// (host ABI v14; value/schema v11 unchanged). Locals, parameters, returns and
// calls retain full types. Expected signatures infer arrow parameter types.
// Named module references have stable identity; each arrow evaluation is fresh.
// typeof is "function"; values are truthy. Calls preserve roots, argument order,
// pending exceptions, status, grants and fatal budgets through checked direct
// dispatch. Captures, function expressions/properties, optional/rest/default,
// async/generic functions and function-valued aggregates/unions/JSON/throws are
// unsupported. Block-arrow non-Number/value results need a context or annotation.
// checked-f64-utf16-v12 adds lexical captures of initialized parameters/locals/
// catch bindings (host ABI v15; value/schema v11 unchanged). Shared managed cells
// preserve live mutations, escaped/nested closures, independent activations and
// distinct per-iteration let bindings. GC traces environments/cells and collects
// unreachable cycles without treating numeric/status metadata as references.
// Entry guards validate captured metadata before effects. Capture before binding
// initialization, active-refinement capture and mutable captured union narrowing
// are rejected; forward TDZ/ReferenceError semantics remain pending. Extra hidden
// environments count toward the 32 physical parameter-slot bound. Broader function
// syntax, function-valued aggregates/JSON and async remain unsupported.
// V12 additionally supports Array.map/filter/forEach/find via the same checked
// functions/closures, with no new ABI/import. Fixed callbacks take a 0..3 prefix
// of (value,index,array); find values include undefined because deleted indices
// are visited. Length is snapshotted; filter/forEach skip removed indices, while
// replacements/regrowth remain visible. Filter/find keep the pre-call value.
// Map/filter return fresh arrays; void map results become undefined, forEach
// discards callback results. Filter/find use checked truthiness. Unannotated
// Array callback blocks infer typed returns and supported unions, including
// bare/fallthrough undefined. No value-return site retains void. Nested arrows
// do not contribute returns; ordinary arrows retain their previous defaults.
// Finally saved results use exact inferred slots/roots, including physical pairs.
// Map traps on a missing visited index until sparse output is implemented.
// Typed thisArg and function expressions are supported in v12+ below. Borrowed
// methods remain unsupported; roots, errors, grants and all
// existing limits persist. Generated wrappers consume user-function/code bounds.
// V12 and successors also support reduce/some/every without ABI/helper changes.
// Some/every use checked predicates and short-circuit, skip absent indices and
// return false/true for empty/all-hole arrays. Reduce accepts a callback and an
// optional seed. Fixed callback parameters are a checked 0..4 prefix of
// (accumulator,value,index,array), result exactly matches the accumulator type.
// The seed expression determines that type, otherwise the element type does;
// inline reducer arrows contextually type parameters and returns. Callback
// annotations/signatures supply seed context; aggregate literals can also use
// surrounding aggregate result context, including empty/nested arrays and
// optional records. Wider outer scalar results alone never widen scalar seeds.
// Operands lower once; ambiguous seeds/types still need annotation. Explicit
// undefined is a seed. Without one the first present entry initializes the
// accumulator without a call; no present entry throws TypeError. Arguments run
// once in order before the length snapshot. Exact roots retain seed/current/
// accumulator values through GC; status, grants and fatal limits are unchanged.
// V12+ anonymous function expressions share inference, captures and fresh identity.
// Explicit this is separate from positional parameters and part of the exact
// function type; use an annotation or expected function type to read it. Its
// physical rooted slot plus closure environment count toward parameter bounds.
// Six Array callbacks accept optional thisArg; reduce keeps its optional seed and
// Undefined receiver. Callee/callback/thisArg/arguments run once in source order.
// fn.call supplies a checked receiver, bare calls supply Undefined. Arrows capture
// lexical this and ignore supplied receiver values while preserving their effects.
// No primitive boxing/global substitution; status/grants/GC/fatal rules persist.
// Named expressions and declaration this are supported below. Implicit method/
// thisArg inference, bind/apply, constructors and arguments remain unsupported.
// No ABI/helper changes.
// V12+ named expressions require an annotated/contextual result and expose a
// private immutable self name only to the body/nested closures. Parameters/locals
// may shadow it; external alias reassignment never changes its identity. A hidden
// rooted self parameter plus receiver/environment count toward physical limits.
// Entry guards check signature/code/environment before effects; escaped self
// uses existing capture cells. Named declarations retain typed this and canonical
// identity across import aliases. Bare calls supply checked Undefined; call/Array
// paths supply explicit receivers. Main and Tool/HTTP v1 stay receiver-free.
// Forward outer-variable initialization remains unsupported; no ABI/layout change.
// V12+ optional/default/rest parameters preserve exact function modes. Optional
// body values admit Undefined; defaults run only for omission/Undefined, after
// all supplied argument effects and left to right. Earlier initialized bindings,
// typed this and private self are visible; own/later TDZ reads and body declarations
// are unavailable. Required positions after defaults still require arguments.
// Rest is a final checked array, fresh/dense and bounded to 1024 values. Incoming
// ABI roots transfer to source bindings/capture cells then clear; pairs/GC/errors/
// finalizers/status/grants retain their existing contracts. Array callbacks use
// actual implicit arguments plus optional/default tails or checked rest. Function/
// physical-result optional unions remain unsupported.
// Main and Tool/HTTP v1 retain required entries. No ABI/layout/helper change.
// V12+ calls accept narrowed checked array spreads and explicit values, including
// fn.call(explicitThis,...arrays). Snapshot each operand before later effects;
// retain child identity; holes become Undefined/present rest entries. Validate
// dynamic count/types before defaults/body (owned TypeError); exceeding 1024
// flattened arguments traps fatally. Root callee/receiver/snapshots/typed slots;
// physical pairs remain outside arrays. Existing slot/function/resource bounds
// apply. Pure guest Undefined tests preserve host budget/status. String/iterable,
// host/builtin/Array method spreads, spread thisArg and aggregate spread literals
// remain unsupported. No import/helper/layout/ABI change.
// V12+ function parameters also admit recursive closed-record/checked-array
// patterns with aliases/static literal keys, nested/default bindings, elisions
// and final rest. The incoming aggregate needs annotation/callback context;
// bound leaves keep ordinary TDZ/capture-cell/self/this/root semantics. Defaults
// run only on Undefined in source order; nullish aggregate reads throw owned
// TypeError before later bindings/body. Array reads observe live mutation and
// persistent exhaustion; sparse rest admits Undefined and fills holes densely.
// Object rest is fresh/shallow and preserves live presence/enumeration order.
// Bound pattern depth/nodes to 64/1024 per function plus existing resource limits.
// Dynamic keys, primitive boxing, string/custom iterables, array-as-object and
// catch patterns remain unsupported. No ABI change.
// V12+ local const/let declarations and ordinary for initializers reuse these
// patterns. Infer the RHS or supply a whole-pattern aggregate annotation; require
// an initializer and evaluate it once. Predeclare all block leaves in TDZ and
// initialize in source order. Const rejects rebinding, while children remain
// mutable; let retains checked assignments/shared cells. For let separates the
// initializer and first iteration cells, then rebinds before updates. Const keeps
// one loop environment. Preserve shadowing, escaping closures, GC/pairs/errors,
// status and actual permissions. Shared pattern budgets also bound each lexical
// declaration. Var/ambient, top-level, catch and for-in patterns
// remain unsupported. No host import/helper/layout/ABI change.
// V12+ destructuring assignment statements update existing initialized mutable
// local/parameter/captured cells in source order after one RHS evaluation. Reuse
// nested patterns/defaults/rest, admitting duplicate targets/prior-value reads;
// preserve successful writes on later throws. Keep target types invariant with
// union widening and checked Undefined narrowing (owned TypeError on absence).
// Defaults use target context; shared roots/limits/status/grants remain intact.
// Parenthesized statements, expression results/chains and for init/update are
// supported. Checked member targets are supported below; dynamic record keys,
// catch and for-in stay unsupported. No ABI change.
// Assignment expressions return the original contextual RHS identity/type,
// retaining unselected fields and holes. Save it once, finish traversal and then
// publish it to arguments/returns/callbacks/chains/admitted expression positions.
// Scoped temporary floors keep earlier operands rooted during embedded statement
// cleanup; input scopes cover errors and ordinary retention protects later use.
// Defaults update targets without filling source properties. Preserve partial
// writes, GC/pairs/finalizers/status/real grants and all limits. No ABI change.
// V12+ assignment patterns admit checked record fields and Number array elements,
// including nested/default/rest targets. Evaluate each receiver/key once before
// source reads/defaults (rest: before copy), then retain that reference across
// allocation/reassignment. Writes resolve current backing after default effects.
// Const children remain mutable; exact types/union widening/Undefined checks and
// presence/order retain their contracts. V12 replaces existing slots; v13 fills
// holes within length; v14 extends and admits length targets with existing
// RangeError/fatal bounds. Declaration/parameter patterns still bind names.
// General record keys/coercion, Error/result mutation and unresolved nullable
// receivers remain unsupported. Effects require real grants; no ABI change.
// V12+ intrinsic array for-of accepts const/let and checked assignment heads,
// including member targets and shared object/array patterns. Evaluate/root the
// source once, then read live length/backing and the current value before head
// reference/default effects. V13+ visits holes as Undefined (element|Undefined);
// v12 dense arrays retain the declared element type. Fresh lexical captures are
// created each iteration; existing assignment targets keep their cells. Normal
// break/continue/finally/return/throw cleanup, GC/pairs, resource limits, status
// and actual grants remain intact. Nullable arrays raise TypeError before heads;
// other sources fail compilation. For-in/await-of, var/using, strings and
// general iterables remain unsupported. No ABI/helper/layout/import change.
// V12+ do-while runs its body before checking the condition. Continue tests after
// finalizers; break/return/throw skip the test unless a finalizer overrides them.
// Body-block names remain local; each execution creates fresh captured bindings.
// Existing truthiness, cleanup/GC/pairs, status/grants and fatal limits apply.
// Guaranteed first-body returns/throws inform completion and callback inference;
// possible break/continue paths still permit fallthrough. Clear test temporaries
// before repeating. Earlier profiles remain unsupported. No ABI change.
// V12+ labels permit named break to active supported statements and named
// continue to while/do-while/for/array-for-of (including label chains). Labels
// have a separate per-function namespace; unknown/duplicate/non-loop continue
// targets and cross-function jumps are rejected. Unlabeled branches are unchanged.
// Named routes cross finalizers and clear only exited scopes; closures, pairs,
// exception state, status/grants and resource bounds retain their contracts.
// Completion/callback analysis preserves exact named targets through inner loops.
// Labels count toward nesting limits. No ABI/import/helper/layout change.
// V12+ Object.keys(record) returns a fresh dense string[] of present own keys:
// canonical array indices first numerically, other strings by insertion order.
// Explicit Undefined is present; absent optional fields are omitted. Input is
// evaluated once; no payload serialization or source/result aliasing occurs.
// Nullable single-record unions throw TypeError on absence. Unsupported kinds
// (including arrays/boxed primitives/Error/result/functions), generic/spread calls
// and shadowed Object remain rejected. Internal roots/floors protect earlier
// operands, returns, GC/pairs and exceptions. Clear spare slots/bits before
// publishing length. Status/grants and all bounds persist; no ABI/helper change.
// checked-f64-utf16-v13 adds sparse presence (host ABI v16/value v12/schema v12).
// Holes differ from explicit undefined through Object.hasOwn; all-hole literals
// require an element annotation. Reads/at return undefined for holes; writes within
// length fill them. Map retains missing indices and snapshots its final length;
// filter/forEach skip holes, find visits undefined. Slice copies bits, pop removes
// even an absent last index, push appends present values. Includes sees holes as
// undefined; indexOf skips them. JSON writes null and parsing creates present
// entries; round trips cannot preserve holes and need an appropriate null schema.
// v6 value imports, value_is_error_class_v2, json_stringify_v12/json_parse_v14 bind
// these layouts. Roots/permissions/status/error boundaries and 1024 elements stay
// bounded; older profiles retain dense semantics. Delete, extension, writable
// length, coercive/dynamic string keys, constructors/prototypes remain unsupported.
// checked-f64-utf16-v14 adds indexed extension, delete and writable length (host
// ABI v17; value/schema v12 unchanged). Writes root base/key/RHS in order, validate
// numeric indices after RHS effects and create intervening holes. Length accepts
// Number only; invalid uint32 values raise RangeError, valid lengths above 1024
// remain fatal resource violations. Shrink clears payloads/bits; growth creates
// holes without resurrecting references. Array-only numeric/literal delete keeps
// length, returns true for absence, and raises TypeError for deleting length.
// Arbitrary/coercive own properties and compound/assignment expressions remain
// unsupported. Old profiles retain their mutation contracts. No new host import,
// capability or fatal-to-catch budget conversion is introduced.
// ABI-only log_f64/number_remainder/process_exit_f64 are compiler/runtime details,
// not additional source APIs. Binary64 remainder and logging preserve host status.
interface VartaHttpHeader { name: string; value: string }
interface VartaHttpRequest { method: string; target: string; body: string; headers: VartaHttpHeader[] }
interface VartaHttpResponse { status: number; body: string; headers: VartaHttpHeader[] }

// The standard JSON.stringify global is implemented for the checked primitive,
// null, closed-record and dense-array profile. Replacers and indentation are not supported yet.
// Binary64 JSON uses ECMAScript number text (-0 -> 0, non-finite -> null).

/** Immutable value and status from one captured system call. */
interface VartaResult<T extends number | boolean | string | void> {
  readonly ok: boolean;
  readonly status: number;
  readonly value: T;
}

declare const varta: {
  /** Compile-time publication; only a top-level entry-module declaration is accepted.
   * The handler must be an exported named function with primitive parameters/results.
   * Metadata must contain only literal name/description fields. This is not a runtime API.
   */
  tool<F extends (...args: never[]) => number | boolean | string | void>(
    metadata: { name: string; description: string }, handler: F
  ): void;
  http: {
    /** One compile-time entry-module declaration; literal IP/port, synchronous handler.
     * Starts after main completes. Each request has a fresh bounded instance.
     */
    serve(options: { hostname: string; port: number }, handler: (request: VartaHttpRequest) => VartaHttpResponse): void;
    /** Stop this owned service after the current response has drained. */
    stop(): void;
  };
  json: {
    /** Decode against the supported closed T schema. Rejects malformed JSON,
     * duplicate/unknown/missing required fields, wrong types and resource-limit violations.
     * Object field order is normalized by the schema. Binary64 schemas accept
     * finite decimals/exponents; overflowing/non-finite input is rejected.
     * In checked-f64-utf16-v5, optional fields may be absent and unions select
     * their unique JSON-kind member. JSON has no undefined token; required fields
     * admitting undefined still cannot be omitted. Invalid input traps.
     * This is the strict typed API, not generic JSON.parse.
     */
    parse<T>(text: string): T;
  };
  result: {
    status(): number;
    /** The argument must be a direct privileged varta host call. */
    capture<T extends number | boolean | string | void>(value: T): VartaResult<T>;
  };
  time: { now(): number };
  env: { has(name: string): boolean; print(name: string): void; get(name: string): string };
  fs: { print(path: string): void; read(path: string): string };
  /** Exit requires an exact finite i32 code in either numeric profile. */
  process: { exit(code: number): void };
  net: { fetch(url: string): number; get(url: string): string };
};
