// Frames copied verbatim (as compact JSON text) from the event_schema=1 recordings
// of the API lane (e2e-canon, 2026-10-30; anonymised ids). Each constant names its
// scenario and event; the text is what the session stream sends as `data`.
// They follow the v1 schema of build bd5dc091 (uarp-sdk 0.13.2): the approval
// frames are the API lane's re-projection with `arguments` next to `args`.

/** plain · run_added · id run-1:added */
export const PLAIN_RUN_ADDED = "{\"run_id\":\"run-1\"}";
/** plain · run.started · id run-1:1 */
export const PLAIN_STARTED = "{\"event_id\":\"evt-1\",\"run_id\":\"run-1\",\"type\":\"run.started\",\"seq\":1,\"payload\":{},\"timestamp\":\"2026-01-01T00:00:00.693Z\",\"stream_type\":\"lifecycle\"}";
/** plain · llm.chunk · id run-1:7 */
export const PLAIN_THINKING_1 = "{\"event_id\":\"evt-4\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":7,\"payload\":{\"chunk_type\":\"thinking\",\"delta\":\"The\",\"index\":0},\"timestamp\":\"2026-01-01T00:00:03.202Z\",\"stream_type\":\"assistant\"}";
/** plain · llm.chunk · id run-1:9 */
export const PLAIN_THINKING_2 = "{\"event_id\":\"evt-5\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":9,\"payload\":{\"chunk_type\":\"thinking\",\"delta\":\" user's message is simply\",\"index\":1},\"timestamp\":\"2026-01-01T00:00:03.246Z\",\"stream_type\":\"assistant\"}";
/** plain · llm.chunk · id run-1:23 */
export const PLAIN_CONTENT = "{\"event_id\":\"evt-19\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":23,\"payload\":{\"chunk_type\":\"content\",\"delta\":\"ok\",\"index\":15},\"timestamp\":\"2026-01-01T00:00:03.384Z\",\"stream_type\":\"assistant\"}";
/** plain · session.context · id run-1:27 */
export const PLAIN_CONTEXT = "{\"event_id\":\"evt-21\",\"run_id\":\"run-1\",\"type\":\"session.context\",\"seq\":27,\"payload\":{\"session_id\":\"session-1\",\"window_tokens\":524288,\"used_tokens\":42343,\"percent\":8,\"source\":\"model\",\"updated_at\":\"2026-01-01T00:00:03.431Z\"},\"timestamp\":\"2026-01-01T00:00:03.439Z\",\"stream_type\":\"other\"}";
/** plain · run.completed · id run-1:29 */
export const PLAIN_COMPLETED = "{\"event_id\":\"evt-22\",\"run_id\":\"run-1\",\"type\":\"run.completed\",\"seq\":29,\"payload\":{\"status\":\"completed\",\"output\":{\"response\":\"ok\"},\"metrics\":{\"duration_ms\":2763,\"steps_count\":1,\"input_tokens\":42343,\"output_tokens\":49,\"thinking_tokens\":0,\"tool_calls_count\":0,\"llm_calls_count\":1,\"guardrail_checks\":0,\"guardrail_violations\":0,\"memory_retrievals\":1,\"memory_extractions\":0,\"total_cost_usd\":0.00892633},\"cost_usd\":0.00892633},\"timestamp\":\"2026-01-01T00:00:03.449Z\",\"stream_type\":\"lifecycle\"}";
/** plain · run_done · id run-1:done */
export const PLAIN_RUN_DONE = "{\"run_id\":\"run-1\",\"status\":\"completed\",\"metrics\":{\"duration_ms\":2763,\"steps_count\":1,\"input_tokens\":42343,\"output_tokens\":49,\"thinking_tokens\":0,\"tool_calls_count\":0,\"llm_calls_count\":1,\"guardrail_checks\":0,\"guardrail_violations\":0,\"memory_retrievals\":1,\"memory_extractions\":0,\"total_cost_usd\":0.00892633,\"pricing_confidence\":\"inferred\"},\"cost_usd\":0.00892633}";
/** approval_approve · llm.chunk · id run-1:28 */
export const APPROVE_TOOL_CHUNK = "{\"event_id\":\"evt-24\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":28,\"payload\":{\"chunk_type\":\"tool_call\",\"delta\":\"{\\\"action\\\":\\\"get\\\",\\\"agent_id\\\":\\\"agent-probe\\\"}\",\"index\":20},\"timestamp\":\"2026-01-01T00:00:04.159Z\",\"stream_type\":\"assistant\"}";
/** approval_approve · run.awaiting_approval · id run-1:31 */
export const APPROVE_AWAITING = "{\"event_id\":\"evt-25\",\"run_id\":\"run-1\",\"type\":\"run.awaiting_approval\",\"seq\":31,\"payload\":{\"tool_calls\":[{\"id\":\"call_rj0hs7ky\",\"name\":\"manage_permissions\",\"arguments\":{\"action\":\"get\",\"agent_id\":\"agent-probe\"},\"args\":{\"action\":\"get\",\"agent_id\":\"agent-probe\"}}]},\"timestamp\":\"2026-01-01T00:00:04.279Z\",\"stream_type\":\"lifecycle\"}";
/** approval_approve · run.started · id run-1:1 */
export const APPROVE_STARTED = "{\"event_id\":\"evt-1\",\"run_id\":\"run-1\",\"type\":\"run.started\",\"seq\":1,\"payload\":{},\"timestamp\":\"2026-01-01T00:00:00.699Z\",\"stream_type\":\"lifecycle\"}";
/** approval_approve · run.started · id run-1:33 */
export const APPROVE_RESUMED = "{\"event_id\":\"evt-26\",\"run_id\":\"run-1\",\"type\":\"run.started\",\"seq\":33,\"payload\":{\"resumed_from\":\"awaiting_approval\",\"approved\":true},\"timestamp\":\"2026-01-01T00:00:04.658Z\",\"stream_type\":\"lifecycle\"}";
/** approval_approve · run.started · id run-1:34 */
export const APPROVE_RESTARTED = "{\"event_id\":\"evt-27\",\"run_id\":\"run-1\",\"type\":\"run.started\",\"seq\":34,\"payload\":{},\"timestamp\":\"2026-01-01T00:00:04.809Z\",\"stream_type\":\"lifecycle\"}";
/** approval_approve · tool.call · id run-1:37 */
export const APPROVE_TOOL_CALL = "{\"event_id\":\"evt-29\",\"run_id\":\"run-1\",\"type\":\"tool.call\",\"seq\":37,\"payload\":{\"tool_call\":{\"id\":\"call_rj0hs7ky\",\"name\":\"manage_permissions\",\"arguments\":{\"action\":\"get\",\"agent_id\":\"agent-probe\"}},\"builtin\":true,\"_event_id\":\"evt-30\"},\"timestamp\":\"2026-01-01T00:00:06.554Z\",\"stream_type\":\"tool\"}";
/** approval_approve · tool.result · id run-1:39 */
export const APPROVE_TOOL_RESULT = "{\"event_id\":\"evt-31\",\"run_id\":\"run-1\",\"type\":\"tool.result\",\"seq\":39,\"payload\":{\"tool_call_id\":\"call_rj0hs7ky\",\"tool_result\":{\"content\":\"{\\\"agent_id\\\":\\\"agent-probe\\\",\\\"exists\\\":false,\\\"permissions\\\":{}}\"},\"latency_ms\":11.658149000257254,\"builtin\":true,\"parent_event_id\":\"evt-30\"},\"timestamp\":\"2026-01-01T00:00:06.569Z\",\"stream_type\":\"tool\"}";
/** approval_reject · run.failed · id run-1:40 */
export const REJECT_FAILED = "{\"event_id\":\"evt-33\",\"run_id\":\"run-1\",\"type\":\"run.failed\",\"seq\":40,\"payload\":{\"error\":\"Not now.\",\"error_code\":\"approval_rejected\",\"rejected\":true},\"timestamp\":\"2026-01-01T00:00:04.243Z\",\"stream_type\":\"lifecycle\"}";
/** ask_user · run.awaiting_input · id run-1:28 */
export const ASK_AWAITING = "{\"event_id\":\"evt-22\",\"run_id\":\"run-1\",\"type\":\"run.awaiting_input\",\"seq\":28,\"payload\":{\"tool_call_id\":\"call_jra9xjtz\",\"question\":\"A or B?\",\"options\":[\"A\",\"B\"],\"context\":\"\"},\"timestamp\":\"2026-01-01T00:00:04.560Z\",\"stream_type\":\"lifecycle\"}";
/** ask_user · run.input_received · id run-1:31 */
export const ASK_RECEIVED = "{\"event_id\":\"evt-24\",\"run_id\":\"run-1\",\"type\":\"run.input_received\",\"seq\":31,\"payload\":{\"response_length\":1},\"timestamp\":\"2026-01-01T00:00:04.753Z\",\"stream_type\":\"lifecycle\"}";
/** delegation · child_run.completed · id run-1:45 */
export const DELEGATION_CHILD = "{\"event_id\":\"evt-37\",\"run_id\":\"run-1\",\"type\":\"child_run.completed\",\"seq\":45,\"payload\":{\"child_run_id\":\"id-2\",\"agent_id\":\"agent-research\",\"agent_name\":\"Canon Research Agent\",\"status\":\"completed\",\"output_preview\":\"ok\",\"duration_ms\":1886},\"timestamp\":\"2026-01-01T00:00:06.377Z\",\"stream_type\":\"other\"}";
/** error · run.cancelled · id run-1:8 */
export const ERROR_CANCELLED = "{\"event_id\":\"evt-4\",\"run_id\":\"run-1\",\"type\":\"run.cancelled\",\"seq\":8,\"payload\":{\"error\":\"Stopped by a person.\",\"error_code\":\"RUN_CANCELLED\"},\"timestamp\":\"2026-01-01T00:00:02.865Z\",\"stream_type\":\"lifecycle\"}";
/** session_end · session_closed · id session_closed */
export const END_SESSION_CLOSED = "{\"session_id\":\"session-1\"}";
/** approval_reject · run.awaiting_approval · id run-1:38 */
export const REJECT_AWAITING = "{\"event_id\":\"evt-32\",\"run_id\":\"run-1\",\"type\":\"run.awaiting_approval\",\"seq\":38,\"payload\":{\"tool_calls\":[{\"id\":\"call_z6jje71g\",\"name\":\"manage_permissions\",\"arguments\":{\"action\":\"get\",\"agent_id\":\"agent-probe\"},\"args\":{\"action\":\"get\",\"agent_id\":\"agent-probe\"}}]},\"timestamp\":\"2026-01-01T00:00:04.073Z\",\"stream_type\":\"lifecycle\"}";
/** wire, 2026-10-30 build 3d07d69d · GET /sessions/<missing>/events · error · id error-0 */
export const WIRE_SESSION_NOT_FOUND = "{\"error\":\"Session not found\",\"code\":\"SESSION_NOT_FOUND\"}";
/** approval_approve · llm.chunk content · ids run-1:69 … run-1:74 */
export const APPROVE_CONTENT: string[] = [
  "{\"event_id\":\"evt-57\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":69,\"payload\":{\"chunk_type\":\"content\",\"delta\":\"Result: no\",\"index\":24},\"timestamp\":\"2026-01-01T00:00:07.750Z\",\"stream_type\":\"assistant\"}",
  "{\"event_id\":\"evt-58\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":70,\"payload\":{\"chunk_type\":\"content\",\"delta\":\" permission record\",\"index\":25},\"timestamp\":\"2026-01-01T00:00:07.759Z\",\"stream_type\":\"assistant\"}",
  "{\"event_id\":\"evt-59\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":71,\"payload\":{\"chunk_type\":\"content\",\"delta\":\" exists for that\",\"index\":26},\"timestamp\":\"2026-01-01T00:00:07.768Z\",\"stream_type\":\"assistant\"}",
  "{\"event_id\":\"evt-60\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":72,\"payload\":{\"chunk_type\":\"content\",\"delta\":\" agent (`\",\"index\":27},\"timestamp\":\"2026-01-01T00:00:07.779Z\",\"stream_type\":\"assistant\"}",
  "{\"event_id\":\"evt-61\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":73,\"payload\":{\"chunk_type\":\"content\",\"delta\":\"exists: false`,\",\"index\":28},\"timestamp\":\"2026-01-01T00:00:07.792Z\",\"stream_type\":\"assistant\"}",
  "{\"event_id\":\"evt-62\",\"run_id\":\"run-1\",\"type\":\"llm.chunk\",\"seq\":74,\"payload\":{\"chunk_type\":\"content\",\"delta\":\" empty permissions).\",\"index\":29},\"timestamp\":\"2026-01-01T00:00:07.808Z\",\"stream_type\":\"assistant\"}",
];
/** approval_approve · run.completed · id run-1:80 */
export const APPROVE_COMPLETED = "{\"event_id\":\"evt-65\",\"run_id\":\"run-1\",\"type\":\"run.completed\",\"seq\":80,\"payload\":{\"status\":\"completed\",\"output\":{\"response\":\"Result: no permission record exists for that agent (`exists: false`, empty permissions).\"},\"metrics\":{\"duration_ms\":6710,\"steps_count\":3,\"input_tokens\":63033,\"output_tokens\":207,\"thinking_tokens\":0,\"cached_tokens\":26752,\"tool_calls_count\":1,\"llm_calls_count\":4,\"guardrail_checks\":0,\"guardrail_violations\":0,\"memory_retrievals\":2,\"memory_extractions\":0,\"total_cost_usd\":0.008887494},\"cost_usd\":0.008887494},\"timestamp\":\"2026-01-01T00:00:07.938Z\",\"stream_type\":\"lifecycle\"}";
