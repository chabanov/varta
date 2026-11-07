# Checked core compatibility inventory

This developer suite pins **ECMAScript 2025**, the **checked-f64-utf16-v38** profile
and the **checked-ts-main-v1** source model. Its original cases are checked
TypeScript programs with a Varta `main` entry point and observable runtime output.
Feature rows identify their CORE work streams and either an ECMAScript section or
a Varta-specific contract. This is a partial compatibility inventory. It does not
run Test262 or establish whole-edition, Deno/Bun, platform or production readiness.

Run from the repository root:

```sh
cargo run -p varta-conformance --locked -- --json
cargo run -p varta-conformance --locked -- --require-supported
```

The first command checks the reviewed baseline and emits a standalone JSON report.
The second additionally fails for known unsupported cases and features without
executable cases. The current inventory has 105 feature rows and 182 cases:
178 pass, 4 are known unsupported, and 8 feature rows have no case. These counts
are inventory observations, not a percentage of ECMAScript or total kernel work.
The strict command currently exits 1 even when the baseline matches.

Each source file is read as an immutable UTF-8 snapshot and checked against its
manifest SHA-256. All `.ts` files beneath `cases/` must be listed. Duplicate IDs,
paths and metadata, unknown versions/profiles/fields, unlisted sources, symlinks,
relative path escapes and resource-limit violations fail inventory loading.
`.gitattributes` preserves the pinned LF bytes across Git checkouts.
The report records the inventory digest, each source digest, profile, source model,
package version, dependency-lock digest, worker timeout, runtime budgets, OS and architecture. Case and feature
results remain separate from the overall baseline check.

Every case runs in a fresh process with a cleared environment, a fresh runtime and
empty requested/ceiling grants, unless its manifest row lists reviewed `grants`:
those are then both the requested grants and the host ceiling. Only `time.timer`
(v36 timers, which wait on the runtime event loop within the execution deadline
and have no other effect) is admitted; any other, duplicate or unsorted grant
fails inventory loading. Compilation, validation and execution have a
15-second wall limit (configurable to 1–60000 milliseconds with `--timeout-ms`).
The guest retains ordinary production fuel, deadline, memory and host-call limits.
Workers accept source snapshots over bounded stdin; concurrent pipe drains keep a
blocked writer or excessive output from blocking monitoring. A timed-out worker
is killed and reaped. Worker stdout/stderr limits are 64/8 KiB; a limit, process,
protocol or internal compiler failure is an infrastructure failure, never a pass.

Other bounds: 1 MiB inventory, 256 cases/features, 1 MiB per source, 16 MiB total
source snapshots, 1024 corpus directory entries and 16 KiB/128 lines of expected
output per case. Sources cannot import files through this executor. These bounds
belong to the development runner and do not change production execution budgets.

| Report status | Meaning |
| --- | --- |
| `passed` | Compilation and default-budget execution succeeded with exactly the expected output and zero exit status |
| `known_unsupported` | Compilation rejected a documented gap with the exact reviewed diagnostic code; counted separately from passes |
| `unexpected_support` | A gap case now executes with expected output; baseline review is required |
| `regression` | Output, compilation diagnostic or execution differs from the reviewed requirement |
| `infrastructure` | Worker, protocol, timeout, output limit or internal compiler failure |

Default exit codes are 0 for a matching baseline, 1 for a mismatch or strict-mode
incompleteness, and 2 for invalid input/CLI/tool I/O. A matching baseline can contain
known gaps. Expected unsupported entries cannot suppress internal compiler errors
or runtime failures. No baseline is automatically learned from observed failures.

## Maintaining the inventory

1. Add an original case under `cases/` and name its exact semantic expectations.
   Type annotations are part of this checked source model. Preserve a separate
   gap case when an annotation isolates an operation from a missing inference rule.
2. Map it to a feature with pinned reference, origin and CORE IDs. Add further
   feature rows without cases when their acceptance work remains unexercised;
   reports and strict mode keep these omissions visible.
3. Review the source, then calculate its SHA-256 and add its manifest row. A known
   gap needs an exact diagnostic code and a concrete reason. Do not move a
   failing case into the gap baseline to hide a supported-feature regression.
