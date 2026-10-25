# CLAUDE.md — Varta Agent Guide

Guide for AI agents and contributors working in this repository. Follow it exactly; when it conflicts with personal preference, this file wins.

## 1. What Varta is

Varta is a compiler and secure runtime for small TS/JS applications:

- TS source is parsed (SWC) and compiled directly to WebAssembly (`varta-compiler`);
- the Wasm module runs in Wasmtime with capability-checked host functions (`varta-runtime`);
- privileged operations (fs, net, env, time, process) exist **only** as host imports and are denied by default;
- the production artifact is a single native binary with the app payload and capability manifest appended (`varta build --single`).

Varta is deliberately **not** a general-purpose JavaScript runtime. Every accepted language feature must stay small, statically analyzable, and map cleanly to Wasm.

## 2. Repository layout

```text
Cargo.toml               # workspace only; crate metadata lives in crates/*/
crates/
  varta-common/          # config schema, capability model, shared error type
  varta-compiler/        # TS (SWC) -> Wasm emission; no runtime dependency
  varta-runtime/         # Wasmtime host: linker, capability enforcement, host APIs
  varta-cli/             # the `varta` binary; thin orchestration, no domain logic
    templates/           # assets embedded with include_str!
docs/
  architecture.md        # system spec — update when ABI/capabilities change
  development-plan.md    # phases and exit criteria
.github/workflows/       # CI mirrors the commands below
```

Placement rules:

- New crates go in `crates/<name>/` and are added to workspace `members` and, if user-facing, `default-members`.
- Domain logic (parsing, emission, capability matching, host I/O) never lives in `varta-cli`. The CLI reads config, calls crates, prints results.
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
- **No magic numbers or strings.** Named constants, enum-driven tables, or struct-resolved indices — always. The Wasm function-index bug produced by a literal `Call(1)` (fixed after it called the wrong import) is the canonical anti-example of this repository: host-import indices must only ever be resolved through the `HostImports` struct.
- **Error handling.** Library crates expose typed errors derived with `thiserror` and return `Result<T, TypedError>` from public APIs — never `anyhow` or `String` in a public signature. The `varta` binary may use `anyhow` with `.context()` for orchestration-level failures. `String` errors in the compiler are legacy — new code introduces typed error enums and does not extend the `Result<_, String>` pattern. Never `unwrap()`/`expect()` on fallible external input (files, user config, Wasm memory, network); `.unwrap()` is acceptable only in tests and on provably-infallible cases (say which in a comment). Mark important returns `#[must_use]`.
- **No `println!` in library crates** except the runtime's host functions, where printing *is* the app's output channel. Debug output uses `tracing`. Leftover debug prints (e.g. a `DBG_…` line) are release blockers.
- Prefer the standard library and the existing dependency set. Adding a dependency requires justification in the PR description; pin exact versions in `[workspace.dependencies]` and reference them with `.workspace = true`.
- Keep functions focused; extract helpers instead of nesting match arms past three levels. Delete dead code and redundant `let _ =` bindings rather than silencing them.
- Unsafe code is prohibited outside `varta-runtime`'s Wasmtime precompiled-module deserialization, which must stay isolated in one function.

## 5. Compiler rules (`varta-compiler`)

- The compiler emits Wasm directly with `wasm-encoder` and is the only crate allowed to do so.
- Every language feature is a deliberate, documented subset decision. Unsupported constructs fail with a clear `unsupported …` error naming the construct — never silently miscompile.
- Static data (string literals) goes into the data section with a deduplicated offset table; the 64 KiB single-page limit is enforced with a proper error.
- Wasm type/import/local indices are computed, never written as literals. Host imports are declared conditionally on use; any emitted `Call` must go through the resolved `HostImports` field for that import.
- Locals: every index the generated code touches (temporaries for `&&`/`||`, switch discriminants, etc.) must be declared. Off-by-one local declarations produce modules that pass our compile step but trap in Wasmtime — always validate.
- Emitted modules must be verified: a compiler change requires a test that instantiates the module with `wasmtime::Module::new` (see `varta-runtime` tests), not just one that checks byte emission.
- Parse errors must carry `file:line:col` and a human-readable message, reusing the shared span-formatting helper.

## 6. Runtime and security rules (`varta-runtime`, `varta-common`)

Security invariants — violating any of these is a bug regardless of feature intent:

1. **Deny by default.** An operation with no matching capability grant fails before any OS call. Empty patterns deny. Omitted namespaces deny.
2. **Check before effect.** Capability validation happens before opening files, sockets, or reading env — the denied path must be observable and tested.
3. **Match the operation, not the namespace.** A grant for `env.has` must not authorize `env.get`; match the full `namespace.operation` id, not a `starts_with` prefix.
4. **Validate all Wasm-derived input.** Every `ptr`/`len` pair from guest memory is bounds-checked, length-capped, and UTF-8-validated before use. Path inputs additionally reject traversal (`..`) and are canonicalized before glob matching — symlink escapes are in scope.
5. **Manifest is the single source of truth.** Runtime enforcement uses grants parsed from the manifest; nothing is ambient, and no integer from guest memory is ever treated as a capability.

Host-API housekeeping: adding a host function touches, in the same change: `HostImports`/`HostUses` in the compiler, the import type index, the linker function in the runtime, the known-import list in `varta validate`, the `.d.ts` template in `crates/varta-cli/templates/`, and a capability-denial test.

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
- `starts_with("env.")`-style capability matching (see §6.3).
- Debug prints left in host functions.
- New host imports missing from `validate`, the `.d.ts` template, or the denial tests.
- `String`-typed errors in new public APIs.