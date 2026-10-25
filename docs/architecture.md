# Varta architecture and implemented contract

This document describes the implemented MVP. Future work is tracked separately in
[development-plan.md](development-plan.md). Varta compiles a deliberately small
TypeScript subset to portable WebAssembly and executes it in a capability-checked
Wasmtime host. It does not provide general JavaScript, Node or npm compatibility.

Primitive Tool Contract v1 and offline discovery are implemented below. Broader
Application Contracts, validated external invocation, live application state and
MCP remain **planned** in the [development plan](development-plan.md#product-direction).
`inspect` reports bytecode, capabilities and an optional public tool catalog.

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
call immediately snapshots its result into a separate compiler-owned heap
before evaluating another expression. Variables and aliases copy the immutable
slice handle; reassignment changes the handle, and parameters/returns can share
the same slice through nested, recursive and cross-module calls. They never
retain an alias into the reusable host buffer. Empty/failed results use an empty
slice and allocate nothing; copying does not clear or replace the host status.

Host snapshots and nonempty concatenation results share a bounded, nonmoving
string heap. Before growing memory, the allocator collects unreachable strings
and reuses their storage. It grows memory only in whole Wasm pages, within the
existing 8 MiB limit. Its private metadata cannot be changed by source code or
host result writes. Heap exhaustion or failed growth traps before a new slice is
published; no live value is overwritten. Copying and collection consume guest
fuel, so the fuel budget can stop allocation before the memory boundary. Each run
creates a fresh instance and heap.

Dynamic strings passed to host calls follow exactly the existing input bounds,
UTF-8 checks and operation/resource authorization. A string value conveys no
authority. No new host import, capability, manifest or artifact version is needed.

### String operators

`string + string` concatenates UTF-8 content into a new immutable slice. Both
operands are evaluated exactly once, left to right, including nested user/host
calls. If either operand is empty, the result shares the other immutable slice
without allocating; the other expression is still evaluated. Nonempty operands
are copied into one heap allocation, with length overflow, per-value size,
heap and memory-growth checks before publishing the new slice. Existing aliases,
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
host snapshots share the bounded 8 MiB memory budget and reclamation rules.
Copying, collection and comparisons
may exhaust the default fuel budget before reaching the byte/memory limits.
Size/allocation/fuel failures stop execution rather than becoming a captured host
failure. Helpers for snapshot, concatenation, comparison, allocation, collection
and temporary roots use signatures and function indices resolved from typed tables.
Programs using only literals/comparisons need no managed heap. Host ABI and artifact versions
remain unchanged, and constructed paths/URLs still undergo the original resource
authorization and input checks.

### String memory reclamation

The compiler maintains exact string roots in a bounded shadow stack. A function
frame reserves one root slot for each string or `VartaResult<string>` binding,
including parameters. Assignments replace that slot; aliases retain independent
slots. Lexical scope exit clears its bindings, including exits through `break`
and `continue`. Function return removes the frame after evaluating its return
value. A binding is conservatively retained until overwrite, scope exit or return,
even if its last source use was earlier; there is no last-use optimization yet.

Expression roots protect evaluated operands and call arguments while later
operands or nested calls allocate. Each expression restores its temporary-root
mark and roots its resulting string payload; complete expression statements and
assignments remove consumed temporaries. Callee parameters are rooted before any
allocation. Return transfer has no intervening allocation between removing the
callee frame and rooting the caller's result. Numeric/boolean values and result
statuses are never mistaken for pointers. Static literals and empty strings do
not require heap marking. `VartaResult<string>` roots only its immutable payload.

Allocated blocks have a 16-byte header and 8-byte alignment. Allocation first
searches the free list and splits sufficiently large blocks. If appending would
grow committed memory, it runs one mark/sweep pass: mark blocks referenced by the
active roots, free unmarked blocks, coalesce adjacent free blocks, and trim a
free trailing run from the logical heap. Allocation retries before requesting
additional pages. The collector neither moves live data nor changes host status;
all scans, free-list traversal and zeroing use guest fuel. Live data, allocation
metadata or fragmentation can still exhaust memory; no compaction is implemented.

The root stack is 64 KiB (8192 packed slice slots) across active frames and
temporaries. A function whose fixed string frame exceeds that bound is rejected
with a source diagnostic. Dynamic frame/temporary overflow traps before writing
past the root region. Programs requiring owned strings initially commit 18 Wasm
pages for static data, host buffer and roots; literal/comparison-only programs
retain the one-page minimum. Actual heap blocks grow memory on demand. Freed
blocks are reused during the run, while committed Wasm pages retain their
high-water size until the instance is dropped. The host ABI, raw pointer/length
representation and portable artifact versions remain compatible with old code.

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

## Tool Contract v1 and offline discovery

The entry module may explicitly publish an exported named function:

```ts
export function previewTitle(nextTitle: string): string { return nextTitle + "!"; }
export function main(): void {}
varta.tool({ name: "document.previewTitle", description: "Preview a title" }, previewTitle);
```

`varta.tool` is a compile-time form, accepted only as a direct top-level call in
the entry module. It emits no guest initialization or object allocation. Its two
arguments must be a literal object with exactly `name` and `description` string
fields and a named function binding explicitly exported by that module. A local
function or an imported/re-exported function may be the handler; exporting a
function alone never publishes it. Dependencies cannot publish tools implicitly.
`main` cannot be a tool. Spreads, computed fields, dynamic metadata, extra fields,
duplicate fields/names and inline callbacks fail with source diagnostics.

The compiler derives ordered named parameters and the result from the checked
function signature. Supported input types are `number` (signed i32), `boolean`
and `string`; `void` is additionally supported as a result. Optional/default/rest
parameters, objects, arrays, `VartaResult` and generic tool signatures are not
supported. Parameter names use ASCII JS identifier characters, at most 64 bytes.
Tool names start with an ASCII letter and contain ASCII letters/digits, `_`, `-`
or `.`, at most 64 bytes. Descriptions are nonblank UTF-8, at most 2048 bytes,
without NUL. There are at most 64 tools and 16 parameters per tool.

One `varta.tools` custom section contains strict JSON `ToolCatalog` metadata,
bounded to 256 KiB, with independent `contractVersion: 1` and
`invocationAbiVersion: 1`. Unknown/duplicate JSON fields, unsupported versions,
noncanonical ordering and invalid limits/types fail closed. The catalog is sorted
by tool name; parameters retain signature order. Applications with no tools omit
the section. Host ABI v1, manifest schema v1 and container format v1 are unchanged.

`inspect` adds `toolCatalog` (null for no-tool artifacts). It projects canonical
types into closed JSON input schemas with every named field required, and output
schemas wrapping the result as `{ "value": ... }`; void maps to null. i32 bounds
are explicit. String schemas describe a character ceiling, while a separate UTF-8
byte ceiling remains authoritative. Schemas are generated, not separately stored
or accepted as a second source of type authority. Version 1 exposes no declared
domain errors (`domainErrors: []`); typed application errors belong to C2.

Each tool exports `varta.tool.<name>` with the checked physical Wasm signature.
Artifact validation requires the listed exports and matching signatures, rejects
unlisted reserved exports and recomputes each handler's transitive privileged
host operations from its direct call graph, including recursive calls. The exact
sorted set must match `requiredEffects`. Unprivileged logging is excluded.
Handlers must be guest-defined functions; string-transfer helpers cannot call
any host imports. Indirect/ref calls are outside tool ABI v1. Tool modules require exactly one
valid `varta.abi` section and cannot have a Wasm start function. Code validation
does not instantiate the module, call `_start`/handlers or run a native launcher.
All artifact commands validate contracts; `validate` additionally reviews grants.

Catalog limits currently equal the shared runtime ceilings: 64 KiB serialized
input, 1 MiB serialized output, 1 MiB per UTF-8 string, 8 MiB guest memory,
1,000,000 fuel, 1024 host calls and a five-second deadline. They are fixed in
contract v1, not configurable grants. The JSON invocation boundary enforcing
input/output envelopes is not implemented yet. Metadata is public and untrusted:
it is neither an authenticity proof nor an authorization decision. Type/ABI checks
cannot prove semantic equivalence between same-signature implementations. Runtime
capability enforcement remains necessary for every actual operation.

### Tool invocation ABI v1 ownership

The low-level ABI is specified and tested for a **fresh instance per invocation**;
there is no public external invocation API yet. The trusted host must validate
inputs, enforce budgets, instantiate without calling `_start`, and then use:

| Value/operation | Physical representation |
| --- | --- |
| number | i32, signed integer bounds |
| boolean | i32, host must accept/produce only 0 or 1 |
| string | i64 immutable handle: high 32 bits UTF-8 byte length, low 32 bits pointer |
| void result | no Wasm result; JSON projection is null |
| `varta.tool_abi.copy_string` | `(i32 ptr, i32 len) -> i64`, copies into the owned string heap |
| `varta.tool_abi.retain_string` | `(i64 handle) -> i64`, retains a copied input in the root stack |

Modules with any string tool parameters/results export both helpers and bounded
32-bit ABI memory. For each string argument, the host writes validated UTF-8 into
the reusable host buffer, copies it, and immediately retains the returned handle
**before another allocation**. This prevents the next input or a host call from
overwriting it and prevents collection during later argument transfer. These
helpers are trusted-host primitives, not tools for untrusted callers. The host
must never accept raw pointers/handles from external JSON.

The host calls the selected tool once, validates the result type/boolean/range,
bounds-checks and UTF-8-validates any string, and copies it into host-owned memory
before another guest allocation or dropping the instance. Result handles are not
retained after return; reading them immediately is required. Every invocation,
including traps, ends by dropping the instance; helpers do not support persistent
instances or releasing host roots between repeated calls. Input roots, allocator
metadata and helper work consume the existing memory/root/fuel budgets. C2 must
implement these obligations in a shared dispatcher before accepting external calls.

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
  `[65536, 1114112)`, exact string roots `[1114112, 1179648)`, then the managed
  string heap `[1179648, 8388608)`, including block headers. Unreachable blocks are
  reused during execution; the entire instance is released when execution ends.
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
| Managed string storage | Within the 8 MiB memory budget, after static page, 1 MiB host buffer and 64 KiB root stack; includes block metadata |
| String roots | 8192 slice slots across active frames and temporaries |
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
| Defined functions | 256 user functions plus at most six compiler string helpers |

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
