# Agent Studio

A place in the console where a developer, together with an LLM, builds a
**personal agent** with their own tools that run on Varta, tries it, connects it
to the tenant's orchestrator (the Head Agent), deploys it and sells it. Nothing
here is implemented. This document records the owner's decisions, the
architecture that follows from them, and what is still open.

## Decisions (owner, 2026-10-30)

| # | Decision |
| --- | --- |
| D1 | The studio is built after everything else works: after Single Chat runs against production on its platforms |
| D2 | A developer builds their own agent with their own tools, and the agent can use UARP's functionality (memory, cloud storage and the rest) with full compatibility. Published to the cloud, it can be hired by other users and attached to their own Head Agent to work in their company |
| D3 | The connection between an agent and the orchestrator needs maximum compatibility with the platform, connection quality and determinism |
| D4 | The Varta cloud is designed on the expectation that UARP itself will later move entirely onto Varta |
| D5 | TS agents are compiled to Wasm by a build service in the Varta cloud (source, checks, artifact and signature in one place), not in the console |
| D6 | The agent reaches UARP services (memory, files, the bare LLM and the rest) through the UARP API with uarp-sdk, with a credential scoped to the agent and its tenant; the Varta engine stays application-agnostic (A2) |
| D7 | The orchestrator executes the agent natively: a UARP execution mode for Varta agents runs the signed artifact in the Varta cloud, with typed task input and the closed v1 run events; Bridge with the same event schema is the interim until that mode exists (A3) |
| D8 | Hiring installs the agent into the buyer's company: it works with the buyer's memory and files, under grants the buyer's admin approves from its manifest, billed to the buyer; the publisher's code and signature travel with it, the publisher's data does not; the buyer's Head Agent delegates to it as to its own (A4) |

## Architecture

Each point names the decision it follows from. A2, A3 and A4 were confirmed by
the owner as D6, D7 and D8.

**A1. The agent is one signed Varta artifact** (D2, D5). Its TS code holds the
agent's tools (`varta.tool`) and its own logic; its manifest lists every
capability it may use. The build service compiles, checks and signs it; the same
artifact runs in the cloud, on a desktop or on a Pi.

**A2. UARP services through the UARP API, not through the engine** (D2, D6, and the
Varta rule that no UARP types or logic enter engine crates). Memory, files,
knowledge, the bare LLM and the rest are reached with uarp-sdk over
`net.request`, with a credential scoped to the agent and the tenant it works
for. Varta stays application-agnostic; UARP compatibility lives in the agent's
code and in the host that runs it.

**A3. A native execution mode, not a remote worker** (D3, D4, D7).
The orchestrator runs the agent itself: UARP gains an `execution_mode` for Varta
agents and executes the signed artifact in the Varta cloud. A delegated task
arrives as typed input, the agent's progress leaves as the same closed v1 run
events Single Chat already parses strictly, and approvals, cancellation and
budgets are the platform's own. No public URL, no hand-written protocol, one
event schema everywhere. Until that mode exists, the agent can attach through
Bridge as an interim, with the same event schema.

**A4. Hiring installs the agent into the buyer's company** (D2, D8).
A hired agent runs for the buyer's tenant: it works with the buyer's memory and
files, under grants the buyer's admin approves from its manifest, with its
spend billed to the buyer. The publisher's code and signature travel with it;
the publisher's data does not. Today's marketplace invocation runs on the
publisher's side and cannot do this.

**A5. The Varta cloud is the platform runtime** (D4). One host that runs signed
artifacts with per-turn budgets and capability checks: developer agents first,
UARP's own agents and services later, on the same runtime. It needs Varta's
artifact signing (C3), remote MCP (C5) and a hosted runtime, and the API's new
execution mode (A3).

**A6. Permissions are a human step** (Safety principle). The LLM in the studio
writes code and proposes grants; only the developer adds a grant, and only the
buyer's admin approves the grants of a hired agent. Nothing widens silently.

## What exists today

From the uarp-sdk 0.13.1 resources and the Varta contracts (October 2026).

