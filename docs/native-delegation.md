# Native session delegation v1

This opt-in, host-only HTTP control plane extends the protected native vault.
It does not change guest imports, language profiles or the v40 ABI. Existing
profiles, browser preview deny-all policy and unconfigured servers keep their
previous meaning. It is the native parent/provenance building block for the
[application-channel draft](application-channel-draft.md), not a browser
activation mechanism or an active private-application launcher.

## Operator configuration

Only the host ceiling may contain `http.delegation`, alongside `http.session`:

```json
{
  "controlTokenEnv": "VARTA_DELEGATION_CONTROL_TOKEN",
  "maxBindings": 16,
  "maxRequestProofs": 64,
  "maxAgeSeconds": 300,
  "pathPrefixes": ["/private"],
  "methods": ["GET", "PATCH", "POST"]
}
```

The named environment variable must contain a separately generated 64-character
lowercase hex control secret before startup. Native state keeps only its hash;
comparisons use constant-time equality. The trusted server adapter needs a
specific existing env-read grant and an existing net.request grant confined to
the native control URL. Use the existing `varta run <artifact> --policy <file>`
or `varta dev --config <project> --policy <file>` entry. Generated/browser applications receive neither. The
operator must protect any remote transport with TLS; insecure loopback requires
the existing explicit development session option. The listener needs more
connection slots than workers, and at least one proof slot per worker.

Maxima are 1024 bindings, 1024 request proofs, 300 seconds, 16 literal path
prefixes and the sorted unique method subset GET/PATCH/POST. Values can only
reduce these ceilings. Prefixes consist of slash-separated alphanumeric,
underscore or hyphen components; the control namespace cannot be delegated.
Routes match exact path or slash boundary; empty/dot components, percent-encoded
paths, backslashes, fragments and whitespace are rejected. Query bytes are
preserved for application validation. The engine knows no owner/resource/release
business schema.

## Authenticated native control

The reserved namespace is `/__varta/session-delegation/v1`, only when enabled.
Every operation requires exactly one `Authorization: Bearer <control secret>`
and the exact configured native session Host/authority. Origin, Cookie,
X-Varta-Session and X-Varta-Request-Proof are forbidden. Controls never emit
CORS or cookies; responses use JSON, no-store and nosniff, bounded to 65536 bytes. They bypass Wasm
worker admission, but retain connection/header bounds. POST bodies require
`application/json`, strict unknown-field rejection, at most 32768 bytes and the
existing header/body-read timeout. Authentication failure is 401; invalid proof,
policy, replay conflict or exhausted capacity is 403; malformed transport is
400/413, unknown route is 404. Diagnostics never contain proof or credentials.

| Operation | Input | Result |
| --- | --- | --- |
| GET `/info` | No query | `{schemaVersion:1,requestProofHeader,policy}` |
| POST `/lookup` | `{requestProof}` | Lookup below |
| POST `/issue` | `{requestProof,requestId,bindingId,context,contextSha256,pathPrefix,methods,ttlSeconds}` | Status below |
| POST `/revoke` | `{requestProof,delegationId}` | Status below |

`requestId` and `bindingId` are opaque 16–128 ASCII alphanumeric/`.`/`_`/`:`/`-`
bytes. `context` is a nonempty opaque UTF-8 string, at most 4096 bytes, and its
lowercase SHA-256 must match exactly. Native storage never parses or reserializes
business JSON. Cloud is responsible for constructing and verifying its schema,
owner, SDK identity, immutable release pins and context expiry. It derives the
opaque parent generation from ordinary lookup before issue; no credential or
native parent reference may enter context or KV.

Lookup is `{schemaVersion:1,mode:"ordinary"|"delegated",parentGeneration:
string|null,context:Verified|null,parentCredential:string|null}`. Verified is
`{bindingId,context,contextSha256,parentGeneration,expiresAtMs}`. Parent credential
is available only to the trusted server, only while the request/parent/scope is
live, and must remain in request RAM for the fresh SDK call. Ordinary lookup has
no delegated context. A missing/invalid proof is an error, never ordinary mode.
A configured adapter must deny if lookup is missing or unavailable.

Status is `{schemaVersion:1,id,bindingId,contextSha256,parentGeneration,state:
"active"|"revoked",expiresAtMs}`. The public scope id is random lowercase hex64,
not a credential. Issue requires an active ordinary request proof with a native
parent lease. A delegated proof cannot issue or revoke bindings. Effective
expiry is the minimum of original requested TTL, parent absolute expiry and its
current idle deadline. It never slides on retry. Same parent+requestId+identity
replays the original receipt, including terminal state; a changed identity is
refused. Terminal receipts count against capacity and are retained for 300
seconds. Cloud must keep its independent durable replay/revocation fence across
native restarts and after this bounded retention window.

## Scoped relay and request lifetime

A trusted adapter sends the original target through
`/__varta/session-delegation/v1/relay/<id><originalPathAndQuery>`, with the same
control authentication and forbidden public credentials as above. Native checks
the live parent, scope, method and literal path ceiling, rewrites the guest URI
to the original target, removes control Authorization and attaches a private
native envelope. The relay retains existing HTTP request-body/lifetime limits.
It does not expose a browser ticket, grant arbitrary forwarding or call external
URLs.

Before any guest initialization effect, native state creates a request-owned
random proof, attaches its cancellation lease and injects the nonce as
`x-varta-request-proof` in the sanitized guest request. The header is only a
lookup key, never evidence of trust. Caller-supplied copies are rejected on all
ordinary and relay requests. The adapter snapshots it, verifies it with native
lookup and removes it before application routing. Body/header mode claims do
not affect native provenance. Unknown proofs and all proofs after request drop
are refused. Proofs are bounded and expire with their request, independently of
scope receipt retention.

Delegated mode is immutable for the request lifetime, even after its live
context becomes null or its scope receipt is collected. There is no owner
bearer/BFF downgrade. Logout, expiry, parent rotation and explicit revoke cancel
linked requests and revoke scope authority; a single native expiry driver also
cancels suspended I/O without another guest call. Delegated session writes and
clear are forbidden. Restart drops all parents/proofs/scopes. These cancellation
fences cannot undo an already committed external effect: Cloud still performs
fresh SDK checks, repeat native lookup after awaits/before reply or write, and
atomic binding/application/release versionstamp checks at commit.

## Acceptance boundary

Synthetic native acceptance covers an actual v40 HTTP service with one worker,
recursive authenticated lookup/issue, exact context relay, rejected public
carrier/credentials/path/method, terminal replay, revoke, logout and restart.
Vault acceptance covers immutable delegated provenance after parent loss and
scope collection, request cleanup, capacity, cross-generation refusal, rotation,
absolute/non-sliding deadlines and autonomous suspended-request expiry.

This API alone does not enable private browser launch. The Cloud adapter,
explicit source/build request policy, browser channel activation, data relay and
fresh guest readiness must pass their own acceptance before production launch.
Historical full reports for execution/toolchain/preview do not cover this new
code. Current verification evidence is recorded separately in
[verification](verification.md).
