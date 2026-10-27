# Changelog

## Unreleased

The workspace currently uses package version `0.1.0`. These entries describe the
development snapshot; they do not announce a published release.
The [2026-10-27 checkpoint](docs/development-plan.md#development-checkpoint--2026-10-27)
records the current core stage, completed increments, validation and next work.

### Added

- Checked TypeScript-to-Wasm compilation, local module loading, source diagnostics
  and explicit, versioned language profiles.
- Opt-in binary64 numbers, UTF-16 strings, typed unions, optional fields, bounded
  arrays, checked JSON, recoverable exceptions, typed functions and lexical closures.
- Array callbacks, optional/default/rest parameters, spread calls, and shared
  object/array destructuring for parameters, local declarations and assignments.
- Expression-valued destructuring assignments and checked record-field/array-element
  targets, including defaults and rest. V14 adds indexed extension and writable length.
- Intrinsic array for-of in v12+, with binding/assignment patterns, per-iteration
  lexical captures, live mutation, sparse Undefined visits and finally cleanup.
- Checked do-while in v12+, with trailing-condition continue, finalizer routing,
  capture/root cleanup and guaranteed-first-body completion/return inference.
- Statement labels and named break/continue in v12+, including loop aliases,
  exact finalizer/scope routing and target-aware completion inference.
- Object.keys for closed records in v12+, with presence-aware JS key ordering,
  fresh dense UTF-16 string-array results, nullable checks and rooted snapshots.
- Object.values for closed records in v12+, with inferred homogeneous/union
  arrays, shared own-property ordering, rooted payload preparation and shallow identity.
- Object.keys/Object.values for bounded arrays in v12+, including sparse own-index
  enumeration, exact element types and rooted host-free decimal key creation.
- Catch object binding patterns in v12+ for name/message, with aliases/defaults,
  TDZ, rooted initialization and existing exception/finalizer routing.
- String.slice/substring in v12+, with bounded Number/Undefined normalization,
  exact UTF-16 ranges, owned result storage and preserved Array.slice dispatch.
- String.includes/startsWith/endsWith in v12+, with exact UTF-16 matching,
  bounded positions, allocation-free helpers and unchanged Array.includes rules.
- Capability-checked host APIs, independent host policy ceilings, programmatic
  operation results, and bounded execution with managed guest memory.
- Checked synchronous HTTP/1 handlers in the default language profile.
- Primitive tool discovery and checked named JSON invocation.
- Portable Wasm artifacts and native executables with an independent execution-only
  runtime companion, without shipping the TypeScript compiler.
- Contributor and security policies, an MIT license, and GitHub issue/PR templates.

### Compatibility and limits

- The default profile retains its checked i32/UTF-8 semantics; newer language
  features require their documented opt-in profiles.
- Native packages target the build machine's OS and CPU. CI covers Linux and
  macOS; Windows and cross-target packaging remain unverified.
- VARTA3 packages contain portable Wasm. Legacy VARTA1/VARTA2 and native AOT
  payloads are rejected and must be rebuilt.
- Full ECMAScript/TypeScript and Node/npm compatibility, asynchronous execution,
  MCP bridges and artifact signing remain planned.

See the [README](README.md) for supported features and the
[development plan](docs/development-plan.md) for remaining work.
