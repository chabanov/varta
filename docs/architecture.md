# Varta architecture and implemented contract

This document describes the implemented MVP. Future work is tracked separately in
[development-plan.md](development-plan.md). Varta compiles a deliberately small
TypeScript subset to portable WebAssembly and executes it in a capability-checked
Wasmtime host. It does not provide general JavaScript, Node or npm compatibility.

## Boundaries

```text
CLI -> varta-project -> varta-compiler -> validated portable Wasm
                    -> varta-runtime -> checked host adapters
                    -> artifact reader/writer
          all layers -> varta-common (schema, ABI, host operation registry)
```

- `varta-common`: strict configuration and manifest schemas, operation identities,
  host signatures, ABI constants and editor declarations.
- `varta-compiler`: bounded module graph, syntax parsing, lexical name resolution, subset/type checking,
  typed intermediate representation, Wasm emission and full bytecode validation.
- `varta-runtime`: bounded module validation, Wasmtime execution, resource limits,
  pinned filesystem capabilities, HTTP transport and host result handling.
- `varta-project`: confined source loading and one checked pipeline for check/dev/build; versioned artifacts,
  manifest serialization, policy review and inspection.
- `varta-cli`: argument parsing, presentation and process exit codes.

Workspace code forbids `unsafe`. New dependencies are exact-pinned: `cap-std` and
`cap-fs-ext` supply descriptor-relative, no-follow filesystem operations;
`url` supplies one structured URL interpretation; `ureq` supplies bounded HTTP
framing and timeouts. Implementing these protocols with ambient path checks and
handwritten HTTP caused the audit's permission bypasses.

## Compilation contract

Every project command loads and validates `varta.config.json`, resolves the entry
and output relative to that configuration file, checks the supported language and
its types, emits Wasm, and validates both bytecode and host ABI. Deno is not a
prerequisite and its presence cannot change whether a project is accepted.

The pipeline is:

```text
bounded source graph -> per-file SWC AST -> linked, typed IR -> Wasm -> validation
```

The semantic stage assigns a distinct local slot to each lexical binding and
checks parameters, assignments, calls, returns, const writes and uses before
initialization. Function signature interning assigns and emits each signature in
one operation, independent of source declaration order. Branch labels account for
every enclosing Wasm block, loop and if. Each nested switch has distinct temporary
locals; default selection and fallthrough follow source order.

Supported:

- local source modules with a zero-argument `main(): void` in the entry file;
- synchronous named functions with `number`/`boolean`/`string` parameters and results;
- immutable `VartaResult<T>` values with typed `ok`, `status` and `value` fields;
- primitive local `let`/`const` bindings with initializers and block scoping;
- integer arithmetic, comparisons, boolean literals, `!`, `&&` and `||`;
- if/else, while, for, switch, unlabelled break/continue, return;
- UTF-8 string literals and retained host results, including dynamic host arguments;
- string concatenation with `+` and content equality/ordering comparisons;
- type-only interfaces/aliases and built-in declarations.

Parameters require explicit primitive or `VartaResult<T>` annotations. Numeric/void function
results may be inferred from the supported return forms; explicit result
annotations are recommended. Value-returning functions must return on every
supported control-flow path. Case-local declarations require an explicit block.
String-returning functions require explicit `: string` result annotations;
result-returning functions require `: VartaResult<T>`. String and captured-result
types may be inferred from local initializers. Assignment, parameters and
returns require exact types; const and use-before-initialization rules also apply
to strings. Conditions, numeric arithmetic and switch remain number/boolean operations.
String indexing, properties, interpolation, implicit coercion, compound assignment
and string truthiness are not yet implemented and fail explicitly.

Numbers have signed **i32** Wasm semantics: addition/subtraction/multiplication
wrap, division truncates, and invalid division traps. This is intentionally not
JavaScript's floating-point `number` semantics. Booleans log as 0/1. `time.now`
returns Unix seconds representable in i32 and reports invalid input outside that
range rather than wrapping.

Unsupported import/export forms, top-level effects, async/generators, classes, closures, var,
objects/arrays, casts, any and unsupported syntax fail
explicitly. They are never silently discarded. Source is capped at 1 MiB with
lexical complexity limits before recursive parsing. Static strings occupy at most
one 64 KiB page. Parse errors retain source line/column positions.

### String values and ownership