4. Run both commands, the runner tests, and the workspace quality gates. Update
   baselines/profile only after reviewing changed behavior. The JSON report
   includes observed values/diagnostics for that review.

`array-union-inference.ts` was a v26 gap and now passes in the reviewed v27
profile. `json-undefined-members.ts` keeps its explicitly typed union array to
independently check serialization. Four further cases cover inferred primitives,
empty array peers, reference aliases and inherited aggregate contexts. Earlier-profile rejection is retained
in compiler/runtime/CLI regressions. Neither case family establishes general
dynamic arrays or unbounded inference.

Full Test262 integration remains future CORE-15 work. Its
[execution contract](https://github.com/tc39/test262/blob/main/INTERPRETING.md)
requires script/module modes, harness bindings and other host behavior. This
executor does not rewrite upstream tests into functions or treat unsupported
scripts as conformance passes. Expand that execution model, edition-wide feature
coverage, historical-profile matrices and independent platform/security work
before making broader compatibility claims.

The v28 review observed the unchanged BigInt addition case as unexpected support
before promoting it. Five further positive cases cover exact arithmetic, primitive
conversions/comparison, recoverable native errors and typed widths/radices. A
separate BigInt array case remains known unsupported: scalar execution does not
close aggregate/union/throw/JSON integration or complete BigInt coercion/built-ins.

The v29 review observed the original unchanged BigInt-array source as unexpected
support before promoting it. Five further cases exercise BigInt graphs, optional
fields, arithmetic through primitive unions, throwing/catching BigInt and JSON's
BigInt rejection/type selection. At that checkpoint the separate coercive-width
case remained a gap; the primitive-method increment below closes it.
Value/schema13 and exception channel4 belong to v29; v28 stays scalar-only.

The v29 primitive-method increment observed the unchanged coercive-width gap as
unexpected support with its pinned source hash and expected output before review
and promotion. A separate radix/error case covers primitive conversion and the
Number-value TypeError distinction from BigInt(). Object ToPrimitive remains an explicit CORE-01 gap outside these passing cases.

Two primitive-update cases cover converted postfix values, exact BigInt kind,
Number-array holes and optional-field NaN/presence. Their normalized output also
matches local Node.js v24.20.0. Passing these checked programs does not admit
object ToPrimitive, arbitrary mutable storage widening or full JS compatibility.

Three compound conversion cases cover primitive arithmetic/concatenation, sparse
reads and RHS order, plus six-member operand unions and mixed numeric errors.
Their output matches normalized local Node.js v24.20.0. Object coercion, full
comparison/equality and broader mutable-storage inference remain separate gaps.

Three ordinary arithmetic/unary/order cases cover primitive conversion, exact
BigInt kind, mixed numeric errors and once-only source evaluation. Their output
matches normalized Node.js v24.20.0. This does not establish full comparison,
equality, object/Symbol semantics or unrestricted JS value storage.

The completed primitive comparison block adds equality, relational and source-order
cases. Runtime tests separately pin a Node.js v24.20.0 matrix of 2048 results
across eight operators, including all six admitted primitive kinds. These cases
do not cover object/Symbol ToPrimitive or unrestricted JS storage.

The v30 deletion increment adds three cases with reviewed Node.js output. Required
closed fields, descriptors, object/Symbol keys and arbitrary dynamic properties
remain outside this checked increment; the CORE ledger stays Partial.

The v37 RegExp stream adds two RegExp cases next to the original `regexp-test`
gap: grouped global `exec` with `lastIndex` and Unicode/case flags, and the
regex string methods with templates, captures in `split` and a replacer. All
three remain known gaps while the inventory pins v36; under v37 they produce
exactly their manifest output, which matches Node.js v24.20.0.

The v30 object-operation increment adds optional-record deletion, primitive delete
keys and Object.entries snapshots for closed records and typed arrays. Five new
pinned cases cover key order, side effects, sparse presence and shared child
identity and match Node.js v24.20.0. Required checked-field deletion, descriptors
and general object coercion remain outside this increment.

The String enumeration increment pins UTF16 surrogate behavior and six-primitive
union dispatch in two more original cases. Both match Node.js v24.20.0; production
capacity and instruction limits remain enforced, rather than raising corpus
budgets to turn capacity stress fixtures into compatibility passes.

The v30 SameValue increment adds an explicit SameValue feature row and two
original cases for Object.is primitives, identity and source-order effects. Their
outputs match Node.js v24.20.0. A separate runtime fixture pins all 324 pairs of
18 primitive values, including signed zeros and NaNs of different signs. These
checks do not establish Symbol, unrestricted objects or whole-edition readiness.

The checked Object.assign increment adds one feature row and three original
cases: record copies/fresh literal shapes, sparse array/String copies, source
ordering/aliases and partial mutation before a standard array-length error.
All match Node.js v24.20.0. Separate runtime tests exercise checked storage guards
and declared layout restrictions; these are not promoted to claims of arbitrary
JavaScript property creation or generic Object.assign support.

The v32 pilot batch moves the pinned profile from v31 to v32; every earlier case
keeps its status. Four feature rows are added (module constants, literal types,
module re-exports, template literals) and five original cases: module constants,
literal types, class inheritance and dispatch on the existing classes row, template
literals, and one case combining classes, literal-typed fields, module constants
and template substitutions. All five outputs match Node.js v24.20.0 type stripping
with booleans normalized to 0/1. The function-local `class-instance` case stays a
known gap, because v32 admits top-level classes only. The re-export row has no
case: this executor compiles single-file sources without a module loader, so
re-exports are covered by compiler, public-API and CLI module tests instead.

The second v32 wave keeps every earlier case at its status. It adds four feature
rows (default exports, interface inheritance, generics, Error subclasses) and
three original cases: interface inheritance with declaration merging and a
generic base, generic interfaces/aliases/functions with inferred and explicit
type arguments and the closed-type built-ins, and an Error-class hierarchy with
`new.target.name`, `cause`, `instanceof`, `toString` and `finally`. All three
outputs match Node.js v24.20.0 type stripping with booleans normalized to 0/1.
The default-export row has no case for the same reason as the re-export row:
importing a default needs a second module, which this single-file executor does
not load; compiler, runtime and CLI module tests cover it.

The v33 generators increment moves the pinned profile from v32 to v33; every
earlier case keeps its status. One original case on the existing generators row
covers `next(v)` delivery, the `TReturn` completion, a completed generator,
`for...of` closing through `finally`, `return(v)` at a yield and `throw(e)` on a
not-started generator; its output matches Node.js v24.20.0 type stripping. The
unannotated `generator-next` case stays a known gap with a new reviewed
diagnostic: v33 requires an explicit `Generator<T, TReturn, TNext>` return type
instead of inferring one. `yield*`, async generators and the iterator protocol
of other objects remain outside this row.

The v34 batch moves the pinned profile from v33 to v34 once; every earlier
case keeps its status and produces the same report under value layout 15. The
records stream adds the `index-signatures`, `recursive-types` and
`wide-records` feature rows with one original case each (`index-signatures.ts`,
`recursive-json-value.ts`, `wide-records.ts`). The unknown stream adds the
`unknown-type` and `checked-assertions` rows with one original case each
(`unknown-narrowing.ts`, `checked-assertions.ts`); under the v34 pin both were
observed as unexpected support with their pinned source hashes and expected
output before review and promotion. All five outputs match Node.js v24.20.0
type stripping.

In the async stream the unchanged `async-function` case was observed
as unexpected support with its pinned source hash and expected output before
review and promotion. Three original cases are added: `promise-jobs` (the first
case of the promise-jobs row: FIFO reactions, interleaved chains and the two
extra jobs of adopting a returned promise), `promise-resolution` (synchronous
executor, one-time resolution, executor exceptions, the self-resolution
`TypeError` and `finally` pass-through and override) and `async-await-errors`
(`try`/`catch`/`finally` across `await` and rejection propagation). All outputs
match Node.js v24.20.0 type stripping. `promise-resolve` stays a known gap with
a new reason: v34 promises have no value tag, so `typeof` rejects them.
Thenables other than internal promises and the combinators remain outside
these rows.

The v35 batch moves the pin to `checked-f64-utf16-v35`. Under it every v34
case keeps its status except `promise-resolve`, which was observed as unexpected
support with its pinned source hash and expected output (v35 promises carry a
value tag, so `typeof` reports `"object"`) and is promoted. The batch adds
eight original cases, all supported under the v35 pin and matching Node.js
v24.20.0 type stripping:

- the records stream's `mixed-index-records` row with `mixed-index-records`
  and `dictionary-iteration-updates` (declared fields next to a string index
  signature, `for...in`, compound updates and spreads), and its `unknown-json`
  row with `json-parse-unknown` (`JSON.parse` to `unknown` and `object`
  narrowing);
- the async stream's `async-class-method` (an overridden async method
  awaiting `super.step`), `async-closure-captures` (an async arrow sharing a
  captured `let` across `await`, and an async function expression) and
  `promise-combinators` (`Promise.all`, `race` and `allSettled` job order and
  results);
