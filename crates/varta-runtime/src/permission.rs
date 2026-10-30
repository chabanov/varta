//! The permission broker: the one decision point for the capability checks
//! of the operations v37 adds or rebinds (`net.request` behind `fetch`,
//! `time.timer` behind the timers, `time.now` behind `Date.now()` and the
//! `varta.time.now` clock), and, in v38 (`turn_budgets`), permission broker
//! v1: per-grant states, `varta.permissions.query`/`request`, required and
//! optional grants, stored decisions keyed by the artifact hash, a prompt
//! seam and an audit log.
//!
//! Every decision is taken before the operation has any effect (no name
//! resolution, socket, armed timer or clock read precedes it), and a denial
//! is delivered to the guest as a recoverable `NotAllowedError` (or a failed
//! host status for `varta.time.now`) instead of ending the execution.
//!
//! The broker decides in this order: (1) the package request intersected with
//! the host ceiling, exactly as before (outside it: `denied`, never a
//! prompt); (2) the stored decision for this application identity; (3) the
//! prompt seam of an application instance, or `denied` where none exists
//! (headless and server hosts). A user decision is checked against the
//! request and the ceiling like any grant, so a prompt can never exceed them.
use crate::policy::Policy;
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;
use varta_common::permission::{
    AuditEntry, Consent, DESCRIPTOR_NO_RESOURCE, DESCRIPTOR_RESOURCE, DESCRIPTOR_SEPARATOR,
    DESCRIPTOR_VERSION, DecisionSource, MAX_AUDIT_ENTRIES, MAX_AUDIT_RESOURCE_BYTES,
    MAX_DISMISSALS, MAX_OPEN_PROMPTS, PermissionError, PermissionPrompt, PermissionState,
    PromptAnswer, StoredDecision,
};
use varta_view::Access;

/// The host prompt seam: a target's own prompt UI (outside the guest's
/// control). The broker calls it between turns, never while guest code runs,
/// so a prompt consumes no fuel and no turn deadline. Headless and server
/// hosts install none; then nothing is ever prompted.
pub trait PermissionPrompter: Send {
    fn prompt(&mut self, prompt: &PermissionPrompt) -> PromptAnswer;
}

/// A broker answer.
pub(crate) enum Decision<T> {
    /// The operation may proceed, with what the check produced (the parsed
    /// URL of a request).
    Granted(T),
    /// The operation is refused; the guest receives a recoverable denial.
    Denied,
}

/// Where a view capability prompt may be answered (v38 `view_protocol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewPrompts {
    /// Between turns, through the instance's prompt seam.
    pub seam: bool,
    /// By the simulated platform of a scripted headless session.
    pub simulated: bool,
}

/// The clock capability `Date.now()` (v37 `date_now`) and `varta.time.now`
/// require.
pub(crate) const CLOCK_CAPABILITY: &str = "time.now";

/// How the broker of one instance is set up.
#[derive(Default)]
pub(crate) struct BrokerSetup {
    /// Permission broker v1 (v38): states, consents and stored decisions.
    pub v1: bool,
    /// The application identity (`sha256:` digest of the portable Wasm).
    pub artifact: String,
    pub consents: BTreeMap<String, Consent>,
    pub stored: BTreeMap<String, StoredDecision>,
    pub prompter: Option<Box<dyn PermissionPrompter>>,
}

/// A prompt opened by `request()` in a user-gesture turn, answered between
/// turns on the one-shot event source `source`.
struct PendingPrompt {
    source: u64,
    capability: String,
}

pub(crate) struct Broker {
    v1: bool,
    artifact: String,
    consents: BTreeMap<String, Consent>,
    /// The state of every requested grant (v1).
    states: BTreeMap<String, PermissionState>,
    /// Stored decisions, including the answers given in this instance.
    stored: BTreeMap<String, StoredDecision>,
    prompter: Option<Box<dyn PermissionPrompter>>,
    dismissals: BTreeMap<String, u32>,
    pending: Vec<PendingPrompt>,
    /// Whether the current turn is a user-gesture turn.
    pub gesture: bool,
    audit: VecDeque<AuditEntry>,
    dropped: u64,
    sequence: u64,
    started: Instant,
}

