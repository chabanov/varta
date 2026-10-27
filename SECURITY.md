# Security Policy

## Reporting a vulnerability

Report suspected vulnerabilities privately to **[agent@snaga.ai](mailto:agent@snaga.ai)**,
the security contact for Squad AI, Inc., which develops Varta. Do not put exploit
details in public issues or pull requests before coordinated disclosure.

Include, where available:

- The affected commit or artifact version, language profile, OS and architecture.
- A minimal reproduction and the exact command used.
- Relevant configuration, package capabilities and any host policy ceiling.
- Expected impact and whether an untrusted guest or artifact can trigger it.
- Sanitized diagnostics or logs; omit credentials and unrelated private data.

Maintainers will assess reports and coordinate fixes and disclosure with the
reporter. There is no published response-time commitment or bug bounty program.
Ordinary bugs and feature requests may use the public issue templates.

## Supported versions

Varta is an experimental development project. Security fixes currently target the
latest development code on `main`; no released-version maintenance or backport
policy has been established. The `0.1.0` Cargo package version identifies a
development snapshot, not a production-support commitment.

## Security model and scope

Guest filesystem, network, environment, clock and process operations require
explicit grants for the exact operation and resource. When supplied, an independent
host policy further restricts package permissions; it cannot grant omitted package
authority. Wasm validation and execution limits are part of this boundary.

Reports are relevant to compiler-generated code, Wasm/ABI validation, capability
enforcement, guest memory and execution bounds, JSON/tool/HTTP boundaries, artifact
and manifest handling, packaging, and CLI/runtime orchestration. See the
[architecture contract](docs/architecture.md) for the implemented trust boundaries.

Native launchers and build tools run with the host user's OS authority and must
come from a trusted source. Wasm isolation does not make an arbitrary downloaded
native executable safe. Source-free compilation does not prevent reverse
engineering or establish artifact authenticity; signed distribution and trusted
publisher verification remain roadmap work.

## License

Varta is provided under the [MIT License](LICENSE.md), including its warranty
disclaimer. This policy does not promise complete isolation or production readiness.
