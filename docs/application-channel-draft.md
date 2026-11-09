# Application data channel v1 — design and implementation boundaries

Local readiness-gated promotion and stable private entry are now implemented under the additive [startup contract](application-readiness.md); targeted engine and actual Cloud Studio Chrome acceptance pass, followed by one successful full gate; see the [final receipt](evidence/2026-11-08-application-readiness-verification.json). Candidate/resume/test-ticket/promote/abort/stop are API operations; editor deployment controls, rollback workflow, schema migration, test-data copy and public TLS hosting remain separate work. The design below is historical where it describes those accepted local boundaries as future.

**The complete production design below remains a roadmap.** The separate
[local application-host implementation](application-host.md) now has scoped
acceptance for app-data-v1 source requests, canonical artifact admission,
per-run protected data ceilings, one-use browser activation, fresh view startup
and native cancellation. Coordinated normal-Chrome Cloud/KV acceptance and one
full gate pass; see the [package receipt](evidence/2026-11-08-application-host-verification.json).
Existing preview remains deny-all.
The [native delegation building block](native-delegation.md) already has its own
accepted immutable checkpoint. Local app-host support does not establish public
TLS hosting, stable private addresses or active release promotion. No bearer,
header or body fallback substitutes for native parent provenance.

The separate [Cloud authorizer checkpoint](evidence/2026-11-08-cloud-data-channel-authorizer.json)
covers intents/revocation, exact private-data ingress, fresh identity/scope/pin
checks and data-write CAS fences. Its 278 source hashes and aggregate digest were
independently verified. Synthetic delegated tests and real HTTP default-off/owner
data regressions do not establish native delegation or browser-channel acceptance.
At that earlier checkpoint the internal intent ledger had a technical cap of
16 lifetime entries per project. Subsequent Cloud retention and native-adapter
work has a separate acceptance boundary; it does not enable browser launch.

The accepted [Cloud native adapter checkpoint](evidence/2026-11-08-cloud-native-data-adapter.json)
contains 299 source hashes and 19 logs, independently matched at acceptance. It
covers actual synthetic SDK plus HTTP/KV/CAS/revoke/logout/restart, without a
browser activation or public production issue/revoke path. The [Root native
package receipt](evidence/2026-11-08-native-delegation-verification.json) records
one completed full gate and the separate [lifecycle checkpoint](evidence/2026-11-08-cloud-data-channel-lifecycle.json)
retains the preceding bounded-retention work. Later Cloud issuance work is a new
package, not a modification of these receipts.

## Trust boundary

In the complete host design, the generic engine stores an opaque binding ID and exact context bytes/digest,
artifact pin, origins, route/method ceiling and bounded expiry. It does not
interpret Cloud owner, tenant, resource or release business rules. Only a
trusted server request with an active native protected-vault parent lease may
issue a binding. A browser body/header cannot supply that parent reference or
establish trusted context. The current session.get API is not delegation.

The separate versioned native runtime/vault seam now provides issuance, context
and revocation with native/Cloud acceptance, without changing guest ABI/profiles.
Names/signatures of future guest host operations are deliberately not
established by this draft.

The parent reference and credential lease remain native-owned and memory-only;
neither enters generated applications, opaque context or application KV. Parent
expiry, logout/clear, native-vault restart and invalidated generation revoke
children and linked in-flight relay work. Credential refresh/rotation cannot
silently transfer authority from an old binding. Native parent rotation already
revokes old children; delegated renewal and browser-channel rotation remain
future contracts.

## Request provenance survives revocation

The native adapter must distinguish an ordinary owner request from an already
delegated request independently of whether its context is still valid. Once
classified as delegated, a request retains that classification for its entire
lifetime. Missing context after revoke, logout, expiry or parent loss denies the
request; it never enables ordinary owner bearer/BFF authorization.

Cloud's application port distinguishes unavailable and delegated mode, with an
asynchronous nullable context read. These are application port semantics, not
implemented engine API names. A delegated mode with a null read remains
delegated. Both the router and binary dispatcher restrict it to private-data
ingress and reject project/auth/job/secret access even when a valid owner bearer
is also present. The provenance marker comes only from native request state,
never a public header, cookie or body field.

Acceptance must cover a parent disappearing before context read or during a
request, followed by attempts to reach owner and binary routes. Revocation must
not cause an authorization downgrade or fallback.

## Cloud opaque context

Cloud serializes compact JSON with keys in ASCII alphabetical order and hashes
the exact UTF-8 bytes (at most 4096 bytes). Host retention and forwarding
preserve those bytes; the engine does not reserialize or canonicalize business
JSON.