- the Web stream's `web-headers` (CORE-10) and `abort-signals` (CORE-09) rows,
  both Varta contracts for the engine-provided Web types, with `web-headers`
  and `abort-controller`.

The async, Web and `promise-resolve` cases were observed as unexpected support
with their pinned hashes and expected output under the v35 pin before review
and promotion. Abort listeners, timers and live header iterators remain outside
these rows.

The v36 batch moves the pin to `checked-f64-utf16-v36`. Its streams add:

- the functions stream's `function-fields` and `optional-chains` rows (CORE-03)
  with one case each, and an `abort-listeners` case under the existing
  `abort-signals` row;
- the records stream's `checked-declared-fields` case on the
  `mixed-index-records` row (declared arrays, records and dictionaries next to
  `[key: string]: unknown`, built by literals and converted from JSON);
- the timers stream's `timers` row (CORE-09, a Varta contract) with
  `timers-event-loop` (main and its microtasks before any timer, equal delays in
  creation order, each timer's microtasks before the next, a cleared timer and
  the sleep pattern), the first case with a reviewed `time.timer` grant;
- the async stream's `async-generators` row (CORE-03, CORE-09) with
  `async-generator-iteration` (requests queued before the first settles,
  `yield await`, a string `TReturn`, and `for await` closing through `finally`
  on `break`).