impl Broker {
    /// Decide the state of every requested grant before any guest code runs.
    /// A refused required grant means the application cannot run.
    pub fn new(
        setup: BrokerSetup,
        policy: &mut Policy,
        started: Instant,
    ) -> Result<Self, PermissionError> {
        let mut broker = Self {
            v1: setup.v1,
            artifact: setup.artifact,
            consents: setup.consents,
            states: BTreeMap::new(),
            stored: setup.stored,
            prompter: setup.prompter,
            dismissals: BTreeMap::new(),
            pending: Vec::new(),
            gesture: false,
            audit: VecDeque::new(),
            dropped: 0,
            sequence: 0,
            started,
        };
        if !broker.v1 {
            return Ok(broker);
        }
        let ids: Vec<String> = broker.consents.keys().cloned().collect();
        for id in ids {
            let (state, source) = broker.initial(&id, policy)?;
            broker.audit(source, &id, "", state);
            broker.set(&id, state, policy);
        }
        Ok(broker)
    }

    fn initial(
        &mut self,
        id: &str,
        policy: &Policy,
    ) -> Result<(PermissionState, DecisionSource), PermissionError> {
        let consent = self
            .consents
            .get(id)
            .cloned()
            .unwrap_or_else(Consent::required);
        if !policy.requested_open(id) {
            // Outside the request or the ceiling: denied, never prompted. A
            // required grant the host ceiling refuses still leaves the
            // operation denied, exactly as in earlier profiles.
            return Ok((PermissionState::Denied, DecisionSource::Policy));
        }
        let stored = self.stored.get(id).copied();
        if consent.required {
            return match stored {
                Some(StoredDecision::Deny) => Err(PermissionError::RequiredDenied {
                    id: id.to_owned(),
                    source_label: "stored decision".into(),
                }),
                Some(StoredDecision::Allow) => {
                    Ok((PermissionState::Granted, DecisionSource::Stored))
                }
                None if self.prompter.is_some() => self.first_run(id, policy, &consent),
                // Headless and server hosts grant required permissions by
                // policy: the package request within the host ceiling.
                None => Ok((PermissionState::Granted, DecisionSource::Policy)),
            };
        }
        Ok(match stored {
            Some(StoredDecision::Allow) => (PermissionState::Granted, DecisionSource::Stored),
            Some(StoredDecision::Deny) => (PermissionState::Denied, DecisionSource::Stored),
            None if self.prompter.is_some() => (PermissionState::Prompt, DecisionSource::Policy),
            // Nothing unresolved is ever granted without a prompt.
            None => (PermissionState::Denied, DecisionSource::Policy),
        })
    }

    /// A required grant without a stored decision on a host with a prompt
    /// seam is asked once, before the application starts.
    fn first_run(
        &mut self,
        id: &str,
        policy: &Policy,
        consent: &Consent,
    ) -> Result<(PermissionState, DecisionSource), PermissionError> {
        let prompt = PermissionPrompt {
            artifact: self.artifact.clone(),
            capability: id.to_owned(),
            scope: policy.requested_scope(id),
            purpose: consent.purpose.clone(),
            required: true,
        };
        let answer = match &mut self.prompter {
            Some(prompter) => prompter.prompt(&prompt),
            None => PromptAnswer::Dismiss,
        };
        if answer == PromptAnswer::Allow {
            self.stored.insert(id.to_owned(), StoredDecision::Allow);
            return Ok((PermissionState::Granted, DecisionSource::Prompt));
        }
        if answer == PromptAnswer::Deny {
            self.stored.insert(id.to_owned(), StoredDecision::Deny);
        }
        self.audit(DecisionSource::Prompt, id, "", PermissionState::Denied);
        Err(PermissionError::RequiredDenied {
            id: id.to_owned(),
            source_label: "refused at first run".into(),
        })
    }

    fn set(&mut self, id: &str, state: PermissionState, policy: &mut Policy) {
        self.states.insert(id.to_owned(), state);
        policy.withhold(id, state != PermissionState::Granted);
    }

