// checked-f64-utf16-v34 (pilot batch, unreleased) promises and async functions use
// TypeScript's own `Promise<T>`; no source-level host declaration is added. The
// compiler emits five internal scheduler imports, never callable from source,
// host ABI30 minimum, no capability, not charged to the host-call budget:
// promise_react (f64 serial, i64 token)->f64 serial, promise_settle (f64)->(),
// scheduler_enqueue (i64 token)->(), promise_unhandled (i64 reason)->f64 id and
// promise_handled (f64 id)->(). Tokens name live frames resumed only through
// varta_resume; the runtime keeps serials, never guest references.
// checked-f64-utf16-v36 (pilot batch, unreleased) timers: the globals below run
// their callback in a later event-loop task, ordered by (due time, creation
// order) on a loop clock that advances only to the due time of the timer that
// fires. Arming a timer requires the exact `time.timer` grant (denied by
// default: the execution ends at the call); clearing needs none. Ids are
// numbers, as in browsers (Node returns Timeout objects). Delays follow Node:
// a delay outside 1..2^31-1 ms is 1 ms. A timer due at or after the 5 s
// execution deadline ends the execution with a budget error. Internal imports,
// host ABI32 minimum: timer_start (f64 serial, f64 delay, i32 flags)->f64 id
// [time.timer, host-call budget], timer_clear (f64 id)->(), timer_take
// (f64 serial)->i32 and timer_uncaught (i64 reason)->(). AbortSignal.timeout
// uses a timer that does not keep the event loop alive.
declare function setTimeout<A extends unknown[]>(
  callback: (...args: A) => void,
  ms?: number,
  ...args: A
): number;
declare function setInterval<A extends unknown[]>(
  callback: (...args: A) => void,
  ms?: number,
  ...args: A
): number;
declare function clearTimeout(id?: number | null): void;
declare function clearInterval(id?: number | null): void;
// checked-f64-utf16-v37 (pilot batch, unreleased) network requests: `fetch`,
// `Request`, `Response`, `ReadableStream`, `URL` and `URLSearchParams` are
// engine preludes (typed guest code) loaded only when a module names them.
// Every request needs the exact `net.request` grant ("net": {"request":
// ["https://api.example.com/v1/**"]} or {"urls": [...], "methods": [...]});
// a denied request rejects with a catchable DOMException `NotAllowedError`
// before any name resolution or socket, and the denial is logged. A denied
// v37 timer start throws the same error synchronously (v36 stays fatal).
// Redirects default to "error" ("follow" re-checks every hop against the
// grant, "manual" returns the 3xx); there are no cookies or credentials.
// Bodies are text: response chunks are UTF-8 decoded strings (bytes wait for
// Uint8Array) and ReadableStream is not generic. Internal imports, host ABI33
// minimum: net_request_start (i32 ptr, i32 len)->f64 id [net.request,
// host-call budget], net_request_watch (f64 serial, f64 id)->(),
// net_request_take (f64 id)->i32 kind, net_request_text (f64 id)->i32 ptr,
// net_body_read (f64 id)->(), net_request_abort (f64 id)->(), and
// timer_start_v2 (f64 serial, f64 delay, i32 flags)->f64 id [time.timer].
type RequestRedirect = 'error' | 'follow' | 'manual';
type BodyInit = string | URLSearchParams;
interface RequestInit {
  method?: string;
  headers?: HeadersInit;
  body?: BodyInit | null;
  signal?: AbortSignal | null;
  redirect?: RequestRedirect;
}
interface ResponseInit {
  status?: number;
  statusText?: string;
  headers?: HeadersInit;
}
interface ReadableStreamReadResult {
  done: boolean;
  /** The empty string once done (WHATWG: undefined). */
  value: string;
}
declare class ReadableStreamDefaultReader {
  read(): Promise<ReadableStreamReadResult>;
  cancel(reason?: unknown): Promise<void>;
  releaseLock(): void;
}
declare class ReadableStream {
  readonly locked: boolean;
  getReader(): ReadableStreamDefaultReader;
  cancel(reason?: unknown): Promise<void>;
  values(): AsyncGenerator<string, void, undefined>;
  [Symbol.asyncIterator](): AsyncGenerator<string, void, undefined>;
}
declare class Request {
  constructor(input: string | Request, init?: RequestInit);
  readonly method: string;
  readonly url: string;
  readonly headers: Headers;
  readonly signal: AbortSignal;
  readonly redirect: RequestRedirect;
}
declare class Response {
  constructor(body?: string | ReadableStream | null, init?: ResponseInit);
  readonly status: number;
  readonly statusText: string;
  readonly headers: Headers;
  readonly ok: boolean;
  readonly url: string;
  readonly redirected: boolean;
  readonly body: ReadableStream | null;
  readonly bodyUsed: boolean;
  text(): Promise<string>;
  json(): Promise<unknown>;
}
declare function fetch(input: string | Request, init?: RequestInit): Promise<Response>;
declare class URLSearchParams {
  constructor(init?: string | Record<string, string> | string[][]);
  readonly size: number;
  append(name: string, value: string): void;
  delete(name: string): void;
  get(name: string): string | null;
  getAll(name: string): string[];
  has(name: string): boolean;
  set(name: string, value: string): void;
  toString(): string;
}
declare class URL {
  constructor(url: string, base?: string | URL);
  static canParse(url: string, base?: string): boolean;
  readonly href: string;
  readonly origin: string;
  readonly protocol: string;
  readonly host: string;
  readonly hostname: string;
  readonly port: string;
  readonly pathname: string;
  readonly search: string;
  readonly hash: string;
  readonly searchParams: URLSearchParams;
  toString(): string;
}
// checked-f64-utf16-v32 (pilot batch, unreleased) inherits v31 and its ABIs with
// no new host declaration: module constants and `as const`, string/number/boolean
// literal types, top-level classes, `export ... from`/`export *` re-exports,
// untagged template literals, default exports, interface extends/merging and
// monomorphized generic interfaces, aliases and top-level functions are
// compile-time language features. Constants, classes and generic functions
// re-export through the same tables as functions and types. Classes may extend
// Error/TypeError/RangeError/SyntaxError; v32 selects exception channel v5 so an
// uncaught Error-class instance is reported with its name and message.
// checked-f64-utf16-v27 inherits v26 and infers one admitted element union for
// uncontextual array literals. Initializers execute once in source order; holes
// stay absent and do not broaden writable element types. Empty/all-hole nested
// peers can use typed array evidence; outer/unresolved empties need annotations.
// Explicit contexts and mutable-array invariance remain. One record shape/array
// kind per union; Function/opaque elements and general dynamic inference remain
// unsupported. Shared ABI26/value12/schema12; rebuild both companions for 30 profiles.
// Wide boxed literals retain source/root/fuel and 64 KiB function-body budgets.
// checked-f64-utf16-v26 inherits v25 and adds checked-array reduceRight.
// Callbacks receive (accumulator,value,index,array); holes/deleted slots are skipped.
// Explicit initial values set the accumulator type; otherwise the rightmost present
// element seeds it. Empty/all-hole unseeded arrays throw recoverable TypeError.
// Callback result must match accumulator type. Ordinary function receivers accept
// Undefined; arrows retain lexical this. Cache source/callback/seed before ignored
// effects, then capture traversal length. Existing reduce/profile rules stay unchanged.
// Shared ABI26/value12/schema12; rebuild both companions for protocol2's 30 profiles.
// Generic array-like/prototype calls and dynamically changing accumulators remain pending.
// checked-f64-utf16-v25 inherits v24, adding direct checked-array findIndex,
// findLast and findLastIndex. Index methods return Number (-1 on no match);
// findLast returns element|undefined. Callbacks accept element|undefined,
// optional index/array, with declared function thisArg or lexical arrow receivers.
// Sparse/deleted positions are visited; reverse methods traverse descending indices.
// Effective operands are cached before ignored trailing argument effects;
// snapshot length is captured afterwards. Existing profiles reject these methods.
// Shared ABI26/value12/schema12 and limits persist. Rebuild both native companions
// for protocol2's exact 30-profile list with v27. Generic array-like/prototype calls remain pending.
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
// Dynamic keys, primitive boxing, string/custom iterables and array-as-object
// patterns remain unsupported. Narrower catch object patterns are specified below.
// No ABI change.
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
// supported. Checked member targets are supported below; dynamic record pattern keys,
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
// V12+ String.slice(start?,end?) and substring(start?,end?) accept narrowed
// String receivers and Number/Undefined bounds, including unions. Omitted or
// undefined start is zero/end is length; NaN -> zero and fractions truncate.
// Slice resolves negatives relative to length and empties reversed ranges;
// substring clamps negative bounds to zero and swaps reversed bounds. Clamp
// infinities/huge finite values before integer conversion. Preserve UTF-16 units,
// lone surrogates/NULs, argument order, roots/GC/pairs/returns/finalizers and actual
// effects. Copy nonempty ranges into owned strings, never interior pointers;
// empty results are canonical after effects. Existing Array.slice/older profiles
// and all bounds remain intact; resource failure/exit stay fatal. No coercion,
// borrowed/computed/generic/spread methods, extra args or nullable receiver without
// narrowing; no host operation, helper export or ABI/layout change.
// V12+ String.includes/startsWith/endsWith require String needles and narrowed
// String receivers, with an optional Number/Undefined position (including union).
// Default includes/startsWith to zero, endsWith to length. NaN -> zero; truncate
// fractions and clamp 0..UTF-16 length before conversion, without relative negatives.
// Empty needles match after effects; compare exact units/case/NULs/surrogates,
// never unaligned byte subsequences or normalized Unicode. Private guest helpers
// allocate/call nothing and return canonical booleans. Initial includes scans
// candidates with fuel-bounded quadratic worst case. Preserve roots/GC/pairs/
// returns/finalizers, status/grants and fatal limits; Array.includes/older profiles
// retain their existing rules. No coercive/regex/omitted needles, extra args,
// borrowed/computed/generic/spread calls or nullable receivers without narrowing.
// No host operation, helper export or ABI/layout change.
// V12+ catch object patterns support static name/message identifier bindings,
// aliases/defaults and empty patterns. Field types are string|undefined across the
// full primitive/Error-family channel. Non-null primitives supply undefined;
// nullish throws produce TypeError before defaults/body. Errors retain existing
// family names and message semantics. Names initialize in source order under TDZ,
// are mutable and get fresh capture cells per entry. Defaults cannot see body
// declarations; failures escape the original handler through outer finalizers.
// Reject arrays/rest/other properties/dynamic keys/annotations/aggregate throws.
// Preserve GC/roots/pairs/status/grants/fatal budgets and older profile behavior.
// No new host operation, helper export or ABI/layout change.
// V12+ Object.hasOwn(record,key) admits dynamic narrowed String keys on narrowed
// closed records. Evaluate/root source then key once; lookup observes presence
// after key effects on the original source. Unknown/inherited names return false,
// present Undefined is true, and no payload/prototype/JSON access occurs. Compare
// exact UTF-16/NULs/case using a balanced tree ordered by UTF-16 rather than UTF-8
// metadata, retaining original presence slots. String literals keep direct lookup,
// including false for unrepresentable names or __proto__, without new initializers.
// Dynamic Number/Boolean/Undefined/union keys, symbols/coercion, unchecked/nullable
// sources without narrowing, generic/spread calls and shadowed Object are rejected.
// V12+ Error families/only-Error unions admit narrowed dynamic String keys: message
// reads its own-presence bit independently of class; name/unknown names are absent.
// Omitted/Undefined messages are absent; explicit empty Strings are present.
// V13+ arrays admit String keys: length is own even when empty, canonical ASCII
// decimal indices test current length and presence; holes differ from Undefined.
// No leading zeros except 0, signs/fractions/exponents/coercion. Numeric array
// keys keep existing behavior; String|Number unions require narrowing. Preserve
// earlier-profile boundaries, source/key order, mutation/throws and GC/roots/
// pairs/returns/finalizers/effects/status/grants and all bounds; no ABI/helper export.
// V12+ record[key] reads accept narrowed closed records and String keys. Join all
// field types with Undefined under existing exact union limits. Evaluate/root
// receiver then key once; observe key mutations on the original receiver. Test
// presence before reading the selected payload; preserve numeric bits/identity.
// Missing/absent non-inherited keys yield Undefined. Inherited names without a
// declared own field throw TypeError; static inherited reads still fail at compile
// time. No prototype lookup/getters/coercion. Preserve literal exact types, older
// profiles, roots/GC/pairs/returns/finalizers/status/grants and all fatal bounds.
// Dynamic pattern targets, non-String/union keys, nullable sources without narrowing and
// unrepresentable result unions remain rejected. No ABI/import/helper export.
// V12+ record[key]=rhs simple statements prepare/root source and String key before
// RHS effects, then check the selected field's exact type before storing. Unknown/
// inherited non-own keys or incompatible values throw TypeError after RHS success.
// Preserve numeric bits, record/array identity/types and exact Error families;
// do not join all field types or erase shapes. Unambiguous aggregate literals
// receive context; impossible RHS kinds fail at their source span. Optional
// Undefined writes publish presence/order through the existing field store.
// IsUnionMember checks tags and exact Error class bits without host calls or
// allocation. Preserve operand effects, GC/roots/child graphs/returns/pairs/finalizers,
// status/grants and fatal bounds. No dynamic property creation, coercion/prototypes,
// pattern targets or deletion. Values, arithmetic compounds and updates below.
// Literal/legacy rules and ABI/import/helper exports remain unchanged.
// V12+ computed record assignment expressions/chains return the original RHS value
// and type after successful stores. Keep literal String/Number static checks and
// dynamic String checks. Widen only storage copies, preserving result unions,
// exact numeric bits/Error families/aggregate identity. Prepare outer receivers/
// keys before inner references and RHS; store inner to outer. Inner failure skips
// outer work; outer failure keeps inner stores/effects. Root original outputs and
// inputs/child graphs; save/restore temporary floors before operands to preserve
// earlier caller values/pairs through statements, GC and finalizers. Existing
// value-block visitors/callback inference use original RHS types/effects. Ordinary
// local/dot/array values, arithmetic compounds and updates are below; Error mutation
// remain unsupported. Keep all bounds/status/grants and ABI/import/helper exports.
// V12+ simple assignment expressions support mutable initialized locals/parameters,
// declared record fields and numeric array elements, sharing prepared references/
// setters with destructuring. Root/cache receiver and raw index before RHS; use
// current backing after success. Reject const/TDZ/guard-refined targets and type
// mismatches. Cache original RHS types/values; widen storage copies only. Preserve
// local function signatures/captures and both physical VartaResult slots/roots.
// Numeric indices validate after RHS: finite integral 0..1023, -0 selects zero.
// V12/v13 require current in-range writes; v14 extends with holes and preserves
// current presence/backing. V14 length/literal length expressions retain RangeError
// for non-uint32 lengths and fatal >1024 limits, returning original Number/-0.
// Shared value-block floors/roots preserve operands/pairs, mixed partial chains,
// cells/functions/results, GC and finalizers. Keep all bounds/status/grants and
// ABI/import/helper exports. Arithmetic compounds and Number updates are below.
// V12+ +=, -=, *=, /= and %= support mutable locals/static fields/numeric arrays;
// v14 adds length. Prepare receiver/key once, then snapshot the old value before
// RHS effects. Array MaybeIndex snapshots missing/invalid reads as Undefined.
// Evaluate RHS, then admit Number/Number, or String/String only for +=, using
// pure union-tag checks without coercion. Impossible static pairs fail at their
// source span; incompatible runtime values throw TypeError after RHS without a
// requested store. Return computed results and widen storage copies only. Keep
// old numeric bits/String ownership and current backing/presence/write limits.
// Missing-value errors do not attempt writes or convert budgets into catchable
// failures. Preserve roots/floors/graphs/pairs/partial chains/GC/finalizers/status/
// grants. Dynamic record keys/logical assignments/Number updates/bitwise compounds
// and v15 exponentiation are below. Reject
// immutable/guard-refined targets and older profiles. No ABI/import/helper export.
// V12+ dynamic record arithmetic compounds require narrowed record/String inputs
// and the existing computed-read union admission: all fields plus Undefined, at
// most one record shape/array element type, no functions/results. Cache receiver/
// exact UTF-16 key once and snapshot the old selected value before RHS. RHS cannot
// redirect the prepared reference or replace its old value. Unknown/absent reads
// fail arithmetic after RHS; undeclared inherited names fail before RHS, with own
// declared names preserved. Keep original exceptions/effects, strict arithmetic
// kinds, computed result types and full selected-field storage checks. No property
// creation/coercion/prototypes. Root original graphs/operands/pairs/returns across
// GC/partial chains/calls/finalizers; visit every effect. Status/grants/ceilings and
// compilation/value/fatal bounds persist. No ABI/import/helper export change.
// V12+ &&=, ||= and ??= read prepared mutable local/parameter/static or dynamic
// record/numeric-array targets once; v14 admits length. Run RHS and checked store
// only for truthy/falsy/nullish old values respectively. Skip returns the exact
// old value without write validation or payload/presence/order changes; successful
// store returns the original RHS, widening only its storage copy. Join RHS with
// possible skipped kinds, excluding always-truthy kinds for &&= and Null/Undefined
// for ||=/??=. Static RHS types must be fully compatible even when skipped;
// dynamic leaves retain full field checks and existing read-union restrictions.
// Unknown ordinary keys skip &&= as Undefined, selected writes fail after RHS;
// inherited non-own names fail before RHS. Array missing/invalid reads may skip,
// selected stores retain current storage/length/profile/fatal rules. Preserve
// exact bits/UTF-16/reference/function identity, arrow context, earlier pairs and
// prepared graphs through partial chains/callbacks/GC/returns/finalizers. Reject
// const/TDZ/guard-refined/result-pair targets, Error/result fields, coercive keys,
// logical patterns and older profiles. Visit every source/key/RHS/store
// effect including runtime-skipped branches. Use existing truth/nullish tests,
// blocks/setters/status/grants/bounds; no new ABI/import/helper export.
// V12+ prefix/postfix ++/-- require an actual Number in mutable initialized local/
// parameter/static or dynamic record/numeric-array targets; v14 admits length.
// Prepare receiver/key and read once, validate Number, compute old +/- 1, then
// checked store. Prefix returns the new Number; postfix the exact old Number only
// after store success, including -0/NaN/infinity/large-number bits. Statically
// impossible targets fail at their span; runtime non-Number/missing values throw
// TypeError before writes. No coercion or property creation. Keep dynamic-read
// union/UTF-16/inherited rules and full checked field types. Array holes/invalid/
// missing numeric reads fail without extension; length keeps RangeError/budget
// rules. Preserve prior effects/completed updates and skip later work on failure.
// Const/TDZ/guard-refined/function/result-pair targets, Error/result fields,
// coercive keys and older profiles stay rejected. Parenthesized simple targets,
// captured/fresh loop cells and callback Number inference use existing rules.
// Root/floor transfer retains graphs/earlier pairs/returns through GC/finalizers;
// visitors see source/key/read/check/store effects, pure helper controls pass and
// host effects fail. Status/grants/ceilings/bounds and ABI/import/helper exports persist.
// V12+ Number-only &, |, ^, ~, <<, >> and >>> require narrowed Number operands;
// reject other types/unnarrowed unions at their span and older
// profiles. No ToNumber coercion or integer value type. Evaluate operands once,
// left to right, before full-range ToInt32/ToUint32 conversion: truncate finite
// fractions/modulo 2^32, map nonfinite/either zero to positive zero. Mask shift
// counts to five bits; return signed Number except >>>'s unsigned 0..4294967295.
// Inline guarded exponent/significand operations do not allocate, call hosts or
// trap on float conversion. Scratch reuse after operand evaluation preserves
// nesting/earlier graphs/pairs/roots. Keep source effects, original exceptions,
// status/grants/ceilings and fatal budgets; no ABI/import/helper export change.
// V12+ &=, |=, ^=, <<=, >>= and >>>= reuse prepared mutable local/parameter,
// static/dynamic record and numeric-array targets; v14 admits length. Cache the
// original receiver/key and old value before RHS, then validate actual Number
// tags after RHS success. Impossible static pairs fail at target/RHS spans;
// incompatible runtime members throw TypeError without coercion/requested store.
// Return computed Number results using existing ToInt32/ToUint32 operations,
// including unsigned >>> storage; widen only storage copies. Keep dynamic-read
// union/UTF-16/inherited timing and full field checks. Missing/unknown/invalid
// numeric reads fail after RHS without attempted writes/property creation; keep
// original exceptions/effects. Valid results use current backing/presence/length
// and profile/fatal rules. Root/floor transfer preserves original graphs, earlier
// pairs and saved returns through partial chains/calls/GC/finalizers. Visitors
// see every operand/check/store effect. Status/grants/ceilings/bounds and existing
// target rules persist; no new IR kind/ABI/import/helper export.
// checked-f64-utf16-v15 adds Number ** and **= with host ABI v18, inheriting v14
// features and value/schema v12. Default and older profile semantics are retained.
// Plain ** requires narrowed Number operands; **= checks actual Number tags after
// RHS using prepared source/key/old snapshots and existing checked stores/roots.
// Preserve source effects, partial chains, optional/sparse absence, current
// backing/length, computed Number results and GC/pairs/returns/finalizers.
// number_power(f64,f64)->f64 is a pure minimum-ABI18 import with no capability or
// result buffer/status reset. Number NaN/zero/infinity/odd-integer/domain rules
// precede platform-approximated finite power; preserve signed zero/underflow and
// NaN**0=1, 1**NaN=NaN, (±1)**(±Infinity)=NaN. No BigInt/coercion.
// Each power charges one existing host call and checks the deadline before/after
// native math. Exhaustion/exit stay fatal; host-free JSON helpers cannot call it.
// Profile/ABI/signature negotiation is mandatory; v15 needs ABI18/value12, while
// the current launcher advertises maxima26/value12 with v16/v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27 support. No value/schema/container/limit change; rebuild both binaries.
// V15 also admits direct unshadowed Math.abs/floor/ceil/trunc/round/sqrt(value),
// with exactly one narrowed Number argument. These are guest-only intrinsics:
// no host import/allowance/status change, new permission/helper export or ABI.
// Round ties toward +Infinity, preserving -0 and near-halfway binary64 values;
// sqrt preserves -0 and yields NaN for negative nonzero inputs. Evaluate the
// argument once, preserving its effects, exceptions, roots and fatal limits.
// Reject omitted/extra/spread/generic args, coercion, unnarrowed unions, shadowed
// Math, computed calls, aliases and other Math methods. Older profiles
// reject all methods. No global Math declaration: the editor uses standard TS lib types.
// Additional V15 Math methods: sign/fround/clz32 need one Number, imul two,
// min/max zero or more. Every argument must be narrowed Number; all evaluate
// once in order, including after NaN in min/max. Wrong arity/spread/coercion fail.
// Empty min/max return +Infinity/-Infinity; zero ties select -0/+0 respectively.
// Sign preserves NaN/±0; fround rounds through binary32 ties-to-even. Clz32 uses
// exact ToUint32 then counts leading zeroes; imul returns signed product low bits
// after exact ToUint32 conversions. All use guest instructions, unchanged status,
// fuel and host-free helper validation; no imports/ABI/layout/helper/limit change.
// V15 Number.isFinite/isNaN/isInteger/isSafeInteger are direct unshadowed,
// non-coercing predicates over any admitted value, including unnarrowed unions.
// Non-Numbers/missing first arguments return false. Extra arguments execute once
// in order and are ignored; preserve errors/fatal limits and full result pairs.
// Finite excludes NaN/Infinity; integer also needs trunc(x)==x; safe integer adds
// abs(x)<=9007199254740991. Both zeroes pass all except isNaN. No type refinement.
// Reject old profiles, shadowing, computed/borrowed/generic/spread calls and other
// Number methods. Guest-only operations retain status, permissions,
// roots/GC, helper validation and ABI18/value12/schema12 without new host budgets.
// checked-f64-utf16-v24 adds direct-local truth guards, sharing ABI26/value12/schema12.
// True excludes Null/Undefined; false excludes true refs, retains Number/String/Bool
// (zero/NaN/empty/false possible). Parens/! recurse; true && combines true guards,
// false || combines false; other compound outcomes conservative. RHS &&/|| sees
// true/false left; RHS ?? sees nullish left. Existing typeof/Array/Error compose.
// Scope facts to branches/while/for body+update/RHS; normal branch joins retain
// shared facts; optional/loop exits filter changed inputs and restore on errors.
// Do-while first body unguarded. No property/call-result/Boolean-call guard inference.
// V24 stores replace old facts with assigned types inside declared union storage.
// If/ternary joins combine normal paths; return/throw/jump paths do not contribute.
// Optional RHS/defaults retain unchanged inputs; logical targets use selected types.
// Loop/switch/label exits join reachable matching breaks and false-test edges;
// update/do-while test entries join normal body fallthrough and matching continues.
// Finally transforms/cancels pending exit facts; earlier sibling exits stay distinct.
// Analyze repeated for parts after the one-time initializer to prevent stale tests.
// For-of keeps empty/exhausted edges. Loop fixed points, precise exceptional/call/
// property and initializer inference remain incomplete. Track all stores and loop/callback effects.
// V24 immutable function provenance and transitive summaries preserve unrelated
// cell facts across known calls. Module callers account for supplied callbacks;
// callee-local writes do not affect caller cells. Mutable/returned/self/parameter
// targets without proof retain reentry fallback. Stable callback parameters now
// propagate symbolic input effects through forwarding, const aliases and captures;
// direct/nested replacement of a parameter remains unknown. Map default/rest/this
// physical inputs before source initialization. Local invalidation is separate from
// committed stores. Guard/loop literal analysis shares the existing64 proof budget. No property/heap immutability is
// inferred; function-valued aggregates remain unsupported. Callback type reachability
// memoizes nodes, bounded64 proof exhaustion is conservative. Checked member
// intrinsics without callbacks preserve cell facts (arrays may still mutate).
// Mutable captures use full cell types and fresh checks after unknown calls;
// const captures inherit stable proofs through nested environments. No new layout.
// Skip unchanged filters without capture marks. V23's prior admission preserved.
// Compiler-only swc_ecma_visit29.0.0 performs bounded AST effect traversal; no runtime
// dependency/native op/ABI constant/permission/layout/limit change. Existing roots,
// payload checks/status/pairs/saved returns/fatal limits persist. Protocol2 supported
// profiles must include24 even with unchanged maximum26/value12; rebuild companions.
// V23 also admits direct Boolean/isFinite/isNaN with unchanged ABI26/value12/schema12.
// Boolean reads ordinary values/unions: Undefined/Null/false/zero/NaN/empty String
// false, other values including refs true. Opaque result pairs require fields.
// New guest ToBoolean IR, actual tags/payloads/UTF16 lengths, no host/allocation.
// Global numeric predicates convert primitive-only inputs/unions to Number first;
// narrowed F64/missing stay guest-only, others reuse PrimitiveNumber (1host).
// Missing yields Boolean false / isFinite false / isNaN true. Cache first input
// before all ignored effects/full pairs. Normal bindings/closures/TDZ win; preserve
// roots/status/pending/pairs/saved returns and fatal fuel/deadline/host budgets.
// No wrappers/borrowed/computed/generic/spread calls or version/helper/limit changes.
// V23 conditions/loops/ternaries/! now share guest ToBoolean; old profiles frozen.
// &&/||/?? cache original left and execute only selected RHS; ToBoolean/IsNullish
// tests are guest-only. Selected operands retain Number bits and reference identity.
// Narrow selected left members (OR/?? remove Null/Undefined, AND removes true refs)
// before joining RHS; known type outcomes avoid unnecessary unions/Fn members.
// Root input/output through selected boxing/GC/pairs/saved returns/finalizers.
// Preserve status/pending/buffers and fatal fuel/deadline/host limits. No new ABI,
// helper export/dependency/limit change. Opaque pairs need fields; dynamic unions
// retain shape/Fn bounds. Opt-in v24 adds scoped truth guards/RHS refinements.
// checked-f64-utf16-v23 adds direct global/Number.parseInt/parseFloat calls.
// ABI26/value12/schema12 inherits v22. Primitive/union input ToString, optional
// parseInt radix ToNumber then Int32 truncate/mod2^32 (nonfinite0). Cache all args
// before conversion, ignore extras with effects/full pairs. Missing input guestNaN.
// Exact leading whitespace; longest valid integer or decimal/Infinity prefix.
// Radix0/default10/optional0x, explicit16 optional0x, other2..36 plain; bad/no digits
// NaN preserves status. Incomplete float exponent rolls back; signed zero retained.
// number_parse_int(i64,i64)->f64 / number_parse_float(i64)->f64 minimum26, capNone,
// checked primitive cells, no result globals/buffer, one host tick and post-check.
// String input borrowed UTF16, Number canonical; integer exact nearest/even bounded
// <=1030-bit scratch, float ASCII scratch<=half1MiB. Preserve roots/pairs/status/
// pending/saved returns and fatal budgets; JSON helpers reject imports. Global
// ordinary bindings/TDZ win, Number requires unshadowed spelling. No wrappers/
// object coercion/borrowed/computed/spread/generic built-ins. Rebuild both native
// companions for mandatory profile23/ABI26, protocol2 maximum26/value12.
// checked-f64-utf16-v22 adds direct narrowed Number decimal precision and valueOf.
// ABI25/value12/schema12 inherits v21. toFixed/toExponential0..100 fraction digits,
// toPrecision1..100 significant digits, primitive ToNumber+truncate/NaN0. Cache
// receiver/all args first; missing/Undefined fixed0, others shortest. Fixed checks
// range BEFORE nonfinite; others convert then return nonfinite BEFORE range check.
// Catchable RangeError boxed/widened. number_to_fixed/exponential/precision each
// (f64,f64)->i32 minimum25, no cap, existing UTF16 globals -> owned Snapshot.
// Raw integer/Inf precision, -1 shortest sentinel for exp/precision; NaN/fraction
// fatal. Exact bounded rounding ties up, pinned fixed formatter, <=123ASCII bytes.
// Tick1/check after; preserve status/pending/roots/pairs/saved returns/finalizers.
// Primitive precision coercion may add existing Number call; budgets fatal.
// valueOf guest-only original Number incl -0 after ignored effects, no host charge.
// No wrappers/general prototype/borrowed/computed/spread/generic/locale methods.
// Mandatory profile22/ABI25 even unused; rebuilt launcher maximum26/value12.
// checked-f64-utf16-v21 adds direct narrowed Number.toString(radix?,...ignored).
// ABI24/value12/schema12, inherited v20. number_to_string (f64,f64)->i32 minimum24,
// no permission; UTF16 result buffer -> owned Snapshot. Cache receiver/all args
// before radix coercion. Missing/Undefined10, other primitives ToNumber then
// truncate/NaN0; outside2..36 throws boxed/rooted RangeError BEFORE nonfinite cases.
// Numeric/default radix needs one formatting host call; primitive coercion may add
// its existing Number call. Decimal canonical; other bases shortest lowercase fixed
// text via exact even/odd midpoint intervals. BigUint0.4.8 now direct runtime use,
// already locked via compiler. Bound1074normalizations/55precision/<2500-bit scratch/
// 1077ASCII bytes. No guest limit/layout change. Preserve roots/pairs/status/buffers/
// pending/saved returns/finalizers and fatal budgets; JSON helpers reject calls.
// Old Error.toString unchanged. No wrappers/general prototype objects/borrowed/
// computed/spread/generic calls or other Number methods. Mandatory ABI24/profile
// even unused; rebuilt protocol2 launcher maxima26/value12 includes v21.
// checked-f64-utf16-v20 adds direct Number(...) on primitives/admitted unions.
// ABI23/value12/schema12, inherited v19. primitive_to_number (i64)->f64, minimum23,
// no permission/result globals/buffer. No args guest+0; UndefinedNaN, Null0,
// Boolean0/1, Number identity including zero sign. Strings trim exact ECMAScript
// whitespace, then require whole decimal/exponent/Infinity or unsigned0x/0b/0o
// grammar. Empty ->+0; invalid ->NaN without status error. Nearest/even parsing,
// signed underflow/overflow. Prefixes scan all digits, keep53+guard/sticky bits;
// no floating accumulation/arbitrary-precision dependency/partial parse.
// Cache first before ignored extras; preserve all effects/throws/roots/pairs/
// saved returns/TDZ. Tick once, pre/post deadline; bad cells/limits remain fatal.
// Keep status/buffers/pending channels, 1MiB encoded inputs and all other limits.
// Host-free helpers reject calls. No new Number/wrappers/object coercion, generic/
// spread/borrowed calls or Number.prototype.toString yet. Mandatory ABI23/profile
// even unused; rebuilt protocol2 launcher maxima26/value12 includes v20. No
// dependency/helper export/layout/container/permission change.
// checked-f64-utf16-v19 adds direct String(...) on primitives/admitted unions.
// ABI22/value12/schema12, inherited v18. primitive_to_string (i64)->i32, minimum22,
// no permission; returns UTF-16 via existing ptr/length globals and owned Snapshot.
// No args -> empty literal/no import; Undefined -> "undefined", Null -> "null",
// Boolean -> "true"/"false", Number -> canonical v18 text, String -> exact units.
// Cache first before ignored extras; all extras execute once/in order before
// conversion. Preserve throws, roots/GC, earlier operands/pairs, saved returns,
// buffers/status/pending channels. One host charge/present arg and pre/post checks;
// unchanged 1 MiB encoded strings, source/type/frame/memory/fuel/deadline limits.
// Host copies input before buffer write; Snapshot owns output before buffer reuse.
// Local/declaration functions named String retain ordinary dispatch and TDZ.
// No object/reference-union conversion, wrappers, generic/spread/borrowed calls,
// Number.prototype.toString or radix support here. Host-free helpers reject calls.
// Mandatory profile/ABI22 even unused; rebuilt protocol2 launcher maxima26/value12
// includes v19/v20/v21/v22/v23/v24/v25/v26/v27. No dependency/helper export/layout/container/permission change.
// checked-f64-utf16-v18 adds canonical decimal Number output in console.log/log,
// including narrowed/union Numbers. ABI21/value12/schema12, inherited v17 values:
// log_f64_v2 (f64)->(), value_log_v7 (i64)->(), minimum21, no permission/results.
// Both zeroes print 0; NaN and signed infinities use their standard names. Shortest
// finite text uses fixed notation at [1e-6,1e21), scientific outside with e/sign/no
// exponent padding. Existing ryu-js1.0.3; bounded<=25 numeric ASCII bytes. No locale.
// Other union kinds, Boolean0/1 logging, aggregates and JSON remain unchanged.
// Old profiles/imports keep historical text; v18 rejects old log imports. One
// host charge, unchanged deadlines/output/status/buffers/roots/pairs/finalizers;
// host-free helpers reject reachable logs. Mandatory profile/ABI21 metadata even
// unused; rebuilt launcher maxima26/value12 includes v18/v19/v20/v21/v22/v23/v24/v25/v26/v27. No layout/limit/dependency
// change. V19 adds primitive String(); Number.prototype.toString/radices and object coercion remain separate work.
// checked-f64-utf16-v17 adds direct Math.random, inheriting v16/Number/values.
// Host ABI20/value12/schema12: number_random ()->f64, minimum20, no permission or
// result globals. All ignored admitted arguments execute once/in order, without
// coercion; throws skip later args and entropy. No spread/generics/computed/borrowed
// calls or shadowing. Fresh getrandom0.3.4 OS word per call; high53 bits scale
// exactly by2^-53 into [+0,1), never 1/negative/NaN/Infinity. No production seed,
// shared PRNG, cache or replay promise. Math has no cryptographic guarantee.
// One host charge, pre/post entropy deadline checks, fatal failure without fallback;
// preserve status, buffers, pairs, GC/earlier roots, saved returns and fatal exit.
// JSON helpers reject host reachability. ABI20/profile metadata is mandatory even
// unused; native launcher maxima26/value12 includes v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27. No layout/container/limit
// change. Test-only entropy injection is store-local and absent from production.
// checked-f64-utf16-v16 adds native Math acos/acosh/asin/asinh/atan/atanh/cbrt/
// cos/cosh/exp/expm1/log/log1p/log10/log2/sin/sinh/tan/tanh (one Number) and atan2
// (two Numbers, y then x), inheriting v15 with host ABI19/value12/schema12. Direct
// unshadowed dot calls only; no coercion/unnarrowed unions/spread/generics/borrowing.
// Distinct number_<name> imports have typed f64 signatures, minimum ABI19 and no
// capability/result buffer/status reset. Exact NaN/zero/infinity/domain boundaries
// and atan2 quadrants precede platform-approximated finite results. Each call uses
// one host allowance and before/after deadline checks. Fatal budgets/exit persist;
// host-free JSON helpers reject all reachable native math. Math.random is absent.
// V16/ABI19 metadata and rebuilt launcher support are mandatory, even without an
// active native import. Old profiles/import contracts/defaults remain unchanged;
// launcher maxima26/value12 includes v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27. No layout/container/dependency/resource-limit change.
// V15 Math.hypot(...numbers) admits zero or more narrowed Numbers (no spread).
// All arguments execute once before selection: ±Infinity wins over NaN, otherwise
// NaN propagates and empty/all-zero calls return +0. Hidden f64 snapshots and an
// exact power-of-two scale, compensated square/product errors and root refinement
// avoid naive squaring overflow/underflow. Finite results are approximated; no
// universal correct rounding promise. Existing ValueBlock roots protect earlier
// operands/pairs across argument effects/GC. No host import, guest array/helper,
// ABI/dependency/permission/limit change; usual type/profile/source gates remain.
// V15 Math.f16round(value) takes one narrowed Number and rounds directly from
// binary64 to binary16 ties-to-even, returning Number. Never route through f32:
// halfway neighbors may double-round. Preserve NaN/±Infinity/±0; subnormals use
// exact scaling by 2^24 and nearest, normals use exponent-selected exact powers
// of two, and abs(x)>=65520 overflows to signed Infinity. No host import, helper,
// allocation, ABI/layout/dependency/permission/limit change. Ordinary argument
// effects, roots/pairs/finalizers, profile/type/arity gates and helper purity apply.
// V15 Math.pow(base,exponent) takes two narrowed Numbers and shares the existing
// number_power import with **: identical special-case bits, finite approximation,
// one host allowance/deadline/status rules. It is rejected in host-free JSON
// helpers, including through nested argument functions. No new ABI/import name.
// V15 Math constants: E/LN10/LN2/LOG10E/LOG2E/PI/SQRT1_2/SQRT2. Number constants:
// EPSILON/MAX_SAFE_INTEGER/MIN_SAFE_INTEGER/MAX_VALUE/MIN_VALUE/NaN/
// NEGATIVE_INFINITY/POSITIVE_INFINITY. Dot or literal String-key reads fold to
// exact f64 values; MIN_VALUE is bits 1, safe bounds are ±9007199254740991.
// Lexical shadows/captures/TDZ keep ordinary member rules, including mutable own
// fields of local Math/Number records. Intrinsic writes/updates/delete/pattern
// targets fail with a read-only type diagnostic. Dynamic keys/namespace values/
// descriptors remain unsupported. Old profiles reject built-in reads. No new
// imports, allocations, helper exports, ABI/layout versions, permissions or limits.
// V12+ Object.keys(record) returns a fresh dense string[] of present own keys:
// canonical array indices first numerically, other strings by insertion order.
// Explicit Undefined is present; absent optional fields are omitted. Input is
// evaluated once; no payload serialization or source/result aliasing occurs.
// Nullable single-record unions throw TypeError on absence. Unsupported kinds
// (boxed primitives/Error/result/functions), generic/spread calls
// and shadowed Object remain rejected. Internal roots/floors protect earlier
// operands, returns, GC/pairs and exceptions. Clear spare slots/bits before
// publishing length. Status/grants and all bounds persist; no ABI/helper change.
// V12+ Object.values(record) returns a fresh dense array in Object.keys order,
// omitting absent fields and keeping explicit Undefined. Infer the common field
// type or an admitted closed union; empty records yield undefined[]. Reject
// incompatible record/array unions rather than casting. Nullable source absence
// throws TypeError and does not widen the element type. Read/widen each payload
// once into rooted slots; preserve exact Number bits, immutable strings/unions
// and shared child identities through allocation, ordering, GC and finalizers.
// Keys/values share enumeration and canonical tail cleanup; status/grants and
// all bounds persist. Same input/call restrictions; no ABI/helper/layout change.
// V12+ Object.keys(array) and Object.values(array) also accept a bounded typed
// array or nullable single-array union. Keys are ascending decimal index strings,
// excluding length. Values preserve the exact element type without widening for
// skipped holes (v13+); explicit Undefined remains present. Results are fresh dense
// arrays with shared nested identities and zero spare slots/bits. Private guest
// helpers root partial prefixes across key allocations and restore temporary roots.
// No host call/coercion/status change; existing bounds and older profiles persist.
// Mixed array/record sources remain rejected; no ABI/layout/helper-export change.
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
// Arbitrary/coercive own properties remain unsupported. Simple/arithmetic/logical
// assignments and Number updates are specified above. Old profiles retain
// their mutation contracts. No new host import,
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