Each case was a reviewed `unsupported_syntax` gap under the v35 pin; under the
v36 pin the five compile-time gaps were observed as unexpected support with
their pinned hashes and expected output, which matches Node.js v24.20.0 type
stripping, and were promoted. The timer case was observed as a denied runtime
failure without a grant, and passes with its reviewed grant. Earlier-profile
rejections stay covered by the compiler/runtime/CLI regression tests.

The v37 language-core stream adds eight original cases while the pin stays at
v36, each a reviewed known gap with the exact v36 diagnostic code and an
output that matches Node.js v24.20.0 type stripping and the v37 compiler:
`tdz-closure-capture` (`temporal-dead-zone`, CORE-03: a closure reading a
`const` before its initialization throws `ReferenceError`, and a nested
function declaration is hoisted), `deferred-let-assignment`
(`definite-assignment`), `string-switch` (`switch-statement`, strict-equality
selection with fallthrough and a leading `default`), `property-path-narrowing`
(`property-narrowing`), `dense-element-reads` (`dense-element-types`, CORE-02)
and `void-function-assignability`, plus `discriminated-unions`
(`record-unions`, CORE-02: a union of three record shapes switched on its
literal `kind`, with `never` in `default`) and `module-variables`
(`module-variables`, CORE-12: module `let` state shared with a
function-valued module constant). The Varta-contract rows reference the
architecture contract. When the pin moves to v37 each is expected to be
observed as unexpected support and reviewed for promotion; the TypeError
deviations of dense reads of holes and missing indices stay pinned in the
runtime tests, not here.
The v37 collections review added tuple, string built-in and UTF-8 text gap cases
(`tuple-destructuring.ts`, `string-split-pad-join.ts`, `text-encoder-roundtrip.ts`)
with the `tuples` and `string-builtins` feature rows. They and the earlier Map, Set,
Uint8Array and Date gap cases run with exactly their pinned output under
checked-f64-utf16-v37 (checked against local Node.js v24.20.0), but stay known
gaps while the inventory is pinned to checked-f64-utf16-v36; moving the pin
promotes them after the usual unexpected-support review.

