// Instantiation probe for the generic functions and methods of the pinned uarp-sdk.
//
// `scripts/check-sdk.sh` copies this file, unmodified, as the entry of the temporary
// project that holds the unmodified SDK sources under `./sdk/`. `varta check`
// compiles the body of a generic function or method only where it is instantiated,
// so the per-file probe alone would accept a generic body that holds a refused
// construct. Each exported probe below instantiates one SDK generic with a concrete
// SDK type, and with its declared default type argument where it has one.
// `check-sdk.sh` fails when the SDK's generic declarations differ from the list it
// keeps next to this probe, so a new generic cannot go unprobed.
import {
  Transport,
  UarpClient,
  autoPaginate,
  collect,
  type Agent,
  type GetHealthResponse,
  type JsonValue,
  type ListAgentsParams,
  type RequestSpec,
  type UarpEvent,
} from "./sdk/index.js";
import type { PageFetcher } from "./sdk/core/pagination.js";
import { pick } from "./sdk/core/util.js";

/** `core/pagination.ts`: `autoPaginate<T>`, as the generated `listAll` methods call it. */
export function probeAutoPaginate(fetchPage: PageFetcher): AsyncIterableIterator<Agent> {
  return autoPaginate<Agent>(fetchPage, "items", "cursor", "has_more");
}

/** `core/pagination.ts`: `collect<T>`. */
export function probeCollect(agents: AsyncIterable<Agent>): Promise<Agent[]> {
  return collect<Agent>(agents);
}

/** `index.ts`: `UarpClient.request<T = unknown>` with an explicit type argument. */
export function probeClientRequest(client: UarpClient, spec: RequestSpec): Promise<GetHealthResponse> {
  return client.request<GetHealthResponse>(spec);
}

/** `index.ts`: `UarpClient.request<T = unknown>` with its default. */
export function probeClientRequestDefault(client: UarpClient, spec: RequestSpec): Promise<unknown> {
  return client.request(spec);
}

/** `core/transport.ts`: `Transport.request<T>`. */
export function probeTransportRequest(transport: Transport, spec: RequestSpec): Promise<GetHealthResponse> {
  return transport.request<GetHealthResponse>(spec);
}

/** `core/sse.ts`: `UarpEvent.json<T = JsonValue>`, implemented by `makeEvent`. */
export function probeEventJson(event: UarpEvent): GetHealthResponse {
  return event.json<GetHealthResponse>();
}

/** `core/sse.ts`: `UarpEvent.json<T = JsonValue>` with its default. */
export function probeEventJsonDefault(event: UarpEvent): JsonValue {
  return event.json();
}

/** `core/util.ts`: `pick<T, K>`, as the generated resource methods call it. */
export function probePick(params: ListAgentsParams | undefined): Record<string, unknown> {
  return pick(params, ["workspace_id", "limit", "cursor"]);
}

export function main(): void {}