// checked-f64-utf16-v28: opaque BigInt scalars, ABI27/value12/schema12.
// Exact literals/operators, BigInt(), String/Number/Boolean/typeof, comparisons,
// toString(Number/Undefined radix), valueOf, asIntN/asUintN(Number,BigInt).
// Managed binary leaf flag2048; pure host work consumes guest fuel.
// Structured BigInt/JSON/throw/compound/update and complete coercive forms pending.

// checked-f64-utf16-v29: ABI28/value13/schema13/exception channel4.
// BigInt tag11 in closed records/sparse arrays/reference unions; exact roots.
// Primitive-union arithmetic, conversions and BigInt throw/catch are admitted.
// JSON.stringify BigInt -> recoverable TypeError; JSON tokens never create BigInt.
// Primitive width/radix coercion and numeric compound/update syntax are supported.
// Primitive update ToNumeric converts nonnumeric primitives within checked storage.
// Object ToPrimitive and general inferred storage widening remain pending.

// V29 also supports BigInt compound/numeric updates and primitive width/radix
// coercion. ToIndex precedes ToBigInt; Number values and BigInt radix throw
// TypeError. Undefined radix defaults to 10. Object ToPrimitive remains pending.

// V29 compound operators convert cached primitives, choosing String concatenation
// for += when either actual operand is String, otherwise numeric Number/BigInt.
// Mixed numeric kinds throw TypeError; checked storage and v28 rules remain.

