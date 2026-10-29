# Varta embedding contract

This document is the normative contract for **third-party hosts**: programs other
than `varta-runtime` that validate and execute Varta-compiled portable Wasm, for
example a JavaScript host in Node or a browser, or a host on another Wasm engine.
It states what exists at this revision of the repository, per host ABI version.
Planned platform work (a generated browser host, an application lifecycle, view
output, async `fetch`) is an accepted design in the
[development plan](development-plan.md#platform-targets-and-application-model),
not implemented and not part of this contract.

The reference host is `varta-runtime` (Wasmtime). Where this document and the
reference host disagree, the disagreement is a bug in one of them; report it rather
than copying the reference host's behaviour. The language-level meaning of each
profile stays in [architecture.md](architecture.md); this document covers only the
module/host boundary.

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119. Where the
right long-term rule differs from today's behaviour, this document states today's
behaviour as normative and lists the change under
[Proposed changes](#proposed-changes); nothing there applies until a later
revision adopts it.

Tables between `generated:` markers are produced from the `varta-common`
registries (`HostOperation`, `LanguageProfile`, `abi`, `value`, `exceptions`,
`function`, `tool`, `http`, `view`) and compared with them by
`crates/varta-common/tests/embedding_documentation.rs` (with its `view`
module). When a registry changes,
replace the block with the test's expected output. Prose never restates a
registry value.

## Conformance levels

A host implements one or more levels and MUST NOT claim a level it implements
only partly. A host that does not implement an import or a section a module needs
MUST refuse that module before instantiation; it MUST NOT supply a stub, a
best-effort approximation or a no-op in place of a registered operation.

| Level | Accepts | Entry point | Specified in |
| --- | --- | --- | --- |
| **T1 — tool host** | `checked-i32-utf8-v1` modules that publish tools | One named tool per fresh instance | [Tool invocation ABI v1](#tool-invocation-abi-v1) |
| **R1 — run host** | Modules whose imports the host implements, including `varta.functions` catalogs checked as in [Function and frame catalogs](#function-and-frame-catalogs) | `_start`, then exception check | [Run lifecycle](#run-lifecycle) |
| **R2 — scheduled run host** | R1 plus frame and scheduler profiles, with the [frame admission](#function-and-frame-catalogs) rules | `_start`, exception check, microtask checkpoint | [Microtask checkpoint](#microtask-checkpoint) |
| **H1 — HTTP host** | `checked-i32-utf8-v1` modules with an HTTP contract | One request per fresh instance | [architecture.md](architecture.md#http-service-v1); not specified for third-party hosts yet |

Level T1 needs only the scalar, string and privileged imports a tool module
actually imports. Most layout-bound pure imports (JSON, value readers, number
formatting, BigInt) are not specified here at the bit level; see
[What is unstable](#what-is-unstable).

## Inputs a host receives

- **Portable Wasm** (`*.wasm`) with an optional strict sidecar `*.manifest.json`.
  A missing sidecar means zero capabilities; an unreadable or malformed one is an
  error, never a fallback.
- **VARTA3 container** (`varta build --single`): `native launcher | Wasm | manifest
  JSON | 32-byte footer`. A host MAY read the portable payload with the footer
  rules in [architecture.md](architecture.md#artifact-format-and-validation); it
  MUST NOT execute the native prefix or scan payload bytes for markers. The
  embedded manifest wins over any sidecar, even when empty.
- **Host ceiling** (optional): a policy owned by the embedder, never by the
  package. It uses the [manifest](#manifest) schema with its defaults, so the
  smallest valid ceiling is `{"capabilities":[]}` (deny everything). Its
  `abiVersion`, when present, must be a supported host ABI but is **not** compared
  with the module's; `entry` and `target` are ignored metadata; the reference CLI
  reads at most 256 KiB. A missing, unreadable or malformed ceiling is an error,
  never a fallback to the package's grants.

Hosts MUST reject native machine code and MUST NOT cache or accept precompiled
engine payloads from package input.

### Manifest

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

| Field | Rule |
| --- | --- |
| `schemaVersion` | Optional, default 1; must be 1 |
| `abiVersion` | Optional, default 1; must be a supported host ABI. In a package manifest it must **equal the module's host ABI**: the `varta.abi` value, or 1 when the module has no `varta.abi` section (legacy and unspecified generic Wasm). A missing sidecar is synthesized with the module's host ABI and no grants |
| `policy` | Optional, default `"strict"`; must be `"strict"` |
| `entry` | Optional string, metadata only |
| `target` | Optional; `"wasm"` or `"native"` |
| `capabilities` | Required array of grants (`[]` denies everything) |

Unknown or duplicate fields, unknown capability ids, duplicate ids and a value of
the wrong kind (a boolean id needs exactly `enabled`, every other id exactly
`patterns`) are errors. Capability ids and the operations they authorize are the
**Capability** column of the [host operation table](#host-operations). Each id has
one grant kind:

| Grant kind | Wire form | Ids |
| --- | --- | --- |
| Boolean | `"enabled": true \| false` | `time.now`, `process.exit` |
| Name/path patterns | `"patterns": [...]` | `env.has`, `env.get`, `env.print`, `fs.read`, `fs.print` |
| URL patterns | `"patterns": [...]` | `net.fetch`, `net.get` |
| Listen endpoints | `"patterns": ["127.0.0.1:8080"]` | `net.listen` |

Policy complexity is capped at 128 grants and 256 patterns in total, and at most
64 patterns per grant. Empty lists and `false` are explicit denials; an omitted id
denies. A pattern is **unsafe**, and the whole manifest is rejected, when:

| Grant kind | Unsafe pattern |
| --- | --- |
| Names, paths and URLs | Longer than 512 bytes; empty or only whitespace; only `*` and `?` characters; contains NUL |
| `fs.*` paths (additionally) | Not absolute, or contains a `..` component |
| URLs (additionally) | Does not start with `http://`; empty authority, or an authority containing `*`, `?`, `@` or `#`; any control or whitespace character; a path containing `#`, `%` or `\`; a `.` or `..` path segment; not parseable as a URL, without a host, with a user name or password, or with `%` in the parsed path |
| Listen endpoints | Not a literal `IP:port` socket address, or not in canonical form (for example `[::1]:8080`); listen endpoints have no wildcards |

## Module validation

A host MUST perform every check below **before instantiation**, in any order, and
reject the module on the first failure. None of these checks executes guest code.
Passing validation is not proof of provenance or authorization.

1. **Size and format.** At most the `abi::MAX_WASM_BYTES` Wasm file limit of the
   [limits table](architecture.md#resource-limits), the standard binary magic and
   version 1.
2. **Version sections.** At most one `varta.abi` section holding exactly four
   bytes, a little-endian `u32` host ABI version listed in the
   [profile table](#profiles); at most one `varta.language` section holding strict
   JSON `{"schemaVersion":1,"profile":"<name>"}` of at most
   `language::MAX_METADATA_BYTES`, with no unknown, missing or duplicate fields and
   a listed profile. Pairing:

   | `varta.language` | `varta.abi` | Result |
   | --- | --- | --- |
   | Listed profile | That profile's host ABI | The declared profile |
   | Absent | 1 | Legacy `checked-i32-utf8-v1` |
   | Absent | Absent | Unspecified generic Wasm (no profile; only ABI 1 imports) |
   | Present | Missing or different | Reject |
   | Absent | Any other version | Reject |

3. **Structure limits.** At most `abi::MAX_WASM_SIGNATURES` function types, each
   with at most `abi::MAX_WASM_PARAMETERS` parameters and 2 results; at most 64 imports; at most one memory, 32-bit and not
   shared, with an initial size of at most `abi::MAX_MEMORY_BYTES /
   abi::WASM_PAGE_BYTES` pages; at most one table with at most 1024 initial
   elements; at most 128 globals; every function body at most
   `abi::MAX_WASM_FUNCTION_BODY_BYTES`; at most the profile's **Max defined
   functions** (`abi::MAX_USER_FUNCTIONS` plus its helper allowance plus
   `abi::MAX_HTTP_HANDLERS`) defined functions, from the
   [acceptance table](#profiles). Unspecified generic Wasm uses the
   `checked-i32-utf8-v1` row.
4. **Memory maximum (third-party hosts).** The reference host caps growth with a
   store limiter. Hosts without one, including every JavaScript host, MUST parse
   the memory section and reject a memory whose maximum is absent or above
   `abi::MAX_MEMORY_BYTES / abi::WASM_PAGE_BYTES` pages; the guest's own
   `memory.grow` is otherwise unbounded. Varta-compiled modules always declare
   that maximum.
5. **Imports.** Every import is a function in module `varta` whose name is in the
   [host operation table](#host-operations); its parameter and result types equal
   the **Signature** column exactly; its minimum host ABI is at most the declared
   host ABI; and the declared profile admits it (**Admitted under**). A JSON import
   must be the profile's own parse/stringify pair. Unspecified generic Wasm may
   import only operations with minimum host ABI 1. A JavaScript host MUST parse
   the type and import sections to compare signatures:
   `WebAssembly.Module.imports` reports no function types, and a JavaScript
   function links against any signature without a `LinkError`.
6. **Exports.** Names and kinds follow the [reserved names table](#reserved-names):
   - `memory` (32-bit, not shared) MUST be exported when the module imports any
     operation other than `log_i32`, `log_f64`, `log_f64_v2`, `log_undefined`,
     `number_remainder`, `process_exit`, `process_exit_f64` and `time_now`;
   - `env_result_len` and `result_status` MUST be exported mutable i32 globals when
     any import has **Result globals** = yes; when exported they MUST have that type;
   - `_start`, when exported, MUST be a `() -> ()` function;
   - exception, tool, HTTP, value-ABI and frame exports are checked as described in
     their sections; an unlisted export with the `varta.exception.`,
     `varta.tool.` or `varta.http_abi.` prefix is an error.
7. **Value section.** At most one `varta.values` section holding exactly four
   bytes, a little-endian `u32` listed under **Accepted `varta.values`** for the
   profile in the [acceptance table](#profiles): equality with the profile's value
   ABI for every binary64 profile, and value ABI 1 or 2 for
   `checked-i32-utf8-v1` (and unspecified generic Wasm). An HTTP contract
   additionally requires value ABI 2 exactly. The section is REQUIRED when the
   module imports a JSON operation or any `union_*`/`value_*` reader, and in every
   profile with an exception channel.
8. **Start function.** Exception profiles, tool modules and HTTP modules MUST NOT
   have a Wasm start section.
9. **Contracts.** A `varta.tools` catalog is validated as in
   [Tool invocation ABI v1](#tool-invocation-abi-v1). A `varta.http` contract and the
   `http_service` import occur together or not at all. A `varta.functions`
   catalog and the `varta_resume` dispatcher follow
   [Function and frame catalogs](#function-and-frame-catalogs).

## Profiles

Each profile fixes one host ABI, one value ABI and the physical encodings below.
**Admitted imports** counts the registered operations a module declaring the
profile may import.

<!-- generated:embedding-profiles:begin (checked by crates/varta-common/tests/embedding_documentation.rs) -->
| Profile | Host ABI | Value ABI | Numbers | Strings | Tool/HTTP entry points | Exception channel | Frames | Scheduler | Admitted imports |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `checked-i32-utf8-v1` | 1 | 2 | i32 | UTF-8 | yes | — | — | — | 15 |
| `checked-f64-utf8-v1` | 2 | 3 | binary64 | UTF-8 | no | — | — | — | 17 |
| `checked-f64-utf8-v2` | 3 | 4 | binary64 | UTF-8 | no | — | — | — | 18 |
| `checked-f64-utf16-v1` | 4 | 5 | binary64 | UTF-16LE | no | — | — | — | 20 |
| `checked-f64-utf16-v2` | 5 | 6 | binary64 | UTF-16LE | no | — | — | — | 26 |
| `checked-f64-utf16-v3` | 6 | 7 | binary64 | UTF-16LE | no | — | — | — | 26 |
| `checked-f64-utf16-v4` | 7 | 8 | binary64 | UTF-16LE | no | — | — | — | 27 |
| `checked-f64-utf16-v5` | 8 | 8 | binary64 | UTF-16LE | no | — | — | — | 27 |
| `checked-f64-utf16-v6` | 9 | 9 | binary64 | UTF-16LE | no | — | — | — | 27 |
| `checked-f64-utf16-v7` | 10 | 9 | binary64 | UTF-16LE | no | pending/value only | — | — | 27 |
| `checked-f64-utf16-v8` | 11 | 10 | binary64 | UTF-16LE | no | `varta.exception.version` = 2 | — | — | 28 |
| `checked-f64-utf16-v9` | 12 | 11 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 28 |
| `checked-f64-utf16-v10` | 13 | 11 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 28 |
| `checked-f64-utf16-v11` | 14 | 11 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 28 |
| `checked-f64-utf16-v12` | 15 | 11 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 28 |
| `checked-f64-utf16-v13` | 16 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 28 |
| `checked-f64-utf16-v14` | 17 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 28 |
| `checked-f64-utf16-v15` | 18 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 29 |
| `checked-f64-utf16-v16` | 19 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 49 |
| `checked-f64-utf16-v17` | 20 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 50 |
| `checked-f64-utf16-v18` | 21 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 50 |
| `checked-f64-utf16-v19` | 22 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 51 |
| `checked-f64-utf16-v20` | 23 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 52 |
| `checked-f64-utf16-v21` | 24 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 53 |
| `checked-f64-utf16-v22` | 25 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 56 |
| `checked-f64-utf16-v23` | 26 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 58 |
| `checked-f64-utf16-v24` | 26 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 58 |
| `checked-f64-utf16-v25` | 26 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 58 |
| `checked-f64-utf16-v26` | 26 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 58 |
| `checked-f64-utf16-v27` | 26 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 58 |
| `checked-f64-utf16-v28` | 27 | 12 | binary64 | UTF-16LE | no | `varta.exception.version_v3` = 3 | — | — | 68 |
| `checked-f64-utf16-v29` | 28 | 13 | binary64 | UTF-16LE | no | `varta.exception.version_v4` = 4 | — | — | 75 |
| `checked-f64-utf16-v30` | 28 | 13 | binary64 | UTF-16LE | no | `varta.exception.version_v4` = 4 | — | — | 75 |
| `checked-f64-utf16-v31` | 28 | 13 | binary64 | UTF-16LE | no | `varta.exception.version_v4` = 4 | — | — | 75 |
| `checked-f64-utf16-v32` | 28 | 13 | binary64 | UTF-16LE | no | `varta.exception.version_v5` = 5 | — | — | 75 |
| `checked-f64-utf16-v33` | 29 | 13 | binary64 | UTF-16LE | no | `varta.exception.version_v5` = 5 | `varta_resume`, catalog 2 | — | 75 |
| `checked-f64-utf16-v34` | 30 | 15 | binary64 | UTF-16LE | no | `varta.exception.version_v6` = 6 | `varta_resume`, catalog 2 | yes | 80 |
| `checked-f64-utf16-v35` | 31 | 16 | binary64 | UTF-16LE | no | `varta.exception.version_v7` = 7 | `varta_resume`, catalog 3 | yes | 80 |
| `checked-f64-utf16-v36` | 32 | 17 | binary64 | UTF-16LE | no | `varta.exception.version_v8` = 8 | `varta_resume`, catalog 3 | yes | 84 |
<!-- generated:embedding-profiles:end -->

Acceptance details per profile (used by validation steps 3, 7 and 9):

<!-- generated:embedding-profile-acceptance:begin (checked by crates/varta-common/tests/embedding_documentation.rs) -->
| Profile | Accepted `varta.values` | `varta.functions` versions | Helper allowance | Max defined functions |
| --- | --- | --- | --- | --- |
| `checked-i32-utf8-v1` | 1, 2 | — | 10 | 267 |
| `checked-f64-utf8-v1` | 3 | — | 10 | 267 |
| `checked-f64-utf8-v2` | 4 | — | 10 | 267 |
| `checked-f64-utf16-v1` | 5 | — | 10 | 267 |
| `checked-f64-utf16-v2` | 6 | — | 10 | 267 |
| `checked-f64-utf16-v3` | 7 | — | 10 | 267 |
| `checked-f64-utf16-v4` | 8 | — | 10 | 267 |
| `checked-f64-utf16-v5` | 8 | — | 10 | 267 |
| `checked-f64-utf16-v6` | 9 | — | 10 | 267 |
| `checked-f64-utf16-v7` | 9 | — | 10 | 267 |
| `checked-f64-utf16-v8` | 10 | — | 10 | 267 |
| `checked-f64-utf16-v9` | 11 | — | 10 | 267 |
| `checked-f64-utf16-v10` | 11 | — | 10 | 267 |
| `checked-f64-utf16-v11` | 11 | — | 10 | 267 |
| `checked-f64-utf16-v12` | 11 | — | 10 | 267 |
| `checked-f64-utf16-v13` | 12 | — | 12 | 269 |
| `checked-f64-utf16-v14` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v15` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v16` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v17` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v18` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v19` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v20` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v21` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v22` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v23` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v24` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v25` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v26` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v27` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v28` | 12 | — | 13 | 270 |
| `checked-f64-utf16-v29` | 13 | — | 13 | 270 |
| `checked-f64-utf16-v30` | 13 | 1 | 13 | 270 |
| `checked-f64-utf16-v31` | 13 | 1 | 13 | 270 |
| `checked-f64-utf16-v32` | 13 | 1 | 13 | 270 |
| `checked-f64-utf16-v33` | 13 | 1 (no frames), 2 (frames) | 13 | 270 |
| `checked-f64-utf16-v34` | 15 | 1 (no frames), 2 (frames) | 13 | 270 |
| `checked-f64-utf16-v35` | 16 | 1 (no frames), 3 (frames) | 13 | 270 |
| `checked-f64-utf16-v36` | 17 | 1 (no frames), 3 (frames) | 13 | 270 |
<!-- generated:embedding-profile-acceptance:end -->

Numbers are signed i32 in `checked-i32-utf8-v1` (wrapping arithmetic, truncating
division) and IEEE 754 binary64 in every other profile. Booleans are i32 `0`/`1`.

## Memory layout and encodings

<!-- generated:embedding-layout:begin (checked by crates/varta-common/tests/embedding_documentation.rs) -->
| Region or unit | Bytes | Source constants |
| --- | --- | --- |
| Static data (literals, descriptors, schemas) | [0, 65536) | `abi::RESULT_BASE` |
| Reusable host result buffer | [65536, 1114112) | `abi::RESULT_BASE`, `abi::MAX_RESULT_BYTES` |
| Exact reference root stack | [1114112, 1179648) | `abi::STRING_ROOT_BASE`, `abi::STRING_ROOT_END` |
| Exception root slot (start of the root stack) | [1114112, 1114120) | `exceptions::ROOT`, `exceptions::RESERVED_ROOT_BYTES` |
| Managed string/record/array heap | [1179648, 8388608) | `abi::OWNED_STRING_BASE`, `abi::MAX_MEMORY_BYTES` |
| Wasm page | 65536 | `abi::WASM_PAGE_BYTES` |
| Root slot | 8 | `abi::STRING_ROOT_SLOT_BYTES` |
| Heap block header / alignment | 16 / 8 | `abi::STRING_HEADER_BYTES`, `value::HEAP_ALIGNMENT` |
| Host input string (stored bytes) | 65536 | `abi::MAX_INPUT_BYTES` |
| One host result / one string value | 1048576 | `abi::MAX_STRING_BYTES` |
<!-- generated:embedding-layout:end -->

- **Strings** are immutable slices. A string value is one `i64` handle: the low
  32 bits are an unsigned byte pointer and the high 32 bits are an unsigned **byte**
  length, `handle = (len << 32) | ptr`. UTF-8 profiles store well-formed UTF-8.
  UTF-16 profiles store UTF-16LE with an even byte length; pointers need not be
  aligned; every `u16` value, including lone surrogates and NUL, is a valid string
  unit.
- **Host string arguments** are a borrowed `(ptr: i32, len: i32)` byte pair. The host
  MUST reject a negative pointer or length, a range outside memory and a length
  above the operation's limit, and MUST validate the encoding before use (UTF-8:
  strict; UTF-16 resource names: even length, no lone surrogate, no NUL).
- **Host string results** are written at `abi::RESULT_BASE`, growing memory in whole
  pages if needed (within the maximum); the host sets `env_result_len` to the byte
  length and returns `abi::RESULT_BASE` as the pointer. In UTF-16 profiles the host
  converts its UTF-8 text to UTF-16LE first. Result bytes are borrowed only until the
  next host operation; compiled code copies them into its heap immediately. Hosts
  MUST NOT keep pointers into guest memory across calls.
- **Records, arrays, unions, errors, promises and frames** live in the managed heap
  with versioned layouts (`varta.values`). Their byte formats are
  [unstable](#what-is-unstable) for third-party hosts.

## Host operations

### Call protocol

Every host operation is a synchronous import call. For each call the host MUST,
in this order:

1. **Budget.** Count one host call (except scheduler operations) and check the
   execution deadline. Exceeding `abi::MAX_HOST_CALLS` or the deadline is a fatal
   trap of the execution, never a status.
2. **Reset.** For a privileged operation (one with a **Capability**) and for the
   HTTP stop operation, set `env_result_len` to 0 and `result_status` to 0 **when
   they are exported**. Pure operations do not reset `result_status`. **Result
   globals** = yes means the module MUST export the globals; operations with "—"
   (for example `time_now`, `env_has`, `process_exit`, `net_fetch`) still report
   their status through `result_status` whenever it is exported, and have no
   observable status otherwise. Varta-compiled modules always export `memory`,
   `env_result_len`, `result_status` and `_start`.
3. **Validate input** as described under [encodings](#memory-layout-and-encodings),
   with `abi::MAX_INPUT_BYTES` for resource names, paths and URLs.
4. **Authorize** the exact operation id and the concrete resource against the
   package grants **and**, when supplied, the host ceiling. A grant for one id never
   authorizes another (`env.has` does not authorize `env.get`); a prefix of an id is
   not a match. Denial happens before any effect.
5. **Effect**, bounded by the remaining deadline.
6. **Complete.** On success return the value (string results as above). On any
   failure set `result_status` to the [status code](#status-and-control-codes),
   `env_result_len` to 0, and return the fallback value: 0 (empty string pointer,
   false, zero), except `net_fetch`/`net_fetch16`, which return -1.

Failures of steps 3–6 are recoverable statuses; the guest decides what to do.

**Output and its budget (today).** The reference host emits one output line per
logging call, per `env_print`/`fs_print` and per failed privileged call (a
diagnostic). Each line costs its UTF-8 byte length plus one against
`abi::MAX_OUTPUT_BYTES`, and overflow is a fatal trap. The cost is computed on the
**reference line text**, whatever a host displays:

| Source | Reference line text |
| --- | --- |
| Logging imports | `varta log: ` followed by the logged text |
| `env_print*` | `varta env: <name>=<value>` |
| `fs_print*` | The file content, unprefixed |
| Failure diagnostic | `varta log: ` followed by `<capability denied: <label>> (<target>)` (label `env` for every `env.*` id, otherwise the id), `<invalid host input>`, `<result too large>` or `<host I/O error: <text>>` |

A host that presents output differently MUST still charge these texts, so budget
exhaustion happens at the same call as in the reference host. The I/O error text
comes from the operating system, so exhaustion points after I/O failures are not
host-independent today; see [Proposed changes](#proposed-changes).

### Operation families

| Family | Imports | Contract |
| --- | --- | --- |
| Logging | `log_i32`, `log_string`, `log_string16`, `log_f64*`, `log_undefined` | Emit one line, no capability; see [Logging](#logging) |
| Clock | `time_now` | Requires `time.now` enabled. Unix seconds as i32; a time outside i32 is status 2 with 0 |
| Environment | `env_has*`, `env_get*`, `env_print*` | Name nonempty, at most 256 bytes, no `=` or NUL, matched against `patterns` with the [pattern rules](#pattern-matching). `env_has` returns 1 if the variable is set and 0 otherwise (status 0 either way). For an allowed name whose variable is **unset** (or whose value is not valid Unicode), `env_get` returns 0 with status 3 (I/O) and `env_print` emits nothing but the diagnostic and sets status 3; a value larger than `abi::MAX_RESULT_BYTES` is status 4. `env_get` returns the value as a string result, `env_print` emits `varta env: <name>=<value>` |
| Files | `fs_read*`, `fs_print*` | Absolute patterns; paths with `..` rejected; opened relative to pinned granted roots with no-follow at every component; regular files only; UTF-8 content. `fs_read` returns the content, `fs_print` emits it |
| Network | `net_get*`, `net_fetch*` | `http:` URLs only; parsed and normalized before matching; no credentials, fragments, control characters, whitespace or percent-encoded paths; no redirects or proxies; bounded by the deadline. `net_get` returns the UTF-8 body; `net_fetch` returns the HTTP status (4xx/5xx are successes) or -1 |
| Exit | `process_exit`, `process_exit_f64` | Requires `process.exit` enabled. Authorized: end the execution with the code (not the host process); no result is produced. Denied: status 1 and the guest continues. `process_exit_f64` requires a finite integral value within i32, otherwise status 2 and the guest continues |
| HTTP | `http_service`, `http_stop` | Feature marker and owned shutdown of [HTTP service v1](architecture.md#http-service-v1) |
| Scheduler | `promise_*`, `scheduler_enqueue` | See [Microtask checkpoint](#microtask-checkpoint) |
| Pure intrinsics | `number_*`, `string_char_*16`, `primitive_*`, `json_*`, `value_*`, `union_*`, `bigint_*` | Capability-free; semantics are the profile contracts in [architecture.md](architecture.md); numeric exactness in [Numeric intrinsics](#numeric-intrinsics); layout-bound ones are [unstable](#what-is-unstable) |

### Logging

| Import | Argument | Logged text |
| --- | --- | --- |
| `log_i32` | i32 | Decimal, with `-` for negative values |
| `log_string` | UTF-8 `(ptr, len)`, at most `abi::MAX_RESULT_BYTES` | The text. An invalid range or invalid UTF-8 sets status 2 and emits the `<invalid host input>` diagnostic instead |
| `log_string16` | UTF-16LE `(ptr, len)`, at most `abi::MAX_STRING_BYTES` | The text with each lone surrogate replaced by U+FFFD (display only). An invalid range or odd length is a **fatal trap**, not a status |
| `log_f64` | f64 (profiles up to the last one admitting it in the [table](#host-operation-table)) | `NaN`, `Infinity`, `-Infinity`; `-0` for negative zero; otherwise the shortest decimal that round-trips, **without exponent notation** (`1e21` logs `1000000000000000000000`, `1e-7` logs `0.0000001`) |
| `log_f64_v2` | f64 | ECMAScript `Number::toString(x)`, exactly JavaScript's `String(x)` (`-0` logs `0`, `1e21` logs `1e+21`) |
| `log_undefined` | i32 that MUST be 0 | `undefined`; any other argument is a **fatal trap** |
| `value_log_*`, `union_log` | Boxed value | Layout-bound, [unstable](#what-is-unstable) |

Logging never resets `result_status`, and display text never authorizes anything.

### Numeric intrinsics

- **Exact.** `number_remainder` is the truncating remainder (C `fmod`, which
  JavaScript's `%` equals); `number_power` applies ECMAScript `Number::exponentiate`'s
  special cases (NaN, zero, infinite and negative bases, odd integral exponents);
  the Math imports return the ECMAScript-required results for NaN, signed zeros,
  infinities and domain boundaries listed in
  [architecture.md](architecture.md#checked-f64-utf16-v16--native-math). These results
  MUST equal the reference bit for bit (NaN payload and sign excepted).
- **Approximated.** Ordinary finite results of `number_power` and the `number_sin`
  family are **implementation-approximated**: the reference host uses the
  platform's `f64` library, so its own results may differ between operating
  systems in the last bits. A host MAY use any faithful implementation, including
  JavaScript's `Math`; it MUST NOT claim last-bit equality, and conformance cases
  MUST NOT depend on the last bits of these results. Programs that need
  cross-host determinism (record/replay) cannot rely on them today; see
  [Proposed changes](#proposed-changes).
- `number_random` is entropy, not deterministic, and needs no grant.

### Pattern matching

Patterns are matched per Unicode scalar value. `*` matches any run of characters;
for filesystem ids it does not cross `/`, and `**` does. `?` matches one character
(not `/` for filesystem ids). Every other character matches itself. A comparison
fails closed when pattern bytes × input bytes exceeds 65,536. URL patterns allow
wildcards only after the authority; scheme, normalized host and effective port
must match exactly, and `?` keeps its query-separator meaning there. Listen
endpoints are canonical `IP:port` strings matched exactly.

### Host operation table

The table lists every registered import, ordered by minimum host ABI.
**Result globals** means the module must export `env_result_len` and
`result_status`. **String in host buffer** means a success writes a string result
at `abi::RESULT_BASE`. **Admitted under** lists the profiles whose modules may
import the operation, as contiguous runs in registry order.

<!-- generated:embedding-host-operations:begin (checked by crates/varta-common/tests/embedding_documentation.rs) -->
| Import `varta.<name>` | Signature | Min host ABI | Capability | Result globals | String in host buffer | Admitted under |
| --- | --- | --- | --- | --- | --- | --- |
| `env_get` | (i32, i32) → (i32) | 1 | `env.get` | yes | yes | `checked-i32-utf8-v1` – `checked-f64-utf8-v2` |
| `env_has` | (i32, i32) → (i32) | 1 | `env.has` | — | — | `checked-i32-utf8-v1` – `checked-f64-utf8-v2` |
| `env_print` | (i32, i32) → () | 1 | `env.print` | — | — | `checked-i32-utf8-v1` – `checked-f64-utf8-v2` |
| `fs_print` | (i32, i32) → () | 1 | `fs.print` | — | — | `checked-i32-utf8-v1` – `checked-f64-utf8-v2` |
| `fs_read` | (i32, i32) → (i32) | 1 | `fs.read` | yes | yes | `checked-i32-utf8-v1` – `checked-f64-utf8-v2` |
| `http_service` | () → () | 1 | none | — | — | `checked-i32-utf8-v1` |
| `http_stop` | () → () | 1 | `net.listen` | — | — | `checked-i32-utf8-v1` – `checked-f64-utf16-v36` |
| `json_parse` | (i32, i32, i32, i32) → (i32) | 1 | none | yes | — | `checked-i32-utf8-v1` |
| `json_stringify` | (i32, i32, i32) → (i32) | 1 | none | yes | yes | `checked-i32-utf8-v1` |
| `log_i32` | (i32) → () | 1 | none | — | — | `checked-i32-utf8-v1` – `checked-f64-utf16-v36` |
| `log_string` | (i32, i32) → () | 1 | none | — | — | `checked-i32-utf8-v1` – `checked-f64-utf8-v2` |
| `net_fetch` | (i32, i32) → (i32) | 1 | `net.fetch` | — | — | `checked-i32-utf8-v1` – `checked-f64-utf8-v2` |
| `net_get` | (i32, i32) → (i32) | 1 | `net.get` | yes | yes | `checked-i32-utf8-v1` – `checked-f64-utf8-v2` |
| `process_exit` | (i32) → () | 1 | `process.exit` | — | — | `checked-i32-utf8-v1` – `checked-f64-utf16-v36` |
| `time_now` | () → (i32) | 1 | `time.now` | — | — | `checked-i32-utf8-v1` – `checked-f64-utf16-v36` |
| `json_parse_v3` | (i32, i32, i32, i32) → (i64) | 2 | none | yes | — | `checked-f64-utf8-v1` |
| `json_stringify_v3` | (i32, i32, i32) → (i32) | 2 | none | yes | yes | `checked-f64-utf8-v1` |
| `log_f64` | (f64) → () | 2 | none | — | — | `checked-f64-utf8-v1` – `checked-f64-utf16-v17` |
| `number_remainder` | (f64, f64) → (f64) | 2 | none | — | — | `checked-f64-utf8-v1` – `checked-f64-utf16-v36` |
| `process_exit_f64` | (f64) → () | 2 | `process.exit` | — | — | `checked-f64-utf8-v1` – `checked-f64-utf16-v36` |
| `json_parse_v4` | (i32, i32, i32, i32) → (i64) | 3 | none | yes | — | `checked-f64-utf8-v2` |
| `json_stringify_v4` | (i32, i32, i32) → (i32) | 3 | none | yes | yes | `checked-f64-utf8-v2` |
| `log_undefined` | (i32) → () | 3 | none | — | — | `checked-f64-utf8-v2` – `checked-f64-utf16-v36` |
| `env_get16` | (i32, i32) → (i32) | 4 | `env.get` | yes | yes | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `env_has16` | (i32, i32) → (i32) | 4 | `env.has` | — | — | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `env_print16` | (i32, i32) → () | 4 | `env.print` | — | — | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `fs_print16` | (i32, i32) → () | 4 | `fs.print` | — | — | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `fs_read16` | (i32, i32) → (i32) | 4 | `fs.read` | yes | yes | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `json_parse_v5` | (i32, i32, i32, i32) → (i64) | 4 | none | yes | — | `checked-f64-utf16-v1` |
| `json_stringify_v5` | (i32, i32, i32) → (i32) | 4 | none | yes | yes | `checked-f64-utf16-v1` |
| `log_string16` | (i32, i32) → () | 4 | none | — | — | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `net_fetch16` | (i32, i32) → (i32) | 4 | `net.fetch` | — | — | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `net_get16` | (i32, i32) → (i32) | 4 | `net.get` | yes | yes | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `string_char_at16` | (i32, i32, f64) → (i32) | 4 | none | yes | yes | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `string_char_code_at16` | (i32, i32, f64) → (f64) | 4 | none | — | — | `checked-f64-utf16-v1` – `checked-f64-utf16-v36` |
| `json_parse_v6` | (i32, i32, i32, i32) → (i64) | 5 | none | yes | — | `checked-f64-utf16-v2` |
| `json_stringify_v6` | (i32, i32, i32) → (i32) | 5 | none | yes | yes | `checked-f64-utf16-v2` |
| `union_log` | (i64) → () | 5 | none | — | — | `checked-f64-utf16-v2` – `checked-f64-utf16-v3` |
| `union_nullish` | (i64) → (i32) | 5 | none | — | — | `checked-f64-utf16-v2` – `checked-f64-utf16-v3` |
| `union_nullish_equal` | (i64, i64) → (i32) | 5 | none | — | — | `checked-f64-utf16-v2` – `checked-f64-utf16-v3` |
| `union_strict_equal` | (i64, i64) → (i32) | 5 | none | — | — | `checked-f64-utf16-v2` – `checked-f64-utf16-v3` |
| `union_truthy` | (i64) → (i32) | 5 | none | — | — | `checked-f64-utf16-v2` – `checked-f64-utf16-v3` |
| `union_typeof` | (i64) → (i32) | 5 | none | yes | yes | `checked-f64-utf16-v2` – `checked-f64-utf16-v3` |
| `json_parse_v7` | (i32, i32, i32, i32) → (i64) | 6 | none | yes | — | `checked-f64-utf16-v3` |
| `json_stringify_v7` | (i32, i32, i32) → (i32) | 6 | none | yes | yes | `checked-f64-utf16-v3` |
| `json_parse_v8` | (i32, i32, i32, i32) → (i64) | 7 | none | yes | — | `checked-f64-utf16-v4` |
| `json_stringify_v8` | (i32, i32, i32) → (i32) | 7 | none | yes | yes | `checked-f64-utf16-v4` – `checked-f64-utf16-v5` |
| `value_is_array_v2` | (i64) → (i32) | 7 | none | — | — | `checked-f64-utf16-v4` – `checked-f64-utf16-v5` |
| `value_log_v2` | (i64) → () | 7 | none | — | — | `checked-f64-utf16-v4` – `checked-f64-utf16-v5` |
| `value_nullish_equal_v2` | (i64, i64) → (i32) | 7 | none | — | — | `checked-f64-utf16-v4` – `checked-f64-utf16-v5` |
| `value_nullish_v2` | (i64) → (i32) | 7 | none | — | — | `checked-f64-utf16-v4` – `checked-f64-utf16-v5` |
| `value_strict_equal_v2` | (i64, i64) → (i32) | 7 | none | — | — | `checked-f64-utf16-v4` – `checked-f64-utf16-v5` |
| `value_truthy_v2` | (i64) → (i32) | 7 | none | — | — | `checked-f64-utf16-v4` – `checked-f64-utf16-v5` |
| `value_typeof_v2` | (i64) → (i32) | 7 | none | yes | yes | `checked-f64-utf16-v4` – `checked-f64-utf16-v5` |
| `json_parse_v9` | (i32, i32, i32, i32) → (i64) | 8 | none | yes | — | `checked-f64-utf16-v5` |
| `json_parse_v10` | (i32, i32, i32, i32) → (i64) | 9 | none | yes | — | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `json_stringify_v9` | (i32, i32, i32) → (i32) | 9 | none | yes | yes | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `value_is_array_v3` | (i64) → (i32) | 9 | none | — | — | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `value_log_v3` | (i64) → () | 9 | none | — | — | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `value_nullish_equal_v3` | (i64, i64) → (i32) | 9 | none | — | — | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `value_nullish_v3` | (i64) → (i32) | 9 | none | — | — | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `value_strict_equal_v3` | (i64, i64) → (i32) | 9 | none | — | — | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `value_truthy_v3` | (i64) → (i32) | 9 | none | — | — | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `value_typeof_v3` | (i64) → (i32) | 9 | none | yes | yes | `checked-f64-utf16-v6` – `checked-f64-utf16-v7` |
| `json_parse_v11` | (i32, i32, i32, i32) → (i64) | 11 | none | yes | — | `checked-f64-utf16-v8` |
| `json_stringify_v10` | (i32, i32, i32) → (i32) | 11 | none | yes | yes | `checked-f64-utf16-v8` |
| `value_is_array_v4` | (i64) → (i32) | 11 | none | — | — | `checked-f64-utf16-v8` |
| `value_is_error` | (i64) → (i32) | 11 | none | — | — | `checked-f64-utf16-v8` |
| `value_log_v4` | (i64) → () | 11 | none | — | — | `checked-f64-utf16-v8` |
| `value_nullish_equal_v4` | (i64, i64) → (i32) | 11 | none | — | — | `checked-f64-utf16-v8` |
| `value_nullish_v4` | (i64) → (i32) | 11 | none | — | — | `checked-f64-utf16-v8` |
| `value_strict_equal_v4` | (i64, i64) → (i32) | 11 | none | — | — | `checked-f64-utf16-v8` |
| `value_truthy_v4` | (i64) → (i32) | 11 | none | — | — | `checked-f64-utf16-v8` |
| `value_typeof_v4` | (i64) → (i32) | 11 | none | yes | yes | `checked-f64-utf16-v8` |
| `json_parse_v12` | (i32, i32, i32, i32) → (i64) | 12 | none | yes | — | `checked-f64-utf16-v9` |
| `json_stringify_v11` | (i32, i32, i32) → (i32) | 12 | none | yes | yes | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `value_is_array_v5` | (i64) → (i32) | 12 | none | — | — | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `value_is_error_class` | (i64, i32) → (i32) | 12 | none | — | — | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `value_log_v5` | (i64) → () | 12 | none | — | — | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `value_nullish_equal_v5` | (i64, i64) → (i32) | 12 | none | — | — | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `value_nullish_v5` | (i64) → (i32) | 12 | none | — | — | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `value_strict_equal_v5` | (i64, i64) → (i32) | 12 | none | — | — | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `value_truthy_v5` | (i64) → (i32) | 12 | none | — | — | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `value_typeof_v5` | (i64) → (i32) | 12 | none | yes | yes | `checked-f64-utf16-v9` – `checked-f64-utf16-v12` |
| `json_parse_v13` | (i32, i32, i32, i32) → (i64) | 13 | none | yes | — | `checked-f64-utf16-v10` – `checked-f64-utf16-v12` |
| `json_parse_v14` | (i32, i32, i32, i32) → (i64) | 16 | none | yes | — | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `json_stringify_v12` | (i32, i32, i32) → (i32) | 16 | none | yes | yes | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `value_is_array_v6` | (i64) → (i32) | 16 | none | — | — | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `value_is_error_class_v2` | (i64, i32) → (i32) | 16 | none | — | — | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `value_log_v6` | (i64) → () | 16 | none | — | — | `checked-f64-utf16-v13` – `checked-f64-utf16-v17` |
| `value_nullish_equal_v6` | (i64, i64) → (i32) | 16 | none | — | — | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `value_nullish_v6` | (i64) → (i32) | 16 | none | — | — | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `value_strict_equal_v6` | (i64, i64) → (i32) | 16 | none | — | — | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `value_truthy_v6` | (i64) → (i32) | 16 | none | — | — | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `value_typeof_v6` | (i64) → (i32) | 16 | none | yes | yes | `checked-f64-utf16-v13` – `checked-f64-utf16-v28` |
| `number_power` | (f64, f64) → (f64) | 18 | none | — | — | `checked-f64-utf16-v15` – `checked-f64-utf16-v36` |
| `number_acos` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_acosh` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_asin` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_asinh` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_atan` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_atan2` | (f64, f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_atanh` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_cbrt` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_cos` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_cosh` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_exp` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_expm1` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_log` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_log10` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_log1p` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_log2` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_sin` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_sinh` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_tan` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_tanh` | (f64) → (f64) | 19 | none | — | — | `checked-f64-utf16-v16` – `checked-f64-utf16-v36` |
| `number_random` | () → (f64) | 20 | none | — | — | `checked-f64-utf16-v17` – `checked-f64-utf16-v36` |
| `log_f64_v2` | (f64) → () | 21 | none | — | — | `checked-f64-utf16-v18` – `checked-f64-utf16-v36` |
| `value_log_v7` | (i64) → () | 21 | none | — | — | `checked-f64-utf16-v18` – `checked-f64-utf16-v28` |
| `primitive_to_string` | (i64) → (i32) | 22 | none | yes | yes | `checked-f64-utf16-v19` – `checked-f64-utf16-v36` |
| `primitive_to_number` | (i64) → (f64) | 23 | none | — | — | `checked-f64-utf16-v20` – `checked-f64-utf16-v36` |
| `number_to_string` | (f64, f64) → (i32) | 24 | none | yes | yes | `checked-f64-utf16-v21` – `checked-f64-utf16-v36` |
| `number_to_exponential` | (f64, f64) → (i32) | 25 | none | yes | yes | `checked-f64-utf16-v22` – `checked-f64-utf16-v36` |
| `number_to_fixed` | (f64, f64) → (i32) | 25 | none | yes | yes | `checked-f64-utf16-v22` – `checked-f64-utf16-v36` |
| `number_to_precision` | (f64, f64) → (i32) | 25 | none | yes | yes | `checked-f64-utf16-v22` – `checked-f64-utf16-v36` |
| `number_parse_float` | (i64) → (f64) | 26 | none | — | — | `checked-f64-utf16-v23` – `checked-f64-utf16-v36` |
| `number_parse_int` | (i64, i64) → (f64) | 26 | none | — | — | `checked-f64-utf16-v23` – `checked-f64-utf16-v36` |
| `bigint_binary` | (i32, i64, i64) → (i32, i32) | 27 | none | yes | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_compare` | (i64, i64) → (f64) | 27 | none | — | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_compare_number` | (i64, f64) → (f64) | 27 | none | — | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_compare_string` | (i64, i32, i32) → (f64) | 27 | none | — | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_from_number` | (f64) → (i32, i32) | 27 | none | yes | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_from_string` | (i32, i32) → (i32, i32) | 27 | none | yes | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_to_number` | (i64) → (f64) | 27 | none | — | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_to_string` | (i64, f64) → (i32, i32) | 27 | none | yes | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_unary` | (i32, i64) → (i32, i32) | 27 | none | yes | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_width` | (i32, i64, f64) → (i32, i32) | 27 | none | yes | — | `checked-f64-utf16-v28` – `checked-f64-utf16-v36` |
| `bigint_from_value` | (i64) → (i32, i32) | 28 | none | yes | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `bigint_to_string_v2` | (i64, i64) → (i32, i32) | 28 | none | yes | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v36` |
| `bigint_width_v2` | (i32, i64, i64) → (i32, i32) | 28 | none | yes | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v36` |
| `json_parse_v15` | (i32, i32, i32, i32) → (i64) | 28 | none | yes | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `json_stringify_v13` | (i32, i32, i32) → (i32, i32) | 28 | none | yes | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `primitive_compare` | (i64, i64) → (f64) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v36` |
| `primitive_equal` | (i32, i64, i64) → (i32) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v36` |
| `primitive_to_number_v2` | (i64) → (f64) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `primitive_to_string_v2` | (i64) → (i32) | 28 | none | yes | yes | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `value_is_array_v7` | (i64) → (i32) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `value_is_error_class_v3` | (i64, i32) → (i32) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `value_log_v8` | (i64) → () | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `value_nullish_equal_v7` | (i64, i64) → (i32) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `value_nullish_v7` | (i64) → (i32) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `value_strict_equal_v7` | (i64, i64) → (i32) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `value_truthy_v7` | (i64) → (i32) | 28 | none | — | — | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `value_typeof_v7` | (i64) → (i32) | 28 | none | yes | yes | `checked-f64-utf16-v29` – `checked-f64-utf16-v33` |
| `bigint_from_value_v2` | (i64) → (i32, i32) | 30 | none | yes | — | `checked-f64-utf16-v34` |
| `json_parse_v16` | (i32, i32, i32, i32) → (i64) | 30 | none | yes | — | `checked-f64-utf16-v34` |
| `json_stringify_v14` | (i32, i32, i32) → (i32, i32) | 30 | none | yes | — | `checked-f64-utf16-v34` |
| `primitive_to_number_v3` | (i64) → (f64) | 30 | none | — | — | `checked-f64-utf16-v34` |
| `primitive_to_string_v3` | (i64) → (i32) | 30 | none | yes | yes | `checked-f64-utf16-v34` |
| `promise_handled` | (f64) → () | 30 | none (scheduler) | — | — | `checked-f64-utf16-v34` – `checked-f64-utf16-v36` |
| `promise_react` | (f64, i64) → (f64) | 30 | none (scheduler) | — | — | `checked-f64-utf16-v34` – `checked-f64-utf16-v36` |
| `promise_settle` | (f64) → () | 30 | none (scheduler) | — | — | `checked-f64-utf16-v34` – `checked-f64-utf16-v36` |
| `promise_unhandled` | (i64) → (f64) | 30 | none (scheduler) | — | — | `checked-f64-utf16-v34` – `checked-f64-utf16-v36` |
| `scheduler_enqueue` | (i64) → () | 30 | none (scheduler) | — | — | `checked-f64-utf16-v34` – `checked-f64-utf16-v36` |
| `value_is_array_v8` | (i64) → (i32) | 30 | none | — | — | `checked-f64-utf16-v34` |
| `value_is_error_class_v4` | (i64, i32) → (i32) | 30 | none | — | — | `checked-f64-utf16-v34` |
| `value_log_v9` | (i64) → () | 30 | none | — | — | `checked-f64-utf16-v34` |
| `value_nullish_equal_v8` | (i64, i64) → (i32) | 30 | none | — | — | `checked-f64-utf16-v34` |
| `value_nullish_v8` | (i64) → (i32) | 30 | none | — | — | `checked-f64-utf16-v34` |
| `value_strict_equal_v8` | (i64, i64) → (i32) | 30 | none | — | — | `checked-f64-utf16-v34` |
| `value_truthy_v8` | (i64) → (i32) | 30 | none | — | — | `checked-f64-utf16-v34` |
| `value_typeof_v8` | (i64) → (i32) | 30 | none | yes | yes | `checked-f64-utf16-v34` |
| `bigint_from_value_v3` | (i64) → (i32, i32) | 31 | none | yes | — | `checked-f64-utf16-v35` |
| `json_parse_v17` | (i32, i32, i32, i32) → (i64) | 31 | none | yes | — | `checked-f64-utf16-v35` |
| `json_stringify_v15` | (i32, i32, i32) → (i32, i32) | 31 | none | yes | — | `checked-f64-utf16-v35` |
| `primitive_to_number_v4` | (i64) → (f64) | 31 | none | — | — | `checked-f64-utf16-v35` |
| `primitive_to_string_v4` | (i64) → (i32) | 31 | none | yes | yes | `checked-f64-utf16-v35` |
| `value_is_array_v9` | (i64) → (i32) | 31 | none | — | — | `checked-f64-utf16-v35` |
| `value_is_error_class_v5` | (i64, i32) → (i32) | 31 | none | — | — | `checked-f64-utf16-v35` |
| `value_log_v10` | (i64) → () | 31 | none | — | — | `checked-f64-utf16-v35` |
| `value_nullish_equal_v9` | (i64, i64) → (i32) | 31 | none | — | — | `checked-f64-utf16-v35` |
| `value_nullish_v9` | (i64) → (i32) | 31 | none | — | — | `checked-f64-utf16-v35` |
| `value_strict_equal_v9` | (i64, i64) → (i32) | 31 | none | — | — | `checked-f64-utf16-v35` |
| `value_truthy_v9` | (i64) → (i32) | 31 | none | — | — | `checked-f64-utf16-v35` |
| `value_typeof_v9` | (i64) → (i32) | 31 | none | yes | yes | `checked-f64-utf16-v35` |
| `bigint_from_value_v4` | (i64) → (i32, i32) | 32 | none | yes | — | `checked-f64-utf16-v36` |
| `json_parse_v18` | (i32, i32, i32, i32) → (i64) | 32 | none | yes | — | `checked-f64-utf16-v36` |
| `json_stringify_v16` | (i32, i32, i32) → (i32, i32) | 32 | none | yes | — | `checked-f64-utf16-v36` |
| `primitive_to_number_v5` | (i64) → (f64) | 32 | none | — | — | `checked-f64-utf16-v36` |
| `primitive_to_string_v5` | (i64) → (i32) | 32 | none | yes | yes | `checked-f64-utf16-v36` |
| `timer_clear` | (f64) → () | 32 | none | — | — | `checked-f64-utf16-v36` |
| `timer_start` | (f64, f64, i32) → (f64) | 32 | `time.timer` | — | — | `checked-f64-utf16-v36` |
| `timer_take` | (f64) → (i32) | 32 | none | — | — | `checked-f64-utf16-v36` |
| `timer_uncaught` | (i64) → () | 32 | none | — | — | `checked-f64-utf16-v36` |
| `value_is_array_v10` | (i64) → (i32) | 32 | none | — | — | `checked-f64-utf16-v36` |
| `value_is_error_class_v6` | (i64, i32) → (i32) | 32 | none | — | — | `checked-f64-utf16-v36` |
| `value_log_v11` | (i64) → () | 32 | none | — | — | `checked-f64-utf16-v36` |
| `value_nullish_equal_v10` | (i64, i64) → (i32) | 32 | none | — | — | `checked-f64-utf16-v36` |
| `value_nullish_v10` | (i64) → (i32) | 32 | none | — | — | `checked-f64-utf16-v36` |
| `value_strict_equal_v10` | (i64, i64) → (i32) | 32 | none | — | — | `checked-f64-utf16-v36` |
| `value_truthy_v10` | (i64) → (i32) | 32 | none | — | — | `checked-f64-utf16-v36` |
| `value_typeof_v10` | (i64) → (i32) | 32 | none | yes | yes | `checked-f64-utf16-v36` |
<!-- generated:embedding-host-operations:end -->

## Status and control codes

<!-- generated:embedding-codes:begin (checked by crates/varta-common/tests/embedding_documentation.rs) -->
| Channel | Code | Name | Meaning |
| --- | --- | --- | --- |
| `result_status` | 0 | `Ok` | Success |
| `result_status` | 1 | `Denied` | Capability denied (checked before any effect) |
| `result_status` | 2 | `InvalidInput` | Invalid input, range or encoding |
| `result_status` | 3 | `Io` | I/O failure |
| `result_status` | 4 | `TooLarge` | Result too large |
| `varta_resume` completion | 0 | `Normal` | Resume normally with the input value |
| `varta_resume` completion | 1 | `Throw` | Resume by throwing the input value |
| `varta_resume` completion | 2 | `Return` | Resume by returning the input value |
| `varta_resume` outcome | 0 | `Suspended` | The frame suspended again |
| `varta_resume` outcome | 1 | `Returned` | The frame completed |
| `varta_resume` outcome | 2 | `Threw` | The frame completed by throwing |
<!-- generated:embedding-codes:end -->

`varta.result.capture` reads `result_status` immediately after one privileged
call, so a host MUST set it before returning from that call and MUST NOT change it
from any later pure operation.

## Reserved names

<!-- generated:embedding-names:begin (checked by crates/varta-common/tests/embedding_documentation.rs) -->
| Name | Kind | Presence | Constant |
| --- | --- | --- | --- |
| `varta.abi` | custom section, u32 LE | Host ABI version; mandatory for every declared profile | `abi::SECTION` |
| `varta.language` | custom section, JSON | Source profile; mandatory above host ABI 1 | `language::SECTION` |
| `varta.values` | custom section, u32 LE | Structured value ABI; required by JSON, union and exception imports | `value::VALUE_SECTION` |
| `varta.tools` | custom section, JSON | Tool catalog; present only when tools are published | `TOOL_SECTION` |
| `varta.http` | custom section, JSON | HTTP service contract; pairs with the `http_service` import | `http::SECTION` |
| `varta.functions` | custom section, binary | Function capture and frame catalog (v13+ closures, v33+ frames) | `function::CATALOG_SECTION` |
| `env_result_len` | export, mutable i32 global | Required when any import needs result globals | `abi::RESULT_LEN` |
| `result_status` | export, mutable i32 global | Required when any import needs result globals | `abi::RESULT_STATUS` |
| `varta_resume` | export, function | Closed resume dispatcher of frame profiles | `frame::RESUME_EXPORT` |
| `varta.tool.<name>` | export, function | One handler per catalog entry | `TOOL_EXPORT_PREFIX` |
| `varta.tool_abi.copy_string` | export, function | Tool string transfer (any string parameter or result) | `TOOL_COPY_STRING` |
| `varta.tool_abi.retain_string` | export, function | Tool string retention | `TOOL_RETAIN_STRING` |
| `varta.http_abi.handle` | export, function | HTTP adapter | `http::HANDLER` |
| `varta.http_abi.copy_string` | export, function | HTTP string transfer | `http::COPY` |
| `varta.http_abi.retain_string` | export, function | HTTP string retention | `http::RETAIN` |
| `varta.value_abi.allocate` | export, function | Guest allocator called back by checked JSON parsing | `value::VALUE_ALLOCATE` |
| `varta.value_abi.retain` | export, function | Guest root retention called back by checked JSON parsing | `value::VALUE_RETAIN` |
| `varta.exception.pending` | export, mutable i32 global | Exception profiles: 1 while an exception is pending | `exceptions::PENDING` |
| `varta.exception.value` | export, mutable i64 global | Exception profiles: the boxed thrown value | `exceptions::VALUE` |
| `varta.exception.version` | export, immutable i32 global | Exception channel version of Error profiles (one per module) | `exceptions::VERSION` |
| `varta.exception.version_v3` | export, immutable i32 global | Exception channel version of Error profiles (one per module) | `exceptions::FAMILY_VERSION` |
| `varta.exception.version_v4` | export, immutable i32 global | Exception channel version of Error profiles (one per module) | `exceptions::BIGINT_VERSION` |
| `varta.exception.version_v5` | export, immutable i32 global | Exception channel version of Error profiles (one per module) | `exceptions::CLASS_ERROR_VERSION` |
| `varta.exception.version_v6` | export, immutable i32 global | Exception channel version of Error profiles (one per module) | `exceptions::DYNAMIC_VERSION` |
| `varta.exception.version_v7` | export, immutable i32 global | Exception channel version of Error profiles (one per module) | `exceptions::PROMISE_VERSION` |
| `varta.exception.version_v8` | export, immutable i32 global | Exception channel version of Error profiles (one per module) | `exceptions::FUNCTION_FIELD_VERSION` |
<!-- generated:embedding-names:end -->

The import module is always `varta`; the memory export is always `memory`; the
entry point is the `_start` export. Helper signatures: tool and HTTP
`copy_string` `(i32, i32) → (i64)`, `retain_string` `(i64) → (i64)`, HTTP
`handle` `(i64) → (i64)`, `varta.value_abi.allocate` `(i32) → (i32)`,
`varta.value_abi.retain` `(i64) → (i64)` and `varta_resume`
`(i64, i32, i64) → (i32)`. Helpers are trusted-host primitives: a host MUST NOT
expose them to untrusted callers or accept raw pointers or handles from external
input.

## Run lifecycle

One **execution** owns one fresh store and instance and is never reused:

1. Validate the module and both policies.
2. Create the execution budgets (fuel, deadline, memory, host calls, output) from
   the [limits table](architecture.md#resource-limits); they cover the whole
   execution, including instantiation and every job.
3. Instantiate exactly once, linking only the imports the module declares. A
   failed instantiation ends the execution; never instantiate the same execution
   twice.
4. In exception profiles, check the [initial exception state](#exception-channel).
5. Call `_start` if it is exported.
6. Check the [exception channel](#exception-channel).
7. In scheduler profiles, run the [microtask checkpoint](#microtask-checkpoint).
8. Drop the instance. Authorized effects are not rolled back.

A trap, a fuel/deadline/host-call/output exhaustion or a scheduler error ends the
execution as a failure. An authorized exit ends it with its exit code (which may
be 0) and is not an exception. Neither is ever delivered to the guest as a
catchable value.

### Exception channel

Profiles with an exception channel export `varta.exception.pending` (mutable i32),
`varta.exception.value` (mutable i64) and, in Error profiles, one immutable i32
version global whose name and value are in the [profile table](#profiles). The
8-byte root slot at `exceptions::ROOT` mirrors the pending value.

- **Initial state** (after instantiation, before `_start`): pending 0, value 0 and
  root slot all zero; the version global, if any, has its registered value.
  Anything else rejects the module.
- **After `_start` and after each job**: pending 0, value 0 and root 0 means no
  exception. Pending 1 with value equal to the root slot means an uncaught guest
  exception: the execution fails with that value. Any other state is invalid and
  fails the execution.

Rendering the thrown value reads the value layout of the profile and is
[unstable](#what-is-unstable); a host that cannot render it MUST still fail the
execution and MAY report the value as opaque.

### Microtask checkpoint

Scheduler profiles (**Scheduler** = yes) keep one bounded FIFO queue of resume
tokens per execution. A token is an opaque `i64`; the host never interprets it and
never retains a guest reference.

- `scheduler_enqueue(token)` queues the token.
- `promise_react(serial, token) → serial` registers the token as a reaction of a
  pending promise. Serial 0 asks the host to allocate a new serial (1, 2, …); any
  other serial must be one the host issued and has not yet settled.
- `promise_settle(serial)` moves that promise's reactions, in registration order,
  to the end of the queue.
- `promise_unhandled(reason) → id` records a rejection without a handler and
  returns a new tracking id (1, 2, …); `promise_handled(id)` withdraws it. The host
  MAY render the boxed `reason` for its diagnostic (unstable layout).
- **Bounds.** A token is *held* from `scheduler_enqueue` or `promise_react` until
  the host takes it from the queue. At most
  `scheduler::MAX_PENDING_CONTINUATIONS` tokens are held at once, **counting queued
  tokens and tokens registered as reactions together**; registering one more is
  fatal. At most `scheduler::MAX_EXECUTION_JOBS` tokens are taken from the queue
  per execution (taking one more is fatal). Serials and tracking ids are separate
  counters, each at most `scheduler::MAX_PROMISE_SERIALS` per execution.
- Holding a token that is already held, a serial or id that is not an integer in
  range or was not issued, settling a serial twice, reacting to a settled serial
  and withdrawing an id twice are fatal errors. Fatal scheduler errors are never
  delivered to the guest.

After `_start` returns with an empty exception channel, the host takes tokens in
FIFO order, including tokens queued meanwhile, and for each calls
`varta_resume(token, 0, 0)` (`Normal` completion, `undefined` input). The result
must be the `Suspended` or `Returned` outcome code; then the exception channel must
be empty. When the queue is empty, any outstanding unhandled rejection fails the
execution. Scheduler imports are not charged to the host-call budget but still
observe the deadline.

## Function and frame catalogs

Profiles with entries in the **`varta.functions` versions** column of the
[acceptance table](#profiles) may carry one `varta.functions` custom section; every
other profile, and unspecified generic Wasm, MUST NOT. Closures in earlier
profiles carry no catalog. A host that never reads guest memory through catalog
metadata (no layout-bound readers) needs only the rules below; they are enough to
run R1 modules with catalogs and R2 modules honestly. The catalog is consistency
metadata: Wasm isolation does not depend on it, and a host never resumes anything
but tokens the guest itself handed over.

**Catalog section (normative minimum).**

1. At most one section, at most `abi::MAX_WASM_BYTES`, strict JSON (unknown or
   duplicate fields rejected) with fields `version`, `value_version`, `types`,
   `bindings`, `functions` and, only in frame versions, `frames`.
2. `version` is one the profile lists: `function::CATALOG_VERSION` with no
   `frames` member, or the profile's frame version (2 with cell layouts in
   v33/v34, 3 with liveness layouts in v35/v36) with a non-empty `frames` array.
   Version 1 never carries frames.
3. `value_version` equals the profile's value ABI.
4. Array lengths stay within the reference bounds (`types` and `bindings` at most
   `function::MAX_CAPTURE_TYPES`, `functions` and `frames` at most
   `abi::MAX_USER_FUNCTIONS`), and every `ty`/`binding` index is in range.

The remaining structural rules (type graph, capture and liveness entries, binding
classes per layout) are defined by `varta_common::function::{catalog, validate,
admission}`; they are [unstable](#what-is-unstable). A host that does not apply them
MUST NOT interpret guest memory through the catalog and MUST say that it applies
reduced catalog validation.

**Frame admission (R2, normative).** For a profile with **Frames** in the
[profile table](#profiles):

1. `varta_resume` is exported **if and only if** the catalog has frame layouts;
   profiles without frames must not export it.
2. It is a guest-defined function (not an import) with signature
   `(i64, i32, i64) → (i32)`.
3. Its body contains no `call_indirect`, `return_call_indirect`, `call_ref`,
   `return_call_ref` or table instruction, and every direct `call`/`return_call`
   targets a guest-defined function other than itself.
4. The distinct direct call targets with signature `(i64, i32, i64) → ()` (resume
   bodies) number exactly as many as the frame layouts; no resume body is exported.
5. Each layout's `code` plus the number of imported functions names a
   guest-defined function that is neither the dispatcher nor a resume body and
   returns exactly one `i64`.

Rules 1–5 need only the export, type, function and import sections plus a decode of
the dispatcher's own body. The host calls `varta_resume` only with tokens it
received from the scheduler imports, `Completion` code 0 and input 0, and treats
any outcome other than `Suspended`/`Returned` as fatal.

## Tool invocation ABI v1

Level T1. A tool module is a `checked-i32-utf8-v1` module with one `varta.tools`
custom section (strict JSON, at most `MAX_TOOL_CATALOG_BYTES`):

<!-- generated:embedding-tool-catalog-example:begin (checked by crates/varta-common/tests/embedding_documentation.rs) -->
```json
{
  "contractVersion": 1,
  "invocationAbiVersion": 1,
  "tools": [
    {
      "name": "text.echo",
      "description": "Return supplied text",
      "export": "varta.tool.text.echo",
      "parameters": [
        {
          "name": "value",
          "valueType": "string"
        }
      ],
      "result": "string",
      "requiredEffects": [],
      "limits": {
        "inputBytes": 65536,
        "outputBytes": 1048576,
        "stringBytes": 1048576,
        "memoryBytes": 8388608,
        "fuel": 1000000,
        "hostCalls": 1024,
        "deadlineMs": 5000
      }
    }
  ]
}
```
<!-- generated:embedding-tool-catalog-example:end -->

The section holds compact JSON; the example is pretty-printed. `limits` are the
[entry-point limits](#entry-point-limits).

**Catalog validation.** Both versions are 1; unknown or duplicate fields are
errors; there is at least one tool and at most `MAX_TOOLS`; tools are sorted by
`name` with no duplicates; a name starts with an ASCII letter and contains ASCII
letters, digits, `_`, `-` or `.`; `export` is `varta.tool.` followed by the name;
the description is nonblank, without NUL; parameter names start with an ASCII
letter, `_` or `$` and continue with ASCII letters, digits, `_` or `$`, and are
unique; `valueType` is `integer`, `boolean` or `string` (`void` only as `result`);
`requiredEffects` are sorted, unique capability ids; `limits` equal the registry
values exactly. Every listed export exists with the physical signature below, no
other `varta.tool.` export exists, the module has exactly one `varta.abi` section
and no start section, and string-using tools come with both `varta.tool_abi.`
helpers. The reference host also recomputes each handler's transitive privileged
imports from its direct call graph and requires them to equal `requiredEffects`.
A third-party host MAY do the same (it needs a decoder for every function body);
a host that does not MUST present `requiredEffects` as unverified metadata. In no
case is `requiredEffects` a grant: only the package grants and the host ceiling
authorize effects.

**Physical signature.**

| Catalog type | Parameter / result |
| --- | --- |
| `integer` | `i32`, signed |
| `boolean` | `i32`; results must be exactly 0 or 1 |
| `string` | `i64` handle (UTF-8) |
| `void` (result only) | no Wasm result; projected as `null` |

**Invocation** (each call is one fresh [execution](#run-lifecycle) that never calls
`_start`):

1. Check the tool name syntax and that the argument bytes are at most the tool's
   `inputBytes`; validate the policies.
2. Validate the module and catalog; look the tool up by exact name (never fall back
   to an arbitrary export).
3. Parse the arguments completely **before instantiation**: exactly one JSON object;
   every declared parameter present; no extra or duplicate keys (also after escape
   decoding); no trailing data. `integer` accepts only a plain signed i32 token (no
   fraction, exponent, `-0` or leading `+`); `boolean` only `true`/`false`; `string`
   only a JSON string whose UTF-8 is at most `stringBytes` and contains no lone
   escaped surrogate.
4. Instantiate. For each string argument, in parameter order: write its UTF-8 bytes
   at `abi::RESULT_BASE`; call `varta.tool_abi.copy_string(RESULT_BASE, len)`;
   check that the returned handle is in bounds and its bytes equal the input; call
   `varta.tool_abi.retain_string(handle)` **before any other guest call** and check it
   returns the same handle.
5. Call the export once with the physical arguments.
6. Validate the result: a boolean is 0 or 1; a string handle is in bounds, at most
   `stringBytes`, valid UTF-8, and is copied out of guest memory before any further
   guest call or dropping the instance.
7. Serialize `{"value": <result>}` and check it is at most `outputBytes`.
8. Drop the instance.

A denied host operation inside a handler is a status the handler sees, not an
invocation failure. Arguments never carry rights. Failures expose no partial
result and no logs.

**Envelopes.** The machine-readable output of `varta invoke --json` and of the
runtime companion's `invoke --json` / packaged `--invoke --json` is one line of
compact JSON with keys in lexicographic order (consumers MUST NOT depend on the
order):

| Outcome | Envelope | Exit status |
| --- | --- | --- |
| Success | `{"logs":[<line>…],"result":{"value":<integer \| boolean \| string \| null>},"status":"ok"}` | 0 |
| Failure, development CLI | `{"code":<code \| null>,"diagnostics":[<diagnostic>…],"error":"<text>","status":"failed"}` | 1 |
| Failure, runtime companion | `{"code":<code \| null>,"error":"<text>","status":"failed"}` (no `diagnostics` member today) | 1 |

`code` is one of the stable codes `invalid_arguments`, `unknown_tool`,
`invalid_artifact`, `invalid_policy`, `invalid_result`, `execution_failed` and
`guest_exit`, or `null` for an error before dispatch that has no invocation code
(for example a file that cannot be read, or a malformed policy); a malformed artifact reports
`invalid_artifact`. `error` is human-readable and not stable; `diagnostics` holds
source diagnostics and is empty for invocation errors. Each `logs` entry is the
**reference line text** of [Output and its budget](#call-protocol), including the
`varta log: ` prefix and failure diagnostics (see
[Proposed changes](#proposed-changes)). Without `--json`, stdout carries only
`{"value":…}` and the log lines go to stderr.

## View protocol draft v0 (unstable)

**Unstable draft v0, data only.** These tables are generated from
`varta_common::view` so that a browser applier, the headless renderer and a
native renderer can be written against one set of codes for the accepted L0 v1
subset ([design](development-plan.md#view-layer-l0l3)). Nothing here is
implemented: `view_commit` is not a registered host operation, no compiler emits
it, no runtime links it and no profile admits it. Codes and layouts may change
until the freeze criterion is met; the frozen protocol gets its own version and
host ABI.

**Encoding.** Little-endian. A batch is the batch header (`version: u16`,
`flags: u16` = 0, `op_count: u32`) followed by `op_count` operations. Each
operation is the operation header (`opcode: u8` and three zero bytes) followed by
its operands in wire order: ids and `u32` values take 4 bytes, codes take a `u16`
plus two zero bytes, `f64` takes 8 bytes, and a string is a `u32` byte length
followed by UTF-16LE code units, zero-padded to the string alignment. Operands
are aligned to 4 bytes only, so an `f64` may start at a 4-byte offset. The host
validates the whole batch before applying any operation; an unknown opcode, code,
id or nonzero padding invalidates it. An event record is the fixed header below
followed by the `key` and `value` strings.

<!-- generated:view-layout:begin (checked by crates/varta-common/tests/embedding_documentation/view.rs) -->
| Item | Value |
| --- | --- |
| Draft version | 0 |
| Patch import (not registered) | `varta.view_commit` (i32, i32) → (i32) |
| Event export (not registered) | `varta.view_abi.event` (i32) → () |
| Root node id | 1 |
| "No node" id | 0 |
| Batch header bytes | 8 |
| Operation header bytes | 4 |
| String alignment bytes | 4 |
| Event header bytes | 48 |
| Event strings after the header | key, value |
| Placeholder: batch bytes / operations | 262144 / 16384 |
| Placeholder: live nodes / listeners | 65536 / 8192 |
| Placeholder: value bytes / stylesheet bytes / pending events | 65536 / 262144 / 64 |
| `listen` option `preventDefault` | bit value 1 |
| `listen` option `stopPropagation` | bit value 2 |
| Event flag `shiftKey` | bit value 1 |
| Event flag `ctrlKey` | bit value 2 |
| Event flag `altKey` | bit value 4 |
| Event flag `metaKey` | bit value 8 |
| Event flag `isComposing` | bit value 16 |
| Event flag `checked` | bit value 32 |
| Event field `version` | offset 0, u16 |
| Event field `kind` | offset 2, u16 |
| Event field `handler` | offset 4, u32 |
| Event field `target` | offset 8, u32 |
| Event field `seq` | offset 12, u32 |
| Event field `flags` | offset 16, u32 |
| Event field `reserved` | offset 20, u32 |
| Event field `scroll_top` | offset 24, f64 |
| Event field `scroll_height` | offset 32, f64 |
| Event field `client_height` | offset 40, f64 |
<!-- generated:view-layout:end -->

**Operations.**

<!-- generated:view-opcodes:begin (checked by crates/varta-common/tests/embedding_documentation/view.rs) -->
| Opcode | Operation | Operands (wire order) |
| --- | --- | --- |
| 1 | `create_element` | node (u32 id), tag (u16 code + 2 pad) |
| 2 | `create_text` | node (u32 id), text (string) |
| 3 | `set_text` | node (u32 id), text (string) |
| 4 | `set_attribute` | node (u32 id), attribute (u16 code + 2 pad), text (string) |
| 5 | `remove_attribute` | node (u32 id), attribute (u16 code + 2 pad) |
| 6 | `set_class` | node (u32 id), text (string) |
| 7 | `set_style` | node (u32 id), property (u16 code + 2 pad), text (string) |
| 8 | `remove_style` | node (u32 id), property (u16 code + 2 pad) |
| 9 | `insert` | parent (u32 id), node (u32 id), before (u32 id) |
| 10 | `remove` | node (u32 id) |
| 11 | `listen` | node (u32 id), event (u16 code + 2 pad), handler (u32 id), options (u32) |
| 12 | `unlisten` | node (u32 id), event (u16 code + 2 pad) |
| 13 | `set_value` | node (u32 id), text (string), base_seq (u32) |
| 14 | `set_checked` | node (u32 id), checked (u32), base_seq (u32) |
| 15 | `command` | node (u32 id), command (u16 code + 2 pad), argument (f64) |
| 16 | `stylesheet` | sheet (u32 id), text (string) |
| 17 | `set_title` | text (string) |
<!-- generated:view-opcodes:end -->

**Elements.**

<!-- generated:view-tags:begin (checked by crates/varta-common/tests/embedding_documentation/view.rs) -->
| Code | Name |
| --- | --- |
| 1 | `div` |
| 2 | `span` |
| 3 | `p` |
| 4 | `h1` |
| 5 | `h2` |
| 6 | `h3` |
| 7 | `ul` |
| 8 | `ol` |
| 9 | `li` |
| 10 | `header` |
| 11 | `nav` |
| 12 | `main` |
| 13 | `section` |
| 14 | `footer` |
| 15 | `strong` |
| 16 | `em` |
| 17 | `code` |
| 18 | `pre` |
| 19 | `br` |
| 20 | `button` |
| 21 | `a` |
| 22 | `form` |
| 23 | `label` |
| 24 | `input` |
| 25 | `textarea` |
| 26 | `select` |
| 27 | `option` |
| 28 | `dialog` |
| 29 | `progress` |
| 30 | `img` |
<!-- generated:view-tags:end -->

**Attributes.**

<!-- generated:view-attributes:begin (checked by crates/varta-common/tests/embedding_documentation/view.rs) -->
| Code | Name |
| --- | --- |
| 1 | `id` |
| 2 | `type` |
| 3 | `name` |
| 4 | `value` |
| 5 | `placeholder` |
| 6 | `disabled` |
| 7 | `readonly` |
| 8 | `maxlength` |
| 9 | `autofocus` |
| 10 | `selected` |
| 11 | `for` |
| 12 | `href` |
| 13 | `src` |
| 14 | `alt` |
| 15 | `tabindex` |
| 16 | `max` |
| 17 | `role` |
| 18 | `aria-label` |
| 19 | `aria-labelledby` |
| 20 | `aria-describedby` |
| 21 | `aria-live` |
| 22 | `aria-expanded` |
| 23 | `aria-selected` |
| 24 | `aria-current` |
| 25 | `aria-busy` |
| 26 | `aria-invalid` |
| 27 | `aria-hidden` |
<!-- generated:view-attributes:end -->

**CSS properties.**

<!-- generated:view-css-properties:begin (checked by crates/varta-common/tests/embedding_documentation/view.rs) -->
| Code | Name |
| --- | --- |
| 1 | `display` |
| 2 | `width` |
| 3 | `height` |
| 4 | `min-width` |
| 5 | `min-height` |
| 6 | `max-width` |
| 7 | `max-height` |
| 8 | `margin` |
| 9 | `margin-top` |
| 10 | `margin-right` |
| 11 | `margin-bottom` |
| 12 | `margin-left` |
| 13 | `padding` |
| 14 | `padding-top` |
| 15 | `padding-right` |
| 16 | `padding-bottom` |
| 17 | `padding-left` |
| 18 | `gap` |
| 19 | `row-gap` |
| 20 | `column-gap` |
| 21 | `flex-direction` |
| 22 | `flex-wrap` |
| 23 | `justify-content` |
| 24 | `align-items` |
| 25 | `align-self` |
| 26 | `flex-grow` |
| 27 | `flex-shrink` |
| 28 | `flex-basis` |
| 29 | `grid-template-columns` |
| 30 | `grid-template-rows` |
| 31 | `grid-column` |
| 32 | `grid-row` |
| 33 | `position` |
| 34 | `top` |
| 35 | `right` |
| 36 | `bottom` |
| 37 | `left` |
| 38 | `z-index` |
| 39 | `overflow` |
| 40 | `border` |
| 41 | `border-width` |
| 42 | `border-style` |
| 43 | `border-color` |
| 44 | `border-radius` |
| 45 | `background-color` |
| 46 | `color` |
| 47 | `opacity` |
| 48 | `outline` |
| 49 | `outline-width` |
| 50 | `outline-style` |
| 51 | `outline-color` |
| 52 | `outline-offset` |
| 53 | `font-family` |
| 54 | `font-size` |
| 55 | `font-weight` |
| 56 | `font-style` |
| 57 | `line-height` |
| 58 | `text-align` |
| 59 | `text-decoration` |
| 60 | `white-space` |
| 61 | `text-overflow` |
| 62 | `overflow-wrap` |
| 63 | `cursor` |
| 64 | `transform` |
| 65 | `transition-property` |
| 66 | `transition-duration` |
| 67 | `transition-timing-function` |
| 68 | `transition-delay` |
<!-- generated:view-css-properties:end -->

**Events.**

<!-- generated:view-events:begin (checked by crates/varta-common/tests/embedding_documentation/view.rs) -->
| Code | Name |
| --- | --- |
| 1 | `click` |
| 2 | `input` |
| 3 | `change` |
| 4 | `submit` |
| 5 | `keydown` |
| 6 | `focusin` |
| 7 | `focusout` |
| 8 | `scroll` |
| 9 | `cancel` |
| 10 | `close` |
<!-- generated:view-events:end -->

**Commands.**

<!-- generated:view-commands:begin (checked by crates/varta-common/tests/embedding_documentation/view.rs) -->
| Code | Name |
| --- | --- |
| 1 | `focus` |
| 2 | `blur` |
| 3 | `select` |
| 4 | `scroll_into_view` |
| 5 | `scroll_to` |
| 6 | `show_modal` |
| 7 | `close` |
<!-- generated:view-commands:end -->

## Entry-point limits

<!-- generated:embedding-entry-limits:begin (checked by crates/varta-common/tests/embedding_documentation.rs) -->
| Contract limit | Value | Source constants |
| --- | --- | --- |
| Tool contract / invocation ABI version | 1 / 1 | `TOOL_CONTRACT_VERSION`, `TOOL_INVOCATION_ABI_VERSION` |
| Tool catalog bytes | 262144 | `MAX_TOOL_CATALOG_BYTES` |
| Tools / parameters per tool | 64 / 16 | `MAX_TOOLS`, `MAX_TOOL_PARAMETERS` |
| Tool or parameter name bytes / description bytes | 64 / 2048 | `MAX_TOOL_NAME_BYTES`, `MAX_TOOL_DESCRIPTION_BYTES` |
| Tool input / output JSON bytes | 65536 / 1048576 | `ToolLimits::default()` |
| Tool string bytes / memory bytes | 1048576 / 8388608 | `ToolLimits::default()` |
| Tool fuel / host calls / deadline | 1000000 / 1024 / 5000 ms | `ToolLimits::default()` |
| HTTP contract version / metadata bytes | 1 / 8192 | `http::VERSION`, `http::MAX_METADATA_BYTES` |
| HTTP headers / header bytes / target bytes / body bytes | 32 / 4096 / 1024 / 8192 | `http::MAX_*` |
| HTTP header / connection timeout | 2 s / 10 s | `http::HEADER_SECONDS`, `http::CONNECTION_SECONDS` |
| Live continuation frames | 64 | `frame::MAX_LIVE_FRAMES` |
<!-- generated:embedding-entry-limits:end -->

## Security obligations

A host that runs Varta modules MUST:

1. **Deny by default.** An operation without a matching grant fails before any
   OS, network or storage call. Omitted ids, `false` and empty lists deny.
2. **Check before effect.** Validate input and authorize the concrete resource
   before opening, connecting, reading or writing anything.
3. **Match the operation exactly.** Compare full `namespace.operation` ids; never a
   prefix or a namespace.
4. **Intersect, never widen.** With a host ceiling, both the package request and the
   ceiling must accept each concrete resource; an empty ceiling denies.
5. **Validate all guest-derived input.** Bounds-check every pointer/length pair,
   cap lengths, validate encodings, reject traversal and resolve paths through
   pinned roots without following links.
6. **Treat guest integers as data.** No pointer, handle, token or serial from the
   guest is ever authority.
7. **Enforce budgets** and report honestly which ones it cannot enforce (below).
8. **Keep helpers private.** Never expose `copy_string`, `retain_string`,
   `allocate`, `retain` or `varta_resume` to untrusted callers.
9. **Fail closed** on unknown sections, versions, profiles, imports and catalog
   entries.

### Budget guarantees

| Budget | Reference host (Wasmtime) | JavaScript host today |
| --- | --- | --- |
| Memory | Store limiter | Module-declared maximum, checked before instantiation (validation step 4) |
| Fuel (instructions) | Engine fuel metering | **Not enforceable**: JavaScript engines expose no instruction metering; a host MUST state that fuel is not enforced. The accepted design adds a deterministic metering transform at browser packaging ([decision 5](development-plan.md#accepted-decisions-2026-10-29)), not implemented yet |
| Deadline | Epoch interruption inside guest code | Checked at host calls only; a guest loop without host calls runs until the embedder terminates it (in a browser: `Worker.terminate()`) |
| Host calls, output | Counted per call | Counted per call (enforceable) |
| Stack | Engine stack limit, trap | Engine stack limit, `RangeError`, treat as a trap |

## What is unstable

The following are **not** part of this contract yet. A third-party host MUST refuse
modules that need them unless it reproduces the reference host bit-exactly and
accepts that the format may change in a later profile:

- value layouts of records, arrays, dictionaries, unions, errors, promises,
  functions and frames (value ABI 2 and later), and therefore every layout-bound
  pure import: `json_*`, `value_*`, `union_*`, `primitive_*`, `bigint_*` and the
  exception renderers;
- the exact text of `number_to_*` formatting and parsing beyond the ECMAScript
  algorithms the profile sections cite;
- the structural rules of `varta.functions` beyond the
  [normative minimum](#function-and-frame-catalogs);
- HTTP service v1 for third-party hosts;
- diagnostic and log text, and the order of validation failures;
- the [view protocol draft v0](#view-protocol-draft-v0-unstable) and every other
  planned interface: long-lived instances, host-to-guest events, timers, async I/O
  and browser capability mappings.

Tool invocation ABI v1, the status codes, the string handle and host-buffer rules
and the call protocol are stable for `checked-i32-utf8-v1` and keep their meaning;
a change requires a new version identity.

## Proposed changes

Not normative yet; each needs its own change to the engine, CLI or contract.

| Topic | Today (normative) | Proposed change | What would change |
| --- | --- | --- | --- |
| `logs` in machine envelopes | Reference line text with the `varta log: ` / `varta env: ` prefixes and failure diagnostics | Raw guest lines only (the logged text, `name=value`, file content); the prefix only in human CLI output; diagnostics in a separate `diagnostics` array | Runtime output capture (`RunOutput` and `ToolInvocationOutput` keep unprefixed records with a kind), the CLI and runtime companion presentation, smoke and runtime tests that assert prefixed lines, and this document; machine envelopes would change meaning, so it needs a new envelope version |
| Output budget | Charged on the reference line text, including prefixes and diagnostics | Charge guest bytes only (UTF-8 bytes of the guest's text plus one per line); bound diagnostics by a separate host budget | The runtime's output accounting and its tests; exhaustion points of existing programs would move, so it applies to a new profile or envelope version |
| Runtime companion failure envelope | No `diagnostics` member | Same envelope as the development CLI, with `"diagnostics":[]` | `varta-launcher` JSON error output and its tests |
| `varta.functions` structure | Normative minimum here; full rules only in code | Publish the full catalog schema and structural rules in this document, generated from the registry | Documentation and a drift test only |
| Approximated Math results | Platform `f64` library, last bits host-dependent | Bit-identical results on every host by linking one pure-Rust implementation into the guest (the accepted thin-host decision) | A new profile whose Math imports are guest code |