A string is an immutable UTF-8 slice represented internally as one i64: the high
32 bits hold byte length, and the low 32 bits hold its memory pointer. User
function signatures and local slots preserve i32 number/boolean and i64 string
types; signatures are interned by complete Wasm types, not just arity. Host
imports still receive the ABI v1 borrowed i32 pointer/length pair.

Literal strings refer to deduplicated static data. Every string-producing host
call immediately snapshots its result into a separate compiler-owned arena
before evaluating another expression. Variables and aliases copy the immutable
slice handle; reassignment changes the handle, and parameters/returns can share
the same slice through nested, recursive and cross-module calls. They never
retain an alias into the reusable host buffer. Empty/failed results use an empty
slice and allocate nothing; copying does not clear or replace the host status.

Host snapshots and nonempty concatenation results share an execution arena after
the full maximum host-result buffer. It grows memory only in whole Wasm pages,
within the existing 8 MiB limit. Its private cursor cannot be changed by source
code or host result writes. Allocated string values remain
alive until the execution ends, even when their variables leave scope or the
result is discarded. There is no garbage collection or per-value reclamation yet.
Arena exhaustion or failed growth traps before a new slice is published; no
previously saved bytes are overwritten. String copying consumes guest fuel, so
the default fuel budget can stop allocation before the memory boundary. Each run
creates a fresh instance and arena.

Dynamic strings passed to host calls follow exactly the existing input bounds,
UTF-8 checks and operation/resource authorization. A string value conveys no
authority. No new host import, capability, manifest or artifact version is needed.

### String operators

`string + string` concatenates UTF-8 content into a new immutable slice. Both
operands are evaluated exactly once, left to right, including nested user/host
calls. If either operand is empty, the result shares the other immutable slice
without allocating; the other expression is still evaluated. Nonempty operands
are copied into one arena allocation, with length overflow, per-value size,
arena and memory-growth checks before publishing the new slice. Existing aliases,
captured results and source operands retain their original content. There is no
implicit conversion: `"count=" + 1` is a type error. Use `s = s + suffix` for
reassignment; `+=` remains outside the supported compound-assignment subset.

`==`, `===`, `!=` and `!==` compare string content, not pointers. Both operands
must be strings, so loose and strict equality have the same behavior within this
subset. `<`, `<=`, `>` and `>=` use lexicographic **UTF-16 code-unit order**, with a
shorter prefix sorting first. They are case-sensitive and perform no Unicode
normalization or locale collation: `"é"` differs from `"e\u0301"`, and `"🙂"`
sorts before `"\uE000"`. Strings remain stored as valid UTF-8; isolated surrogate
values remain unsupported. Embedded NUL bytes are ordinary content, not terminators.

Comparison allocates nothing and does not call the host or change result status.
The helper compares valid UTF-8 bytes with EE/EF lead bytes ordered above F0–F4,
preserving UTF-16 order around BMP/supplementary boundaries without materializing
a UTF-16 copy. Matching handles can return immediately; other comparisons visit
the common byte prefix and stop at the first difference. Loops consume guest fuel.
Neither concatenation nor comparison resets `varta.result.status()`, and saved
`VartaResult<T>` pairs remain unchanged.

Each runtime string value is capped at **1 MiB of UTF-8 bytes**. Concatenations and
host snapshots share the bounded 8 MiB memory budget and execution lifetime;
discarded intermediate strings are not reclaimed yet. Copying and comparisons
may exhaust the default fuel budget before reaching the byte/memory limits.
Size/allocation/fuel failures stop execution rather than becoming a captured host
failure. New helpers are emitted only when used: snapshot, concatenation and
comparison, with signatures and function indices resolved from typed tables.
Programs using only comparisons need no arena. Host ABI and artifact versions
remain unchanged, and constructed paths/URLs still undergo the original resource
authorization and input checks.

## Local modules

The compiler accepts named function imports (`import { f as g } from "./lib.ts"`),
direct function exports (`export function f(...)`) and local export lists
(`export { f as g }`). Local export lists may forward an imported function.
Each module has a separate function scope; private helpers with the same name in
different files do not collide. Imports must name exported functions, and their
parameter/result types are checked at each call. Duplicate or reserved bindings,
duplicate exports and unresolved exports fail before emission. Type aliases and
interfaces retain their existing erased behavior; they are not importable values.

The entry module must declare its own `main(): void` with no parameters. An
imported binding or an export alias cannot supply the entry point. A dependency
may declare an ordinary function named `main`, with its own signature. Recursion
within a file is supported; calls to the entry function remain rejected.