// V29 ordinary arithmetic, exponentiation, bitwise/shifts and unary +/−/~ admit
// primitive conversion. Minus/not preserve BigInt; plus rejects BigInt after
// canonical validation. Full comparison/equality and object coercion pending.

// V29 equality/ordering supports all six admitted primitives, exact BigInt
// comparison, UTF16 order, NaN/unordered and nullish/Boolean coercion rules.
// Object and Symbol semantics remain pending.

// V30 adds optional closed-record deletion and primitive record/array delete keys.
// Required checked fields and array length are protected. Layouts stay v29-compatible.

// V30 Object.entries accepts a closed record or typed array (optionally nullish),
// returns fresh dense arrays of [key,value] pairs, and keeps nested identities.
// Pair elements use the admitted String/value union; tuples are not inferred.
// Skip absent fields/holes, retain explicit Undefined, snapshot own-key order.
// Evaluate source and ignored arguments once before enumeration. Nullish/missing
// inputs throw TypeError; Number/Boolean/BigInt inputs produce an empty array.
// V30 String inputs enumerate UTF16 units; primitive unions dispatch by kind.
// Symbol, object coercion and mixed aggregate shapes remain pending.
// V29 admission, ABI28/value13/schema13 and all production budgets stay unchanged.

// V30 Object.keys/values/entries enumerate String UTF16 code units (not Unicode
// scalars), preserving lone surrogates, and dispatch all-primitive unions by kind.
// Numeric/Boolean/BigInt wrappers yield empty arrays; nullish/missing values throw.
// Keys/values now evaluate ignored arguments before enumeration, like entries.
// Above 1024 String code units, throw recoverable RangeError. Guest-only helpers
// preserve existing ABI/layouts, fuel/memory ceilings and historical call rules.

