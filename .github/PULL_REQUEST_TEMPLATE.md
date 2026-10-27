## Summary

Describe the problem, resulting behavior and relevant compatibility changes.

## Validation

Report the checks actually run, including failures, omissions and platform limits.
See [CONTRIBUTING.md](https://github.com/chabanov/varta/blob/main/CONTRIBUTING.md)
for setup and the full verification sequence.

## Quality gates

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check -p varta-common -p varta-compiler -p varta-runtime -p varta-artifact --lib --locked`
- [ ] `cargo build -p varta-launcher --bin varta-runtime --locked`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo build --workspace --all-targets --locked`
- [ ] `cargo test --workspace --locked`
- [ ] `cargo test -p varta --test smoke --locked`

## Security and compatibility impact

Describe effects on capabilities, guest/host boundaries, validation, resource
limits, language profiles, ABI or artifact compatibility. For vulnerabilities,
use the private process in [SECURITY.md](https://github.com/chabanov/varta/blob/main/SECURITY.md).

## Documentation

- [ ] `docs/architecture.md` updated when required
- [ ] README, development plan and changelog updated when relevant
