# Local preview and browser launch host v1

This separate trusted host activates a checked browser artifact. Compilation,
source loading, native OS/service execution and deployment are outside this
contract. Runtime/launcher retain no compiler dependency. Browser execution uses
the official metered Wasm Worker and renderer on a different hostname from
Studio and API. Cloud owns authenticated owner/project records, CAS/fences and
consent.

## Authority and setup

`varta preview-host --ceiling /absolute/ceiling.json` reads runtime-only
`VARTA_PREVIEW_TOKEN` (32 random bytes encoded as 64 lowercase hex). It is
distinct from tooling/UARP credentials and never enters source, browser, argv or
evidence. The executable is privately copied/pinned. Bounded validation workers
reuse the trusted native process resource/lifetime guard; uploaded code is never
executed by the host process or validation worker.

Example operator ceiling (all fields exact):

```json
{
  "schemaVersion": 1,
  "controlListen": "127.0.0.1:8801",
  "publicListen": "[::1]:8800",
  "protectedOrigins": ["http://127.0.0.1:8787", "http://127.0.0.1:8788"],
  "maxImports": 4,
  "maxRuns": 4,
  "maxRunSeconds": 300,
  "importSeconds": 60,
  "retentionSeconds": 300,
  "validationDeadlineMs": 15000,
  "cpuSeconds": 10,
  "addressSpaceBytes": 4294967296,
  "budgets": {}
}
```