// V30 Object.is implements SameValue: NaNs compare equal; +0 and -0 differ.
// Compare Strings by UTF16 contents, BigInts exactly, and references by identity.
// Different primitive kinds are unequal. Dispatch admitted unions without coercion.
// Arguments run once in order; missing/void inputs are Undefined. Ignored extras
// execute before comparison. Root operands across all later argument allocations.
// Number comparisons are guest-only. Native result pairs require explicit fields;
// Symbol and unimplemented value kinds remain pending. No ABI or budget change.

// V30 checked Object.assign copies enumerable own record/array/String properties,
// preserving target and child identity, source/key order, holes and explicit Udf.
// Evaluate all args before target validation; snapshot each source after prior copies.
// Plain fresh record literals reserve known record-source fields before allocation;
// added fields are optional and original literal storage may widen to an admitted union.
// Existing targets keep declared layouts, field guards and array element invariance.
// Nullish sources skip; nullish/missing targets throw. Function/Error/non-String
// primitive sources have no enumerable fields. Array length requires Number;
// failure preserves earlier writes. Unknown indexed keys retain checked write guards.
// Primitive target boxing, arbitrary new properties, descriptors and Symbol remain
// pending. No ABI/layout/ceiling change, and historical source admission stays frozen.

// V31 adds named interface/type alias annotations, resolved per module and across
// imports. import type / { type T } / export type { T as U } / export interface are
// admitted; ./x.js specifiers name ./x.ts sources. Named types are exact closed
// aliases (no width subtyping). Generics, extends, recursion, Record<K,V>,
// re-exports with from and local type declarations remain rejected. Shared
// ABI28/value13/schema13; no runtime representation or limit change.
