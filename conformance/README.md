# Checked core compatibility inventory

This developer suite pins **ECMAScript 2025**, the **checked-f64-utf16-v34** profile
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
executable cases. The current inventory has 54 feature rows and 96 cases:
86 pass, 10 are known unsupported, and 8 feature rows have no case. These counts
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
empty requested/ceiling grants. Compilation, validation and execution have a
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
