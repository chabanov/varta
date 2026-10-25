# Varta — Architecture

## 1. Vision

Varta is a typed and secure engine for TS/JS applications.

The primary promise:

- developers write TS/JS;
- source is compiled into WebAssembly;
- privileged operations are exposed only through capability-based host APIs;
- final release artifacts can be **single native binaries** containing application code and the required runtime shim.

Varta is not only a JavaScript runtime. It is a **compile-time + runtime system** for small, secure, deployable applications.

---

## 2. Design goals

1. **Type safety by default**
   - TS-first workflow.
   - Strict type checking in CI and local builds.
   - Host APIs must ship with generated types.

2. **Security by default**
   - Deny-by-default capabilities.
   - No ambient filesystem/network/process access.
   - Explicit import/capability declarations.

3. **Portable deployment**
   - Development can target Wasm.
   - Production targets a single native binary per platform.

4. **Small attack surface**
   - Avoid `eval`, dynamic code injection, and unsafe host bridges.
   - Reduce runtime dependencies.

5. **Deterministic builds**
   - Reproducible compilation where practical.
   - Lockfile and dependency resolution must be part of architecture.

---

## 3. High-level model

```text
TS/JS source
    ↓
parser + type checker
    ↓
intermediate representation
    ↓
Wasm compiler
    ↓
Wasm module
    ↓
AOT linker / native compiler
    ↓
single native binary
```

At runtime:

```text
native binary
  ├── application code
  ├── Varta runtime shim
  ├── embedded asset manifest
  └── host capability configuration
```

The runtime executes application logic and calls host functions only through explicitly granted interfaces.

---

## 4. Source and project model

Recommended project layout:

```text
varta.config.ts
src/
  main.ts
  lib/
tests/
varta.lock
```

Core concepts:

- `varta.config.ts`: project configuration, targets, capabilities, dependencies.
- `src/main.ts`: entry point.
- strict TS configuration is the default.
- `any`, implicit unsafe casts, and untyped host boundaries should produce warnings or errors.
- ambient global APIs should be intentionally limited.

---

## 5. Compilation pipeline

### Stage 1: Parsing

- Parse TS/JS into AST.
- Preserve source maps and diagnostic spans.
- Support TS syntax and typed JS.

### Stage 2: Type checking

- Type check against project TS config and Varta host API types.
- Validate capability usage.
- Reject unsupported dynamic constructs where policy requires.

### Stage 3: Lowering

- Lower typed TS/JS into an internal IR.
- Normalize modules, imports, exports, classes, closures, and control flow.
- Prepare async lowering.

### Stage 4: Wasm emission

- Emit a Wasm module targeting a stable Varta ABI.
- Functions, strings, arrays, records, and host handles are represented using linear memory conventions.
- Keep source-to-binary debugging metadata.

### Stage 5: Capability binding

- Attach requested capabilities as module imports/exports metadata.
- Validate that source-declared capabilities match configuration.

### Stage 6: AOT native compilation

- Compile Wasm into native object code for target platform.
- Link with Varta runtime shim and host adapter libraries.
- Emit one executable.

### Stage 7: Packaging

- Embed assets and manifests.
- Add version, source map, and dependency provenance metadata.

---

## 6. Runtime architecture

### 6.1 Execution engine

Development target:

- Wasm VM for fast iteration and sandbox validation.

Production target:

- AOT-compiled native binary.

Both targets must expose the same Varta ABI so behavior remains consistent.

### 6.2 Memory model

- Each application instance owns linear memory.
- Strings are represented as offset + length.
- Buffers are represented as offset + length.
- Ownership must be explicit in ABI docs.
- Allocators should favor arenas/regions for request-scoped values.
- Avoid hidden GC dependency unless consciously chosen.

### 6.2.1 ABI v0 memory ownership

For ABI v0, strings and buffers use linear-memory pointers plus lengths.

Rules:

- host functions must document ownership for every pointer-sized argument;
- values passed from app to host may be borrowed only during the call unless explicitly retained;
- host-allocated values returned to app must have a documented free rule;
- app-allocated values shared with host must not be mutated while host is processing them;
- path and URL inputs are immutable from host perspective;
- BLOB/buffers must use explicit length, never sentinel termination;
- ABI data structures must be versioned and size-checked.

