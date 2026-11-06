# Varta

Varta compiles strictly typed TypeScript to portable WebAssembly ahead of time and
runs it in a thin, capability-checked host: write typed TS, compile it once, and
run the safe program on any supported OS. There is no JavaScript engine on the
target device. System access is denied unless the exact operation and resource
are declared, and an application can ship as a single native executable that
contains portable Wasm and its runtime.

**Status:** experimental research project in active development, not a production
release. The language is a checked TypeScript subset that grows through explicit,
versioned language profiles; it is not general JavaScript, Node or npm
compatibility yet. See the [status summary](docs/development-plan.md#status) and
the [research thesis](docs/development-plan.md#research-thesis).

Current development checkpoint (2026-11-06, `main`): v38 remains
unreleased and in stabilization. The application runtime, official metered
browser host, OPFS KV storage, native/browser network replay and packaged
`varta-shell` are integrated. The pinned uarp-sdk runs natively against the
loopback service and its SDK groups replay in Chrome with native-equal results.
The native renderer includes inline borders/decorations, control defaults,
positioned painting, modal dialogs, system fonts with platform fallback faces and
macOS colour emoji, `scale()` transforms, classic scrollbars and CSS transitions/
translations; the console has headless first-frame and button-state evidence.
The earlier experimental examples (TSX Showcase, UARP console, web-host
prototype) were removed on 2026-11-06; a structured developer examples package
replaces them.

HTTP v2 streams response bodies, takes Web `Request` handlers with streamed
request bodies and runs requests as async Wasmtime tasks with pooled instances,
bounded admission, native TLS 1.3 and HTTP/2; the modern-server stages H1–H4 are
merged, with per-stream write-idle and lifecycle bounds. h2c, HTTP/3, certificate
reload and Linux/sustained-load acceptance remain open. See the
[current status](docs/development-plan.md#status) and
[server stages](docs/development-plan.md#k7-merged-values-approved-2026-11-06).

The latest full gate passes on the audit fixes of 2026-11-06: 3949 Rust tests,
21 ignored, 218 browser Node tests and the reviewed v38 conformance baseline.
These are local macOS implementation checks, with browser Node tests;
real-browser and other-target acceptance retain their separate scope.
See [current verification evidence](docs/verification.md#current-verification-evidence--2026-11-06).

Real-window/native system-input acceptance, sustained live-service sessions,
complete browser/platform matrices and release/security/performance acceptance
remain open. The optional wgpu/Vello UI backend has direct mixed-border color
fields; direct GPU opacity groups and typed guest graphics/3D-library ports remain
[planned work](docs/development-plan.md#gpu-and-game-graphics-future-direction).
There is no guest GPU API or unmodified Three.js compatibility. The
[CORE ledger](docs/development-plan.md#core-completion-ledger) records remaining
scope without treating test counts as a completion percentage.

[Repository](https://github.com/chabanov/varta) · [Contributing](CONTRIBUTING.md) ·
[Security](SECURITY.md) · [Changelog](CHANGELOG.md) · [MIT License](LICENSE.md)

Developed by **Squad AI, Inc.** Architecture and technology by **Pavlo Chabanov**.

## Documentation

| Document | Purpose |
| --- | --- |
| [Functional overview](docs/functional-overview.md) | Complete implemented surface, current limitations, platforms and evidence at the latest checkpoint |
| [Language guide](docs/language-guide.md) | Feature tour with examples, by profile |
| [Architecture contract](docs/architecture.md) | Normative language, ABI, capability and limit contracts; [version matrix](docs/architecture.md#version-matrix) |
| [Embedding contract](docs/embedding.md) | Normative contract for third-party hosts (JavaScript, other Wasm engines): validation, imports, layout, Tool ABI v1, security obligations |
| [Development plan](docs/development-plan.md) | Status, research thesis, roadmap and the [uarp-sdk pilot](docs/development-plan.md#pilot-acceptance-package-uarp-sdk) |
| [Verification](docs/verification.md) | Scoped and full quality gates |
| [Compatibility inventory](conformance/README.md) | Pinned checked conformance cases |
| [History](docs/history.md) | Completed increment logs and superseded text |
| [CLAUDE.md](CLAUDE.md) and [profile rules](docs/profile-rules.md) | Rules for agents and contributors |

## Quickstart

Build from a local checkout of this repository. CI uses Rust **1.98.0** on Linux
and macOS; native compiler/linker tools are required. See
[development setup](CONTRIBUTING.md#development-setup) for cloning and toolchain
installation. Windows and cross-target native packaging are not yet verified.

```sh
cargo install --path crates/varta-launcher --locked
cargo install --path crates/varta-cli --locked
varta init my-app
cd my-app
varta check
varta dev
varta build --single
./dist/app
```

Both installed binaries must share a directory. In this repository, `cargo build`
builds both; use `cargo run --bin varta -- <command>`. No external
TypeScript checker is required: check, dev and build share the same subset/type
and Wasm validation pipeline.

`build --single` uses the sibling `varta-runtime` binary, or an explicit trusted
`--launcher PATH`. It checks the native target, versions and bytecode before
writing outputs, without starting the guest application. It never falls back to
embedding the development CLI. Native packages target the build machine's OS/CPU;
this is not cross-compilation. The packaged program needs no source files,
configuration, development CLI or companion binary at deployment.
Companion negotiation now uses protocol v2 with supported language profiles;
rebuild/install both binaries together. Existing VARTA3 payloads remain readable.

The execution-only interface is:

```sh
./dist/app [--policy host-policy.json]
./dist/app --inspect --json
./dist/app --invoke toolName --params '{"name":"value"}' --json
varta-runtime run dist/app.wasm [--policy host-policy.json]
varta-runtime inspect dist/app --json
varta-runtime invoke dist/app.wasm toolName --params '{}' --json
```

Inspection validates code and reports schemas without starting the application.
Invocation calls only the published tool. An optional `--policy PATH` after the
command narrows package permissions for execution/invocation; it also appears in
inspection output. Standalone runtime inspection reports `manifest`, `module`,
`toolCatalog` (with generated JSON schemas) and `hostCeiling`; development CLI
inspection keeps its existing presentation.

```ts
export function main(): void {
  console.log("hello from varta");
  console.log(varta.time.now());
  console.log(varta.result.status());
}
```

## Language profiles

Every module declares one profile in `varta.language` metadata (default
`checked-i32-utf8-v1`); unknown profiles fail before execution. Each opt-in profile
inherits the one before it. Host and value ABI versions per profile are in the
[version matrix](docs/architecture.md#version-matrix).

<!-- generated:profile-index:begin (names checked by crates/varta-common/tests/documentation.rs) -->
| Profile | Adds |
| --- | --- |
| `checked-i32-utf8-v1` | Default: i32 numbers, UTF-8 strings, closed records, dense arrays, checked JSON |
| `checked-f64-utf8-v1` | Binary64 numbers with typed aggregates and JSON |
| `checked-f64-utf8-v2` | Distinct `undefined` and void expressions |
| `checked-f64-utf16-v1` | UTF-16 strings, including lone surrogates |
| `checked-f64-utf16-v2` | Closed primitive unions and runtime type checks |
| `checked-f64-utf16-v3` | Optional fields and missing primitive reads |
| `checked-f64-utf16-v4` | Record/array reference unions and `Array.isArray` |
| `checked-f64-utf16-v5` | Strict JSON parsing of optional fields and unions |
| `checked-f64-utf16-v6` | Resizable arrays: push/pop, at/slice, includes/indexOf |
| `checked-f64-utf16-v7` | Primitive `throw` and try/catch/finally across calls |
| `checked-f64-utf16-v8` | Nominal Error objects |
| `checked-f64-utf16-v9` | TypeError, RangeError and SyntaxError; `String.repeat` |
| `checked-f64-utf16-v10` | Catchable checked JSON syntax/type failures |
| `checked-f64-utf16-v11` | Typed function values and capture-free arrows |
| `checked-f64-utf16-v12` | Lexical closures, array callbacks, destructuring, loops, labels, computed records |
| `checked-f64-utf16-v13` | Sparse arrays with hole presence |
| `checked-f64-utf16-v14` | Indexed extension, `delete` and writable length |
| `checked-f64-utf16-v15` | Exponentiation, direct Math intrinsics, Number predicates and constants |
| `checked-f64-utf16-v16` | Native Math: trigonometry, logarithms, exponentials |
| `checked-f64-utf16-v17` | `Math.random` from OS entropy |
| `checked-f64-utf16-v18` | ECMAScript Number output |
| `checked-f64-utf16-v19` | Primitive `String(...)` conversion |
| `checked-f64-utf16-v20` | Primitive `Number(...)` conversion and numeric string parsing |
| `checked-f64-utf16-v21` | `Number.toString` in bases 2–36 |
| `checked-f64-utf16-v22` | `toFixed`/`toExponential`/`toPrecision`, `valueOf` |
| `checked-f64-utf16-v23` | `parseInt`/`parseFloat`, `Boolean(...)`, global `isFinite`/`isNaN`, guest truth |
| `checked-f64-utf16-v24` | Truth-based local guards and write-aware refinement |
| `checked-f64-utf16-v25` | `findIndex`/`findLast`/`findLastIndex` |
| `checked-f64-utf16-v26` | `reduceRight` |
| `checked-f64-utf16-v27` | Uncontextual array element-union inference |
| `checked-f64-utf16-v28` | BigInt scalars |
| `checked-f64-utf16-v29` | BigInt in records/arrays/unions, primitive coercion, comparisons |
| `checked-f64-utf16-v30` | Property deletion, `Object.entries`/`is`/`assign`, String own keys |
| `checked-f64-utf16-v31` | Named interface/alias types, type-only imports/exports, `.js` specifiers |
| `checked-f64-utf16-v32` | Pilot batch (integration, unreleased): module constants and `as const`, literal types, classes, re-exports and `export *`, template literals, default exports, interface extends and merging, generics, Error subclasses |
| `checked-f64-utf16-v33` | Suspension frames and sync generators (in development) |
| `checked-f64-utf16-v34` | Pilot batch (in development): index signatures, recursive and wide records, `unknown` and checked `as`, Promise and async/await |
| `checked-f64-utf16-v35` | Pilot batch in development: Web types, mixed records, async methods, frame liveness |
| `checked-f64-utf16-v36` | Pilot batch in development: checked declared fields, function fields, async generators, timers |
| `checked-f64-utf16-v37` | Pilot batch in development: language core, class generics, collections and binary text, RegExp, net.request |
| `checked-f64-utf16-v38` | Batch in development: program and data limits, turn budgets, linked intrinsics, view protocol and TSX, structural subtyping, codegen v2; wave 2: hashed dictionaries (value 18), strict members, Web bytes, virtual file roots, text imports; wave 3: Deno KV storage, hashed `Map`/`Set` |
<!-- generated:profile-index:end -->

## Browser bundle (in development)

Run a checked application locally with `varta dev --target browser`. The command
prints its loopback URL; open it in a browser and use Ctrl-C to stop the server.
`--listen 127.0.0.1:8080` chooses a port. `--policy` and `--decisions` remain runtime
inputs, `--app` enables a non-view application instance, and `--budget-report`
displays usage in the page. Native execution remains the default target.

The dev server rebuilds changed source/configuration/resource inputs and reloads
the page after a successful build. Compile errors appear on the page while the
previous working version remains available. Each verified in-memory version pins
its assets under a content-derived `/__varta/…/` address, with CSP/SRI and a file
allowlist; bare asset paths at the root are unavailable. The server writes no
`dist` output, mounts no native execution roots and serves no source or policy
files. Host policy and decision-file changes require restart. The initial project
must compile successfully before the server starts. Production
`build --target browser` retains its separate policy-free distribution path.
HTTP service contracts require the native target and are rejected by browser
build/dev commands.

For a supported console/Promise or v38 view program (for example an existing project
using `checked-f64-utf16-v38`), build and serve the generated page over loopback:

```bash
varta build --target browser
python3 -m http.server 8000 --bind 127.0.0.1 --directory dist/web
```

The browser directory is `web` beside the configured `out` prefix (`dist/web`
for `out: "dist/app"`). It must not already exist. Checks and unsupported-effect
errors occur before creating output directories. This is a distribution flag;
the original artifact's manifest `target` remains `wasm` or `native` as configured.
The host supports numeric/undefined/UTF-16 logging, Promise/async microtask
checkpoints, bounded rejection snapshots, timers with exact `time.timer` permission
checks, v38 asynchronous HTTP(S) requests with URL/method scopes, metered Wasm
and Worker limits. The browser transport requires CORS and has explicit redirect/
header/encoding limits documented in [architecture](docs/architecture.md#browser-fuel-transform-foundation-in-development).
V38 views now run in long-lived Worker sessions, with checked DOM batches, events,
turn budgets and link/clipboard permission checks. Unpackaged resources and other
unsupported effects remain explicit gaps; broader browser delivery is in development. `--policy` is runtime input and is never embedded by build.

The generated page mounts view programs automatically and uses its own consent
dialog and IndexedDB decisions when the package requests permissions. A trusted embedder can
import `mount` from `varta-host.js`, provide an attached `container`, runtime
`budgetCeiling`/`permissionCeiling` and an optional abort signal, then await
`app.ready` and call `app.close()` on disposal. `app.finished` reports termination;
importing the module alone starts nothing. `run({container})` remains bounded and
renders without delivering UI events. Package PNG images through
`"browser": {"resources": {"icons/red.png": "images/red.png"}}` in the v38 config,
then use `icons/red.png` as the view image's `src`. Paths stay inside the project
without symlinks. Files are bounded, hashed and decoded before execution; arbitrary
image URLs are rejected. Add `"fonts": {"Brand": "fonts/brand.woff2"}` inside the
same `browser` section for TrueType/OpenType/WOFF/WOFF2 files, then use `Brand` in
`font-family`. Fonts load from verified bytes before execution, have private names
per session and release on termination. Quote family names containing spaces.
Pass native-schema `decisions` and an optional synchronous `onAudit` observer to
`run`/`mount` for stored choices, `varta.permissions.query/request` and an owned
`permissions` report. Decisions are runtime inputs, never build inputs. Clipboard
state also respects the browser's exposed permission. For origin-local persistence,
pass `decisionStore: createDecisionStore()` instead of `decisions`; its atomic
IndexedDB updates preserve other applications' choices. `await app.revoke(id)`
withholds new operations in both the page and Worker and saves a denial when a
store is supplied; operations already authorized keep running. Required stored
denials prevent the next startup. For interactive consent, provide
`onPrompt: createPermissionPrompt()` to `mount`, or your own callback returning
`allow`, `deny` or `dismiss`. Required choices happen before execution; optional
requests prompt only in a trusted gesture turn. Waiting consumes no turn budget;
the answer resumes in a separate event. `run` and `mount` without a prompt callback
retain headless policy behavior. Links covered only by the any-HTTPS form require
fresh consent for each full address; that choice is never stored. The Worker pauses
between turns while the dialog is open. Broader platform acceptance remains open.

`mount()` also runs a non-view application without a container. The guest can use
`varta.events.listen/close`; the trusted host calls
`await app.deliver({source: 'ui', record: 'text'})`. Deliveries are queued for
separate turns and report `queued`, `coalesced` or `dropped`. Records are copied at
the call boundary and bounded as UTF-8; closing/reopening a name cannot retarget
old deliveries. `gesture` and `coalesce` default to false; only a trusted host
should mark a record as a user gesture. A non-view application finishes when its
listeners, timers and active operations no longer keep it alive.

## Commands

- `init <name>`: create a project without overwriting an existing directory
- `check [--config] [--json]`: validate language, types, config and generated Wasm
- `dev [--config] [--policy] [--target native|browser]`: execute natively or serve a local browser snapshot; `--listen` is browser-only
- `build [--config] [--single | --target wasm|native|browser] [--launcher PATH]`: produce Wasm, a native executable or the checked browser bundle; `--launcher` requires `--single` or `--target native`
- `run <path> [--policy]`: execute portable Wasm or a packaged artifact
- `invoke <path> <name> [--params JSON] [--json] [--policy]`: call one published tool without running `main`
- `inspect [path] [--json] [--policy]`: show validated code and full capability metadata
- `validate [path] [--json] [--policy]`: check code, ABI, manifest and import declarations

Failures return nonzero exit codes. `validate` adds policy review: every privileged
import needs a declaration, even though omitted permissions remain safe to execute
because the runtime denies them.

Native packaging now uses a fixed VARTA3 footer and portable Wasm. Legacy VARTA1/
VARTA2 and native AOT payloads are rejected; rebuild old artifacts. This avoids
accepting untrusted machine code through Wasmtime deserialization.
## Repository scope

This repository develops the Varta engine and its development tools. Application
entities, business rules and application-specific permissions belong to separate
TS/JS projects built on the engine's public APIs.

| Directory | Responsibility |
| --- | --- |
| `crates/varta-bigint` | Budgeted integer arithmetic and canonical binary transport for checked BigInt scalars |
| `crates/varta-common` | Engine contracts, ABI, permissions, diagnostics and shared values |
| `crates/varta-compiler` | Checked TS subset and portable Wasm generation |
| `crates/varta-runtime` | Validation, inspection, bounded execution and JSON tool invocation |
| `crates/varta-artifact` | Portable Wasm/manifest and native container I/O, without VM or compiler |
| `crates/varta-launcher` | The execution-only `varta-runtime` binary distributed with applications |
| `crates/varta-project` | Development tooling: project configuration/source loading, packaging and policy files |
| `crates/varta-cli` | Commands and output, calling the project tooling layer |
| `crates/varta-conformance` | Developer-only pinned compatibility inventory and isolated case execution |
| `docs`, `.github` | Implemented specifications, core roadmap and verification |
| `target` | Ignored, generated Rust build output |

Engine and distribution crates cannot depend on project tooling or CLI, including
through tests. Runtime distribution has no TS compiler, SWC or development command
parser in its production dependencies. Build these components independently:

```sh
cargo check -p varta-common -p varta-compiler -p varta-runtime -p varta-artifact --lib --locked
cargo build -p varta-launcher --bin varta-runtime --locked
cargo test -p varta-runtime --test public_api --locked
```

The [public API test](crates/varta-runtime/tests/public_api.rs) compiles in-memory
TS modules, inspects the resulting bytecode and invokes a published tool using
only engine APIs. It needs no project directory, configuration or CLI process.
`VartaConfig` and `CapabilitySpec` are tooling types exported by `varta-project`;
their JSON format is unchanged. Backend-independent bytecode metadata is returned
by `VartaRuntime::inspect_module` as `WasmInspection`.

Reference applications remain deferred until the generic core mechanisms exist.
Embedded TS snippets in tests verify the engine; they are not application modules.
## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, scope and contribution guidelines.

The workspace separates common schemas/ABI, compiler, runtime, artifact I/O,
runtime launcher, project services and CLI. Workspace code forbids unsafe. Use scoped checks during development and one
full verification for the completed code batch:

```sh
python3 scripts/verify.py local varta-bigint
python3 scripts/verify.py local varta-runtime --filter array_inference
python3 scripts/verify.py feature array_inference
python3 scripts/verify.py full
```

See the [verification map](docs/verification.md) for scope selection, boundary
coverage and evidence. The full workspace run already includes CLI tests.
## Checked compatibility inventory

The [checked core suite](conformance/README.md) pins ECMAScript 2025, profile v30
and explicit source hashes. It reports successful cases, known unsupported cases,
regressions, infrastructure failures and feature rows without executable cases.
Its current 78 cases yield 67 passes and 11 known gaps; 7 feature rows remain
untested. The inventory covers checked TypeScript `main` programs and does not
establish full ECMAScript/Test262 or production readiness.

```sh
cargo run -p varta-conformance --locked -- --json
cargo run -p varta-conformance --locked -- --require-supported
```

The strict command currently exits 1 for the remaining gaps/omissions. CI checks
the reviewed baseline and records its JSON report in the job log. Each case uses
a fresh process, default runtime limits and empty host authority. The tool is a
development consumer of public compiler/runtime APIs and is excluded from native
runtime distribution. See the [CORE ledger](docs/development-plan.md#core-completion-ledger)
for the broader acceptance work.