Exact context fields:

- schemaVersion, bindingId, ownerId, tenantId, projectId, applicationId, resourceId
- environment: live or test
- releaseId, applicationRevision, artifactSha256, compilerSha256
- dataSchemaVersion: 1
- parentGeneration: native correlation only, never a credential
- access: read or read-write
- expiresAtMs

The digest covers immutable binding claims, not mutable revocation/status
fields. Cloud TTL is at most 300 seconds. Effective host/parent expiry may only
shorten it; clamping never changes the stored opaque bytes. Cloud's trusted
adapter reads the binding from its own KV and verifies fresh SDK identity, full
ownership/scope, selected release/revision, artifact/compiler pins and durable
revocation on every request. Writes include binding and application/release
versionstamps in their CAS, so revoke or release change cannot race a pending
commit.

## Data ingress

Guest URLs select no owner, app, resource, environment or credential. Those
facts come exclusively from the stored binding.

| Path | Methods |
| --- | --- |
| /api/private-data/resource | GET |
| /api/private-data/records?cursor=... | GET |
| /api/private-data/records/{uuid} | GET, PATCH |
| /api/private-data/records/{uuid}/delete | POST |

Read bindings allow GET only. Write bindings allow GET/PATCH/POST only on these
routes. DELETE and project/auth/job/secret routes are excluded. The native
scoped transport and Cloud's exact ingress parser both enforce this boundary,
including the binary handler. Prefix checks require a path boundary; escaped
separators, dot segments and alternate context forms are rejected before relay.

Ordinary fetch retains credentials:omit. A scoped request needs an explicit
package net.session request AND an exact trusted host ceiling. The engine-owned
transport handles its scoped HttpOnly cookie and strips/rejects guest overrides
of Cookie, Origin, Authorization and trusted context. Genuine browser transport
headers are verified by the host, never treated as caller claims. Generated
source sees no provider/tooling/session credential. The upstream trusted context
is native request state, not a public X-* header or JSON field. A TypeScript
interface or boolean is not proof of transport provenance.

## Planned control wire

These routes describe the intended separate channel host, not current CLI
routes. It will require its own explicit operator policy, authenticated trusted
control transport, native parent-reference provenance and bounded storage.

Issue: POST /v1/channels + stable Idempotency-Key:

```json
{"schemaVersion":1,"hostSha256":"<host pin>","parentRef":"<native-only reference>","bindingId":"<opaque ID>","contextSha256":"<SHA256 of exact bytes>","context":"<opaque UTF-8 JSON text>","artifactSha256":"<admitted artifact>","compilerSha256":"<caller provenance>","clientOrigin":"<exact origin>","relayOrigin":"<exact origin>","pathPrefix":"/api/private-data","methods":["GET","PATCH","POST"],"ttlSeconds":300}
```

The trusted adapter acquires parentRef from native request state, never user
input. Host validates artifact bytes; compiler provenance stays an application
build-record responsibility. Same key/body replays before capacity; changed
input conflicts. Native expiry cannot exceed the parent or the requested TTL.

Read: GET /v1/channels/{id} returns
`{id,bindingId,contextSha256,artifactSha256,compilerSha256,state,expiresAtMs}`;
state is issued, active, revoked or expired. No secret/ticket in status or
replay.

Activate: POST /v1/channels/{id}/tickets returns a short one-use fragment
launchUrl and expiresAtMs. An engine-owned activation page removes the fragment
before I/O and establishes only scoped HttpOnly custody. Parent validity is
checked at redemption and on every relay request.

Revoke: POST /v1/channels/{id}/revoke is terminal/idempotent; deny new requests
immediately and cancel linked in-flight work. Restart restores no ephemeral
native authority. Cloud retains its durable revocation decision independently.

Trusted read for the Cloud authorizer is intentionally not a public HTTP API:
the native adapter supplies bindingId/contextSha256/parentGeneration and a
server-only parent credential/session lease. Without that adapter, production
ingress returns 503, including for requests carrying an otherwise valid bearer.

## Implementation prerequisites

1. Explicit source/artifact request policy: local-only-v1 currently emits no
capability requests; a runtime ceiling cannot expand an empty package request.
2. Native parent delegation/context/revocation seam and scoped trusted relay,
independently tested for spoofing, expiry, reset, rotation and cancellation. 3.
Accepted browser launch/route policy with exact artifact binding and fresh
guest-ready/health evidence before Cloud CAS active-release promotion.

Saved/selected scope validation is not health or active promotion. Native HTTP
execution does not implement this browser channel. The draft authorizer may be
accepted as groundwork while launchAvailable remains false.
