# Varta development plan

## Product direction

Varta targets contract-driven, typed TS/JS applications whose supported source
compiles to portable Wasm and ships with a controlled execution environment.
Developers should be able to build desktop applications, data services and robot
software, then explicitly expose selected application operations to external AI
agents through MCP. The application owns its state and business rules; UI, API
and AI callers enter the same command/query layer.

This is the target direction, not a claim that these platform features already
exist. The current implementation is the compiler/runtime foundation described
in [architecture.md](architecture.md): a checked TypeScript subset, local modules,
strings and reclamation, captured host results, capability policies, native
packaging, primitive Tool Contract v1 and offline discovery. Broader Application
Contracts, validated external invocation, a long-lived application service, MCP,
package signatures, GUI/database/hardware adapters and full JS/npm compatibility
are not implemented yet.

### Architecture commitments

- Define the supported TS/JS language profile explicitly. Grow it through concrete
  application requirements; unsupported dynamic behavior continues to fail clearly.
- Keep source compilation separate from interface publication. An ordinary source
  export does not automatically expose an AI tool. Developers explicitly choose
  which operations form the public tool surface.
- Distinguish **Application Contract** (commands, queries, data, errors, state,
  revisions and lifecycle) from **Tool Contract** (the AI-exposed subset, names,
  descriptions, schemas, required effects and limits). Both share one definition
  of the operation rather than duplicating business logic per transport.
- Preserve runtime boundary checks. External/AI arguments and results require
  bounded validation even when their TS definitions were checked at build time.
  Schemas describe data; host policy separately authorizes concrete effects.
- Treat artifacts, callers and AI-generated arguments as separate trust inputs.
  Effective authority must satisfy package requests, host ceilings and caller
  permissions. Discovery and tool annotations never grant execution rights.
- Ship public contract metadata together with executable code. Validate its link
  to real handlers. Signing must bind code, contracts and requested permissions;
  a recomputable checksum alone does not establish a trusted publisher.
- Keep the portability promise precise: portable Wasm plus compatible host
  interfaces, with native launchers built per supported OS/CPU. Do not promise
  one native executable for every platform or hard real-time behavior.
- Avoid including source implementations in release tool catalogs. Names, schemas
  and descriptions are intentionally public. Wasm remains analyzable/editable;
  compilation does not guarantee secrecy of business logic, and signature checks
  require a trusted verifier. Credentials belong to trusted host-side providers.

### Reference application profiles

| Profile | Example contract operations | Required behavior |
| --- | --- | --- |
| Desktop editor | Inspect a document, rename or patch it, update selection | Connect to the existing application instance; preserve revisions, history/undo and UI consistency |
| Data service | Find records, change an allowed business status | Enforce identity/data scopes, transactions, bounded queries and stable errors |
| Robot software | Read telemetry, start a mission, request a stop | Validate state/command limits; define duplicate, timeout and disconnect behavior; keep the local safety controller independent of AI transport |

The selected first demonstration is a **headless document application**: a title,
revision and small command/query interface, controlled by two test clients and
then an external MCP client. It exercises shared state and conflicting changes
without requiring a GUI toolkit, database service or physical hardware. This is
the agreed first application profile, not a commitment to a GUI framework.

## Completed milestone: architectural hardening

The first milestone is a checked, bounded single-file TypeScript subset with
capability-controlled execution and one-file native packaging. The implemented
contract is in [architecture.md](architecture.md).

The October 2026 architecture audit is addressed as follows:

| Audit finding | Implemented change | Regression evidence |
| --- | --- | --- |
| Untrusted native deserialization | Native AOT entry points removed; portable Wasm only | Rejection through runtime and all artifact commands |
| Filesystem boundary and check/open race | Component-aware scopes and pinned no-follow descriptors | Allowed file, sibling, traversal, final/intermediate/internal symlink cases |
| Malformed manifest grants expand rights | One strict schema and operation-specific grant types | Wrong/missing fields and unknown operations rejected |
| HTTP request injection | Parsed authorized URLs and bounded HTTP library | CR/LF denied before I/O, normalization and authority tests |
| Unbounded resources | Store memory limits, fuel, epoch deadline, I/O/output/call budgets | Infinite loop, guest memory.grow, output and oversized file tests |
| Function type order mismatch | Signature interning shared with emission | Mixed-arity functions execute correct results |
| Silent semantic changes | Lexical scopes, typed IR, structured branch labels | Shadowing, nested switch, default/fallthrough, continue and top-level-effect rejection |
| Artifact validation misses invalid code | Full Wasm/ABI validation before publish and every artifact command | Invalid operand stack and missing ABI globals rejected |
| Divergent check/build/dev | Shared project pipeline and deterministic internal subset typing | Identical errors for all three commands |
| Stale string length and sentinel errors | Reusable result buffer, cleared result globals, status API | Success then denial, structured status, bounded output |
| Container marker collisions | Fixed versioned EOF footer with checked lengths | Marker literals survive native packaging; overflow lengths rejected |
| CLI reports success after failure | Central command error/exit handling | Run, inspect and validate return code 1 on bad input |

Additional hardening: source complexity is bounded before recursive parsing;
syntax errors return typed diagnostics instead of a dropped-diagnostic panic;
workspace code forbids unsafe; operation/signature metadata has one owner;
filesystem devices are not readable; proxies and redirects are disabled;
unsupported compat and HTTPS configurations fail explicitly.