    fn audit(
        &mut self,
        source: DecisionSource,
        operation: &str,
        resource: &str,
        outcome: PermissionState,
    ) {
        self.sequence += 1;
        if self.audit.len() >= MAX_AUDIT_ENTRIES {
            self.audit.pop_front();
            self.dropped += 1;
        }
        let mut resource = resource.to_owned();
        if resource.len() > MAX_AUDIT_RESOURCE_BYTES {
            let mut end = MAX_AUDIT_RESOURCE_BYTES;
            while !resource.is_char_boundary(end) {
                end -= 1;
            }
            resource.truncate(end);
        }
        self.audit.push_back(AuditEntry {
            sequence: self.sequence,
            at_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            source,
            operation: operation.to_owned(),
            resource,
            outcome,
        });
    }

    fn operation<T>(&mut self, id: &str, resource: &str, decision: Decision<T>) -> Decision<T> {
        let outcome = match decision {
            Decision::Granted(_) => PermissionState::Granted,
            Decision::Denied => PermissionState::Denied,
        };
        let source = match self.states.get(id) {
            Some(_) if self.stored.contains_key(id) => DecisionSource::Stored,
            _ => DecisionSource::Policy,
        };
        self.audit(source, id, resource, outcome);
        decision
    }

    /// Arming a timer (`time.timer`, an exact boolean grant).
    pub fn timer(&mut self, policy: &Policy) -> Decision<()> {
        let id = varta_common::scheduler::TIMER_CAPABILITY;
        let decision = if policy.enabled(id) {
            Decision::Granted(())
        } else {
            Decision::Denied
        };
        self.operation(id, "", decision)
    }

    /// Reading the wall clock (`time.now`, an exact boolean grant).
    pub fn clock(&mut self, policy: &Policy) -> Decision<()> {
        let decision = if policy.enabled(CLOCK_CAPABILITY) {
            Decision::Granted(())
        } else {
            Decision::Denied
        };
        self.operation(CLOCK_CAPABILITY, "", decision)
    }

    /// One network request (`net.request`: URL scopes and an optional method
    /// list); a granted request carries the URL the transport must use.
    pub fn request(&mut self, policy: &Policy, url: &str, method: &str) -> Decision<url::Url> {
        let decision = match policy.request(url, method) {
            Some(url) => Decision::Granted(url),
            None => Decision::Denied,
        };
        self.operation(
            varta_common::net::REQUEST_CAPABILITY,
            &format!("{method} {url}"),
            decision,
        )
    }

    /// `ui.open_url` at an activation (v38 `view_protocol`): explicit
    /// patterns of the request and the ceiling grant by policy; a URL matched
    /// only by the any-HTTPS form prompts with the full URL on every
    /// activation and is never stored. The prompt goes through the prompt
    /// seam when `prompts` allows it (between turns); without a seam a
    /// simulated platform (a scripted headless session) answers it and
    /// reports through [`Self::prompted`]; any other host denies. Every
    /// outcome is audited and reaches the guest as a `command_result`.
    pub fn open_url(&mut self, policy: &Policy, url: &str, prompts: ViewPrompts) -> Access {
        let id = varta_common::view::OPEN_URL_CAPABILITY;
        match policy.open_url(url) {
            Access::Granted => {
                self.operation(id, url, Decision::Granted(()));
                Access::Granted
            }
            Access::Denied => {
                self.operation::<()>(id, url, Decision::Denied);
                Access::Denied
            }
            Access::Prompt => {
                if prompts.seam
                    && let Some(prompter) = &mut self.prompter
                {
                    let prompt = PermissionPrompt {
                        artifact: self.artifact.clone(),
                        capability: id.to_owned(),
                        scope: vec![url.to_owned()],
                        purpose: self.consents.get(id).and_then(|c| c.purpose.clone()),
                        required: false,
                    };
                    let allowed = prompter.prompt(&prompt) == PromptAnswer::Allow;
                    self.prompted(id, url, allowed);
                    return if allowed {
                        Access::Granted
                    } else {
                        Access::Denied
                    };
                }
                if prompts.simulated {
                    return Access::Prompt;
                }
                self.operation::<()>(id, url, Decision::Denied);
                Access::Denied
            }
        }
    }

    /// The answer to a view capability prompt: audited, never stored.
    pub fn prompted(&mut self, id: &str, resource: &str, allowed: bool) {
        let outcome = if allowed {
            PermissionState::Granted
        } else {
            PermissionState::Denied
        };
        self.audit(DecisionSource::Prompt, id, resource, outcome);
    }

