# Changelog

## Unreleased

The workspace currently uses package version `0.1.0`. These entries describe the
development snapshot; they do not announce a published release.
The [2026-10-27 checkpoint](docs/development-plan.md#development-checkpoint--2026-10-27)
records the current core stage, completed increments, validation and next work.

### Added

- Checked prefix/postfix `++` and `--` in v12+ for mutable variables, static/dynamic
  record fields and numeric arrays, plus length in v14. Require an actual Number,
  store through one prepared reference, then return the exact old/new value;
  preserve partial effects, captured loop cells, roots and fatal boundaries.
- Checked `&&=`, `||=` and `??=` in v12+ for mutable variables, static/dynamic
  record fields and numeric arrays, plus length in v14. RHS evaluation and checked
  stores run only in the selected branch, retaining original values/references,
  presence, result types and rooted partial-chain/GC/finalizer behavior.
- Dynamic record `+=`, `-=`, `*=`, `/=` and `%=` in v12+, retaining the original
  receiver/key and old value across RHS effects, with checked computed results,
  exact selected-field stores and existing read-union/absence/prototype boundaries.
- Checked `+=`, `-=`, `*=`, `/=` and `%=` in v12+ for locals, static fields
  and numeric array references, with cached old values, matching Number/String
  guards, computed results, rooted nested effects and existing v14 length checks.
- Simple local, dot-field and numeric-array assignment expressions in v12+, with
  shared prepared references, original RHS types/values, full result/function
  ownership and v14 length-expression checks.
- Computed record assignment expressions/chains in v12+, returning the original
  RHS type/value with prepared references, static literal checks, rooted temporary
  floors and preserved inner writes on outer failure.
- Typed computed String assignment statements on closed records in v12+, with
  prepared receiver/key roots, exact selected-field checks, Error family guards,
  optional presence/order and catchable failures after RHS effects.
- Typed computed String reads on closed records in v12+, with inferred field/Undefined
  unions, presence-aware UTF-16 lookup, preserved payload identity and rooted operand
  effects. Unsupported inherited lookups raise TypeError; prototype access stays closed.
- Dynamic String own keys for sparse arrays in v13+ and Error families in v12+,
  preserving canonical indices/length, holes versus explicit Undefined and own
  message presence, argument effects, exact roots and older profile boundaries.

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
- String.indexOf/lastIndexOf in v12+, with UTF-16 code-unit indices,
  method-specific position normalization and allocation-free helpers.
- Array.lastIndexOf, Array.reverse, and Array.fill in resizable-array profiles,
  with tagged-value matching/bounds, in-place mutation, sparse handling and
  ownership preservation.
- String.trim/trimStart/trimEnd in v12+, with exact UTF-16 whitespace
  removal, surrogate/NUL preservation, and owned result storage.
- Dynamic String keys for Object.hasOwn on closed records in v12+, with exact
  UTF-16 decision ordering, rooted argument evaluation and presence-only lookup.
- Capability-checked host APIs, independent host policy ceilings, programmatic
  operation results, and bounded execution with managed guest memory.
- Checked synchronous HTTP/1 handlers in the default language profile.
- Primitive tool discovery and checked named JSON invocation.
- Portable Wasm artifacts and native executables with an independent execution-only
  runtime companion, without shipping the TypeScript compiler.
- Contributor and security policies, an MIT license, and GitHub issue/PR templates.

### Fixed

- Dispatch Array.lastIndexOf to reverse search instead of Array.indexOf. Omitted
  positions start at the final element; explicit undefined/NaN select zero,
  positive infinity clamps to the final index and positions below -length miss.
- Generate valid sparse Array.reverse helpers by retaining the i64 array handle
  for bitmap lookup; keep payloads, presence and identity together in v13/v14.
- Preserve U+0085 in trim/trimStart/trimEnd: it is not ECMAScript WhiteSpace or a
  LineTerminator. Tests cover all 25 admitted whitespace/line-terminator units.
- Restore v13 rejection of writable length/deletion and fatal indexed extension
  checks, remove the temporary .bak file and add source-free regression coverage.

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
