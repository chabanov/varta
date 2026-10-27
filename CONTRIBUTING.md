# Contributing to Varta

Varta is developed by Squad AI, Inc. Its architecture and technology were created
by Pavlo Chabanov. Contributions focus on the general-purpose engine and its
development tools; application-specific features belong in separate TS/JS projects.

## Development setup

CI uses Rust **1.98.0**, with `rustfmt` and `clippy`, on Linux and macOS. Install
Rust through rustup and provide the native compiler/linker tools for your platform.
Windows and cross-target native packaging are not yet verified.

```sh
git clone https://github.com/chabanov/varta.git
cd varta
rustup toolchain install 1.98.0 --component rustfmt --component clippy
rustup override set 1.98.0
cargo build --workspace --all-targets --locked
```

The build produces both `target/debug/varta` and `target/debug/varta-runtime`.
Keep the binaries together: native packaging needs the runtime companion. No Node,
npm or external TypeScript checker is required for these development commands.
See the [README](README.md) for project creation and execution.

## Before submitting changes

Run the same checks as [CI](.github/workflows/varta.yml), from the workspace root:

```sh
cargo fmt --all -- --check
cargo check -p varta-common -p varta-compiler -p varta-runtime -p varta-artifact --lib --locked
cargo build -p varta-launcher --bin varta-runtime --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
cargo test -p varta --test smoke --locked
```

Build the runtime companion before running CLI tests separately. Preserve the
checked-in lockfile; update dependencies deliberately. On machines where parallel
tests contend for CPU, set `RUST_TEST_THREADS=4` for test commands. Do not increase
guest fuel, deadlines or memory limits to make a failing regression pass.

## Guidelines

- Keep changes focused and minimal.
- Add meaningful tests for changed behavior. Compiler features need execution of
  the generated Wasm, and capability changes need allowed and denied paths.
- Do not add dependencies without justification.
- Do not weaken denied-by-default capability security.
- Preserve the [engine/tooling boundaries](README.md#repository-scope).
- Update the [architecture contract](docs/architecture.md) when ABI, capabilities
  or configuration change. Keep the README, roadmap and changelog accurate.
- Use clear imperative commit messages.

## Pull requests

- Describe the change and why it was needed.
- Report the checks you actually ran, including failures or platform limitations.
- Explain security, ABI, language-profile or artifact compatibility impact.
- Keep unrelated changes in separate pull requests.

Search existing [issues](https://github.com/chabanov/varta/issues) before filing a
bug or feature request. For vulnerabilities, follow the private reporting process
in [SECURITY.md](SECURITY.md); do not open a public issue with exploit details.
