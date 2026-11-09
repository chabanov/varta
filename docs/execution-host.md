# Bounded local native HTTP execution host v1

This is a separate source-free execution boundary, not a compiler service or an
extension of browser preview authority. The first target is a short-lived native
HTTP service. Native GUI windows, persistent deployment, private resource
sessions and stable application routing need separate policies. Cloud owns
owner/project checks, durable artifacts, release pointers, data and CAS/fences.

## Authority

`varta execution-host --ceiling /absolute/ceiling.json` reads a distinct
runtime-only `VARTA_EXECUTION_TOKEN` (32 random bytes, lowercase hex). Only a
literal loopback control listener is exposed. Exact Host and one Bearer
credential are required; Origin, Cookie, query and absolute-form requests are
refused. No CORS. Credentials never enter source, Wasm, argv, artifact bytes or
status. Trusted execution uses a private pinned copy of this CLI; runtime and
launcher dependencies stay compiler-free.

The operator ceiling has exact fields:

```json
{"schemaVersion":1,"listen":"127.0.0.1:8803","maxRuns":2,"maxImports":4,"maxRunSeconds":300,"importSeconds":60,"retentionSeconds":300,"startupDeadlineMs":15000,"validationDeadlineMs":15000,"cpuSeconds":30,"addressSpaceBytes":8589934592,"budgets":{}}
```

All guest budgets are the existing production ceilings or lower. Individual
workers retain the existing native CPU/address-space enforcement (at most 30 CPU
seconds and 8GiB growth each), with parent-lifetime cancellation. This is
neither an OS sandbox nor a total-RSS bound. At most two execution processes and
one validation process exist; maximum configured virtual-growth allowances
therefore sum to 24GiB, including Wasmtime's virtual-memory reservations, not
physical RAM. The operator may lower every limit. Output is not exposed or
accumulated.

Policy `loopback-http-v1` accepts only checked-f64-utf16-v40, HTTP contract v2
and an exact `127.0.0.1:0` or `[::1]:0` listener. The only requested grant is
precisely that `net.listen` endpoint. No wildcard listener, external address,
filesystem, KV, environment, outbound network, browser session, TLS files or
stored consent is admitted. The host supplies an explicit matching ceiling,
lower budgets and one HTTP request worker. Listening is loopback-only and not
authenticated app routing; this policy is not a private-data service.

## Immutable import

`GET /v1/info` returns schemaVersion, hostSha256, target `native-http`,
artifactPolicy `native-http-v1`, policyId `loopback-http-v1` and effective
limits. The client pins the authenticated executable identity before enqueue.

Import uses the same bounded descriptor and binary chunk framing as preview:
`POST /v1/artifacts` + Idempotency-Key (16–128 ASCII alphanumeric or `._:-`):

```json
{"schemaVersion":1,"hostSha256":"<pin>","artifactSha256":"<catalog SHA>","files":[{"path":"app.manifest.json","sha256":"<file SHA>","bytes":123},{"path":"app.wasm","sha256":"<file SHA>","bytes":456}]}
```

Only these two sorted paths exist. At most 8MiB total, 8192-byte binary chunks:
`PUT /v1/artifacts/{id}/files/{fileIndex}/chunks/{chunkIndex}`. Last chunk has
the exact remainder; empty files have zero chunks. Equal-byte retry returns 204;
changed bytes conflict 409. `POST /v1/artifacts/{id}/seal` begins one bounded
validator, returns 202. `GET /v1/artifacts/{id}` observes
`uploading|sealing|ready|cancelled|expired|failed`. DELETE cancels/fences an
unsealed import. Ready imports cannot be cancelled; their original lifetime
never slides.

Status fields are
`{id,hostSha256,requestSha256,artifactSha256,state,error,expiresAtMs}`. Per-file
SHA and the existing sorted catalog SHA domain `varta-toolchain-artifact-v1` +
NUL / LEu64 path length / path / LEu64 bytes length / bytes are verified
independently. The native artifact ABI and full runtime validation are checked
in a bounded process without instantiating/main/binding. Imported data and
receipts remain ephemeral. Source and native machine code are never admitted.
Compiler provenance is retained by Cloud, not inferred from hashes.

## Start, readiness and stop

`POST /v1/runs` + stable Idempotency-Key:

```json
{"schemaVersion":1,"hostSha256":"<host pin>","compilerSha256":"<Cloud build pin>","artifactSha256":"<sealed catalog SHA>","target":"native-http","policyId":"loopback-http-v1","ttlSeconds":180}
```

Reply 202 / GET `/v1/runs/{id}`:
`{id,hostSha256,compilerSha256,requestSha256,artifactSha256,state,address,error,expiresAtMs}`.
States are `starting|ready|stopping|stopped|expired|exited|failed`. Address is
null until a trusted runtime Listening event; it is a literal loopback socket
only. The compiler digest is immutable caller-supplied provenance, not host
attestation. Same key/body replays before missing-artifact and busy checks.
Changed body conflicts. A new missing artifact returns 404 without creating a
run receipt.

A fresh process receives immutable bytes over bounded framed stdin, inherits no
service environment, and revalidates before execution. It runs startup once and
uses existing isolated request instances. A startup trap, early exit, CPU/memory
failure or missing Listening event by the startup deadline fails the run. Guest
console text cannot forge lifecycle events and is not returned in status.

`ready` means checked startup completed and the listener bound; it is not a
semantic health assertion. Cloud must use a fresh status and any application
health check it requires before atomic release promotion. Stop/expiry fences
late readiness. `POST /v1/runs/{id}/stop` returns `stopping` until the OS
process has been killed and reaped, then terminal `stopped` (or `expired` for
TTL). The address is revoked on stopping; terminal state is never published
while the child lives. No automatic restart. Successful natural service exit is
`exited`; traps are `failed`. A request trap retains the existing per-request
isolation guarantee.

`POST /v1/runs/cancel-by-key` body `{schemaVersion:1,requestId:"<start key>"}`
stops an accepted run or creates a bounded pre-create tombstone. Lost202
recovery and Cloud cancellation fences must reconcile by the same key. Host
restart/receipt expiry does not permit Cloud to forget its durable terminal
decision. TTL starts at admission, includes startup, and never renews through
polling/readiness.

If OS termination/reaping cannot be confirmed, the run remains `stopping`, new
admissions fail closed and controlled shutdown reports failure. A failed
validator reap retains its process slot. Neither status nor cleanup claims
success without a confirmed process wait. Validation deadlines include queueing
and input writes.

The host retains at most 128 total imports/run receipts/tombstones, 64MiB bundle
data, 4 uploads and 2 executions. Validation is single-flight; bodies,
connections, IPC and deadlines are bounded. Controlled shutdown cancels imports,
kills/reaps all execution and validation children, closes the listener and
removes the private executable copy. Native execution has no browser activation
tickets or cookie API.

Implementation and targeted acceptance pass: native HTTP replies, per-request
trap isolation, startup trap refusal, fixed TTL, natural service exit,
stop/reap, terminal replay, cancellation-before-create and parent-loss
termination. Shared preview/toolchain callers pass targeted regression checks.
One final full gate passes all 14 commands, 4716 Rust tests (26 ignored), 195
Node and all three compatibility inventories on unchanged inputs. See the [dated
receipt](evidence/2026-11-08-execution-host-verification.json). Existing
immutable receipts retain their original meaning.