    /// One `copy_text` (v38 `clipboard.write`, an exact boolean grant).
    pub fn clipboard_write(&mut self, policy: &Policy) -> Access {
        let id = varta_common::view::CLIPBOARD_WRITE_CAPABILITY;
        if policy.enabled(id) {
            self.operation(id, "", Decision::Granted(()));
            Access::Granted
        } else {
            self.operation::<()>(id, "", Decision::Denied);
            Access::Denied
        }
    }

    /// The state of a concrete descriptor; `None` for a malformed one.
    pub fn state(&self, policy: &Policy, descriptor: &Descriptor) -> Option<PermissionState> {
        let covered = policy.covers(&descriptor.name, descriptor.resource.as_deref())?;
        if !covered {
            return Some(PermissionState::Denied);
        }
        Some(
            self.states
                .get(&descriptor.name)
                .copied()
                .unwrap_or(PermissionState::Denied),
        )
    }

    /// Whether `request()` may open a prompt for `id` now.
    pub fn may_prompt(&self, id: &str) -> bool {
        self.gesture
            && self.prompter.is_some()
            && self.pending.len() < MAX_OPEN_PROMPTS
            && self.dismissals.get(id).copied().unwrap_or(0) < MAX_DISMISSALS
    }

    pub fn open_prompt(&mut self, source: u64, id: &str) {
        self.pending.push(PendingPrompt {
            source,
            capability: id.to_owned(),
        });
    }

    pub fn prompts_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Ask every open prompt between turns; each answer is the record of its
    /// one-shot source, delivered as a host task.
    pub fn resolve(&mut self, policy: &mut Policy) -> Vec<(u64, PermissionState)> {
        let pending = std::mem::take(&mut self.pending);
        let mut answers = Vec::with_capacity(pending.len());
        for prompt in pending {
            let id = prompt.capability;
            let shown = PermissionPrompt {
                artifact: self.artifact.clone(),
                capability: id.clone(),
                scope: policy.requested_scope(&id),
                purpose: self.consents.get(&id).and_then(|c| c.purpose.clone()),
                required: false,
            };
            let answer = match &mut self.prompter {
                Some(prompter) => prompter.prompt(&shown),
                None => PromptAnswer::Dismiss,
            };
            let state = match answer {
                PromptAnswer::Allow => {
                    self.stored.insert(id.clone(), StoredDecision::Allow);
                    PermissionState::Granted
                }
                PromptAnswer::Deny => {
                    self.stored.insert(id.clone(), StoredDecision::Deny);
                    PermissionState::Denied
                }
                PromptAnswer::Dismiss => {
                    let count = self.dismissals.entry(id.clone()).or_insert(0);
                    *count += 1;
                    if *count >= MAX_DISMISSALS {
                        // Repeated refusals become a stored denial.
                        self.stored.insert(id.clone(), StoredDecision::Deny);
                        PermissionState::Denied
                    } else {
                        PermissionState::Prompt
                    }
                }
            };
            self.audit(DecisionSource::Prompt, &id, "", state);
            self.set(&id, state, policy);
            answers.push((prompt.source, state));
        }
        answers
    }

    /// Revoke a grant at runtime: new operations are denied; operations
    /// already authorized keep running.
    pub fn revoke(&mut self, id: &str, policy: &mut Policy) -> bool {
        if !self.v1 || !self.states.contains_key(id) {
            return false;
        }
        self.stored.insert(id.to_owned(), StoredDecision::Deny);
        self.audit(DecisionSource::Stored, id, "", PermissionState::Denied);
        self.set(id, PermissionState::Denied, policy);
        true
    }

    pub fn audit_log(&self) -> (Vec<AuditEntry>, u64) {
        (self.audit.iter().cloned().collect(), self.dropped)
    }

    pub fn decisions(&self) -> BTreeMap<String, StoredDecision> {
        self.stored.clone()
    }

    pub fn artifact(&self) -> &str {
        &self.artifact
    }
}

/// A parsed permission descriptor (`varta_common::permission`).
pub(crate) struct Descriptor {
    pub name: String,
    pub resource: Option<String>,
}