| Goal | UARP API | Varta |
| --- | --- | --- |
| 1. A bare LLM without an agent | `providers.llmChatCompletion` (`POST /api/v1/llm/chat/completions`, OpenAI-shaped, tool calls included), `listLLMModels`; Anthropic-compatible `/v1/messages`. Billed and quota-gated; spend can be attributed with `X-Agent-Id`. Users cannot bring their own key (credentials are super-admin only) | A TS program can call it through `net.request` under an exact grant |
| 2. Tools | An agent's tools come from installed SPECs, MCP servers (`mcp.createMCPServer`, `setAgentMCPServers`: the platform calls an HTTP/streamable-HTTP MCP URL, SSRF-checked, secrets encrypted) or fixed-registry integrations. An agent's model and system prompt are managed by the Head Agent | Tool Contract v1: `varta.tool({ name, description }, handler)` publishes an exported function; the schema is derived from its types; the catalog lists its transitive effects; `varta invoke` runs one tool in a fresh, budgeted instance. Parameters are `number`, `boolean`, `string` only; at most 64 tools. MCP: local stdio bridge planned (C4), remote HTTPS MCP deferred (C5) |
| 3. Attaching to the orchestrator | Bridge (`bridgeRegister`, then WebSocket or poll, task events back; `bridgeDelegate` from the Head Agent); Hire (fixed catalog, MCP worker tools `wait_for_task` / `complete_task`); A2A inbound, and external A2A workers inside squads; a capability manifest the team router reads | An application can be an HTTP/1 server or client under exact grants |
| 4. Deploying to the cloud | Nothing for hosting user code. SPEC packages (`registry.registryPublish`, `runtime_scope: cloud \| local`) are the nearest thing; their execution model is not described in the SDK, and attestation is not verified | No hosted Varta runtime in the plan. Artifact signing and trusted loading (C3) are planned, not built |
| 5. Selling | `marketplace.publishListing` for one of your own agents, with pricing (free, per run, per token, subscription); buyers invoke it cross-tenant, it runs on the publisher's side. No payout or earnings endpoints | Same artifact on every platform; signing (C3) is needed before third-party code is trusted |

## Flow

1. **Describe.** "What should your agent do?" The developer talks to the LLM;
   the LLM drafts a name, a purpose and a list of tools with typed signatures.
   The plan is shown as cards the developer edits, never as hidden state.
2. **Tools.** The LLM writes each tool as TS; the developer sees the code. Varta
   compiles it: errors come back with exact positions and the LLM fixes them in
   the same conversation. The catalog appears as forms generated from the derived
   schemas, with the effects each tool needs ("calls api.example.com", "reads
   nothing").
3. **Try.** Each tool runs with filled-in forms in a fresh instance; results,
   logs, fuel and denials are shown. Then the whole agent runs in a private chat
   on the bare LLM, its tool calls shown as the same compact lines as in Single
   Chat.
4. **Permissions.** One screen lists everything the agent may do (grants from its
   manifest). Nothing outside the list is possible; adding a grant is an explicit
   step.
5. **Connect.** The agent joins the tenant: the Head Agent can delegate to it,
   and a delegation appears in Single Chat as a delegated run with the agent's
   name.
6. **Deploy.** The same artifact runs hosted, so it works when the developer's
   machine is off.
7. **Publish.** A marketplace listing with pricing; the listing shows the
   agent's permissions and its signature.

The developer always sees one progress line: Describe → Tools → Try → Connect →
Deploy → Publish, with the current step and what the next one needs.

## What is missing, by owner

| Need | Owner |
| --- | --- |
| The build service in the Varta cloud: compile, check and sign an agent's TS (D5) | Varta core |
| The Varta cloud host: run signed artifacts with per-turn budgets and capability checks, for developer agents now and UARP itself later (D4) | Varta core (C5) and a hosting service |
| Artifact signing and trusted loading before third-party tools run or are sold | Varta core (C3) |
| Richer tool parameters (objects, arrays) for real tools | Varta core (tool contract v2) |
| The native execution mode for Varta agents: typed task input, closed v1 run events, platform approvals, cancellation and budgets (D7) | UARP API |
| A credential scoped to one agent and one tenant, for the agent's own calls to the UARP API (D6) | UARP API |
| Hiring that installs an agent into the buyer's company, with the buyer admin's approval of its grants (D8); today's marketplace invokes on the publisher's side | UARP API |
| Seller payouts and earnings | UARP API |
| Bringing your own model key for a personal agent (optional) | UARP API |

## Open

- Who builds the Varta cloud host and the build service (Varta core lane, the
  API lane, a new lane).
- Tool contract v2 (objects and arrays as tool parameters), needed for real
  tools.