Recommended primitive shape:

```ts
type VartaString = {
  ptr: number;
  len: number;
};
```

GC strategy defaults to **explicit linear-memory management**. Any future GC must be introduced deliberately and declared in the runtime contract.

### 6.3 Event loop

The event loop is host-owned.

Responsibilities:

- timers;
- async I/O completion;
- promise continuation scheduling;
- cancellation tokens;
- graceful shutdown.

Async TS/JS code is lowered into continuations or host-mediated async handles.

### 6.3.1 Async execution contract

Async behavior must be identical across Wasm development mode and native production mode.

Rules:

- microtasks are flushed before host I/O completion events;
- `Promise` resolution order is deterministic for a given runtime;
- async host calls return host handles, not raw OS resources;
- cancellation must propagate through async operation chains;
- host handles have explicit lifetimes;
- timeout behavior is defined per host API;
- async operations are cancel-safe by default where practical.

Development and production runtimes must share one documented async semantics specification.

### 6.4 Error model

- ABI-level errors use result codes or typed error objects.
- TS-level exceptions are represented through defined runtime structures.
- Host call failures must be explicit in types, where practical.

---

## 7. Host API and capabilities

### 7.1 API namespaces

Examples:

- `varta.fs`
- `varta.net`
- `varta.http`
- `varta.env`
- `varta.time`
- `varta.crypto`
- `varta.process`
- `varta.log`

### 7.2 Capability model (canonical developer model)

Canonical declaration style is a scoped permission manifest:

```ts
export const capabilities = {
  fs: {
    read: ["/data/**"],
  },
  net: {
    fetch: ["https://api.example.com/**"],
  },
  http: {
    listen: [8080],
  },
} as const;
```

At the application boundary, code uses a scoped runtime instead of repeating permission tokens:

```ts
const app = createApp(capabilities);

const message = await app.fs.readFileText("/data/message.txt");
```

The explicit token-passing form shown in earlier low-level ABI discussions is reserved for:

- internal implementation;
- advanced tooling;
- generated host bindings.

It should not be the primary developer-facing API.

Capability types must be branded/opaque so normal application code cannot forge or cast them into valid permissions.

### 7.3 Canonical capability declaration rules

Rules:

- permissions are grouped by namespace (`fs`, `net`, `http`, `env`, etc.);
- each operation maps to an array of allowlist patterns;
- empty arrays mean deny;
- omitted namespaces/operations mean deny;
- overly broad patterns can be rejected by CI policy;
- all capabilities remain explicit and serializable into the manifest.

The compiler and runtime must both validate capabilities.
### 7.4 Host function ABI

Host APIs are Wasm imports.

Representations:

- scalar arguments: `i32`, `i64`, `f32`, `f64`;
- strings/buffers: linear memory pointers + lengths;
- capabilities: opaque handles.

Rules:

- host owns capability validation;
- app cannot cast arbitrary integers into capabilities;
- allocation ownership must be documented per function.

---

## 7.5 Developer-facing capability scoping

Rather than passing an access token into every API call, Varta creates a **scoped runtime**:

```ts
const app = createApp(capabilities);

const message = await app.fs.readFileText("/data/message.txt");
```

Design properties:

- permissions are declared once;
- application code remains readable;
- the scoped object only exposes operations backed by the manifest;
- security remains explicit at the boundary.

This is an ergonomics layer over the strict permission model, not a weakening of it.

## 7.6 Capability manifest

Capabilities are serialized into both development metadata and the production binary:

```json
{
  "schemaVersion": 1,
  "capabilities": [
    { "id": "fs.read", "patterns": ["/data/**"] },
    { "id": "net.fetch", "patterns": ["https://api.example.com/**"] },
    { "id": "http.listen", "ports": [8080] }
  ]
}
```

The source-level grouped object is a developer-friendly shorthand. The canonical serialized manifest uses the normalized array form above.

The manifest is used for:

- security review;
- `varta inspect`;
- CI policy checks;
- audit/provenance reporting;
- runtime enforcement initialization.

