//! Native consent: stored decisions, first-run approval, prompts and the
//! audit log of a packaged application (P10b).
//!
//! The runtime's permission broker decides every operation
//! ([`varta_runtime::PermissionPrompter`] is its prompt seam). The shell
//! supplies what the broker leaves to its host:
//!
//! - **Stored decisions** per application identity in a host-owned
//!   [`ConsentStore`]: read when a packaged launch is prepared (so they
//!   apply on every host, with or without a prompt) and written back after
//!   each change ([`Consent::sync`]).
//! - **First-run approval.** A host with a prompt seam is asked for every
//!   required grant without a stored decision before any guest code runs;
//!   Allow and Deny answers are stored even when the start is refused, so
//!   a refused required grant keeps the application from starting until
//!   the decision is changed.
//! - **Prompts** in a layer the guest cannot draw over or reach
//!   ([`PromptLayer`]), which the window presents in its own host window.
//!   Every requested pattern is shown in full (paged); a request that
//!   cannot be shown in full within [`MAX_RESOURCE_LINES`] is never shown:
//!   the seam answers it Deny without asking and audits an
//!   [`UNPRESENTABLE_PROMPT`] row.
//! - **Revocation** of a stored grant from outside the instance
//!   ([`ConsentStore::revoke`]): a running instance applies it to new
//!   operations when [`Consent::sync`] next rereads the file, which it does
//!   at most once per [`STORE_POLL_INTERVAL`] unless the instance's own
//!   decisions changed.
//! - **Audit log:** every broker decision of the instance, appended to the
//!   store's bounded log.
//!
//! Every read-modify-write of the decision file (an instance's sync, a
//! refused start's answers, `--revoke`) holds the store's advisory lock
//! ([`LOCK_FILE`]), so an outside denial is never overwritten by a
//! concurrent answer: the instance sees it under the lock and revokes.
//!
//! Prompts are rate-limited by the broker exactly as in the browser host:
//! `permission::MAX_OPEN_PROMPTS` open prompt and
//! `permission::MAX_DISMISSALS` dismissals before a stored denial.
mod glyphs;
mod layer;
mod store;

pub use layer::{
    MAX_QUOTE_LINES, MAX_RESOURCE_LINES, MAX_RESOURCE_PAGES, PROMPT_INPUT_DELAY, PromptKey,
    PromptLayer, PromptTooLong, RESOURCE_PAGE_LINES,
};
pub use store::{
    AUDIT_FILE, CONSENT_FILES, ConsentStore, DECISIONS_FILE, LOCK_FILE, MAX_AUDIT_LOG_BYTES,
    PREVIOUS_AUDIT_FILE,
};

use super::ShellError;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use varta_common::permission::{
    AuditEntry, DecisionSource, PermissionError, PermissionPrompt, PermissionState, PromptAnswer,
    StoredDecision,
};
use varta_runtime::{Application, PermissionPrompter, RuntimeError};

/// How often an instance rereads its decision file for revocations made
/// outside it (another `varta-shell --revoke`).
pub const STORE_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// The `refused` value of the audit row of a request that was never shown
/// because it cannot be shown in full ([`PromptTooLong`]).
pub const UNPRESENTABLE_PROMPT: &str = "prompt-too-long";

/// One answered first-run prompt.
type Answered = (String, PromptAnswer);

/// A request the seam refused to show, waiting for its audit row.
struct Unpresentable {
    error: PromptTooLong,
    at: Instant,
}

/// The consent state of one packaged application instance.
pub struct Consent {
    store: ConsentStore,
    /// The decision file as last read or written.
    observed: BTreeMap<String, StoredDecision>,
    /// The instance's decisions as of the last sync.
    baseline: BTreeMap<String, StoredDecision>,
    stamp: Option<store::Stamp>,
    polled: Option<Instant>,
    /// The last audit sequence written.
    audited: u64,
    /// When this session started, in milliseconds since the Unix epoch:
    /// the `run` of its audit rows.
    run: u64,
    started: Instant,
    /// Required prompts answered before the instance existed.
    first_run: Arc<Mutex<Vec<Answered>>>,
    /// Requests refused without a prompt, not yet audited.
    unpresentable: Arc<Mutex<Vec<Unpresentable>>>,
}