The dependency graph is a DAG: cycles, including self-imports, fail with an import
site diagnostic. There are no top-level execution or initialization semantics.
Dependencies are visited in sorted path order, once per resolved identity, and
linked before their dependents. Function slots are then allocated in module order
and declaration order, independent of import declaration order. Modules are
linked at compile time: running a built artifact never loads source files.
All linked host calls use the application's manifest and optional host ceiling;
an imported module cannot introduce permissions of its own. Source imports read
build inputs and do not grant guest filesystem access.

The project loader requires explicit relative `.ts` paths starting with `./` or
`../`. It normalizes dot segments lexically and rejects paths outside the
canonical directory containing the configuration file. Absolute/bare paths,
URLs, backslashes, controls, query strings and fragments are rejected; there is
no extension probing, directory/index lookup, npm resolution or network loading.
Parent segments are allowed when the normalized path stays inside the root.
The root directory is pinned once; every dependency is opened relative to that
descriptor with no-follow at every component. Both external and internal
symlinks are rejected. Inputs must be bounded regular UTF-8 files. The explicitly
configured entry retains support for legacy layouts outside the config directory;
this does not expand the boundary for discovered dependencies.

Limits: 64 unique modules including the entry, 256 import declarations, 32 modules
on the active dependency path, 1 MiB per source, 4 MiB aggregate source and 256
user functions across the graph. Existing per-file lexical/semantic complexity
limits still apply. Default, namespace, side-effect, type-only and attributed
imports, export-from/export-star/default exports and dynamic imports fail
explicitly. Unused imports still resolve and validate their dependency.

Embeddings use `compile_modules(ModuleSource, &dyn ModuleLoader)`. The loader
separates stable identity resolution from bounded text loading and owns its
source-access policy; the compiler performs no filesystem or network I/O.
Single-source `compile_typescript[_with_path]` remains available and requires an
explicit loader when source imports dependencies. Parse-only helpers do not link.
The result returns Wasm plus dependency metadata. `CheckedProject.modules` and
successful `check --json` expose `modules` as dependency-first `{path, imports}`
records, with sorted, deduplicated import paths. This metadata is for build-time
inspection and is not persisted into Wasm or manifests. ABI and artifact format
are unchanged.

## Source diagnostics

The project service resolves the configured entry to its canonical file path and
passes that identity into the compiler. Public `compile_typescript_with_path`
and `parse_typescript_with_path` accept source text and a diagnostic path without
reading that path; the original in-memory helpers retain `input.ts` as their
compatibility name. Naming a source does not change emitted Wasm bytes.

Parsing, lexical resolution, subset checking and type checking share one source
context per file. Nested failures keep the innermost available span rather than being
relabeled as the containing call or function. Function and expression spans also
survive typed IR lowering, so static-data emission errors identify the source
literal that exceeds the budget. This is compile-time source metadata; Wasm
runtime source maps and stack-trace symbolication are not implemented yet.

Diagnostics have stable category codes: `syntax_error`, `type_error`,
`module_error`, `unsupported_syntax`, `invalid_program` and `internal_error`. Missing
modules, cycles and path-policy failures point to the import site; dependency
syntax, type and static-data failures retain the dependency's filename and range.
Type mismatches
include expected/actual scalar types where available. The shared diagnostic
record contains the filename, message, a half-open source range, and an optional
bounded source excerpt. Lines and columns are **one-based Unicode scalar**
positions, not UTF-16 editor offsets; `byte_offset` is zero-based UTF-8 in the
original supplied text. A leading BOM is excluded from character columns but
included in byte offsets. CRLF, non-ASCII identifiers and end-of-file positions
preserve these conventions. Excerpts are windowed to 160 source characters around
the error, expand tabs for display and replace control characters; full ranges
remain available even when the displayed excerpt is truncated. Source-wide size
limits report the file start without copying an oversized source into the parser.

`check --json` preserves `status` and the human-readable `error` string and adds a
`diagnostics` array. Successful checks return an empty array; non-source failures,
such as an unreadable configuration file, also return an empty array rather than
inventing an entry-file location. The compiler currently stops at the first
failure. One diagnostic object for `let x: string = 7` is:

```json
{
  "code": "type_error",
  "message": "type mismatch for x: expected string, found number",
  "file": "/work/app/src/main.ts",
  "range": {
    "start": { "line": 2, "column": 19, "byte_offset": 49 },
    "end": { "line": 2, "column": 20, "byte_offset": 50 }
  },
  "snippet": {
    "text": "  let x: string = 7;",
    "highlight_start": 18,
    "highlight_width": 1
  }
}
```

Human-facing failures use `file:line:column: message` followed by the excerpt and
a caret. JSON stdout remains valid JSON; human diagnostics go to stderr. All
project commands use the same source validation path and return nonzero on these
failures before producing artifacts.

## Capabilities and trust

All host access is denied unless the **exact operation** is granted. A grant for
`env.has` cannot authorize `env.get`; `fs.read` cannot authorize `fs.print`.
Logging has no system-access capability but is output-budgeted.

Configuration shorthand:

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

Only `strict` is implemented. `compat` is rejected until it has an actual
behavioral contract. Unknown operations, unknown fields, duplicate manifest
grants, wrong value types and unsafe patterns are errors. Empty lists and false
are valid explicit denials; omitted grants also deny. Policy complexity is capped
at 128 grants and 256 patterns, with at most 64 patterns per grant and 512 bytes
per pattern. A wildcard comparison also fails closed when the product of
pattern/input byte lengths exceeds 65,536, bounding work outside Wasm fuel.

Canonical manifest:

```json
{
  "schemaVersion": 1,
  "abiVersion": 1,
  "policy": "strict",
  "capabilities": [
    { "id": "env.get", "patterns": ["APP_NAME"] },
    { "id": "time.now", "enabled": true }
  ]
}
```

`entry` and `target` metadata are optional. Legacy unversioned JSON manifests use
schema/ABI version 1 defaults but must pass the same strict grant validation.
Legacy string grants and implicit `enabled: true` are no longer accepted.

The embedding application can supply trusted grants directly, or use
`run_wasm_bytes_with_policy(bytes, requested, ceiling)` to constrain package
requests with a separate trusted host ceiling. The two policies are independently
validated and checked for every concrete operation/resource. Wildcard patterns
are never flattened into an approximate intersection. A ceiling can only restrict
requested permissions, never introduce an operation or resource the package did
not request.

The CLI preserves declared-manifest behavior when `--policy` is absent. Supply
`--policy host-policy.json` to constrain `run`, `dev`, `inspect`, `validate` or the
packaged application's own invocation. The host file uses the same strict
manifest schema. An explicit `{"capabilities":[]}` denies all privileged access;
a missing, unreadable or malformed policy is an error, never a fallback to the
package's permissions. Policy paths are resolved relative to the invocation's
working directory. Host policy is local execution input and is never embedded or
written into the artifact; `init`, `check` and `build` reject this flag.

```json
{
  "schemaVersion": 1,
  "abiVersion": 1,
  "capabilities": [
    { "id": "env.get", "patterns": ["APP_NAME"] },
    { "id": "time.now", "enabled": false }
  ]
}
```

```sh
varta run dist/app.wasm --policy host-policy.json
varta dev --policy host-policy.json
varta inspect dist/app.wasm --policy host-policy.json --json
./dist/app --policy host-policy.json
```

Inspection and JSON validation include an `authorization` object with the mode,
`combination: "allOf"`, complete requested and host-ceiling scopes, and one review
decision per requested operation. `denied` means an omitted/false/empty filter;
`granted` applies to enabled boolean operations; `runtime_check` means that the
scoped filters must both accept the actual resource. It does not promise that
arbitrary wildcard scopes overlap or that every call will succeed. A null
`hostCeiling` means no ceiling was supplied; an empty array is an explicit denial.
Validation still checks artifact/ABI/schema/declarations, not application behavior:
a valid deny-all policy can validate successfully while its operations are denied
at execution. `inspect` without a host policy continues to describe the declared
permissions and labels its authorization mode `declared`.

A native launcher itself has the authority of the OS user launching that
executable. Rebuild existing launchers to make their own CLI accept `--policy`;
the portable artifact format and ABI are unchanged.

### Filesystem

Patterns must be absolute and cannot contain traversal. `*` matches within a
single path component, `**` can span separators, and `?` matches one non-separator
character. Root/component boundaries remain intact: `/data/**` cannot match
`/data-secret/file`.

