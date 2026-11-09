# Engine-side boundaries for the Cloud stage 4 editor package

This is a coordination record, not a new engine contract. It freezes, in one
place, the accepted engine surface that the coordinated Cloud editor package
(candidate/resume/ticket/readiness/promote, update and code-only rollback,
`templates/web-app` startup, explicit test→live copy, declarative schema
migrations) builds on. No engine code, wire, profile, budget or root grant
changes in this record; the pinned receipts below remain the only acceptance
evidence, and each quoted value comes from those frozen files.

The complementary engine contracts stay normative and unchanged:
[application-host v1](application-host.md) and the additive
[startup readiness contract](application-readiness.md). The [channel draft](application-channel-draft.md)
keeps its role as the production roadmap; entries of this record do not enable
public hosting, production admission or `launchAvailable`.

## 1. Engine pin for this package

A stage 4 lifecycle that observes readiness before promotion must run on the
accepted **application-readiness** binaries, because the earlier application-host
pin does not implement that surface. The two receipts are:

| Package | Receipt | Code fingerprint | CLI (`varta`) sha256 | Companion (`varta-runtime`) sha256 |
| --- | --- | --- | --- | --- |
| application-host v1 | `evidence/2026-11-08-application-host-verification.json` | `73dfce54c25dd6e800419fa1da4cb9be9c52d5ef453fe5c4215a4d6311d3e37c` (2678 files) | `4e2ecbef800010da2345370de7aea825e3901d69a1f357b15c3c7d0c90a6e541` | `a46e98f58e6c9a5cfc2a1151929ac34603c22deedfbe251a4f3789d12e570171` |
| application-readiness (current accepted) | `evidence/2026-11-08-application-readiness-verification.json` | `fde1e8856e7f3a9bb1fffb100aacba2c1f6efd728bb315b288f0c78f09b7d6fe` (2681 files) | `aa1485c6bc09b63aede0e54d05acdb8dbef51fc8c6665ac5cd2eafb4005af768` | `b55377d890167a433c117b531377843ef8650a0220f0703b2922127e598efccc` |

The readiness receipt's frozen source fingerprint matches commit `b620b805`
(`Add isolated hosts, delegated sessions, and private application readiness`),
which contains both the application-host boundary and the additive readiness
routes; its full gate passed on a byte-identical tree. The application-host
package is immutable evidence for its own code and is **superseded
operationally** by the additive package: the readiness build passes every
application-host test plus the additive routes.

Verification against the retained binaries (paths under `target/`, hashes above):

- the application-host binary contains neither `hostInstanceId` nor
  `validUntilMs`;
- the readiness binary implements `GET /v1/info` with `hostInstanceId` (in
  application mode) and `GET /v1/runs/{id}/readiness` under the existing
  bounded, Bearer-authenticated control channel.

A Cloud package that pins the application-host runtime (`a46e98f5…`) and then
calls readiness would always fail closed; the correct pin for a
readiness→promote lifecycle is the readiness pair (`aa1485…` CLI,
`b55377…` runtime). `varta-runtime` stays execution-only; the application
control host is the `varta application-host --ceiling …` CLI process, and
`GET /v1/info`'s `hostSha256` is that CLI executable's identity.

Nothing in this record changes those binaries or their gates. Any future engine
package that touches application policy produces a new receipt and a new pin by
the standing process; this record then needs a dated update in the same commit.

## 2. Frozen compile and application policy

The stage 4 package compiles and admits artifacts inside the existing policy;
these facts stay as implemented (see `crates/varta-project/src/application/contract.rs`,
`crates/varta-project/src/toolchain/contract.rs`,
`crates/varta-project/src/preview/bundle.rs`,
`crates/varta-project/src/preview/store/application.rs`):

- dependency policy `app-data-v1`, policy id `scoped-json-v1`, language profile
  exactly `checked-f64-utf16-v40`, target `wasm` in the generated
  `app.manifest.json`;
- the compile request's optional `application` field carries exactly
  `{publicOrigin, access}`; `publicOrigin` must be a canonical
  `http://localhost:<non-default-port>` origin listed in the operator
  `applicationOrigins` ceiling;
- the generated package manifest requests exactly `env.get` for the one
  nonsecret name and `net.session` for `<publicOrigin>/runs/**` with the
  access-selected methods; no other capability, filesystem, HTTP, session,
  budget or host configuration is admitted;
- at run admission the application worker's ceiling narrows `net.session` to the
  exact per-run `<publicOrigin>/runs/<runId>/data/**` scope and injects the data
  URL as the environment input; a broad package grant never widens another run.

Stage 4 stays inside this boundary. Nothing in the editor lifecycle (update,
rollback, migrations, templates) widens the requested grants or the ceiling.

## 3. Opaque context, data schema versions and migrations

