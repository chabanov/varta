# CLAUDE.md — Varta Agent Guide

Guide for AI agents and contributors working in this repository. Follow it exactly; when it conflicts with personal preference, this file wins.

## 1. What Varta is

Varta targets contract-driven cross-platform TS/JS applications with explicit
AI tool interfaces. The current implementation is a compiler and secure runtime
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

The platform roadmap is in `docs/development-plan.md`: broader Application
Contracts and shared application state, artifact
trust, then MCP bridges and deployment profiles remain planned. Ordinary source exports must not automatically become
AI tools; external callers enter the same authorized command/query path as UI/API
callers, and application state must outlive agent connections.

Varta currently does not provide general JavaScript compatibility. Every accepted language feature must stay small, statically analyzable, and map cleanly to Wasm; prioritize concrete contract/application needs over language feature count.

## 2. Repository layout

```text
Cargo.toml               # workspace only; crate metadata lives in crates/*/
crates/
  varta-common/          # config schema, capability model, shared error type
  varta-compiler/        # explicit TS module graph (SWC) -> Wasm; no runtime dependency
  varta-runtime/         # Wasmtime host: budgets, capability enforcement, host APIs
  varta-project/         # confined source loader, check/dev/build, artifacts and inspection
  varta-cli/             # the `varta` binary; thin orchestration, no domain logic
  varta-common/src/varta.d.ts # one owner for embedded host declarations
docs/
  architecture.md        # system spec — update when ABI/capabilities change
  development-plan.md    # phases and exit criteria
.github/workflows/       # CI mirrors the commands below
```

Placement rules:

- New crates go in `crates/<name>/` and are added to workspace `members` and, if user-facing, `default-members`.
- Domain logic (parsing, emission, capability matching, host I/O, artifact packaging and validation) never lives in `varta-cli`. The CLI calls varta-project and prints results.
- Shared types and errors belong in `varta-common`. Compiler and runtime must not re-declare them.
- A `lib.rs` that grows past a few hundred lines is split into modules by concern (e.g. `host.rs`, `expr.rs`, `glob.rs`). Module-private helpers stay private; the crate API surface stays small and documented.
- Unit tests live in `#[cfg(test)] mod tests` at the bottom of the module they test; cross-crate behavior goes in `crates/*/tests/`.

## 3. Commands and quality gates

All three gates must pass before every commit, and they are run frequently during work — not only at the end:

```bash
cargo fmt --all -- --check              # formatting; plain `cargo fmt --all` fixes
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p varta --test smoke        # CLI end-to-end tests
```

CI runs exactly this set. If clippy fails, fix the code, not the lint; `cargo clippy --fix --allow-dirty` may apply the mechanical part, but review the result as code. Report failures honestly with output; never claim tests pass without running them.

Useful during development:

```bash
cargo check                             # fast type-check, no codegen
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
4. **Validate all Wasm-derived input.** Every `ptr`/`len` pair from guest memory is bounds-checked, length-capped, and UTF-8-validated before use. Path inputs reject traversal (`..`), match component-aware scopes, and open through pinned directory descriptors with no-follow at every component. Never reintroduce check-then-open ambient paths.
5. **Explicit grants are the only authority.** Runtime enforcement uses package requests and, when supplied, the separately validated host ceiling. Every concrete resource must satisfy both; empty host grants never trigger fallback. Filesystem authority must be pinned from the host's roots, not package aliases. Nothing is ambient, and no integer from guest memory is ever treated as a capability.

Host-API housekeeping: adding a host function touches, in the same change: the shared `HostOperation` registry and grant schema in varta-common, AST-to-IR lowering, the runtime linker handler, `crates/varta-common/src/varta.d.ts`, and positive/denial/ABI regression tests. Artifact validation uses that same registry.

## 7. Testing requirements

- Write the test in the same change as the code it covers — never after the fact.
- Every compiler feature gets: a compile-and-validate test (Wasmtime instantiation) and a behavior test through `varta dev` where practical.
- Every capability rule gets a denial regression test in `crates/varta-cli/tests/smoke.rs` (positive and negative path).
- Smoke tests use temp directories and clean up; never touch the repo tree.
- No test may depend on network, environment variables beyond ones it sets, or execution order.

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
