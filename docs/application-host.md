# Local application host v1

This is the separate `application-host --ceiling <file>` contract. Current
acceptance evidence is tracked in [verification](verification.md). It preserves
`preview-host`/`deny-all-v1` and the original
`local-only-v1` source policy. No new guest API, import or ABI is introduced.
Production public hosting and Cloud active-release promotion are outside this
local contract. This first policy is local HTTP loopback, not public TLS
hosting or a stable application address.

## Explicit source policy

The tooling host can explicitly opt in with `applicationOrigins`, a sorted,
unique list of at most 16 canonical `http://localhost:<non-default-port>` origins.
Absent this field it accepts only the preceding policy. A new compile request
uses `dependencyPolicy:"app-data-v1"`, profile v40 and an additional
`application:{publicOrigin,access:"read"|"read-write"}`. The origin must be in
the operator ceiling. Source cannot supply arbitrary capabilities or host config.
Toolchain GET `/v1/info` preserves `dependencyPolicy:"local-only-v1"` and, only
when opted in, adds `applicationPolicy:{dependencyPolicy:"app-data-v1",
publicOrigins,environmentName:"VARTA_APP_DATA_URL",access:["read","read-write"]}`.

The same confined TS/TSX/text loader, source limits, environment-cleared compiler worker and
immutable artifact catalog remain in use; check/build never run initialization.

The manifest requests exactly:

- `env.get` for the one nonsecret name `VARTA_APP_DATA_URL`;
- `net.session` for `<publicOrigin>/runs/**`, GET for read, or GET/PATCH/POST for
  read-write, with only content-type visible from responses.

It requests no ordinary networking, filesystem, server session, KV or arbitrary
environment authority. A runtime ceiling cannot expand the empty request of
local-only-v1; such artifacts are not promoted to application authority.

## Operator ceiling

The new JSON wraps the existing bounded preview transport/admission ceiling:

```json
{
  "host": {
    "schemaVersion": 1,
    "controlListen": "127.0.0.1:8811",
    "publicListen": "[::1]:8810",
    "protectedOrigins": ["http://127.0.0.1:8787", "http://127.0.0.1:8788"],
    "maxImports": 4,
    "maxRuns": 4,
    "maxRunSeconds": 300,
    "importSeconds": 60,
    "retentionSeconds": 300,
    "validationDeadlineMs": 15000,
    "cpuSeconds": 15,
    "addressSpaceBytes": 4294967296,
    "budgets": {}
  },
  "publicOrigin": "http://localhost:8810",
  "nativeOrigin": "http://127.0.0.1:8787",
  "nativeControlTokenEnv": "VARTA_DATA_CHANNEL_CONTROL_TOKEN",
  "pathPrefix": "/private"
}
```

`VARTA_APPLICATION_TOKEN` is the separate hex64 control credential. The named
native control token is also hex64 and stays in trusted host RAM. The public
hostname must differ from every protected Studio/API hostname; a different port
is insufficient. The fixed advertised localhost port must match the public
listener. The native endpoint must be an explicitly protected loopback origin;
there is no DNS-selected or source-selected upstream. No credential enters
validation workers, artifacts, browser environment, status diagnostics or KV.
The prefix is an operator-selected literal namespace, not a Cloud business rule.

## Authenticated control wire

Control uses the existing exact-Host, no-Origin/no-Cookie Bearer channel. It
retains bounded JSON/binary reads, one validation worker, process AS/CPU/deadline
limits, cancel/reap, receipts, tombstones and aggregate byte bounds. Import uses
the unchanged catalog/hash/chunk protocol: POST `/v1/bundles`, PUT chunks, POST
seal and GET/DELETE import status. Admission reconstructs official browser files
from validated original Wasm/manifest and compares every byte. Custom JS, extra
authority, host binding/config and non-v40 profiles fail. App workers are a
separate hidden entry; the preview worker refuses app authority.

A trusted preparation worker may stage/verify the artifact before a short
request-bound run admission. Chunk upload has no guest execution and needs no
native parent proof; scope issuance/admission still require the application's
fresh ordinary-parent and owner checks. Do not put a complete large upload into
one bounded legacy HTTP handler turn or raise production budgets to accommodate
it. Retain immutable compiler/catalog correlation across both phases and
reprepare if the bounded catalog was evicted. Native request proofs must not be
persisted across requests.

GET `/v1/info` reports bundlePolicy `official-app-browser-v1`, policyId
`scoped-json-v1`, hostSha256, publicOrigin and the existing limits.

POST `/v1/runs` with a stable Idempotency-Key takes exactly:

```json
{
  "schemaVersion": 1,
  "hostSha256": "<pinned host>",
  "bundleSha256": "<admitted immutable catalog>",
  "target": "browser",
  "policyId": "scoped-json-v1",
  "ttlSeconds": 120,
  "delegationId": "<native id>",
  "bindingId": "<opaque binding>",
  "contextSha256": "<opaque context digest>"
}
```

The host verifies the native scope, exact binding/digest/prefix, original deadline
and sufficient native methods. It records that immutable descriptor with the
artifact. Effective expiry only shortens native/requested expiry. Same key/body
replays the original run, including terminal state; altered input conflicts.
Native IDs are server-side references, never browser credentials. Cloud verifies
compiler provenance and that its canonical binding/release artifact matches the
submitted catalog before requesting this generic host operation.

GET `/v1/runs/{id}` checks the native parent before reporting a live app run.
Run status has the previous host/request/bundle pins, state, guestState, error
and expiresAtMs, plus `channel:{delegationId,bindingId,contextSha256,
parentGeneration,access,methods,dataUrl}` on application responses only. This
control status is not exposed to browser source. Stop and cancel-by-key are
terminal, deny future resources/data immediately and close linked upstream
exchanges. They do not assert that a Cloud durable/native revoke was committed;
Cloud's separate request-bound control service owns that confirmation.

## Browser activation, data and readiness

POST `/v1/runs/{id}/tickets` returns a short one-use fragment launchUrl. The
trusted activation page removes the fragment before network I/O and establishes
only `varta_application`, HttpOnly, SameSite=Strict and Path `/runs/{id}/`.
The native parent is checked at ticket creation/redemption, resource access,
claim/renew/report, data relay and live control status. Unrelated localhost
cookies on activation/static routes are ignored in application mode. Run routes
select exactly one well-formed `varta_application` cookie from the bounded Cookie
header; missing, malformed or duplicate own cookies fail closed. Ambient cookies
never authenticate a run, enter browser runtime inputs or reach the native relay.
The preceding preview policy retains its strict cookie behavior. Cookie
authentication precedes upstream probes. One unique client claim starts the short lease.

The claim reply supplies only a nonsecret data URL and host-derived runtime
inputs. The actual net.session ceiling is narrowed to the exact
`<publicOrigin>/runs/<runId>/data/**` path/methods. A broad package request does
not grant another run. Source reads `varta.env.get("VARTA_APP_DATA_URL")` and
uses `varta.session.fetch(base + "/...")`. Ordinary fetch retains no authority.
Origin or genuine same-origin Fetch metadata plus the host session marker are
required; caller Authorization/native-proof overrides are refused. No public
marker selects a native parent, scope, credential or business identity.

The Rust relay maps the suffix to its stored native prefix, forwards only
content-type/accept/idempotency-key, strips browser cookies and uses its own
control credential with the existing native `/relay/<id><target>`. Canonical
literal paths and method intersections are enforced; encoded paths, dot segments,
fragments and alternate authority forms fail. Query bytes are preserved for
application validation. Bodies and JSON replies are bounded to 256 KiB;
upstream exchanges are bounded to two seconds and the outer connection to the
existing five seconds. There are no redirects, ambient credentials or detached
connection drivers. Native parent loss never becomes ordinary owner transport.

Runtime delivery keeps base distribution loader/Worker bytes unchanged. For
view apps, a runtime-only verified frame hook delays the ready report until both
a fresh execution checkpoint and a successfully applied view commit exist.
Console apps report their fresh checkpoint. Readiness is observed guest startup,
not semantic application health, permanent availability or active promotion.
A failed/closed guest, lease loss, fixed expiry, parent loss or stop terminates
host authority; the trusted browser bridge closes the Worker when scheduled.
Stopping closes in-flight native transport; already committed data is not rolled
back. Fresh SDK and durable binding/application/release CAS remain Cloud duties.

## Native scope descriptor extension

This package adds authenticated POST
`/__varta/session-delegation/v1/scope` with exact JSON `{delegationId}` to the
preceding [native control plane](native-delegation.md). It uses the same Host,
Bearer, forbidden-header, body and no-CORS rules. It returns the existing native
status plus `pathPrefix` and `methods`, with no context bytes, parent credential
or request proof. Native expiry/parent loss is checked on read. Unknown/collected
IDs fail; a retained terminal ID reports revoked. The old pinned native binary
has no such operation and fails closed; deployment requires the newly accepted
binary. Existing issue/lookup/revoke fields and v40 guest semantics stay frozen.