## 7.7 Validation layers

Capability enforcement happens at two levels.

### Compile-time

- validate capability object structure;
- validate API/capability compatibility;
- validate statically known paths, hosts, ports, globs, and supported URL schemes;
- reject clearly invalid or overly broad patterns where policy requires.

### Runtime

- normalize and validate actual paths, URLs, and ports;
- enforce filesystem path resolution and symlink boundaries;
- enforce host/path allowlists;
- fail with a typed error before performing the operation.

### Network transport note (MVP)

- The development and runtime host HTTP client supports `http://` only.
- Capability patterns may still be recorded as `https://...` for review and
  auditing, but the runtime does not perform TLS transport yet.
- `varta.net.fetch` and `varta.net.get` reject unsupported URL schemes with an
  explicit capability/transport error before any network I/O.
- CI and security review should currently approve net capability patterns only
  for `http://`.
- TLS (`https://`) is scheduled for Phase 2 (core runtime) and may also be
  gated behind a compat/policy profile.

### Example denial

```ts
await app.fs.readFileText("/etc/passwd");
```

Expected result:

```text
VartaCapabilityError: fs.read denied for "/etc/passwd"
required: fs.read:/data/**
```

## 7.8 Least privilege in code

- No ambient global APIs expose system access.
- Dependencies receive only the scoped runtime object needed for their task.
- Capabilities are opaque handles, not forgeable integers or strings.
- Broad permissions must be explicitly justified in config and reviewable in the manifest.

## 7.9 Capability expiration and revocation

Future design may support:

- build-time capability pinning;
- runtime revocation for temporary grants;
- environment-specific manifests;
- signed capability manifests for deployed binaries.

For MVP, capabilities are build-time declared and runtime enforced without mutation.

---

## 8. Type safety architecture

### 8.1 Typed host APIs

Every host API ships with generated `.d.ts` and possibly runtime schemas.

Example:

```ts
export function readFile(
  path: string,
  capability: FsReadCapability
): Promise<Uint8Array>;
```

### 8.2 Boundary validation

Compile-time checks are not enough for external data.

Runtime validation is required for:

- host responses crossing the ABI boundary if serialized;
- configuration files;
- CLI arguments;
- network payload parsing;
- deserialized storage.

### 8.3 Safety restrictions

Default restrictions may include:

- no `eval`;
- no dynamic imports beyond declared modules;
- no arbitrary FFI;
- no unsafe host handle casting;
- no direct memory manipulation outside typed wrappers.

---

## 8.4 AOT strategy for single native binaries

Production AOT output is not a blind conversion from Wasm to native code. It is a **packaged runtime artifact**.

The production binary contains:

- AOT-compiled application Wasm;
- the Varta Wasm execution runtime;
- host adapter layer;
- capability manifest;
- embedded asset manifest and provenance.

Implications:

- binary size includes required runtime shim;
- platform-specific artifacts are expected;
- reproducibility depends on toolchain pinning;
- debug metadata may be stripped independently.

For MVP, Wasm-first AOT is preferred because it preserves the development/production behavioral contract.

---

## 8.5 Security policy profiles

Varta should support explicit policy profiles:

### `strict` (default)

- deny ambient APIs;
- strict host boundaries;
- no direct FFI;
- no runtime code generation;
- minimal default runtime surface.

### `compat`

- permits a broader Node-like compatibility layer where needed;
- allows declared dynamic module loading;
- enables more CommonJS/ESM interop shims.

This separates **secure defaults** from **ecosystem compatibility** rather than weakening the default model.

---

## 9. Module and dependency system

### 9.1 Varta-native packages

Preferred package format:

- typed TS source or compiled Wasm;
- manifest with capabilities;
- lockfile-compatible dependency graph.

### 9.2 npm compatibility boundary

npm support should be treated as a **best-effort compatibility layer**, not full Node.js compatibility.

Supported scope initially:

- pure JS/TS packages;
- statically analyzable ESM/CJS modules;
- packages using a defined subset of Node-like host APIs.

Unsupported or isolated scope:

