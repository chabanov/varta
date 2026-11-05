// The API lane's recorded event_schema=1 scenarios (fixtures/events-v1), built
// into the mock as text: the requests the console made and the session stream
// the server sent. The mock replays them as the UARP API would; the shapes are
// the fixtures' own (fixtures/events-v1/README.md), and frame data stays opaque.
import approvalApprove from "../../../fixtures/events-v1/approval_approve.json" with { type: "text" };
import askUser from "../../../fixtures/events-v1/ask_user.json" with { type: "text" };
import sessionEnd from "../../../fixtures/events-v1/session_end.json" with { type: "text" };

export interface RecordedFrame {
  event: string;
  id?: string;
  data: unknown;
}

export interface RecordedRequest {
  step: string;
  run_id?: string;
  request?: unknown;
  status: number;
  body: unknown;
  content_type: string;
}

export interface Recording {
  scenario: string;
  description: string;
  stream: string;
  recorded: string;
  not_sent_in_v1: unknown;
  requests: RecordedRequest[];
  frames: RecordedFrame[];
}

/** The scenarios the mock serves, by name. */
export function recordingText(scenario: string): string | null {
  if (scenario === "approval_approve") return approvalApprove;
  if (scenario === "ask_user") return askUser;
  if (scenario === "session_end") return sessionEnd;
  return null;
}
