# Varta — Development Plan & Tech Stack

## 1. Strategic direction

Varta will be built as a small, secure, TS/JS-to-Wasm runtime with optional native binary packaging.

Primary scope for MVP:

- single TS entry file;
- capability-based host APIs;
- Wasm compilation;
- dev execution in Wasm VM;
- production packaging into one standalone artifact.

Non-goals for MVP:

- full Node.js compatibility;
- arbitrary npm native addons;
- plugin system;
- multi-language compilation.

---

## 2. Recommended tech stack

### 2.1 Core implementation language

**Rust**

Reasons:

- good fit for compiler/runtime tooling;
- strong memory safety;
- close ecosystem to Bun/Deno-style runtime development;
- excellent Wasm tooling and native packaging;
- safe host-side implementation.

### 2.2 CLI

- `clap` for argument parsing;
- subcommands: `init`, `check`, `dev`, `build`, `test`, `inspect`, `run`.

### 2.3 TS/JS parsing

MVP:

- `swc_parser` / `swc_core` for fast TS/JS parsing and transformation.

Type checking:

- shell out to `tsc --noEmit` initially;
- later evaluate Rust-based TypeScript checker integration.

### 2.4 IR / Wasm emission

- custom internal IR;
- `wasm-encoder` or `walrus` for Wasm module generation;
- `wasmparser` for validation and inspection.

### 2.5 Wasm runtime (development)

- Wasmtime.

Reasons:

- mature sandbox;
- capability-oriented runtime;
- AOT compilation support;
- async execution APIs;
- fuel/resource limits possible.

### 2.6 Native binary packaging (production)

MVP approach:

- compile application to Wasm;
- use Wasmtime AOT compilation;
- package Wasm + AOT artifact into a Rust launcher binary;
- embed manifest and assets with `include_bytes!` / runtime asset loader.

This keeps one behavioral contract between dev and prod while producing a deployable binary.

### 2.7 Host APIs

Implemented in Rust:

- `varta.log`
- `varta.env`
- `varta.time`
- `varta.fs`
- `varta.net`
- `varta.http`

All host APIs enforce capabilities before performing OS-level operations.

### 2.8 Async runtime

- Tokio for async host I/O;
- Wasmtime async API where needed;
- one documented async execution contract.

### 2.9 Observability

- `tracing` for structured logs/traces;
- source maps for diagnostics;
- capability audit metadata in debug builds.

### 2.10 Testing

- Rust unit/integration tests in `crates/*/tests`;
- spec-style tests for CLI behavior;
- Wasm integration tests for runtime behavior;
- capability-denial regression tests.

### 2.11 Packaging and repo layout

Recommended:

```text
varta/
  Cargo.toml
  crates/
    varta-cli/
    varta-compiler/
    varta-wasm/
    varta-runtime/
    varta-host/
  docs/
  examples/
```

---

## 3. Development phases

### Phase 0 — Specification freeze

Deliverables:

- `varta.config` schema;
- Varta ABI v0;
- capability manifest schema;
- memory ownership rules;
- async contract;
- canonical capability syntax;
- strict and compat policy profiles;
- `docs/architecture.md` aligned with all decisions.

Exit criteria:

- no contradictory examples;
- architecture reviewed and stable enough for MVP implementation.

---

### Phase 1 — Single-file compiler MVP

Deliverables:

- Rust CLI scaffold;
- `varta init`;
- `varta check`;
- Rust-based TS/JS parser integration;
- type-check invocation;
- simple IR;
- Wasm emission for a small TS subset;
- `varta dev` runs Wasm in Wasmtime;
- host APIs: `log`, `env`, `time`;
- basic capability enforcement.

Test cases:

- function declarations;
- variables;
- strings and numbers;
- if/while/for;
- async functions with `await`;
- basic `fetch`-like typed host call.

Exit criteria:

- one `src/main.ts` compiles and runs in Wasm;
- denied capability test fails as expected.

---

### Phase 2 — Core host platform

Deliverables:

- filesystem host API;
- network HTTP client;
- HTTP server;
- environment/clock/crypto basics;
- event loop and promise semantics;
- source maps;
- structured errors.

Test cases:

- read allowed file;
- deny disallowed path;
- fetch allowed URL;
- deny disallowed host;
- listen on allowed port;
- deny unknown port.

Exit criteria:

- end-to-end hello-world-style app works in dev mode.

---

### Phase 3 — Native single binary

Deliverables:

- AOT compilation pipeline;
- embedded capability manifest;
- embedded asset manifest;
- `varta build` produces platform binary;
- `varta run` or direct executable execution;
- debug metadata toggle.

Test cases:

- build produces one executable;
- executable runs without external runtime installation;
- behavior matches dev Wasm execution.

Exit criteria:

- users can run a single binary artifact successfully.

---

### Phase 4 — Dependency and compatibility layer

Deliverables:

- package manifest and lockfile;
- Varta-native packages;
- tree-shaking;
- basic npm pure-package compatibility;
- CommonJS/ESM subset support;
- dependency capability inheritance rules.

Exit criteria:

- selected pure JS/TS packages work;
- native addon packages are clearly rejected or isolated.

---

### Phase 5 — Production hardening

Deliverables:

- security audit;
- policy profiles enforcement;
- resource limits (memory/time/fuel);
- binary size optimization;
- reproducible builds where possible;
- diagnostics and inspect tooling;
- crash reporting improvements.

Exit criteria:

- documented security guarantees;
- stable MVP release criteria met.

---

## 4. Architecture deliverables tracked to plan

| Deliverable | Phase | Status in architecture.md |
|---|---:|---|
| Capability manifest schema | 0 | Added |
| ABI v0 memory ownership | 0/1 | Added |
| Async contract | 0/2 | Added |
| Canonical capability syntax | 0 | Added |
| Policy profiles | 0 | Added |
| AOT strategy | 3 | Added |
| npm compatibility boundary | 4 | Added |

---

## 5. Immediate implementation order

1. Rust workspace scaffold.
2. CLI skeleton.
3. Wasmtime dev runner.
4. TS parser integration.
5. Capability config parser and manifest validator.
6. Minimal Wasm emitter.
7. `varta dev` end-to-end.
8. ABI v0 host imports.
9. Capability denial tests.
10. AOT/native packaging.

---

## 6. Key risks

- TS type checking without relying on heavy external tooling may be harder than expected.
- Wasm async semantics can diverge between dev and prod if not specified early.
- Single native binary size may be large due to embedded Wasm engine.
- npm compatibility can expand scope significantly.
- Capability policy must remain both secure and ergonomic.

---

## 7. Recommended Definition of Done for MVP

MVP is complete when:

- one TS file can be parsed and type-checked;
- it compiles to Wasm;
- it runs in `varta dev`;
- it builds into one standalone binary;
- filesystem, HTTP, and network calls require explicit capabilities;
- denied capabilities produce deterministic typed errors;
- at least one end-to-end example and its tests pass.