/// Parse `version \n name \n flag \n resource`. A malformed structure is
/// fatal (only a tampered or miscompiled module can produce one); an unknown
/// name is an ordinary invalid descriptor the guest reports as `TypeError`.
pub(crate) fn descriptor(text: &str) -> Result<Descriptor, varta_common::events::EventError> {
    let mut fields = text.splitn(4, DESCRIPTOR_SEPARATOR);
    let malformed = varta_common::events::EventError::State;
    if fields.next() != Some(DESCRIPTOR_VERSION) {
        return Err(malformed);
    }
    let name = fields.next().ok_or(malformed.clone())?.to_owned();
    let resource = match (fields.next(), fields.next()) {
        (Some(DESCRIPTOR_NO_RESOURCE), Some("")) => None,
        (Some(DESCRIPTOR_RESOURCE), Some(resource)) => Some(resource.to_owned()),
        _ => return Err(malformed),
    };
    Ok(Descriptor { name, resource })
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_common::{CapabilityGrant, CapabilityValue};

    struct Answers(
        Vec<PromptAnswer>,
        std::sync::Arc<std::sync::Mutex<Vec<PermissionPrompt>>>,
    );
    impl PermissionPrompter for Answers {
        fn prompt(&mut self, prompt: &PermissionPrompt) -> PromptAnswer {
            self.1.lock().unwrap().push(prompt.clone());
            self.0.remove(0)
        }
    }

    fn grants() -> Vec<CapabilityGrant> {
        vec![
            CapabilityGrant {
                id: "time.timer".into(),
                value: CapabilityValue::Boolean(true),
            },
            CapabilityGrant {
                id: "env.get".into(),
                value: CapabilityValue::PathGlobs(vec!["APP_*".into()]),
            },
        ]
    }
    fn consents(optional: &str) -> BTreeMap<String, Consent> {
        grants()
            .iter()
            .map(|g| {
                (
                    g.id.clone(),
                    Consent {
                        required: g.id != optional,
                        purpose: Some("why".into()),
                    },
                )
            })
            .collect()
    }
    fn setup(
        optional: &str,
        stored: &[(&str, StoredDecision)],
        prompter: Option<Box<dyn PermissionPrompter>>,
    ) -> BrokerSetup {
        BrokerSetup {
            v1: true,
            artifact: "sha256:test".into(),
            consents: consents(optional),
            stored: stored
                .iter()
                .map(|(id, d)| ((*id).to_owned(), *d))
                .collect(),
            prompter,
        }
    }
    fn query(
        broker: &Broker,
        policy: &Policy,
        name: &str,
        resource: Option<&str>,
    ) -> Option<PermissionState> {
        broker.state(
            policy,
            &Descriptor {
                name: name.into(),
                resource: resource.map(Into::into),
            },
        )
    }

    #[test]
    fn headless_states_follow_policy_and_stored_decisions() {
        let grants = grants();
        let mut policy = Policy::new(&grants);
        let broker = Broker::new(setup("env.get", &[], None), &mut policy, Instant::now()).unwrap();
        // Required: granted by policy. Optional without a decision: denied
        // (headless hosts never prompt), so the operation is withheld.
        assert_eq!(
            query(&broker, &policy, "time.timer", None),
            Some(PermissionState::Granted)
        );
        assert_eq!(
            query(&broker, &policy, "env.get", Some("APP_X")),
            Some(PermissionState::Denied)
        );
        assert!(!policy.env("env.get", "APP_X"));
        // Outside the request: denied; malformed descriptors: none.
        assert_eq!(
            query(&broker, &policy, "env.get", Some("OTHER")),
            Some(PermissionState::Denied)
        );
        assert_eq!(
            query(&broker, &policy, "time.now", None),
            Some(PermissionState::Denied)
        );
        assert_eq!(query(&broker, &policy, "time.timer", Some("x")), None);
        assert_eq!(query(&broker, &policy, "net.unknown", None), None);

        let mut policy = Policy::new(&grants);
        let allowed = Broker::new(
            setup("env.get", &[("env.get", StoredDecision::Allow)], None),
            &mut policy,
            Instant::now(),
        )
        .unwrap();
        assert_eq!(
            query(&allowed, &policy, "env.get", Some("APP_X")),
            Some(PermissionState::Granted)
        );
        assert!(policy.env("env.get", "APP_X"));
        // A stored allow never widens the request.
        assert!(!policy.env("env.get", "OTHER"));
        let (audit, dropped) = allowed.audit_log();
        assert_eq!(dropped, 0);
        assert!(audit.iter().any(|e| e.operation == "env.get"
            && e.source == DecisionSource::Stored
            && e.outcome == PermissionState::Granted));

        let mut policy = Policy::new(&grants);
        let denied = Broker::new(
            setup("env.get", &[("time.timer", StoredDecision::Deny)], None),
            &mut policy,
            Instant::now(),
        );
        assert!(
            matches!(denied, Err(PermissionError::RequiredDenied { id, .. }) if id == "time.timer")
        );
    }

    #[test]
    fn prompts_never_exceed_the_ceiling_and_dismissals_become_denials() {
        let grants = grants();
        // The ceiling grants only the timer: env.get stays denied and is
        // never prompted, whatever the user would answer.
        let ceiling = vec![grants[0].clone()];
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut policy = Policy::with_ceiling(&grants, Some(&ceiling));
        let prompter = Answers(vec![PromptAnswer::Allow], seen.clone());
        let mut broker = Broker::new(
            setup("env.get", &[], Some(Box::new(prompter))),
            &mut policy,
            Instant::now(),
        )
        .unwrap();
        // The required timer was asked once at first run and allowed.
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert_eq!(seen.lock().unwrap()[0].capability, "time.timer");
        assert_eq!(
            query(&broker, &policy, "env.get", Some("APP_X")),
            Some(PermissionState::Denied)
        );
        broker.gesture = true;
        assert!(broker.may_prompt("env.get"));
        assert_eq!(broker.states["env.get"], PermissionState::Denied);

        // Without a ceiling the optional grant is in prompt state; three
        // dismissals become a stored denial.
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut policy = Policy::new(&grants);
        let prompter = Answers(
            vec![
                PromptAnswer::Allow,
                PromptAnswer::Dismiss,
                PromptAnswer::Dismiss,
                PromptAnswer::Dismiss,
            ],
            seen.clone(),
        );
        let mut broker = Broker::new(
            setup("env.get", &[], Some(Box::new(prompter))),
            &mut policy,
            Instant::now(),
        )
        .unwrap();
        assert_eq!(
            query(&broker, &policy, "env.get", Some("APP_X")),
            Some(PermissionState::Prompt)
        );
        broker.gesture = true;
        for expected in [
            PermissionState::Prompt,
            PermissionState::Prompt,
            PermissionState::Denied,
        ] {
            assert!(broker.may_prompt("env.get"));
            broker.open_prompt(7, "env.get");
            assert_eq!(broker.resolve(&mut policy), [(7, expected)]);
        }
        assert!(!broker.may_prompt("env.get"));
        assert_eq!(broker.decisions()["env.get"], StoredDecision::Deny);
        let shown = seen.lock().unwrap();
        assert_eq!(shown[1].scope, ["APP_*"]);
        assert_eq!(shown[1].purpose.as_deref(), Some("why"));
        // Revocation denies new operations.
        assert!(broker.revoke("time.timer", &mut policy));
        assert!(!policy.enabled("time.timer"));
    }

    #[test]
    fn descriptors_are_versioned_and_bounded_fields() {
        let d = descriptor(&format!(
            "{DESCRIPTOR_VERSION}\nnet.request\n1\nhttps://a.test/x"
        ))
        .unwrap();
        assert_eq!(d.name, "net.request");
        assert_eq!(d.resource.as_deref(), Some("https://a.test/x"));
        assert!(
            descriptor(&format!("{DESCRIPTOR_VERSION}\ntime.now\n0\n"))
                .unwrap()
                .resource
                .is_none()
        );
        for bad in [
            String::new(),
            "varta-permission-0\ntime.now\n0\n".into(),
            format!("{DESCRIPTOR_VERSION}\ntime.now\n0\nx"),
            format!("{DESCRIPTOR_VERSION}\ntime.now\n2\n"),
            format!("{DESCRIPTOR_VERSION}\ntime.now"),
        ] {
            assert!(descriptor(&bad).is_err(), "{bad:?}");
        }
    }
}
