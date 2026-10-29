# Contributing to Varta

Varta is developed by Squad AI, Inc. Its architecture and technology were created
by Pavlo Chabanov. Contributions focus on the general-purpose engine and its
development tools; application-specific features belong in separate TS/JS projects.

## Development setup

CI uses Rust **1.98.0**, with `rustfmt` and `clippy`, on Linux and macOS. The
workspace declares `rust-version = "1.95.0"`, but CI does not test that minimum. Install
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

Use Python 3 and Cargo from the workspace root. During development, check the
changed component and selected behavior; complete related work as one coherent
batch before running the full gate.

```sh
python3 scripts/verify.py local varta-bigint
python3 scripts/verify.py local varta-runtime --filter array_inference
python3 scripts/verify.py feature array_inference
python3 scripts/verify.py full
```

The [verification map](docs/verification.md) explains which scope to use, broad
boundary checks and how to record evidence. Full verification matches
[CI](.github/workflows/varta.yml); workspace tests include the CLI suite once.
A separate CLI repeat is required for launcher/packaging changes or when
investigating unstable tests, rather than for every engine increment.
Documentation-only changes need document/command checks; verification-tooling
changes need focused checks of the tooling itself. Do not rerun unchanged engine
suites solely for those changes or claim old results verify new engine code.

Preserve the checked-in lockfile; update dependencies deliberately. The helper
uses RUST_TEST_THREADS=4 unless explicitly set. Do not increase guest fuel,
deadlines or memory limits to make a failing regression pass.

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
- Do not reference agent sessions or add AI co-author/attribution lines
  (for example `Co-Authored-By: Claude …`) in commits or PR descriptions.

## Pull requests

- Describe the change and why it was needed.
- Report the checks you actually ran, including failures or platform limitations.
- Explain security, ABI, language-profile or artifact compatibility impact.
- Keep unrelated changes in separate pull requests.

Search existing [issues](https://github.com/chabanov/varta/issues) before filing a
bug or feature request. For vulnerabilities, follow the private reporting process
in [SECURITY.md](SECURITY.md); do not open a public issue with exploit details.
