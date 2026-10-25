# Varta

Varta compiles a deliberately small TypeScript subset to WebAssembly and runs it
in a capability-checked Wasmtime host. Privileged operations exist only as
explicit host imports and are denied by default. Production artifacts can be
packaged as a single native binary with `varta build --single`.

Status: MVP / active development. See
[`docs/architecture.md`](docs/architecture.md) and
[`docs/development-plan.md`](docs/development-plan.md) for details.

## What it is (and is not)

- Varta is a compiler and secure runtime, not a general-purpose JavaScript
  runtime.
- Every accepted language feature stays small, statically analyzable, and maps
  cleanly to Wasm.
- Capability IDs such as `fs.read`, `net.fetch`, `env.get`, and `time.now` are
  matched exactly at runtime.

## Quickstart

```sh
cargo build
varta init my-app
cd my-app
varta check
varta dev
varta build --single
./dist/my-app
```

`varta dev` compiles `src/main.ts` to Wasm and executes it in Wasmtime.
`varta build --single` produces a standalone native executable.

## Project layout

```text
my-app/
  varta.config.json
  src/main.ts
  varta.d.ts
  dist/app.wasm
```

## Capability declarations

`varta.config.json`:

```json
{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": {
    "time": { "now": true },
    "env": { "has": ["PATH"], "print": ["PATH"], "get": ["PATH"] },
    "fs": { "read": ["/data/**"] },
    "net": { "fetch": ["http://127.0.0.1/**"] }
  }
}
```

Host bindings in `varta.d.ts`:

```ts
declare const varta: {
  time: { now(): number };
  env: { has(name: string): boolean; print(name: string): void; get(name: string): string };
  fs: { read(path: string): string };
  net: { fetch(url: string): number };
};
```

Empty or omitted capability sets deny all host operations.

## Sample app

```ts
// src/main.ts
export function main(): void {
  console.log(varta.time.now());
}
```

Denied calls produce deterministic denials such as:

```text
varta log: <capability denied: fs.read>
```

## Commands

- `init <name>`: scaffold a project and capability manifest
- `check [--config] [--json]`: parse and validate
- `dev`: compile and run a development Wasm build
- `build [--config] [--single]`: create Wasm or single native artifact
- `run <path>`: execute a produced artifact
- `inspect [path] [--json]`: show binary and capability metadata
- `validate [path] [--json]`: validate an artifact's manifest

Run `varta --help` for option-level details.

## Security model

- Capability enforcement is deny by default before effects.
- Operation IDs are matched exactly (`fs.print` does not authorize
  `fs.read`).
- Guest `ptr/len` values are bounds-checked, capped, and UTF-8 validated.
- Filesystem paths reject traversal and are canonicalized before matching.

## Quality gates

CI runs exactly:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p varta --test smoke
```

## Notes and limitations

- The supported language subset remains intentionally small.
- Network client support is HTTP-oriented; HTTPS is not implemented yet.
- Full Node/npm compatibility and advanced storage are long-term work.
- See [`docs/architecture.md`](docs/architecture.md) for the complete contract.

## License

No license has been declared in this repository yet.
```

Check line count.