Both listeners are explicit loopback HTTP addresses in this local version. The
public hostname MUST differ from every protected hostname (ports do not isolate
cookies). Protected origins cannot be empty. No wildcard/DNS resolution or
source-owned origin/policy enters admission. 127/8 and ::1 loopback are
potentially trustworthy for browser secure-context APIs under the [Secure
Contexts specification](https://www.w3.org/TR/secure-contexts/). This is not
production HTTPS or a platform-matrix claim. Operator budgets may only lower the
existing production budgets; no engine limit is raised.

Control routes require exact Host and one Bearer credential; Origin, Cookie,
absolute targets and queries are refused. No CORS. Bodies, connections and
caches are bounded. Responses are no-store/nosniff; control data is never an
HTML preview. The public origin never accepts that service credential.

## Wire

JSON uses camelCase, schemaVersion 1, exact fields and 64 lowercase hex digests.
`Idempotency-Key` is 16–128 ASCII letters/digits or `._:-`; application UUIDs
work. Identifiers/keys are opaque correlations, never user/project identities.
Every admission request pins `hostSha256` obtained from authenticated `GET
/v1/info`. Info also returns target `browser`, bundlePolicy
`official-browser-v1`, policyId `deny-all-v1`, publicOrigin and limits. Limits
exact fields: maxFiles=64, maxBundleBytes=8388608, chunkBytes=8192,
maxImports/maxRuns from ceiling, maxReceipts=128, maxRetainedBytes=67108864,
maxRunSeconds/importSeconds/ retentionSeconds/validationDeadlineMs from ceiling,
ticketSeconds=30, leaseMs=3000 and pollMs=1000. Errors are exactly
`{error:string}` with fixed codes. There is no compiler-host dependency.

### Import immutable artifact bytes

`POST /v1/bundles` with a stable Idempotency-Key:

```json
{"schemaVersion":1,"hostSha256":"<info pin>","artifactSha256":"<catalog SHA>","files":[{"path":"app.wasm","sha256":"<file SHA>","bytes":123}]}
```

Files must be strictly sorted ASCII paths, no hidden/traversal/escaping/case
collisions. Up to 64 files and 8MiB total. Reply 202:
`{id,hostSha256,requestSha256,artifactSha256,state,error,expiresAtMs}`. States:
`uploading`, `sealing`, `ready`, `cancelled`, `expired`, `failed`. `GET
/v1/bundles/{id}` returns that status.

`PUT /v1/bundles/{id}/files/{fileIndex}/chunks/{chunkIndex}` sends
application/octet-stream. Chunks are 8192 bytes except the exact final
remainder; empty files have zero chunks. A same-byte retry returns 204; changed
bytes 409. No uploaded path maps to the filesystem. `POST /v1/bundles/{id}/seal`
returns 202 and starts one bounded validator. Missing bytes 409. `DELETE
/v1/bundles/{id}` cancels an unsealed/sealing import and fences late results; a
ready catalog is retained until its original expiry and cannot be silently
replaced.

Seal independently verifies per-file SHA and the existing sorted catalog domain
`varta-toolchain-artifact-v1`+NUL / LEu64 path length / path / LEu64 data length
/ bytes. It validates app.wasm and app.manifest.json using full engine
admission, requires v40 and no requested capabilities, filesystem roots, session
binding or HTTP policy, and compares every uploaded file to the canonical
official bundle regenerated from that original. Arbitrary HTML/JS/private
image/renderer is refused even with a self-consistent catalog hash. Preparation
never calls main. The runtime delivery is then freshly regenerated with explicit
deny-all host ceiling and operator budgets. It never runs uploaded HTML/JS or
inherits Studio session, environment, network, KV or permission persistence.

### Create, activate and stop a run

`POST /v1/runs` + stable Idempotency-Key:

```json
{"schemaVersion":1,"hostSha256":"<info pin>","bundleSha256":"<sealed catalog SHA>","target":"browser","policyId":"deny-all-v1","ttlSeconds":120}
```

Reply 202 and `GET /v1/runs/{id}`:
`{id,hostSha256,requestSha256,bundleSha256,state,guestState,error,expiresAtMs}`.
States: `ready`, `active`, `stopped`, `expired`, `failed`. Guest state is
`pending`, `ready`, `closed` or `failed`; it is independent of asset readiness.
Ready means prepared/launchable, never a claim that main/frame rendering passed.
Active means a browser activation occurred. Fixed error codes contain no guest
source, credentials or arbitrary diagnostics.

`POST /v1/runs/{id}/tickets` (control credential, ready run only) returns
`{launchUrl,expiresAtMs}`. One-use ticket expires within 30 seconds and the
run's original TTL; a fresh ticket invalidates an older unconsumed one. No
ticket ever appears in create/status/replay or a durable application row.
launchUrl is the public origin root with ticket only in the URL fragment. Do not
log it.

The engine-owned public activation page removes the fragment before network I/O,
then submits it to `/activate` with exact public Origin. The host consumes it
and sets only a random HttpOnly SameSite=Strict cookie scoped `/runs/{id}/`,
never a UARP/session/tooling credential, then the page opens the regenerated run
index. Cold loading/parsing the trusted loader has at most 30 seconds to make
its first claim, starting no guest beforehand. The first trusted bootstrap claim
gets a private page lease; duplicate claims cannot create another Worker for the
same run. Only that page may renew/report lifecycle. Cookie and lease values are
bounded RAM-only authority for this run. Assets are no-store, nosniff,
same-origin CORP, restrictive CSP, no framing/forms or external connections.
Only the official integrity-pinned loader starts Wasm.

`POST /v1/runs/{id}/stop` is terminal/idempotent. Stop/TTL revoke assets and
leases immediately. The trusted loader closes its Worker on failed lifecycle
poll or local lease expiry (3-second granted leases, polling each second).
Client callbacks depend on browser scheduling; a suspended/modified browser is
not a synchronous remote OS process kill. Stop is immediate server-side
authority revocation, not a claim of instantaneous browser termination.
Readiness/lifecycle polling never extend the original run TTL. Guest main cannot
renew a lease or see the control credential/cookie/page lease.

`POST /v1/runs/cancel-by-key` body `{schemaVersion:1,requestId:"<stable key>"}`
stops an existing run or creates a cancellation tombstone before create arrives.
Reply `{requestId,stopped:true}`. A later start under a tombstone conflicts 409
`cancelled_request`, never starts again. Same start key/input replays the
original terminal result before bundle-availability and busy checks; changed
input 409 `idempotency_conflict`. Syntactic/schema/key/host-pin validation still
comes first. A new start with no sealed bundle returns 404 `bundle_not_found`,
creating no receipt. A host pin mismatch returns409 `host_mismatch`; capacity
503 `busy`. Receipts/tombstones expire only after the host retention interval;
application durable terminal state must prevent resubmission after host
restart/expiry. Run restart needs a new deliberate requestId. Cloud fences must
also stop late accepted runs after cancellation.

## Bounds and acceptance

Max 4 imports, 4 live runs, 128 receipts, 64MiB registry-retained bundle data.
Validation IPC is separately bounded to 12MiB input / 24MiB output and one
worker; connection references may retain an already admitted immutable response
briefly. Import TTL<=60s, run TTL<=300s, receipt TTL<=300s; one validator at a
time, wall<=30s, CPU<=30s, address-space growth512MiB..8GiB. Worker resource
enforcement fails closed; it is not an OS sandbox or hard RSS promise. Shutdown
cancels/reaps validators, closes both listeners and revokes all runs; restart
restores none. No unbounded log/event queue or URL/file proxy is exposed.

Root native/real Chrome and Cloud durable-artifact/native/real Chrome acceptance
pass. The sole full attempt failed on four outer test-host readiness waits;
test-only wait corrections and all remaining checks pass. The [dated
receipt](evidence/2026-11-08-preview-host-verification.json) keeps the original
failed full separate from successful scoped completion. Earlier
toolchain/artifact receipts remain immutable history.