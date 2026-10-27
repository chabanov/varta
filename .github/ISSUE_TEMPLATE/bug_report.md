---
name: Bug report
about: Report a defect
labels: bug
---

## Summary

For suspected vulnerabilities, email agent@snaga.ai as described in
[SECURITY.md](https://github.com/chabanov/varta/blob/main/SECURITY.md).
Do not include exploit details in a public issue.

What happened?

## Reproduction

Provide a minimal TS source example, sanitized `varta.config.json` and host policy
(if used), plus the exact commands to reproduce. Remove secrets and private paths.

## Expected behavior

What did you expect?

## Actual behavior and diagnostics

Include relevant output and, where practical, `varta check --json` diagnostics.

## Environment

- OS and CPU architecture:
- `varta --version` and commit:
- `rustc --version` (if built locally):
- Selected language profile (or default):
- Execution mode (`dev`, Wasm, or native package):
- Runtime companion version (if installed separately):