The v37 net-request stream adds the `url` row (CORE-11, a Varta contract) with
`url-search-params` (relative resolution with scheme/host lowercasing and the
default port dropped, and an `application/x-www-form-urlencoded` list), a
reviewed `unsupported_syntax` gap under the v36 pin (`URL` is a v37 prelude).
`fetch` has no inventory case: it needs a `net.request` grant and a loopback
server, and the reviewed per-case `grants` field admits only `time.timer`.
That stays a recorded gap; the runtime tests compare `fetch` with Node.js
v24.20.0 against a loopback server instead.

The v37 integration moves the pin to `checked-f64-utf16-v37`. Under it the 22
v37 cases kept as known gaps under the v36 pin (the eight language-core cases,
the three class-generics cases, the Map, Set, Uint8Array, Date, tuple, string
built-in and UTF-8 text cases of the collections stream, the three RegExp
cases and `url-search-params`) were observed as unexpected support with
exactly their pinned hashes and expected output, which matches Node.js
v24.20.0 type stripping, and were promoted. The move first exposed three
regressions of supported cases (`unknown-narrowing`, `module-constants`,
`generics`): dense index reads threw on `undefined` checks that the cases
rely on, which the integration fixed in the compiler rather than in the
cases. The four remaining known gaps (`symbol-type`, `class-instance`,
`define-data-property`, `generator-next`) are unchanged, and `fetch` remains
the recorded gap above.

The v38 integration moves the pin to `checked-f64-utf16-v38`. Under it every
earlier case keeps its status with unchanged output, and the four known gaps
are rejected with their reviewed diagnostic codes, so nothing was promoted;
the `class-instance` and `generator-next` reasons now name v38. Seven feature
rows, each a Varta contract with its architecture anchor, and ten original
cases cover the v38 paths the pinned SDK and the console use:

- `web-bytes` (CORE-05, CORE-10) with `web-bytes-bodies`: `Blob` parts of
  text and bytes, slices, `ArrayBuffer` copies, `Response` bodies read as
  text, bytes and JSON (a second read is a `TypeError`) and a `Request` built
  with a byte body;
- `typed-global-this` (CORE-10, CORE-12) with `global-this-detection`: an
  optional `globalThis.fetch`, `crypto.randomUUID`/`getRandomValues`, an
  optional `process.env` read and `typeof` of engine globals;
- `strict-members` (CORE-02) with `strict-member-unions`: a twenty-member
  discriminated union selected from JSON, an exhaustive `switch` ending in
  `never`, optional chains over declared fields and a literal computed key;
- `structural-subtyping` (CORE-02, CORE-03) with `structural-views` (object
  literals and class instances viewed as narrower interfaces, writes through
  a view, callbacks with fewer parameters, an extra optional parameter and a
  boolean-field guard) and the known gap `union-record-argument`;
- `fetch` (CORE-10, CORE-11) with `fetch-injected-transport` (an SDK-style
  injected `FetchLike` answering with constructed `Response`s, a retry loop,
  `Request`/`Headers` construction) and `fetch-denied`;
- `kv-storage` (CORE-11, CORE-13) with `kv-denied-open`;
- `sdk-paths` (CORE-04, CORE-05) with `sdk-error-causes`: `cause` options
  through `super` and native families, causes read through `unknown`, and
  `instanceof` of `Uint8Array`, `Map` and `Set` members of closed unions;
- `template-literal-forms` on the existing `template-literals` row: nested
  templates, private fields and getters, optional chains with `??`, BigInt
  and union values, multi-line text and dictionary keys.