- native `.node` addons unless rebuilt for Varta/Wasm;
- arbitrary dynamic code generation;
- undocumented global mutations;
- packages requiring full Node runtime internals.

Strategies:

1. Transpile pure JS/TS npm packages into Varta Wasm modules.
2. Provide a documented Node-like host API subset.
3. Isolate or reject packages that require native addons unless explicitly rebuilt.

Compatibility limitations must be documented per package/runtime version.

### 9.3 Tree-shaking

Single binary output requires aggressive dead-code elimination:

- unused modules removed;
- only used host APIs retained;
- capability graph influences reachable code.

---

## 10. Security architecture

### 10.1 Wasm sandbox

- application cannot access host memory outside its linear memory;
- imports are explicitly defined;
- no ambient system calls.

### 10.2 Capability grants

Capabilities are assigned at build time and optionally restricted at runtime by manifest.

Examples:

- file path globs;
- allowed network hosts;
- environment variable allowlists;
- time/randomness access levels.

### 10.3 Supply chain

- dependency lockfile;
- package hashes;
- reproducible builds;
- provenance metadata embedded into binary.

### 10.4 Dynamic execution

Avoid runtime code generation unless explicitly sandboxed and audited.

---

## 11. Observability

Single binary should support:

- structured logs;
- metrics export;
- trace spans;
- crash diagnostics;
- source maps embedded optionally.

Debug builds may include:

- Wasm stack traces;
- type metadata;
- capability audit reports.

Release builds may strip non-essential metadata.

---

## 12. Tooling

CLI surface:

```text
varta init
varta dev
varta check
varta test
varta build
varta inspect
varta run
```

Responsibilities:

- `init`: scaffold project and config;
- `check`: parse and type check;
- `dev`: compile to Wasm and run in dev runtime;
- `test`: run tests in sandbox;
- `build`: produce production artifact;
- `inspect`: show capabilities, dependency graph, binary metadata;
- `run`: execute produced binary.

---

## 13. Single binary format

Production binary contains:

- AOT-compiled application code;
- runtime shim;
- host adapter;
- embedded asset manifest;
- dependency provenance;
- optional debug metadata;
- capability manifest.

Benefits:

- no runtime installation;
- easier deployment;
- deterministic runtime environment;
- reduced tampering surface.

Trade-offs:

- larger binaries;
- platform-specific artifacts;
- less dynamic plugin loading by default.

---

## 14. Roadmap

### Phase 0: Specification

- define ABI;
- define capability model;
- define config format;
- define module manifest.

### Phase 1: MVP

- TS parser/type checker integration;
- Wasm compilation;
- basic host APIs: log, time, env;
- `varta dev` and `varta build --wasm`.

### Phase 2: Core runtime

- fs/net/http APIs;
- HTTP and limited HTTPS/TLS transport scope decision;
- async event loop;
- memory management;
- error handling;
- source maps.

### Phase 3: Native binary

- AOT compilation;
- linker integration;
- single binary output;
- packaging metadata.

### Phase 4: Compatibility

- JS support parity;
- selected npm package compatibility;
- Node-like API subset.

### Phase 5: Production hardening

- security audit;
- performance optimization;
- binary size reduction;
- reproducible builds;
- diagnostics.

---

## 15. Open questions

1. Which TS/JS compiler frontend should be primary?
2. Should production use Wasm-first AOT or direct LLVM lowering?
3. What GC strategy is acceptable, if any?
4. How should async lowering be represented?
5. Which npm compatibility guarantees are realistic?
6. How to balance binary size with debuggability?
7. How should plugin/extensibility work while remaining secure?
8. Which TLS backend and trust assumptions should Varta use for HTTPS?

---

## 16. Immediate next work

1. Create `varta.config` schema.
2. Define Varta ABI v0, including memory ownership rules.
3. Define capability manifest schema v0.
4. Define async execution contract.
5. Define canonical capability declaration syntax.
6. Define AOT strategy and security policy profiles.
7. Define capability type primitives.
8. Implement minimal compiler: TS → Wasm.
9. Implement minimal host runtime: log, env, time.
10. Implement `varta build` for a single-file TS app.
11. Add integration test for capability denial.