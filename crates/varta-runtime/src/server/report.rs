//! Bounded server evidence, independent of whether a client receives its reply.
use crate::{AuditEntry, BudgetReport};
use std::time::Duration;
use varta_common::{http::MAX_REPORTED_REQUESTS, permission::MAX_AUDIT_ENTRIES};

/// What the isolated guest invocation did. A returned envelope still has to
/// pass transport framing validation; this is not proof the client received it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpRequestOutcome {
    Returned { status: i32 },
    Failed,
    Cancelled,
}

/// Owned evidence of one completed v2 HTTP worker, delivered after Store drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequestReport {
    /// Completion sequence in this server, starting at one. Request execution
    /// is serial; rejected connections that start no worker have no entry.
    pub sequence: u64,
    pub outcome: HttpRequestOutcome,
    /// Absent if request execution setup failed before a Store existed.
    pub budget: Option<BudgetReport>,
    /// Entry sequence and timestamps are local to this request's fresh broker.
    pub audit: Vec<AuditEntry>,
    pub audit_dropped: u64,
}

/// Server-wide totals and a bounded prefix of request details. Startup remains
/// in RunReport's original budget/audit fields; these counters cover workers only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HttpReport {
    pub requests: Vec<HttpRequestReport>,
    pub total_requests: u64,
    pub returned: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub total_fuel: u64,
    pub total_host_calls: u64,
    pub total_microtasks: u64,
    pub total_wall: Duration,
    pub peak_memory_bytes: usize,
    pub total_output_bytes: u64,
    /// Detail records omitted after MAX_REPORTED_REQUESTS.
    pub requests_dropped: u64,
    /// Audit entries omitted across the retained prefix and per-request logs.
    pub audit_dropped: u64,
}
impl HttpReport {
    /// Assign a sequence and retain bounded evidence. The observer receives
    /// the original per-request audit, independent of aggregate retention caps.
    pub(super) fn record(&mut self, request: &mut HttpRequestReport) {
        self.total_requests = self.total_requests.saturating_add(1);
        request.sequence = self.total_requests;
        match request.outcome {
            HttpRequestOutcome::Returned { .. } => self.returned = self.returned.saturating_add(1),
            HttpRequestOutcome::Failed => self.failed = self.failed.saturating_add(1),
            HttpRequestOutcome::Cancelled => self.cancelled = self.cancelled.saturating_add(1),
        }
        if let Some(budget) = &request.budget {
            self.total_fuel = self.total_fuel.saturating_add(budget.total_fuel);
            self.peak_memory_bytes = self.peak_memory_bytes.max(budget.peak_memory_bytes);
            self.total_output_bytes = self
                .total_output_bytes
                .saturating_add(budget.output_bytes as u64);
            // A bounded request closes one execution entry, without renewing
            // allowances across its timer/network tasks.
            for turn in &budget.turns {
                self.total_host_calls = self
                    .total_host_calls
                    .saturating_add(u64::from(turn.host_calls));
                self.total_microtasks = self.total_microtasks.saturating_add(turn.microtasks);
                self.total_wall = self.total_wall.saturating_add(turn.wall);
            }
        }
        self.audit_dropped = self.audit_dropped.saturating_add(request.audit_dropped);
        if self.requests.len() == MAX_REPORTED_REQUESTS {
            self.requests_dropped = self.requests_dropped.saturating_add(1);
            self.audit_dropped = self
                .audit_dropped
                .saturating_add(request.audit.len() as u64);
            return;
        }
        let retained = self.requests.iter().map(|r| r.audit.len()).sum::<usize>();
        let mut stored = request.clone();
        stored
            .audit
            .truncate(MAX_AUDIT_ENTRIES.saturating_sub(retained));
        let omitted = request.audit.len().saturating_sub(stored.audit.len()) as u64;
        stored.audit_dropped = stored.audit_dropped.saturating_add(omitted);
        self.audit_dropped = self.audit_dropped.saturating_add(omitted);
        self.requests.push(stored);
    }
}