Quality gates:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
cargo test -p varta --test smoke --locked
```

The CI workflow runs the gates on Linux and macOS. Tests are hermetic and use
synthetic sources, process-local environment values and temporary files; they do
not need a network service or external TypeScript checker.

## Completed follow-up: host policy ceilings

`run`, `dev`, `inspect`, `validate` and packaged launchers accept an explicit
`--policy` file. Runtime authorization requires both package and host filters;
empty ceilings deny all privileges and malformed files never fall back. File
access uses pinned host directory roots, including when a package supplies a
symlink as its own root. Inspection reports the two scopes without approximating
wildcard intersection. Host policies are not persisted into artifacts.

Regression coverage includes environment/path narrowing in both directions,
operation independence, scoped network authorization without network I/O, boolean
permissions, malformed/missing files, deny-all packaged execution and symlink-root
escapes. The public runtime and project services expose the same constrained-run
contract as the CLI.

## Completed follow-up: source diagnostics

The shared compiler source context preserves real entry filenames and precise
semantic ranges through expression/function IR. Human diagnostics show bounded
excerpts and carets; `check --json` exposes typed categories, UTF-8 byte offsets
and Unicode line/column ranges. Nested expression errors retain their original
locations. Virtual source paths are supported without file I/O, and naming a
source does not change its generated bytecode.

Regression coverage includes expected/actual type errors, nested calls, unsupported
annotations, Unicode/CRLF/BOM coordinates, parser/resource-limit errors, bounded
large-line excerpts, static-data overflow, config-relative entry paths and
consistent diagnostics across check/build/dev. Valid named-source modules are
instantiated and executed through Wasmtime as well.

## Completed follow-up: local modules

Named function imports/exports and aliases now link through an explicit,
deterministic dependency graph. Functions retain file-local scopes and source
diagnostics, imported calls retain their types, and dependencies are embedded
into portable/native builds. `check --json` exposes the graph for inspection.
Compiler embeddings supply a loader; project source loading stays within the
configuration directory through pinned no-follow descriptors. Cycles and
unsupported initialization/import forms fail explicitly. Module count, imports,
depth, total/per-file bytes and global function count are bounded.

Regression coverage includes shared dependencies, alias forwarding, recursive
library functions, private name isolation, imported type errors and static-data
diagnostics, deterministic bytecode, graph budgets, missing/private exports,
path escapes, symlinks, and execution after deleting sources. Imported host calls
obey both application grants and host ceilings. ABI and packaging are unchanged.

## Completed follow-up: retained string values

Strings now work in inferred/annotated local bindings, assignments, parameters,
explicitly annotated returns and dynamic host arguments across module boundaries.
Literal slices are static; host results are copied immediately into an immutable,
bounded execution arena. Saved values survive host-buffer reuse, nested calls and
reassignment. Full Wasm value types determine signature interning and local slots.
The raw host ABI, capability model and artifact versions remain unchanged.

Regression coverage includes Unicode/empty values, recursive and mixed-signature
functions, scope/control-flow slots, cross-module returns, native and Wasm runs,
multiple host arguments, dynamic environment/filesystem permission denials,
host ceilings, fuel exhaustion, and bounded heap behavior with additional
test fuel. String operators and reclamation are covered by the follow-ups below;
implicit string coercion remains unsupported.

## Completed follow-up: programmatic host errors

`varta.result.capture` now executes a direct privileged host call once and retains
an immutable typed `VartaResult<T>` containing `.ok`, `.status` and `.value`.
Values and failures stay paired across later host calls, aliases, reassignment,
recursive calls and module boundaries. String payloads keep their safe snapshot
ownership; the result itself uses a bounded two-value Wasm representation with no
record allocation. Function annotations, parameter limits, physical local slots
and multi-value returns are checked consistently. Shared editor declarations
document the API. Existing host imports, policies and artifacts remain compatible.

Regression coverage includes empty/false success, all five status codes, retained
Unicode payloads, nested call evaluation, readonly fields and type errors,
parameter budgets, identical check/build/dev rejection, Wasm/native execution,
host ceilings, denied network/process calls and authorized process exit. Resource
traps remain execution failures; capture does not introduce general exceptions.

## Completed follow-up: string comparison and concatenation

String `+` now creates immutable concatenated values, with empty-operand sharing,
left-to-right single evaluation and bounded allocation. Equality compares content;
ordering uses UTF-16 code units without normalization or locale collation.
Both operands must be strings. Host snapshots and concatenations share the same
checked arena; each string is bounded to 1 MiB of UTF-8 bytes. Comparisons require
no allocation, and pure string helpers do not affect saved or live host status.
Helper types/indices are resolved conditionally, including with the maximum user
function count. Host imports, capabilities, ABI and packaging are unchanged.

Regression coverage includes all eight comparisons across 484 pairs of empty,
NUL, ASCII, combining and BMP/supplementary Unicode samples against UTF-16 order;
nested/cross-module concatenation, retained host/captured values, operand evaluation
order, native/Wasm execution, diagnostic ranges, constructed resource denials,
per-value limits, bounded heap behavior, growth failures and comparison fuel exhaustion.

## Completed follow-up: string memory reclamation

Owned strings now use a bounded, nonmoving mark/sweep heap with block reuse,
splitting and adjacent-free coalescing. Collection runs before memory growth;
dead trailing blocks can lower the logical heap end. Committed Wasm
pages stay available for reuse until the instance is dropped. Static data and the
host result buffer remain separate. Host signatures, capabilities and artifacts
stay compatible.

Compiler-managed roots preserve bindings, aliases, string result payloads,
expression operands and call arguments. Scope exit, overwrite, `break`, `continue`
and return remove roots at the appropriate boundary. A fixed 64 KiB root stack
bounds frames and temporaries; root overflow, all-live heap exhaustion and failed
growth remain controlled execution failures. Collection and bookkeeping consume
guest fuel. Bindings are conservatively retained to scope exit; compaction and
last-use optimization are not implemented.

Regressions cover copying more bytes than fit in the heap with bounded committed
memory, nested expression/function transfers, recursive captured results,
middle-block aliases, free-block splitting/coalescing, loop/switch exits, static
and host buffer isolation, per-value limits, live-set exhaustion, root capacity,
fuel, and real CLI/Wasm/native behavior with unchanged permission denials.

## Delivery sequence

The contract platform becomes the main development track. Bounded collections
remain planned, but no longer precede discovery and invocation merely to expand
the language. The initial boundary can use primitive guest parameters/results
with a host-side JSON envelope; that does not require general guest objects.
The names below are work packages, not CLI commands. C1 now specifies the
versioned tool metadata and low-level invocation ABI; later application and MCP
contracts remain to be designed.

### C1 — Tool Contract v1 and offline discovery (completed)

Entry-module `varta.tool({ name, description }, handler)` declarations now
publish explicitly exported functions. Checked primitive signatures supply named
parameters and results; metadata does not duplicate handwritten schemas.
Ordinary exports and dependency modules cannot silently publish tools.

- Shared strict types, limits and generated JSON schemas live in `varta-common`.
- Compiler lowering resolves the actual handler (including imported/re-exported
  bindings), derives transitive system requirements and emits a sorted catalog
  plus typed Wasm exports.
- Runtime validation binds metadata to actual Wasm export signatures and recomputes
  transitive effects. Malformed, duplicate, oversized, incompatible and inconsistent
  catalogs fail before execution. Indirect calls and Wasm start functions are
  outside tool invocation ABI v1.
- Project inspection returns public schemas, effects, limits and existing package
  permissions. It compiles/validates portable code without instantiating or running
  it. The CLI only presents that metadata.
- Contract v1, invocation ABI v1, host ABI v1, manifest schema v1 and native
  container format v1 are distinct. Existing no-tool applications keep working;
  tool metadata is inside the Wasm payload, so packaging requires no new footer.
- String copy/retain exports establish fresh-instance invocation ownership and are
  tested across input-buffer reuse and collection. An external JSON dispatcher is
  still C2 work. `domainErrors: []` explicitly leaves typed domain errors to C2.

Exit verified: compile a primitive string tool, inspect the same catalog in Wasm
and native packages after deleting sources, reject invalid catalogs/signatures/
effects, preserve permission denials and demonstrate that inspection does not run
privileged application code. No LLM connection or MCP server is implemented.

The complete implemented syntax, limits and ownership rules are in the
[tool contract](architecture.md#tool-contract-v1-and-offline-discovery).

### C2 — Validated invocation and a live application core (next)

Deliver a typed invocation path and a headless application whose state survives
agent/client connections and is shared by all entry points.

- Add a host API to invoke an explicitly published operation by ID with bounded
  input/output validation. Unknown fields, wrong types, invalid ranges and string
  limits fail before application effects; i32 number semantics remain explicit.
- Define stable contract/domain errors separately from host status codes, protocol
  errors and execution traps. Do not force revision conflicts into host I/O errors.
- Define application identity, startup/shutdown, state ownership, request identity,
  ordering, revision checks, cancellation and retry/idempotency semantics.
- Validate a proposed initial model: a long-lived trusted application service owns
  state and serializes mutations; fresh bounded Wasm invocations implement commands.
  The application's lifetime is independent of any agent or MCP connection.
  Decide this model explicitly before adding persistent guest memory or concurrency.
- Make string input/output transfer safe across invocation boundaries and collection.
  Host-owned state must never retain borrowed guest pointers after a call ends.
- Keep one command/query path for UI/API/MCP callers. Apply authorization and state
  preconditions at that path, then commit a mutation atomically. A stale revision
  yields a typed conflict; transport failure must not trigger an automatic write retry.
- Enforce per-call budgets and bounded application-level queues/accounting so that
  reconnecting or making repeated calls cannot reset all resource controls.

Exit: two test clients inspect and rename the same document through the same
service; state persists across connections, stale writes fail, unauthorized calls
have no effect, and malformed/trapping calls do not partially commit changes.
GUI rendering and general guest arrays/objects are outside this first demonstration.

### C3 — Artifact provenance and trusted loading

Deliver package verification before third-party distribution or treating tool
metadata as publisher-authenticated.

- Define trust roots, signed bytes, contract/code/permission binding and signature
  verification before invocation. Cover versions and payload lengths as well as
  code; changing the catalog or manifest must invalidate authenticity.
- Define publisher identity, key rotation/revocation, update and rollback rules.
  Keep signing credentials outside shipped artifacts and guest memory.
- Separate package authenticity from caller authorization and native OS signing.
  A valid signature cannot expand host grants, and an unsigned developer build
  must not silently acquire trusted-package status.
- Use explicitly trusted local development builds for earlier prototypes. Preserve
  safe portable Wasm validation and do not restore arbitrary native deserialization.

Exit: code, contract and permission tampering is rejected by a trusted loader;
trusted/unknown/revoked publishers are distinguishable, and host ceilings still
restrict a correctly signed application. AOT remains a later, separate design.

### C4 — Local MCP bridge to the running application

Deliver external-agent discovery and control of the C2 application through MCP.

- Pin the supported MCP revision(s) and transcript compatibility tests. Implement
  deterministic, bounded `tools/list` and validated `tools/call` from the existing
  catalog/dispatcher, with protocol metadata and errors appropriate to that revision.
- Start with stdio. Separate protocol output from application logs and enforce
  request size, queue, response size, timeout and cancellation limits.
- For an already-running desktop application, the stdio bridge connects through
  an authenticated local channel to a specific existing instance. Launching another
  bridge must not silently create a second document or divergent application state.
- Bind callers to the intended application/contract identity, expose the permitted
  catalog, and authorize every concrete invocation. Tool descriptions/annotations
  remain untrusted data and do not override host or caller policy.
- Test disconnect/reconnect, multiple agents, duplicate requests and changing
  document revisions. State belongs to the application, not the MCP session.

Exit: an external MCP client lists tools from a compiled app, reads a document,
renames it through the common command path and observes a conflict when another
client has changed it. No source code, embedded model or AI-provider SDK is needed
inside the Wasm module. No remote listener is enabled by this local milestone.

### C5 — Deployment profiles and remote access

- Desktop: select UI/rendering and event-loop adapters; route edits through the
  same commands and history/undo model used by tools. Choose a GUI framework only
  after the headless contract/control path works.
- Server: add database adapters with scoped credentials, transaction boundaries,
  concurrency limits and tenant/record authorization; keep secrets host-owned.
- Remote MCP: add authenticated Streamable HTTP with HTTPS, explicit listener
  configuration, authorization and network deployment tests before remote exposure.
- Robot: build a simulator first, then hardware adapters. Keep hard real-time and
  safety/interlock enforcement in the local controller; define command expiry,
  deduplication, progress and disconnect/stop behavior before physical deployment.
- Establish a supported OS/CPU/runtime matrix and packaging tests. Evaluate small
  devices against actual runtime/memory constraints rather than assuming support.

Exit criteria are profile-specific; desktop, server and robot support must not be
claimed merely because the shared Wasm and MCP layers run on a development host.

### Supporting language and runtime work

- Expand the language profile through the reference application: bounded records,
  arrays, JSON interoperability and enums only when their semantics/ownership are
  specified and the contract boundary needs them. Full JS/npm compatibility is
  a separate compatibility effort.
- String-containing collections require tracing their referenced values; the flat
  string collector must not be assumed sufficient for an object graph.
- Async/events require cancellation, task/handle lifetime, retained roots, scheduling
  and queue budgets before exposing promises or long-running guest operations.
- Continue model/differential tests, fuzzing and performance measurements of the
  compiler, collector, boundary schemas and artifact parser throughout these phases.

Every implementation milestone retains identical shared check/build/dev acceptance,
versioned documentation, positive and denial tests, bounded malformed-input handling,
Wasmtime validation/behavior coverage and the existing quality gates. Planned
features are described as planned until their exit criteria actually pass.

Protocol references for planning: [MCP tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
and [MCP transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports).
Choose and test exact compatibility targets when implementing C4; the catalog
and application core remain independent of transport/protocol revisions.

## Later milestones

- HTTPS deployment and asynchronous execution follow the C5 and supporting-work
  contracts above; neither is implied by the current synchronous HTTP client.
- Authenticated AOT: define who trusts the signer, which bytes are authenticated,
  and how engine version, target and CPU requirements are pinned. Never restore
  arbitrary native deserialization behind a filename or format marker.
- Package compatibility: only after module/type/memory semantics are stable;
  evaluate packages against a documented subset rather than promising Node parity.
- Production: fuzz schemas/containers/Wasm boundaries, expand platform coverage,
  isolate compilation and blocking host operations where hard OS-level resource
  isolation is required, and obtain an independent security review.
