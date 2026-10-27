# Changelog

## Unreleased

The workspace currently uses package version `0.1.0`. These entries describe the
development snapshot; they do not announce a published release.

### Added

- Checked TypeScript-to-Wasm compilation, local module loading, source diagnostics
  and explicit, versioned language profiles.
- Opt-in binary64 numbers, UTF-16 strings, typed unions, optional fields, bounded
  arrays, checked JSON, recoverable exceptions, typed functions and lexical closures.
- Array callbacks, optional/default/rest parameters, spread calls, and shared
  object/array destructuring for parameters, local declarations and assignments.
- Expression-valued destructuring assignments and checked record-field/array-element
  targets, including defaults and rest. V14 adds indexed extension and writable length.
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
