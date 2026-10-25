# Varta

Varta compiles a small, checked TypeScript subset to WebAssembly and runs it in a
capability-checked Wasmtime host. System access is denied unless the exact
operation and resource are declared. Applications can be distributed as a single
native executable containing portable Wasm and its runtime.

Status: MVP / active development. The [architecture contract](docs/architecture.md)
describes the implemented language, trust boundaries, ABI and limits. The
[development plan](docs/development-plan.md) tracks audit fixes and next milestones.

## Platform direction

Varta targets typed, contract-driven TS/JS applications for desktop, server and
robot environments. The intended architecture compiles supported source into
portable Wasm and exposes explicitly selected application operations to external
AI agents through MCP. UI, API and agent callers share the application's command
and query layer, business rules and state; connecting another agent does not
create another application state.

Primitive Tool Contract v1, offline discovery and checked JSON invocation are
implemented. A broader Application Contract and a long-lived application
service, MCP bridges, package signatures and desktop/database/hardware adapters
remain planned. The [delivery sequence](docs/development-plan.md#delivery-sequence)
continues with invocation/shared state, artifact provenance and a local MCP bridge.
Language expansion follows those application needs.

Portable Wasm requires a compatible host; native launchers target a supported
OS/CPU. Source-free compilation does not guarantee that business logic cannot be
analyzed or modified. Authenticity requires verification by a trusted loader,
while concrete actions always remain subject to host and caller permissions.

## Quickstart

```sh
cargo install --path crates/varta-cli --locked
varta init my-app
cd my-app
varta check
varta dev
varta build --single
./dist/app
```

For development in this repository, use `cargo run -- <command>`. No external
TypeScript checker is required: check, dev and build share the same subset/type
and Wasm validation pipeline.

```ts
export function main(): void {
  console.log("hello from varta");
  console.log(varta.time.now());
  console.log(varta.result.status());
}
```

## Public tool contracts

Publish selected exported functions with a compile-time entry-module declaration:

```ts
export function previewTitle(nextTitle: string): string {
  return nextTitle + "!";
}
export function main(): void {}

varta.tool({
  name: "document.previewTitle",
  description: "Preview a proposed document title"
}, previewTitle);
```

Build normally, then read the catalog without running the application:

```sh
varta build --single
varta inspect dist/app.wasm --json
varta inspect dist/app --json
```

`toolCatalog` includes generated input/output schemas, required system operations
and limits. Metadata lives inside Wasm and survives native packaging and removal
of source files. Ordinary exports do not become tools. Imported functions may be
published through an explicit entry-module export. Parameters support `number`
(signed i32), `boolean` and `string`; results also support `void`. Output schemas
wrap the result as `{ "value": ... }`.

Catalog versions, sizes, handler signatures and transitive system requirements are
validated on check/build/dev/run/invoke/inspect/validate. Publishing a tool grants no
permissions. Shared document state and MCP remain planned; this example declares
a stateless tool. See the
[tool contract](docs/architecture.md#tool-contract-v1-and-offline-discovery).

## Invoke a tool

```sh
varta invoke dist/app.wasm document.previewTitle --params '{"nextTitle":"Draft"}' --json
varta invoke dist/app document.previewTitle --params '{"nextTitle":"Draft"}' --json
```

Both commands return one JSON record:

```json
{"status":"ok","result":{"value":"Draft!"},"logs":[]}
```

Invocation runs only the published handler, with a fresh instance and the existing
memory, fuel, time and host-call limits. It does not call `main`. All parameters
are checked before guest execution: one object, exact required fields, no duplicate
or unknown fields, no coercion, signed i32 integers, booleans and UTF-8 strings.
The input is limited to 64 KiB, including JSON whitespace/escaping. Integer input
uses plain integer notation; decimal/exponent and negative-zero forms are rejected.
The encoded `{ "value": ... }` result is limited to 1 MiB; logs have their own
existing output limit. Strings are copied into host-owned results.

`--policy host-policy.json` also restricts invocation. Concrete system operations
still require both package and host grants. A handler can recover from denied
operations through `VartaResult`; that does not grant access. Boundary failures
return exit code 1 and, with `--json`, a stable `code` such as `invalid_arguments`,
`unknown_tool`, `invalid_result` or `execution_failed`. File/configuration errors
before dispatch keep the ordinary CLI error handling. Without `--json`, stdout
contains only `{ "value": ... }` and application logs go to stderr.

This local interface does not provide remote authentication, persistent document
state or rollback of host effects. Failed calls are not retried automatically.

## Diagnostics

`check`, `build` and `dev` report the actual source file and the failing source
range, including errors inside nested expressions:

```text
/path/to/app/src/main.ts:2:19: type mismatch for x: expected string, found number
  |   let x: string = 7;
  |                   ^
```

`varta check --json` returns structured `diagnostics` with category, filename,
line/column range, UTF-8 byte offsets and a bounded excerpt. Unicode, CRLF and BOM
positions are supported. See the [diagnostic contract](docs/architecture.md#source-diagnostics)
for coordinate conventions and public compiler APIs.

## Local modules

Split functions across files using explicit relative `.ts` paths:

```ts
// src/math.ts
export function twice(n: number): number { return n * 2; }

// src/main.ts
import { twice as double } from "./math.ts";
export function main(): void { console.log(double(21)); }
```

Named function imports/exports and local export aliases are supported. Private
functions keep their own file scope. The entry file declares `main(): void`;
dependencies are linked into the Wasm and native artifacts at build time.
`varta check --json` includes the resolved dependency graph in `modules`.

Dependencies must stay within the configuration directory; symlinks and cycles
are rejected. Default/namespace imports, export-from, dynamic imports, npm and
URL imports are not supported. See the [module contract](docs/architecture.md#local-modules)
for path rules and graph limits.

## Project and permissions

```text
varta.config.json
src/main.ts
varta.d.ts
dist/app.wasm
dist/app.manifest.json
dist/app                 # build --single
```

Example `varta.config.json`:

```json
{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": {
    "time": { "now": true },
    "env": { "get": ["APP_NAME"] },
    "fs": { "read": ["/data/**"] },
    "net": { "fetch": ["http://example.com/public/**"] }
  }
}
```

Omitted grants, empty lists and false deny access. Permissions for read/print,
get/has and fetch/get are independent. Paths retain directory boundaries and are
opened through pinned descriptors with symlinks rejected. Network authority is
exact; URLs are validated before I/O, with redirects and ambient proxies disabled.
Only the strict profile and HTTP transport are implemented.

`inspect --json` shows full permission scopes. Without a host policy the CLI uses
the package's declared grants. To restrict an untrusted package, supply a separate
host policy in the manifest format:

```json
{
  "capabilities": [
    { "id": "env.get", "patterns": ["APP_NAME"] }
  ]
}
```

```sh
varta run dist/app.wasm --policy host-policy.json
varta inspect dist/app.wasm --policy host-policy.json --json
# A newly built native launcher accepts the same option:
./dist/app --policy host-policy.json
```

An operation must satisfy both the package request and the host policy. Empty
host capabilities deny everything; malformed or missing policy files fail before
execution. The host cannot add unrequested rights, and the package cannot override
the host ceiling. Inspection shows both filters and distinguishes unconditional
denials from resource checks performed at runtime. `--policy` also works with
`dev` and `validate`; it is never saved into a build. Native executables themselves
run with the launching user's OS authority.

## Language and execution

The subset includes local named-function modules, typed primitive functions,
lexical let/const bindings, integer arithmetic, boolean operations, conditions,
loops and switch. Numbers use signed i32 Wasm semantics. Strings work as variables,
parameters, explicit function results and dynamic host inputs:

```ts
function read(name: string): string { return varta.env.get(name); }
export function main(): void {
  const name = read("APP_NAME");
  const other = read("OTHER_NAME");
  console.log(name); // Still holds the first result.
  console.log(other);
}
```

Host results are copied into immutable, bounded storage. Unreachable strings are
collected and their storage is reused during execution; aliases and saved results
remain valid. Every read still needs the corresponding capability. Strings support `+`,
`==`, `!=`, `===`, `!==`, `<`, `<=`, `>` and `>=`:

```ts
export function main(): void {
  const message = "Привіт" + " 🦀";
  console.log(message == "Привіт 🦀"); // 1
  console.log("apple" < "banana");   // 1
}
```

Comparisons use content and UTF-16 code-unit order, without locale sorting or
normalization. Both operands must be strings; numbers are not converted to text.
One string is limited to 1 MiB of UTF-8 bytes. Allocations share the memory/fuel
budgets. Unused expression values, overwritten bindings and out-of-scope locals
can be reclaimed. Live values stay in place; committed memory pages remain until
the run ends. See the [reclamation contract](docs/architecture.md#string-memory-reclamation).
See the [operator contract](docs/architecture.md#string-operators) and
[ownership contract](docs/architecture.md#string-values-and-ownership).
Unsupported constructs, top-level effects and type mismatches fail
explicitly. There is no general Node/npm compatibility or async execution yet.

Runtime defaults include 8 MiB guest memory, instruction fuel, a 5-second
execution/HTTP deadline, and bounded host calls, I/O results and output. Host
failures clear result lengths and expose status through `varta.result.status()`:
0 success, 1 denied, 2 invalid input, 3 I/O failure, 4 result too large.

## Programmatic host errors

Capture a direct system call to keep its value and status together:

```ts
export function main(): void {
  const result = varta.result.capture(varta.env.get("APP_NAME"));
  if (result.ok) {
    console.log(result.value);
  } else {
    console.log(result.status);
  }
}
```

The readonly `ok`, `status` and `value` fields survive later system calls. Results
can be passed or returned as `VartaResult<string>`, `VartaResult<number>`,
`VartaResult<boolean>` or `VartaResult<void>`. Capture the system call inside a
user wrapper before returning it. Existing permission checks still apply, and
resource exhaustion still ends execution. The old API and status inspection stay
available. See the [result contract](docs/architecture.md#programmatic-host-results).

## Commands

- `init <name>`: create a project without overwriting an existing directory
- `check [--config] [--json]`: validate language, types, config and generated Wasm
- `dev [--config] [--policy]`: check, compile and execute
- `build [--config] [--single]`: produce validated Wasm and optional native launcher
- `run <path> [--policy]`: execute portable Wasm or a packaged artifact
- `inspect [path] [--json] [--policy]`: show validated code and full capability metadata
- `validate [path] [--json] [--policy]`: check code, ABI, manifest and import declarations

Failures return nonzero exit codes. `validate` adds policy review: every privileged
import needs a declaration, even though omitted permissions remain safe to execute
because the runtime denies them.

Native packaging now uses a fixed VARTA3 footer and portable Wasm. Legacy VARTA1/
VARTA2 and native AOT payloads are rejected; rebuild old artifacts. This avoids
accepting untrusted machine code through Wasmtime deserialization.

## Development

The workspace separates common schemas/ABI, compiler, runtime, project/artifact
services and CLI. Workspace code forbids unsafe. Run:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p varta --test smoke --locked
```
