# Varta

Varta compiles a small, checked TypeScript subset to WebAssembly and runs it in a
capability-checked Wasmtime host. System access is denied unless the exact
operation and resource are declared. Applications can be distributed as a single
native executable containing portable Wasm and its runtime.

Status: MVP / active development. The [architecture contract](docs/architecture.md)
describes the implemented language, trust boundaries, ABI and limits. The
[development plan](docs/development-plan.md) tracks audit fixes and next milestones.

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

Host results are copied into immutable, bounded storage that lives until the run
ends. Every read still needs the corresponding capability. String concatenation,
comparison and garbage collection are not implemented yet. See the
[ownership contract](docs/architecture.md#string-values-and-ownership).
Unsupported constructs, top-level effects and type mismatches fail
explicitly. There is no general Node/npm compatibility or async execution yet.

Runtime defaults include 8 MiB guest memory, instruction fuel, a 5-second
execution/HTTP deadline, and bounded host calls, I/O results and output. Host
failures clear result lengths and expose status through `varta.result.status()`:
0 success, 1 denied, 2 invalid input, 3 I/O failure, 4 result too large.

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