Seven of the nine new supported cases, all but `fetch-denied` and
`kv-denied-open`, print what Node.js v24.20.0 type stripping prints (with
`await main();` appended); the stored output of the known gap
`union-record-argument` is also Node's. Those two are Varta contracts: the runner grants nothing, so an
ungranted `fetch` rejects with a logged, recoverable `NotAllowedError` before
any socket, where Node attempts the connection, and `varta.kv.open` (which
Node lacks) rejects the same way before any file is touched. Both fetch cases
also run with the same output under v37; the v37 compiler rejects the other
new cases. The reviewed per-case `grants` still admit only `time.timer`: a
granted `fetch` needs a loopback server and a KV database a virtual file root
with a host directory, which this executor does not provide, so those paths
stay covered by the runtime's loopback and Deno-fixture KV tests. Running
granted KV and `fetch` cases inside the runner would change its security model
and remains an open owner decision. A runner
test pins that `net.request`, `kv.read`, `kv.write` and `env.get` grants fail
inventory loading and that the v38 `process.env` and KV globals disclose
nothing of the parent environment.

`union-record-argument` is a reviewed `type_error` gap: TypeScript accepts an
`A | B` of record interfaces where `{ n: number }` is expected (Node prints
`1 2`), while v38 views only a single record or class instance as a narrower
record. Writing the cases also met forms v38 rejects that are not inventory
cases yet: `Array.prototype.shift`, a union of `Map` and
`Set`, template-literal computed keys in object literals, `await` inside a
template literal or inside a call argument such as `String((await g()).n)`
(an awaited property base in a declaration, `const n = (await g()).n`, is
admitted), `===` between two `Uint8Array` values, `Request` body
readers such as `arrayBuffer()`, a local binding
named `log`, and function members read through `globalThis.globalThis`.

The v38 iteration-protocol review (2026-11-06) adds the `iteration-protocol`
feature row with three passing cases: an iterable class with an overridden
`[Symbol.iterator]()` generator method and an explicit iterator call
(`iterable-class.ts`), array literal spreads and `Array.from` over arrays,
generators, a `Set` and an iterable instance (`array-spread.ts`), and `yield*`
over a generator with `throw`/`return` forwarding and over an array
(`yield-delegation.ts`). Their expected output is Node.js v24.20.0's. The
`symbol-type` gap is unchanged: `Symbol.iterator` is recognized only as a
computed method key, and no `Symbol` value exists.

The K9 A type-level review (2026-11-06) adds a third origin,
`typescript_handbook` (references under
`https://www.typescriptlang.org/docs/handbook/`; tsc 5.x is the type-system
reference), and nine feature rows with one passing case each: conditional
types with distribution, `infer` (constrained, tuple, template and function
patterns) and recursive aliases (`ts-conditional.ts`); mapped types with `as`
remapping and `+/-readonly`/`+/-?` over records, tuples and arrays
(`ts-mapped.ts`); template literal and intrinsic string types
(`ts-templates.ts`); every built-in utility type (`ts-utilities.ts`);
`keyof`, precise `T[K]`, `typeof`/`keyof typeof` queries and record
intersections (`ts-queries.ts`); type predicates, assertion functions,
`satisfies` and `never` exhaustiveness (`ts-narrowing.ts`); call and construct
signatures, constructor types and polymorphic `this` (`ts-signatures.ts`);
named, optional, rest and variadic tuple elements (`ts-tuples.ts`); and
literal inference and widening (`ts-literals.ts`). Every expected output is
Node.js v24.20.0's. These cases exercise type-level evaluation at compile
time; they do not establish tsc's complete assignability or inference.

The K9 A step-2 review (2026-11-06) adds the `ts-class-subtyping` row with
`ts-containers.ts` (subclass instances in arrays, tuples, records, `Map`/`Set`
values and unions, `B[]` and `(B | C)[]` as `A[]`) and four cases on existing
rows: `ts-brands-and-queries.ts` (branded primitives, `typeof` of locals,
`keyof`/indexed access/`Partial` over a class and `keyof` of a tuple),
`ts-narrowing-more.ts` (subclass, `unknown`-to-object and `this is T`
predicates), `ts-inference-more.ts` (`infer` through a declared generic, a
union pattern and an optional element; elements after a rest element) and
`ts-signatures-more.ts` (overloads, function intersections, generic call
signatures, `this` in object types, pattern template types). Their output
is Node.js v24.20.0's; the two intentional deviations (a lying predicate's
`TypeError`, the copy of a `B[]` viewed as `A[]`) are pinned by runtime tests,
not inventory cases.

