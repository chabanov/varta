// A discriminated union of 24 closed record interfaces (a generic event
// stream): `JSON.parse(text) as Event` selects the member by its `type`
// literal, and a `switch` on it narrows each case to one member.
interface CreatedEvent { type: "created"; seq: number; text: string }
interface QueuedEvent { type: "queued"; seq: number; count: number; note?: string }
interface StartedEvent { type: "started"; seq: number; tags: string[] }
interface ProgressEvent { type: "progress"; seq: number; detail: { code: number; reason: string | null } }
interface ChunkEvent { type: "chunk"; seq: number; text: string }
interface DeltaEvent { type: "delta"; seq: number; count: number; note?: string }
interface ToolCallEvent { type: "tool_call"; seq: number; tags: string[] }
interface ToolResultEvent { type: "tool_result"; seq: number; detail: { code: number; reason: string | null } }
interface MessageEvent { type: "message"; seq: number; text: string }
interface WarningEvent { type: "warning"; seq: number; count: number; note?: string }
interface NoticeEvent { type: "notice"; seq: number; tags: string[] }
interface RetryEvent { type: "retry"; seq: number; detail: { code: number; reason: string | null } }
interface PausedEvent { type: "paused"; seq: number; text: string }
interface ResumedEvent { type: "resumed"; seq: number; count: number; note?: string }
interface CheckpointEvent { type: "checkpoint"; seq: number; tags: string[] }
interface MetricEvent { type: "metric"; seq: number; detail: { code: number; reason: string | null } }
interface LogEvent { type: "log"; seq: number; text: string }
interface HeartbeatEvent { type: "heartbeat"; seq: number; count: number; note?: string }
interface CancelledEvent { type: "cancelled"; seq: number; tags: string[] }
interface TimeoutEvent { type: "timeout"; seq: number; detail: { code: number; reason: string | null } }
interface CompletedEvent { type: "completed"; seq: number; text: string }
interface FailedEvent { type: "failed"; seq: number; count: number; note?: string }
interface ArchivedEvent { type: "archived"; seq: number; tags: string[] }
interface DeletedEvent { type: "deleted"; seq: number; detail: { code: number; reason: string | null } }
type Event =
  | CreatedEvent
  | QueuedEvent
  | StartedEvent
  | ProgressEvent
  | ChunkEvent
  | DeltaEvent
  | ToolCallEvent
  | ToolResultEvent
  | MessageEvent
  | WarningEvent
  | NoticeEvent
  | RetryEvent
  | PausedEvent
  | ResumedEvent
  | CheckpointEvent
  | MetricEvent
  | LogEvent
  | HeartbeatEvent
  | CancelledEvent
  | TimeoutEvent
  | CompletedEvent
  | FailedEvent
  | ArchivedEvent
  | DeletedEvent;
function describe(e: Event): string {
  switch (e.type) {
    case "created":
      return `created ${e.seq} ${e.text}`;
    case "queued":
      return `queued ${e.seq} ${e.count} ${e.note ?? "-"}`;
    case "started":
      return `started ${e.seq} ${e.tags.join("+")}`;
    case "progress":
      return `progress ${e.seq} ${e.detail.code} ${e.detail.reason ?? "none"}`;
    case "chunk":
      return `chunk ${e.seq} ${e.text}`;
    case "delta":
      return `delta ${e.seq} ${e.count} ${e.note ?? "-"}`;
    case "tool_call":
      return `tool_call ${e.seq} ${e.tags.join("+")}`;
    case "tool_result":
      return `tool_result ${e.seq} ${e.detail.code} ${e.detail.reason ?? "none"}`;
    case "message":
      return `message ${e.seq} ${e.text}`;
    case "warning":
      return `warning ${e.seq} ${e.count} ${e.note ?? "-"}`;
    case "notice":
      return `notice ${e.seq} ${e.tags.join("+")}`;
    case "retry":
      return `retry ${e.seq} ${e.detail.code} ${e.detail.reason ?? "none"}`;
    case "paused":
      return `paused ${e.seq} ${e.text}`;
    case "resumed":
      return `resumed ${e.seq} ${e.count} ${e.note ?? "-"}`;
    case "checkpoint":
      return `checkpoint ${e.seq} ${e.tags.join("+")}`;
    case "metric":
      return `metric ${e.seq} ${e.detail.code} ${e.detail.reason ?? "none"}`;
    case "log":
      return `log ${e.seq} ${e.text}`;
    case "heartbeat":
      return `heartbeat ${e.seq} ${e.count} ${e.note ?? "-"}`;
    case "cancelled":
      return `cancelled ${e.seq} ${e.tags.join("+")}`;
    case "timeout":
      return `timeout ${e.seq} ${e.detail.code} ${e.detail.reason ?? "none"}`;
    case "completed":
      return `completed ${e.seq} ${e.text}`;
    case "failed":
      return `failed ${e.seq} ${e.count} ${e.note ?? "-"}`;
    case "archived":
      return `archived ${e.seq} ${e.tags.join("+")}`;
    case "deleted":
      return `deleted ${e.seq} ${e.detail.code} ${e.detail.reason ?? "none"}`;
  }
}
const inputs: string[] = [
  '{"type":"created","seq":0,"text":"t0"}',
  '{"seq":11,"count":15,"type":"delta"}',
  '{"type":"notice","seq":22,"tags":["g0","g1"]}',
  '{"seq":9,"detail":{"code":165,"reason":"r15"},"type":"metric"}',
  '{"type":"completed","seq":20,"text":"t20"}',
  '{"seq":7,"count":3,"note":"n1","type":"queued"}',
  '{"type":"tool_call","seq":18,"tags":["g0"]}',
  '{"seq":5,"detail":{"code":121,"reason":null},"type":"retry"}',
  '{"type":"log","seq":16,"text":"t16"}',
  '{"seq":3,"count":63,"type":"failed"}',
  '{"type":"started","seq":14,"tags":["g0","g1","g2"]}',
  '{"seq":1,"detail":{"code":77,"reason":"r7"},"type":"tool_result"}',
  '{"type":"paused","seq":12,"text":"t12"}',
  '{"seq":23,"count":51,"note":"n17","type":"heartbeat"}',
  '{"type":"archived","seq":10,"tags":["g0","g1"]}',
  '{"seq":21,"detail":{"code":33,"reason":null},"type":"progress"}',
  '{"type":"message","seq":8,"text":"t8"}',
  '{"seq":19,"count":39,"type":"resumed"}',
  '{"type":"cancelled","seq":6,"tags":["g0"]}',
  '{"seq":17,"detail":{"code":253,"reason":"r23"},"type":"deleted"}',
  '{"type":"chunk","seq":4,"text":"t4"}',
  '{"seq":15,"count":27,"note":"n9","type":"warning"}',
  '{"type":"checkpoint","seq":2,"tags":["g0","g1","g2"]}',
  '{"seq":13,"detail":{"code":209,"reason":null},"type":"timeout"}',
];
export async function main(): Promise<void> {
  let seqs = 0;
  for (const text of inputs) {
    const e = JSON.parse(text) as Event;
    seqs += e.seq;
    console.log(describe(e));
    await Promise.resolve();
  }
  const last = JSON.parse(inputs[inputs.length - 1]) as Event;
  console.log(`${seqs} ${last.type} ${JSON.stringify(last)}`);
}
