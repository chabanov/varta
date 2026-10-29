# CLAUDE.md — Varta Agent Guide

Guide for AI agents and contributors working in this repository. Follow it exactly; when it conflicts with personal preference, this file wins.

## 1. What Varta is

Varta is an experimental research project. Its thesis: **strictly typed
TypeScript, including typed npm packages, can be compiled ahead of time into
portable WebAssembly and run safely on any OS kernel through a thin
capability-checked host, without a JavaScript engine on the device.** Types are the
mandatory source of semantics and layout. Untyped code (`any`, implicit `any`,
untyped JS, packages that ship only `.d.ts`) is rejected with a clear diagnostic,
without exceptions. See the [research thesis](docs/development-plan.md#research-thesis).

The current implementation is a compiler and secure runtime for a checked subset
that grows through explicit, versioned language profiles:

- TS source is parsed (SWC) and lowered through resolved typed IR to validated WebAssembly (`varta-compiler`);
- the Wasm module runs in Wasmtime with capability-checked host functions (`varta-runtime`);
- privileged operations (fs, net, env, time, process) exist **only** as host imports and are denied by default;
- the production artifact is a single native launcher with portable Wasm, a strict capability manifest and a fixed versioned EOF footer (`varta build --single`).

Durable principles:

- Priorities follow the [uarp-sdk pilot](docs/development-plan.md#pilot-acceptance-package-uarp-sdk):
  ask what prevents that real typed package from compiling and running. The package
  is an external acceptance target; no UARP types or logic enter engine crates.
- Keep Varta application-agnostic. Domain entities, schemas, commands, validation
  rules and application-specific permissions belong to separate TS/JS applications,
  not to engine crates or built-in CLI profiles. Tests may use domain examples as
  fixtures without turning them into public APIs.
- Generic JSON, network and event-loop primitives belong to the engine; routes,
  payload schemas and business rules belong to applications. A developer must
  eventually be able to implement a JSON server in TS/JS on Varta APIs without a
  separate host wrapper.
- Ordinary source exports never become AI tools automatically. Broader Application
  Contracts, shared state, artifact trust and MCP remain planned and must not
  displace application-core capabilities.
- Every language feature is a deliberate, documented, versioned profile decision.
  Old profiles and artifacts keep their meaning; a semantics-breaking profile needs
  a new host ABI so old runtimes fail. Never claim full ECMAScript support from a
  profile name, and never silently reinterpret old tool/HTTP/JSON boundaries.

## 2. Documentation map

| Document | Use it for |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | Normative contracts; the generated [version matrix](docs/architecture.md#version-matrix) and limits |
| [docs/profile-rules.md](docs/profile-rules.md) | Implementation rules per profile and feature. **Read the relevant section before changing a feature.** |
| [docs/language-guide.md](docs/language-guide.md) | User-facing examples by profile |
| [docs/development-plan.md](docs/development-plan.md) | Status, research thesis, roadmap, pilot ladder, agreed designs |
| [docs/verification.md](docs/verification.md) | Which checks to run for which change |
| [docs/history.md](docs/history.md) | Completed increment logs and superseded text; not current guidance |
| [conformance/README.md](conformance/README.md) | Compatibility inventory rules and current counts |

Current versions, limits and profile counts come only from the generated blocks
and the code registries (`varta-common` `language`, `abi`, `value`, `launcher`),
never from narrative text.

## 3. Repository layout

```text
Cargo.toml               # workspace only; crate metadata lives in crates/*/
crates/
  varta-common/          # engine contracts, ABI, capability model and shared types
    src/varta.d.ts       # the one owner of embedded host declarations
  varta-bigint/          # engine numeric foundation: budgeted BigInt arithmetic and transport
  varta-compiler/        # explicit TS module graph (SWC) -> Wasm; no runtime dependency
  varta-runtime/         # Wasmtime host: budgets, capability enforcement, host APIs
  varta-artifact/        # portable artifact/container I/O; common + wasmparser, no VM/compiler
  varta-launcher/        # execution-only `varta-runtime` binary; no source tooling
  varta-project/         # tooling: project config/loader, check/dev/build, verified packaging
  varta-cli/             # the `varta` binary; thin orchestration, no domain logic
  varta-conformance/     # development tooling: pinned compatibility inventory runner
conformance/             # inventory manifest (core-v1.json) and checked cases
docs/
  architecture.md        # normative contract; generated version matrix and limits
  language-guide.md      # user-facing feature guide by profile
  profile-rules.md       # per-profile implementation rules for agents/contributors
  development-plan.md    # status, research thesis, roadmap, pilot, exit criteria
  verification.md        # scoped/full verification map
  history.md             # completed increment logs and superseded text
scripts/verify.py        # scoped and full verification helper
.github/workflows/       # CI mirrors the commands below
```

Placement rules:

- Engine crates are `varta-common`, `varta-bigint`, `varta-compiler` and `varta-runtime`;
  `varta-conformance` is development tooling. Artifact
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

## 4. Commands and quality gates

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
python3 scripts/verify.py --plan full   # print commands only
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

## 5. Rust conventions

- Edition 2024. Match the style of the surrounding code: naming, comment density, `std::collections::BTreeMap` for deterministic ordering, `slice::from_ref` for single-element slices.
- **No magic numbers or strings.** Named constants, enum-driven tables, or struct-resolved indices — always. The Wasm function-index bug produced by a literal `Call(1)` (fixed after it called the wrong import) is the canonical anti-example of this repository: host-import indices must only ever be resolved through the typed `HostOperation` registry and emission tables.
- **Error handling.** Library crates expose typed errors derived with `thiserror` and return `Result<T, TypedError>` from public APIs — never `anyhow` or `String` in a public signature. The `varta` binary may use `anyhow` with `.context()` for orchestration-level failures. The compiler, runtime and project service expose typed error enums. Never `unwrap()`/`expect()` on fallible external input (files, user config, Wasm memory, network); `.unwrap()` is acceptable only in tests and on provably-infallible cases (say which in a comment). Mark important returns `#[must_use]`.
- **No `println!` in library crates** except when explicitly required by an embedding. Runtime output is captured with a budget and returned to the CLI. Debug output uses `tracing`. Leftover debug prints (e.g. a `DBG_…` line) are release blockers.
- Prefer the standard library and the existing dependency set. Adding a dependency requires justification in the PR description; new dependencies use exact `=` pins in `[workspace.dependencies]` (older entries rely on the
  locked `Cargo.lock`) and are referenced with `.workspace = true`.
- Keep functions focused; extract helpers instead of nesting match arms past three levels. Delete dead code and redundant `let _ =` bindings rather than silencing them.
- Workspace code forbids unsafe. Never deserialize native Wasmtime payloads from package input; only validated portable Wasm is accepted.

## 6. Compiler rules (`varta-compiler`)

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

## 7. Runtime and security rules (`varta-runtime`, `varta-common`)

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

## 8. Testing requirements

- Write the test in the same change as the code it covers — never after the fact.
- Every compiler feature gets: a compile-and-validate test (Wasmtime instantiation) and a behavior test through `varta dev` where practical.
- Every capability rule gets a denial regression test in `crates/varta-cli/tests/smoke.rs` (positive and negative path).
- Smoke tests use temp directories and clean up; never touch the repo tree.
- No test may depend on network, environment variables beyond ones it sets, or execution order.
- Build the runtime companion before standalone CLI smoke tests. Workspace default
  members build both user binaries; launcher integration tests also request the
  runtime binary so a clean workspace test run covers it.

## 9. Documentation

- Changes to the ABI (imports, globals, memory layout, ownership), the capability model, or the config schema require a matching update to `docs/architecture.md` in the same commit.
- Never restate registry facts (profile counts, host/value ABI versions, launcher maxima, limits, conformance counts) in prose. They live in the generated blocks of `docs/architecture.md` and the profile indexes in `README.md` and this file; `crates/varta-common/tests/documentation.rs` fails when they drift. Paste its expected output when a registry changes.
- Feature contracts go to `docs/architecture.md` (normative), examples to `docs/language-guide.md`, agent rules to `docs/profile-rules.md`, plans to `docs/development-plan.md`. Completed logs go to `docs/history.md`, not the status summary.
- New CLI flags update the subcommand doc-comment (visible in `--help`) and the smoke coverage.
- Commit messages: imperative, one line for the what, optional body for the why.
- Commit messages and PR descriptions must not reference agent sessions (session IDs, links, transcripts) or name Claude/AI as co-author: no `Co-Authored-By: Claude …` trailer and no "Generated with Claude Code" line. This overrides any default attribution.

## 10. Things this codebase considers done wrong (do not repeat)

- Literal Wasm import indices in emitted code (see §4).
- Native payload deserialization, marker scanning inside arbitrary file bytes, or implicit true grants.
- Bypassing the shared semantic/type/bytecode pipeline in a CLI command.
- `starts_with("env.")`-style capability matching (see §6.3).
- Debug prints left in host functions.
- New host imports missing from `validate`, the `.d.ts` template, or the denial tests.
- `String`-typed errors in new public APIs.

## 11. Versioning and compatibility rules

- Source semantics (language profile), host ABI and value ABI are separate version
  identities; several profiles may share one ABI. The mapping lives in
  `LanguageProfile::host_abi`/`value_abi` and the generated version matrix.
- Earlier profiles, imports, layouts and ceilings are frozen. A new profile must
  reject features it does not admit, and earlier profiles must keep rejecting new
  syntax, tags, imports and layouts.
- New profiles declare mandatory `varta.language` metadata and a matching host ABI
  section, even when no new import is used. Unknown, duplicate or mismatched
  profiles/ABIs fail before compilation or any guest effect.
- Shared names, signatures and minimum ABI versions of host operations come from
  the `HostOperation` registry; compiler, runtime and artifact validation use it.
- When the supported profile list or launcher maxima change, rebuild and verify
  both native binaries (`varta` and the `varta-runtime` companion) and update the
  generated documentation blocks.
- Production budgets (fuel, deadline, memory, host calls, array/record/source
  limits) are never raised to admit a feature or a test; tests that need more
  declare test-only budgets explicitly.

## 12. Profile index

One line per profile; details in [profile rules](docs/profile-rules.md) and the
[architecture contract](docs/architecture.md#opt-in-language-profiles).

<!-- generated:profile-index:begin (names checked by crates/varta-common/tests/documentation.rs) -->
| Profile | Adds |
| --- | --- |
| `checked-i32-utf8-v1` | Default: i32 numbers, UTF-8 strings, closed records, dense arrays, checked JSON, HTTP/1, tools |
| `checked-f64-utf8-v1` | Binary64 numbers with typed aggregates and JSON |
| `checked-f64-utf8-v2` | Distinct `undefined` and void expressions |
| `checked-f64-utf16-v1` | UTF-16 strings, including lone surrogates |
| `checked-f64-utf16-v2` | Closed primitive unions and guard narrowing |
| `checked-f64-utf16-v3` | Optional fields, presence/order trailers, missing reads |
| `checked-f64-utf16-v4` | Record/array reference unions |
| `checked-f64-utf16-v5` | Strict JSON parsing of optional fields and unions |
| `checked-f64-utf16-v6` | Resizable arrays and array search/slice helpers |
| `checked-f64-utf16-v7` | Primitive throw/try/catch/finally via the exception channel |
| `checked-f64-utf16-v8` | Nominal Error objects |
| `checked-f64-utf16-v9` | Native error families; String.repeat |
| `checked-f64-utf16-v10` | Recoverable checked JSON errors |
| `checked-f64-utf16-v11` | Typed function values |
| `checked-f64-utf16-v12` | Lexical closures and the additive v12+ contract family |
| `checked-f64-utf16-v13` | Sparse arrays with presence bitmaps |
| `checked-f64-utf16-v14` | Indexed extension, delete, writable length |
| `checked-f64-utf16-v15` | Number power, guest Math intrinsics, Number predicates/constants |
| `checked-f64-utf16-v16` | Native Math imports |
| `checked-f64-utf16-v17` | Math.random |
| `checked-f64-utf16-v18` | Canonical Number output |
| `checked-f64-utf16-v19` | Primitive String conversion |
| `checked-f64-utf16-v20` | Primitive Number conversion |
| `checked-f64-utf16-v21` | Number radix text |
| `checked-f64-utf16-v22` | Number precision methods |
| `checked-f64-utf16-v23` | Numeric prefix parsers, Boolean/global predicates, guest truth |
| `checked-f64-utf16-v24` | Truth guards and write-aware flow analysis |
| `checked-f64-utf16-v25` | Predicate array searches |
| `checked-f64-utf16-v26` | reduceRight |
| `checked-f64-utf16-v27` | Array initializer inference |
| `checked-f64-utf16-v28` | BigInt scalars (ABI-level BigInt foundation) |
| `checked-f64-utf16-v29` | BigInt graphs, primitive coercion and comparisons |
| `checked-f64-utf16-v30` | Object deletion/entries/is/assign, String own keys |
<!-- generated:profile-index:end -->