/// The prompt seam the runtime receives: the host's prompter, recording
/// first-run answers so a refused start can still store them. A request
/// that cannot be shown in full never reaches the host's prompter: it is
/// answered [`PromptAnswer::Deny`] (stored like any Deny of a grant, since
/// the package's request never changes; never stored for a `ui.open_url`
/// link) and recorded for the audit log.
struct Recording {
    inner: Box<dyn PermissionPrompter>,
    first_run: Arc<Mutex<Vec<Answered>>>,
    unpresentable: Arc<Mutex<Vec<Unpresentable>>>,
}
impl PermissionPrompter for Recording {
    fn prompt(&mut self, prompt: &PermissionPrompt) -> PromptAnswer {
        let answer = match PromptLayer::check(prompt) {
            Ok(_) => self.inner.prompt(prompt),
            Err(error) => {
                locked(&self.unpresentable).push(Unpresentable {
                    error,
                    at: Instant::now(),
                });
                PromptAnswer::Deny
            }
        };
        if prompt.required {
            locked(&self.first_run).push((prompt.capability.clone(), answer));
        }
        answer
    }
}

/// The contents of a mutex, also after a holder panicked.
fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Consent {
    /// Open the consent state of `store` and read its decisions.
    pub fn open(store: ConsentStore) -> Result<Self, ShellError> {
        let observed = store.load()?;
        Ok(Self {
            stamp: store.stamp(),
            baseline: observed.clone(),
            observed,
            store,
            polled: None,
            audited: 0,
            run: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |since| {
                    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
                }),
            started: Instant::now(),
            first_run: Arc::default(),
            unpresentable: Arc::default(),
        })
    }

    /// The store this state reads and writes.
    #[must_use]
    pub fn store(&self) -> &ConsentStore {
        &self.store
    }

    /// The stored decisions the instance starts from, as the runtime reads
    /// them.
    #[must_use]
    pub fn decision_file(&self) -> varta_common::permission::DecisionFile {
        self.store.file(&self.observed)
    }

    /// Wrap the host's prompter for the runtime.
    pub(super) fn prompter(
        &self,
        inner: Box<dyn PermissionPrompter>,
    ) -> Box<dyn PermissionPrompter> {
        Box::new(Recording {
            inner,
            first_run: self.first_run.clone(),
            unpresentable: self.unpresentable.clone(),
        })
    }

    /// A start the runtime refused: store the Allow and Deny answers of
    /// its first-run prompts (a dismissal stores nothing) and audit them,
    /// after the requests refused without a prompt.
    pub(super) fn refused(&mut self, error: &RuntimeError) -> Result<(), ShellError> {
        let mut rows = self.unpresentable_rows();
        let answered = if matches!(
            error,
            RuntimeError::Permission(PermissionError::RequiredDenied { .. })
        ) {
            std::mem::take(&mut *locked(&self.first_run))
        } else {
            Vec::new()
        };
        let mut stored = Vec::with_capacity(answered.len());
        for (sequence, (id, answer)) in (1u64..).zip(&answered) {
            let (decision, outcome) = match answer {
                PromptAnswer::Allow => (Some(StoredDecision::Allow), PermissionState::Granted),
                PromptAnswer::Deny => (Some(StoredDecision::Deny), PermissionState::Denied),
                PromptAnswer::Dismiss => (None, PermissionState::Denied),
            };
            if let Some(decision) = decision {
                stored.push((id.clone(), decision));
            }
            rows.push(self.row(&AuditEntry {
                sequence,
                at_ms: self.elapsed_ms(),
                source: DecisionSource::Prompt,
                operation: id.clone(),
                resource: String::new(),
                outcome,
            }));
        }
        if !stored.is_empty() {
            self.store.locked(|| {
                let mut decisions = self.store.load()?;
                decisions.extend(stored);
                self.store.save(&decisions)
            })?;
        }
        self.store.append_audit(&rows)
    }

    /// The audit rows of the requests refused without a prompt since the
    /// last call.
    fn unpresentable_rows(&self) -> Vec<serde_json::Value> {
        std::mem::take(&mut *locked(&self.unpresentable))
            .into_iter()
            .map(|refused| {
                serde_json::json!({
                    "artifact": self.store.identity(),
                    "run": self.run,
                    "at_ms": u64::try_from(
                        refused.at.saturating_duration_since(self.started).as_millis()
                    )
                    .unwrap_or(u64::MAX),
                    "operation": refused.error.capability,
                    "outcome": PermissionState::Denied,
                    "refused": UNPRESENTABLE_PROMPT,
                    "resource_lines": refused.error.lines,
                    "max_resource_lines": MAX_RESOURCE_LINES,
                })
            })
            .collect()
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// One audit row: the broker's entry with the identity and this run.
    fn row(&self, entry: &AuditEntry) -> serde_json::Value {
        let mut row = serde_json::to_value(entry).unwrap_or_default();
        if let Some(object) = row.as_object_mut() {
            object.insert("artifact".into(), self.store.identity().into());
            object.insert("run".into(), self.run.into());
        }
        row
    }

    /// Whether [`Self::sync`] would reread the decision file at `now`.
    #[must_use]
    pub fn poll_due(&self, now: Instant) -> bool {
        self.polled
            .is_none_or(|polled| now.saturating_duration_since(polled) >= STORE_POLL_INTERVAL)
    }

    /// Reconcile the instance with its store, between turns:
    ///
    /// 1. a denial stored from outside since the last read revokes the
    ///    grant in the instance, so new operations are denied (a stored
    ///    allow from outside applies only to later instances);
    /// 2. the instance's own new decisions (prompt answers, dismissals that
    ///    became denials, revocations) are written, except where the file
    ///    changed from outside, which wins;
    /// 3. the broker's new audit entries are appended to the log.
    ///
    /// The file is reread when the instance's decisions changed or, at
    /// most every [`STORE_POLL_INTERVAL`], when its stamp changed.
    pub fn sync(&mut self, app: &mut Application) -> Result<(), ShellError> {
        let now = Instant::now();
        let own = app.decisions();
        let changed = own != self.baseline;
        if changed || self.poll_due(now) {
            self.polled = Some(now);
            let stamp = self.store.stamp();
            if changed || stamp != self.stamp {
                self.reconcile(app)?;
            }
        }
        self.audit(app)
    }

    /// [`Self::reconcile_locked`] under the store's lock, so no other
    /// writer (`--revoke`, another instance) changes the file between the
    /// read and the write.
    fn reconcile(&mut self, app: &mut Application) -> Result<(), ShellError> {
        let store = self.store.clone();
        store.locked(|| self.reconcile_locked(app))
    }

    fn reconcile_locked(&mut self, app: &mut Application) -> Result<(), ShellError> {
        let current = self.store.load()?;
        let external: BTreeSet<String> = current
            .keys()
            .chain(self.observed.keys())
            .filter(|id| current.get(*id) != self.observed.get(*id))
            .cloned()
            .collect();
        for id in &external {
            if current.get(id) == Some(&StoredDecision::Deny)
                && app.decisions().get(id) != Some(&StoredDecision::Deny)
            {
                app.revoke(id);
            }
        }
        let own = app.decisions();
        let mut next = current;
        let mut write = false;
        for (id, decision) in &own {
            if external.contains(id) || self.baseline.get(id) == Some(decision) {
                continue;
            }
            if next.insert(id.clone(), *decision) != Some(*decision) {
                write = true;
            }
        }
        if write {
            self.store.save(&next)?;
        }
        self.observed = next;
        self.stamp = self.store.stamp();
        self.baseline = own;
        Ok(())
    }

    fn audit(&mut self, app: &Application) -> Result<(), ShellError> {
        let mut rows = self.unpresentable_rows();
        let (entries, _) = app.audit();
        let fresh: Vec<&AuditEntry> = entries
            .iter()
            .filter(|entry| entry.sequence > self.audited)
            .collect();
        let Some(last) = fresh.last().map(|entry| entry.sequence) else {
            return self.store.append_audit(&rows);
        };
        rows.reserve(fresh.len() + 1);
        // Entries the bounded broker log dropped before this sync saw them.
        let dropped = fresh[0].sequence - self.audited - 1;
        if dropped > 0 {
            rows.push(serde_json::json!({
                "artifact": self.store.identity(),
                "run": self.run,
                "dropped": dropped,
            }));
        }
        rows.extend(fresh.into_iter().map(|entry| self.row(entry)));
        self.store.append_audit(&rows)?;
        self.audited = last;
        Ok(())
    }
}