The engine's only obligations toward the application context are (see
`crates/varta-common/src/session/delegation.rs` and
`crates/varta-runtime/src/session/vault/delegation.rs`):

- the context is opaque UTF-8 bytes bounded by `MAX_CONTEXT_BYTES`, hashed into
  `contextSha256`, immutable per binding, and never parsed by the engine;
- the application host verifies the exact `contextSha256` against the native
  scope descriptor at `POST /v1/runs` and on every scope read, and the relay
  preserves the delegate classification regardless of the context's validity.

Therefore `schemaVersion`, `dataSchemaVersion`, release, environment and owner
fields inside the context are **Cloud semantics**. Stage 4's declarative
migration plan is a Cloud-side transaction; the engine needs no change for it.
The boundary conditions on the engine side are unchanged:

- a migration that changes the effective context produces a new binding
  (`bindingId` plus `contextSha256`), which requires a fresh native
  issue/lookup chain with the same native parent provenance rules — never a
  browser or saved credential;
- revocation, parent loss, native-vault restart and expiry still deny new
  operations and end linked in-flight relay work; committed data is not rolled
  back by the engine;
- release promotion may not promote from cached readiness, and a failed
  migration leaves the previous active release and its data untouched. These
  are Cloud duties; the engine supplies only the per-run evidence described in
  the readiness contract.

## 4. Manifests and inert metadata

The engine reads exactly two manifest sources and nothing else:

- the generated `app.manifest.json` inside the admitted browser bundle;
- the operator's `ApplicationCeiling` file.

There is no `varta.app.json`, template manifest or schema-version manifest in
the engine's admission path. Inert files in a toolchain source request are
accepted as path-valid entries but never reach the artifact unless imported
(with explicit relative `.ts`/`.tsx` code imports or `with { type: "text" }`).
Cloud should therefore store schema version, template identity and editor
metadata in its own records, not inside compiled artifacts. If a template needs
a runtime-visible constant, it is ordinary source code compiled into the module.

## 5. Rollback, exact pins and compiler provenance

Artifact identity is byte identity. The engine consults:

- the pinned `hostSha256` (the application-host CLI executable);
- the sealed catalog digest and per-file digests, independent of any Cloud
  compiler field;
- the exact application manifest request for the pinned `publicOrigin`/`access`.

`compilerSha256` appears only as a compile-time equality check between a
toolchain request and the toolchain host's own pinned compiler (see
`crates/varta-project/src/toolchain/store.rs`), and as a stored Cloud build pin
in the separate native execution contract. Application-host and preview
admission never consult it. Consequences for the editor package:

- **Rollback of a code release** to a previously accepted artifact works by
  re-admitting the retained bytes into the same pinned host; no rebuild is
  required and the Cloud queue's `publicOrigin`/access checks plus exact
  promotion pins are sufficient.
- **Changing the pinned engine** is different: admission reconstructs the
  official browser runtime from the original module and compares every byte
  with the imported bundle, so bundles built by one engine version are
  admissible only under a host whose canonical runtime files are byte-identical.
  After an engine update, previously retained bundles must be rebuilt from
  their sources with the matching toolchain (new artifact digest, normal
  promotion), or the stage 4 update/rollback path must treat the engine
  transition as its own migration step.

## 6. Entry point and readiness requirements for templates

A template project must satisfy the standing rules; the readiness evidence then
follows from the engine, not from template code:

- `entry` ends in `.ts` or `.tsx`; profile v40 with `app-data-v1`; the compiled
  manifest is the `app-data-v1` artifact request exactly;
- a console (non-view) template becomes ready at its fresh startup checkpoint;
- a view template becomes ready only after the startup checkpoint **and** the
  first successfully applied view commit (the loader acknowledges the applied
  frame, and the trusted bridge reports `ready` only when both are true); this
  is wired by the engine from the module's `view_commit` import, with no
  template configuration.

Practical requirements that keep a template inside the ready window:

- commit at least one view batch during startup (an empty list is a valid
  batch); do not wait indefinitely on a scoped network result before the first
  commit;
- never let `main` end with an uncaught value; a failed guest reports `failed`
  instead of `ready`;
- the scoped transport is the only networking authority; ordinary `fetch`
  stays credential-free and cannot reach run data.

Readiness remains bounded, fresh evidence: a current client claim, a live
lease, the guest's ready state and a validity window bounded by the lease, the
run and the native scope. It is not business health, permanent availability or
durable promotion authority; the promotion caller re-observes it and requires
the same incarnation at its final durable switch, per the readiness contract.

## 7. Explicitly out of scope

Public TLS hosting, public addresses, production admission and `launchAvailable`
remain disabled. Editor UX, business release rules, migration plan semantics,
test→live copy decisions and quotas/backups are Cloud responsibilities. Root
grants, preview deny-all, guest ABI, language profiles and production budgets
keep their existing meaning.
