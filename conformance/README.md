# Checked core compatibility inventory

This developer suite pins **ECMAScript 2025**, the **checked-f64-utf16-v27** profile
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
executable cases. The current inventory has 38 feature rows and 40 cases:
28 pass, 12 are known unsupported, and 7 feature rows have no case. These counts
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