At execution setup, explicitly granted directory roots are opened and pinned as
capability descriptors. With a host ceiling, these descriptors come from the
host's scopes. Requested paths must also pass the package's lexical scopes before
any file is opened; a package-supplied root alias cannot acquire authority outside
the pinned host root. Each guest path is matched relative to the corresponding
root and opened through those descriptors, with **no-follow at every component**.
Symlinks below the granted root are rejected, including links to other files
inside that root. Directory and file opens do not reuse a checked ambient path.
This closes the canonicalize/open race. Root authority is acquired once at setup;
a declared root alias is resolved at that time.

Only regular files are read. Reads are nonblocking at open, capped before the
result is allocated unboundedly, and checked for UTF-8 where the ABI returns text.
OS filesystem operations remain synchronous; a stalled remote filesystem is not
made cancellable by the guest's fuel budget.

### Network

HTTP only is implemented. HTTPS grants and unsupported schemes are rejected.
URLs are parsed before authorization; scheme, normalized hostname and effective
port must match exactly. Wildcards are permitted only after the authority; `*`
can match path/query characters and `?` retains its URL query-separator meaning.
Control characters, whitespace, credentials and fragments are rejected. Encoded
paths are conservatively rejected to avoid server-specific decoding aliases.

The same parsed URL is passed to the transport. Automatic redirects and ambient
proxies are disabled, so a permitted response cannot silently grant another
request target. The HTTP client bounds headers and body and applies an end-to-end
remaining deadline including name resolution, connect and body reading. There is
no handwritten request-line/header construction.

## ABI v1 and errors

Imports live under `varta`. `HostOperation` in `varta-common` owns their names,
capability identifiers and scalar Wasm signatures. Pointer-taking imports require
an exported non-shared 32-bit `memory`. String-returning imports also require
exported mutable i32 globals `env_result_len` and `result_status`. Invalid import
signatures, result globals and advertised ABI versions fail before instantiation.

- Input strings are borrowed for the call and checked for bounds, length and UTF-8.
- Static strings occupy `[0, 65536)`.
- Host string results use a host-owned reusable buffer starting at 65536.
- Result pointers/bytes remain valid until the next host operation; there is no
  unbounded host bump allocation and no guest-controlled host heap pointer.
- Compiled string values snapshot those borrowed bytes into immutable execution
  storage. The layout is static data `[0, 65536)`, reusable host results
  `[65536, 1114112)`, then retained strings `[1114112, 8388608)`. The retained arena
  is bounded and reclaimed as a whole when execution ends.
- Every host operation first clears result length/status. Failed string calls
  return pointer 0 and length 0. No previous result length can be reused.
- `varta.result.status()` reads the structured status without clearing it.

Status codes:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Capability denied |
| 2 | Invalid input or UTF-8 |
| 3 | I/O failure |
| 4 | Result too large |

`net.fetch` returns the HTTP status or -1 on failure. String failures return an
empty result plus the status above. Human-readable diagnostics remain available.
Guest exit is a controlled runtime outcome, never `std::process::exit` inside the
library. The CLI alone translates the result into a process exit code. Runtime,
compilation and artifact errors return nonzero CLI status; JSON errors stay
machine-readable on stdout, with diagnostics on stderr.

## Programmatic host results

`varta.result.capture` is a compiler intrinsic that executes one direct privileged
host call and immediately pairs its value with that call's status:

```ts
function read(path: string): VartaResult<string> {
  return varta.result.capture(varta.fs.read(path));
}

export function main(): void {
  const result = read("/data/config.txt");
  if (result.ok) {
    console.log(result.value);
  } else {
    console.log(result.status);
  }
}
```

The immutable fields are `ok: boolean` (status equals zero), `status: number`
(the ABI codes 0–4 above) and `value: T`. `T` is exactly `string`, `number`,
`boolean` or `void`. Locals can infer their result type; function parameters and
returns use the built-in global `VartaResult<T>` annotation. Copies, aliases,
whole-value reassignment, nested/recursive calls and cross-module parameters and
returns preserve both components. Later host calls, logging and reads of the
legacy `varta.result.status()` cannot change a previously captured result.

The argument is evaluated once. It must lower directly to one privileged host
operation, optionally in parentheses; its own arguments may contain expressions
and calls. For nested host arguments, the result describes the outer call.
Capturing a variable, arbitrary expression, user-function call, status inspection
or logging call is rejected: these would not reliably identify one fresh host
status. Put capture inside a user wrapper and return its `VartaResult<T>` instead.
All existing time, environment, filesystem, network and process operations are
supported. Logging is excluded because it does not reset status on success.

