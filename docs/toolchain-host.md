# Local toolchain host v1

This tooling service checks/builds immutable source bundles. It is not a guest
global, application backend, execution launcher or grant to run an artifact.
Runtime/launcher retain no compiler dependency. Cloud owns users, projects,
durable jobs, retries, leases and fencing; none of those fields enter this wire.

The first policy is `local-only-v1`: explicit `.ts`/`.tsx` relative code imports,
confined relative text imports, and only the supplied UTF-8 files. No package
download, `node_modules`, source-owned configuration, host filesystem root,
environment value or native executable enters compilation. The output manifest
requests no capabilities. Check and build use the same compiler and browser
admission; neither executes module initialization or `main`. Broader build
configuration and execution are separate future contracts. This slice uses the
module compiler directly: it does not perform CLI Tailwind preprocessing of a
CSS-first Tailwind entry. Ordinary supplied CSS/text imports remain available.

## Host setup and authority

`varta toolchain --ceiling /absolute/path/ceiling.json` requires the host runtime
environment `VARTA_TOOLCHAIN_TOKEN`: 64 lowercase hexadecimal characters from
32 random bytes. Supply it as a runtime secret to the trusted application server
as well; never a browser bundle, URL, source file, evidence or command argument.
The server client additionally needs explicit `env.get` and `net.request` grants
and ceilings for this one variable and service origin. Runtime `--policy` and
`--decisions` do not configure this tooling host. Its explicit ceiling is:

```json
{
  "schemaVersion": 1,
  "listen": "127.0.0.1:8799",
  "operations": ["check", "build"],
  "profiles": ["checked-f64-utf16-v40"],
  "maxJobs": 8,
  "maxWorkers": 1,
  "deadlineMs": 15000,
  "cpuSeconds": 10,
  "addressSpaceBytes": 4294967296,
  "retentionSeconds": 300
}
```

Only loopback HTTP is admitted in v1. This is a local service-to-service channel,
not a production or browser deployment. Every route requires the exact Host and
one `Authorization: Bearer <token>`. Cookies, Origin, absolute request targets
and query strings are refused; no CORS permission is returned. Tokens and source
are never logged. Credential-hash comparisons use the existing constant-time
cryptographic primitive. Downloaded files use attachment/octet-stream, nosniff and a
restrictive CSP; the host never serves an executable preview.

The executable is copied to a private host directory and SHA-256 hashed before
listening. That exact copy runs workers even if the developer rebuilds the CLI.
Workers inherit no environment; the fixed host-owned MallocNanoZone=0 allocator
setting avoids a large unused private allocator reservation on Darwin. Workers
start only this trusted executable directly,
without a shell or source-selected command. Unix address-space-growth/CPU limits and
disabled core dumps apply before input parsing. Startup probes that a reservation
larger than the growth allowance fails; unavailable enforcement fails closed.
Wall time, request/source/output sizes, concurrency and retained data are bounded.
The addressSpaceBytes value is an allowance for growth above the worker's
startup virtual mappings, not an absolute virtual size or an RSS limit. Existing
Darwin shared-cache mappings can exceed a small absolute AS limit. Modern XNU
refuses limits below existing mappings ([kernel resource implementation](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/bsd/kern/kern_resource.c));
the host brackets that baseline using the safe resource API. Linux uses bounded
`/proc/self/statm` plus the kernel page size. Startup allocation refusal proves
enforcement on the actual kernel; unsupported platforms fail closed. This is not
an OS sandbox or a hard total-RSS claim. Inherited OS limits may reduce the
available resources below the configured ceilings.

## Wire

All JSON fields are exact, unknown fields are refused, digests are 64 lowercase
hexadecimal characters, positions are one-based Unicode scalar coordinates.

Authenticated `GET /v1/info` returns schemaVersion, compilerSha256, target
`browser`, dependencyPolicy `local-only-v1`, allowed operations/profiles and limits.
The application must establish/pin this trusted compiler identity before enqueue.

`POST /v1/jobs` requires Content-Type `application/json` and one
`Idempotency-Key` header (16–128 ASCII letters/digits or `._:-`). Keep that
correlation key stable when recovering a lost202 response. The body is:

```json
{
  "schemaVersion": 1,
  "compilerSha256": "<digest from the trusted info response>",
  "operation": "build",
  "target": "browser",
  "languageProfile": "checked-f64-utf16-v40",
  "dependencyPolicy": "local-only-v1",
  "entry": "src/main.ts",
  "files": [{"path": "src/main.ts", "content": "export function main(): void {}"}]
}
```

Reply202 is a status object: id, compilerSha256, requestSha256, state, error,
result, artifact and forgotten. The request digest sorts files and hashes canonical JSON;
array order does not alter input identity. A compiler mismatch is409, operation
or profile denial403, full admission503, malformed input400, oversized body413.
Failed authorization is401 and precedes body collection or job admission.
Same key and canonical request returns the original operation before busy checks;
changed input conflicts409. Cancellation and terminal replays never compile again.
After host restart/terminal expiry, that key can create a new operation because
receipts are ephemeral. An application can replay POST with its same key to
recover/cancel an accepted operation whose202 reply was lost.

`GET /v1/jobs/<id>` polls states `queued`, `running`, `succeeded`, `failed`,
`cancelled`. Terminal success/compile failure has
`result:{ok,diagnostics,artifactSha256}`. Diagnostics are
`{severity,code,path,line,column,message}`. Check success has an empty artifact
digest; build success has a validated browser bundle and a content digest.
Infrastructure failures have a fixed error code and no fabricated compiler
diagnostic. Compilation errors are terminal `failed` with `error:compile_error`.

`POST /v1/jobs/<id>/cancel` is idempotent and returns the current status. Terminal
results remain terminal. Cancellation fences later publication and kills/reaps
the active compiler process; queued cancellation starts no compiler. The wall
deadline includes time waiting for a worker. Client abort alone is not the job
cancel protocol: an accepted operation can continue until cancel or its deadline.
Applications must cancel an owned operation when their AbortSignal fires.

Build status also includes `artifact:{sha256,files:[{path,sha256,bytes}]}`.
`GET /v1/artifacts/<artifact-sha256>/<path>` returns exact bytes from that closed
in-memory catalog. No request maps to a filesystem path. Artifact identity is
SHA-256 of domain `varta-toolchain-artifact-v1` + NUL, then sorted file entries:
little-endian u64 path-byte length, UTF-8 path, little-endian u64 data length,
data. Per-file digests are ordinary SHA-256. File names/content are immutable.

`DELETE /v1/jobs/<id>` releases only a terminal operation's artifact bytes;
active operations409. The bounded idempotency/status receipt remains with
`forgotten:true` until its original terminal expiry, so replay cannot silently
compile again. Its artifact becomes unavailable unless another retained
operation has that digest. Completed operations/artifacts expire after retentionSeconds. This is a
bounded ephemeral cache, not durable artifact storage. A client must retrieve
needed files before forgetting/expiry. Restart discards host operations/cache;
the application owns durable recovery and must not treat a compiler request as
an authorization to deploy or execute.

## Bounds

256 files, 512 UTF-8 bytes per path, 1 MiB per file, 4 MiB total source,
12 MiB request JSON, 32 MiB per artifact and 64 MiB retained artifact data.
Existing profile/compiler limits still apply. One diagnostic has at most16 KiB
of message. Up to16 active operations and256 terminal/idempotency receipts,
four compiler workers with at most8 GiB combined address-space allowance,
30 seconds wall/CPU and 300 seconds terminal retention may be configured.
The HTTP host admits16 connections,32 headers,8192 header-buffer bytes and
five seconds per connection. A full cache refuses publication rather than
silently evicting another operation's artifact. SIGINT/SIGTERM and embedding shutdown cancel/reap workers, close connections
and remove the private executable copy. An internal framed-stdin lifetime
channel stops a worker if its controller dies without destructors, including
SIGKILL. A controller crash can leave its private trusted-executable copy;
source, tokens and artifacts are not written there.

Engine acceptance includes real CLI build/cancel/artifact validation and
concurrent native KV identity. Cloud headless native TSX through real
HTTP/API/KV/separate worker and six direct native cases pass on the pinned final
CLI. One completed-package full gate passes all 14 commands on unchanged sources:
4700 Rust tests (26 ignored), 195 Node tests and all three compatibility gates.
See the [package evidence](evidence/2026-11-07-toolchain-host-verification.json).
This is local macOS acceptance; it adds no new Chrome/HTTPS/live-provider or
production deployment claim. The previous Studio stabilization receipt remains
historical.
