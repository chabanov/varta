// API responses copied verbatim (as compact JSON text) from the `requests` of the
// API lane's event_schema=1 recordings (e2e-canon, 2026-10-30; anonymised ids).

/** plain · send_message · 202 */
export const SEND_ACCEPTED = "{\"run_id\":\"run-1\"}";
/** approval_approve · approve · 200 */
export const APPROVED = "{\"approved\":true,\"run_id\":\"run-1\"}";
/** approval_reject · reject · 200 */
export const REJECTED = "{\"rejected\":true,\"run_id\":\"run-1\",\"reason\":\"Not now.\"}";
/** ask_user · respond · 200 */
export const RESPONDED = "{\"status\":\"ok\",\"run_id\":\"run-1\"}";
/** error · message_to_missing_session · 404 */
export const SESSION_NOT_FOUND = "{\"type\":\"about:blank\",\"title\":\"Not Found\",\"status\":404,\"detail\":\"session \\\"id-2\\\" not found\",\"correlationId\":\"corr-1\",\"correlation_id\":\"corr-1\",\"code\":\"NOT_FOUND\"}";
export const SESSION_NOT_FOUND_STATUS = 404;
/** error · message_without_content · 422 */
export const CONTENT_REQUIRED = "{\"type\":\"about:blank\",\"title\":\"Validation Error\",\"status\":422,\"detail\":\"Request validation failed: content: Required\",\"errors\":[{\"field\":\"content\",\"message\":\"Required\"}],\"correlationId\":\"corr-2\",\"correlation_id\":\"corr-2\",\"code\":\"VALIDATION_ERROR\"}";
export const CONTENT_REQUIRED_STATUS = 422;