A successful empty string, false or zero has `ok == true`; a failed call keeps its
original fallback value with `ok == false`. Fallbacks are empty string, false and
zero, except `net.fetch`, whose failure value remains -1. A void result has no
consumable `.value`; it can expose `.ok` and `.status`, and discarding `.value` has
no effect. Check `.ok` before using a value as a successful result. HTTP response
codes, including 4xx/5xx, are successful `net.fetch` return values when the request
itself completes; transport failures are host errors. Existing human diagnostics
still accompany host failures.

Result values are represented as two Wasm values: payload then i32 status. String
payloads retain the existing immutable i64 slice representation and snapshot
ownership. Number/boolean payloads use i32; void uses an unused zero i32 payload.
Each logical result local/parameter maps to two physical slots, and a result
return uses two Wasm results. Field selection evaluates its base once and keeps
the selected component; `.ok` compares the saved status with zero. Records require
no heap allocation or extra host call. The existing 32 physical Wasm-parameter
limit remains, so a result consumes two of those slots. Raw host imports, global
exports, ABI v1, capabilities and artifact formats remain unchanged.

The built-in type cannot be redeclared or fabricated through object literals.
Nested result types, arbitrary generics, destructuring, computed fields, field
writes and implicit conversion/comparison of a whole result are unsupported.
The shared editor declarations expose readonly fields and the same capture API.
This provides explicit branching on recoverable host failures, without introducing
general objects or exceptions. Fuel/deadline/call/output exhaustion, Wasm traps
and retained-string allocation failure still stop execution. An authorized
`process.exit` still exits with its requested code and produces no result;
a denied exit returns a failed `VartaResult<void>`.

## Resource limits

| Resource | Default limit |
| --- | --- |
| Guest linear memory | 8 MiB, enforced by Store limits including memory.grow |
| Retained string storage | Within the 8 MiB memory budget, after the 1 MiB host-result reservation and static page |
| One string value | 1 MiB of UTF-8 bytes |
| Guest instructions | 1,000,000 fuel units |
| Execution/HTTP deadline | 5 seconds |
| Host calls | 1024 per execution |
| Captured output | 1 MiB |
| Host input string | 64 KiB; narrower path/URL/name limits also apply |
| One host result | 1 MiB |
| HTTP response headers | 64 KiB |
| Wasm file | 4 MiB |
| Manifest/config | 256 KiB |
| Wasm signatures | 256 distinct complete parameter/result signatures |
| Function parameters | 32 physical Wasm values; each captured result uses two |
| Defined functions | 256 user functions plus at most three compiler string helpers |

Preflight limits also bound signatures, function count/body size, memories,
tables and globals before native compilation. Store budgets apply before Wasm
start functions execute. Fuel/epoch interruption bound guest execution; HTTP uses
the remaining deadline. Output is captured with a budget and returned to the
caller. Errors and guest exit do not terminate an embedding process.

## Artifact format and validation

Standalone Wasm has a strict sidecar `*.manifest.json`. A missing sidecar means
zero capabilities; an unreadable or malformed sidecar is an error. Embedded
manifests are authoritative even when empty and never fall back to a sidecar.

`build --single` emits:

```text
native launcher | portable Wasm | manifest JSON | fixed 32-byte footer
```

The little-endian footer contains Wasm length (u64), manifest length (u64), format
version 1 (u32), reserved zero flags (u32), and `VARTA3\0\0` (8 bytes). It is read
at a fixed EOF position with checked arithmetic and capped lengths. Payload bytes
are never scanned for markers. Outputs are written through temporary files and
renamed into place.

Native AOT payloads, `VARTA1`/`VARTA2` containers and public native deserialization
are removed. Rebuild existing native artifacts. Native launchers currently compile
embedded portable Wasm at startup. Reintroducing cached/AOT machine code requires
a separate, authenticated provenance design; checksums alone are insufficient.

`check`, `dev` and `build` share semantic/type/bytecode/ABI checks. Artifact `run`
and `inspect` fully validate bytecode and ABI too. `validate` additionally performs
policy review, requiring a declaration for every privileged import; this does not
mean that false or empty declarations grant access. Missing declarations are safe
to execute because the runtime denies those calls. Inspection includes full grant
scopes, enablement, versions, imports and exports.