The K9 D standard-library review (2026-11-06, selector `standard_library`)
adds five feature rows with seven passing cases and four cases on existing
rows: `array-copying-methods` (CORE-02) with `array-sort-comparator` (V8's
comparator call order, `toSorted` stability, the default String order),
`array-copying-methods` (`toReversed`, `with` and its `RangeError`,
`toSpliced`, `Array.of`, mapped `Array.from`) and `array-splice-flat`
(`splice`, `copyWithin`, `shift`/`unshift`, `concat`, `flat`, `flatMap`);
`iterator-helpers` (CORE-03) over an infinite generator and an array
iterator, including the `take` `RangeError`; `set-methods` (CORE-05);
`grouping` (CORE-02, CORE-05) with `group-by` (`Object.groupBy`,
`Map.groupBy`, `Object.fromEntries` of tuples and a `Map`);
`aggregate-error` (CORE-04); `json-stringify-options` on `json-stringify`,
`date-components` on `date` (the complete `Date` in the UTC zone),
`string-well-formed` on `string-builtins` and `math-spread` on `math`. Their
expected output is Node.js v24.20.0's with `TZ=UTC`. `Array.prototype.shift`,
listed above as a rejected form, is admitted now. The `symbol-type` gap is
unchanged: `Symbol()` is refused with the reason (a value tag awaiting an
owner decision).

The v38 declarations-and-classes review (K9 B, 2026-11-06) uses the
`typescript_handbook` origin for language features that ECMAScript does not
define. It adds eight feature rows and
eleven original cases, each printing what Node.js v24.20.0 prints for the
tsc 5.9.3 emit (strict, ES2022, `useDefineForClassFields`, `await main();`
appended):

- `enums` (typescript_handbook; CORE-01, CORE-02) with `enum-members` (auto-increment,
  string and bit-flag members, reverse mapping of constant and dynamic keys,
  an exhaustive `switch`, `let` widening to the enum type) and `const-enum`
  (string and computed `const enum` members in interface fields);
- `namespaces` (typescript_handbook; CORE-02, CORE-03) with `namespace-merging`
  (merged namespace declarations, a nested namespace, a namespace type and a
  function merged with a namespace);
- `abstract-classes` (typescript_handbook; CORE-02) with `abstract-members`
  (abstract methods and getters dispatched through an abstract base type);
- `overloads` (typescript_handbook; CORE-03) with `function-overloads` (function and
  method overloads selecting their result types);
- `class-accessors` (ECMAScript method definitions; CORE-02, CORE-03) with
  `class-setters` (getter/setter pairs, a setter-only accessor, compound
  assignment and update through accessors) and `auto-accessor`;
- `static-class-members` (ECMAScript class definitions; CORE-02, CORE-03)
  with `static-fields` (static fields, `readonly` statics and `this` in a
  static method called through a subclass) and the known gap
  `static-block` (`unsupported_syntax`: static blocks run code at class
  definition, pending the module-initialization design);
- `class-expressions` (ECMAScript class definitions; CORE-02, CORE-03) with
  `class-expression` (anonymous and named class expressions bound by
  `const` at module level and in a function);
- `decorators` (typescript_handbook; CORE-02, CORE-03) with the known gap
  `class-decorator` (`unsupported_syntax`, for the same reason as static
  blocks).

Classes declared inside functions are hoisted to module classes like class
expressions (a class that would capture the function's locals is refused),
so the `class-instance` gap is promoted to supported with its pinned hash
and output.

Module evaluation (K9 B, 2026-11-06) runs static blocks where their class is
defined, so the `static-block` gap passes with its pinned hash and output.
Enum and namespace objects add `enum-object` (own keys, values and entries
of numeric, string and heterogeneous enum objects, `typeof` and `keyof
typeof`) and `namespace-object` (a namespace object's keys, its use as a
`typeof NS` argument and a nested namespace object), both matching Node.js
v24.20.0.
`enum-assignability` pins tsc 5's enum assignability (a `number` value to a
numeric enum, nominal string enums, `'x' as E`); tsc accepts it and Node.js
v24.20.0 prints the stored output.
The `class-index-signatures` row (typescript_handbook; CORE-02) adds
`class-index-signature`: undeclared members of instances with a string index
signature, by name and computed key, also through a subclass; it matches
Node.js v24.20.0.
