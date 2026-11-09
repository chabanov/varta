# Application startup readiness and private entry paths

This additive contract extends [application-host v1](application-host.md).
The earlier wire, native delegation, source policies and distribution loader
remain unchanged. Cloud owns release selection, durable active slots, private
addresses, authentication and promotion. The engine supplies only startup
evidence and an explicitly enabled generic browser shell route.

## Fresh startup evidence

Application-host control `GET /v1/info` additionally returns `hostInstanceId`, a
random 64-character lowercase hexadecimal identity unique to this in-memory
registry incarnation. It is distinct from the executable `hostSha256`.
Preview-host info is unchanged.

Application-host control `GET /v1/runs/{id}/readiness` uses the existing bounded,
bearer-authenticated control channel. Browser cookies, Origin requests, public
listener requests and unknown routes never authorize this operation. The host
rechecks the exact native scope, then under the registry lock expires stale
records and requires an active run, a claimed client, a live lease and the
guest's ready checkpoint. There is no synthetic client claim or second guest
execution. A view application reports ready only after its startup checkpoint
and the associated applied frame; this does not establish business health.

The response is:

```json
{
  "schemaVersion": 1,
  "hostInstanceId": "<64 lowercase hexadecimal characters>",
  "observedAtMs": 0,
  "validUntilMs": 0,
  "run": "<the existing complete run status object, including channel>"
}
```

The example uses placeholders; `run` is an object on the wire. Both times are
host Unix milliseconds. `validUntilMs` is at most one second after observation,
and is bounded by the current client lease, fixed run expiry and native scope
expiry. Pending/unclaimed/not-ready runs return `409 not_ready`; unknown runs
return `404 not_found`; ended runs or lost native scope return the existing
`410 ended`. Unavailable native authorization fails closed. Ordinary status and
idempotent start replay are not readiness observations.

A promotion caller obtains fresh host info and readiness, checks the same
incarnation, exact run/request/bundle/host/channel pins, current owner and native
parent, and requires current time before `validUntilMs` at its final durable CAS.
It never persists readiness as reusable authority or promotes from cached status.
This local contract assumes the caller and host share a clock; remote clocks and
distributed atomic liveness are outside it. A process can fail immediately after
an observation. Cloud's generation checks fence data authority independently of
later best-effort runtime cleanup.

## Explicit browser shell entry

`varta dev --target browser --browser-entry-prefix /apps` enables serving the
ordinary trusted development shell at `/apps/<opaque-id>`. This flag does not
exist for native execution. The prefix is one literal ASCII alphanumeric,
underscore or hyphen component of 1..64 characters; `__varta` is reserved. The
identifier is one such component of 1..128 characters. Percent escapes, dots,
extra components, empty identifiers and query strings are rejected. Without
this option these paths retain their existing 404 behavior.

The shell points at the current immutable development revision. Source files,
bare mutable asset routes and expired asset revisions remain unavailable. The
runtime-only thin browser bridge passes `location.pathname` as the public
`VARTA_ENTRY_PATH` environment input. Both the application's `env.get` request
and any host ceiling must explicitly grant this name; enabling the route grants
no capabilities. No host process environment or credential is embedded in the
HTTP loader. Rebuild retains a canonical entry path while loading the new
immutable revision; direct immutable-asset pages still return to the root.
Distribution bundles, the original loader, Worker and guest ABI are unchanged.

For Cloud's local acceptance, the trusted Studio entry is
`http://127.0.0.1:8795/apps/{addressId}`, its protected API is on
`http://127.0.0.1:8796`, and the isolated application host is on
`http://localhost:8810`. These are Cloud configuration, not engine routes or
constants. Varta Studio parses its public address identifier, restores or signs
in through the existing protected session transport, resolves an authorized
launch and opens its in-memory one-use ticket. Owner cookies remain on the
trusted host. The bookmarked URL is a stable private entry; after navigation the
application has a bounded, ephemeral run URL. Reentry creates a new scoped
execution and claim, never reissues an already-active run's ticket.

Cloud's release slot keeps draft selection separate from active deployment.
Candidates may initialize with reads, but data writes require the active
generation. Promotion validates fresh startup and actor evidence and switches
the exact candidate and old generation in one durable transaction. Failed,
expired, stopped or conflicting candidates leave the previous active pointer
unchanged. This policy belongs to Cloud's typed source and is separately tested
there; the engine does not interpret release or address records.
