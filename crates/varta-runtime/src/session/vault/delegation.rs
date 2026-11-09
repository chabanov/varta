//! Native-only delegation and immutable request provenance. No business schemas.
use super::*;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use varta_common::session::delegation as wire;

pub(super) struct Delegations {
    policy: wire::DelegationPolicy,
    control: Token,
    scopes: BTreeMap<Token, Scope>,
    proofs: BTreeMap<Token, ProofEntry>,
}
struct Scope {
    id: String,
    key: String,
    identity: Token,
    parent: Token,
    binding: String,
    context: String,
    context_sha: String,
    prefix: String,
    methods: Vec<String>,
    until: Instant,
    expires_ms: u64,
    terminal: Option<Instant>,
}
struct ProofEntry {
    parent: Option<Token>,
    scope: Option<Token>,
    owner: crate::cancellation::Cancellation,
}
/// Lifetime of a native request proof, not a caller-supplied marker.
pub(crate) struct Proof {
    vault: Weak<Vault>,
    key: Token,
}
impl Drop for Proof {
    fn drop(&mut self) {
        if let Some(vault) = self.vault.upgrade()
            && let Ok(mut s) = vault.state.lock()
            && let Some(d) = s.delegation.as_mut()
        {
            d.proofs.remove(&self.key);
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Issue {
    pub request_proof: String,
    pub request_id: String,
    pub binding_id: String,
    pub context: String,
    pub context_sha256: String,
    pub path_prefix: String,
    pub methods: Vec<String>,
    pub ttl_seconds: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub schema_version: u32,
    pub id: String,
    pub binding_id: String,
    pub context_sha256: String,
    pub parent_generation: String,
    pub state: &'static str,
    pub expires_at_ms: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Verified {
    pub binding_id: String,
    pub context: String,
    pub context_sha256: String,
    pub parent_generation: String,
    pub expires_at_ms: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Lookup {
    pub schema_version: u32,
    pub mode: &'static str,
    pub parent_generation: Option<String>,
    pub context: Option<Verified>,
    pub parent_credential: Option<String>,
}
fn hex(b: Token) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(64);
    for n in b {
        let _ = write!(s, "{n:02x}");
    }
    s
}
fn random() -> Result<String, &'static str> {
    let mut b = [0; 32];
    getrandom::fill(&mut b).map_err(|_| UNAVAILABLE)?;
    Ok(hex(b))
}
fn id(s: &str) -> bool {
    (16..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
fn status(scope: &Scope) -> Status {
    Status {
        schema_version: 1,
        id: scope.id.clone(),
        binding_id: scope.binding.clone(),
        context_sha256: scope.context_sha.clone(),
        parent_generation: hex(scope.parent),
        state: if scope.terminal.is_none() {
            "active"
        } else {
            "revoked"
        },
        expires_at_ms: scope.expires_ms,
    }
}
fn live_scope(s: &State, key: &Token, now: Instant) -> bool {
    s.delegation
        .as_ref()
        .and_then(|d| d.scopes.get(key))
        .is_some_and(|v| {
            v.terminal.is_none()
                && v.until > now
                && s.entries.get(&v.parent).is_some_and(|p| alive(p, now))
        })
}
fn revoke_scope(d: &mut Delegations, key: Token, now: Instant) {
    if let Some(scope) = d.scopes.get_mut(&key)
        && scope.terminal.is_none()
    {
        scope.terminal = Some(now);
        for proof in d.proofs.values().filter(|p| p.scope == Some(key)) {
            proof.owner.cancel();
        }
    }
}
pub(super) fn revoke_parent(state: &mut State, parent: Token) {
    if let Some(d) = state.delegation.as_mut() {
        let keys: Vec<_> = d
            .scopes
            .iter()
            .filter_map(|(k, s)| (s.parent == parent && s.terminal.is_none()).then_some(*k))
            .collect();
        for key in keys {
            revoke_scope(d, key, Instant::now());
        }
    }
}
pub(super) fn expire(state: &mut State, now: Instant) {
    if let Some(d) = state.delegation.as_mut() {
        let keys: Vec<_> = d
            .scopes
            .iter()
            .filter_map(|(k, s)| (s.until <= now && s.terminal.is_none()).then_some(*k))
            .collect();
        for key in keys {
            revoke_scope(d, key, now);
        }
        d.scopes.retain(|_, s| {
            s.terminal.is_none_or(|t| {
                now.duration_since(t) < Duration::from_secs(wire::RETENTION_SECONDS)
            })
        });
    }
}
pub(super) fn deadline(state: &State) -> Option<Instant> {
    state.delegation.as_ref().and_then(|d| {
        d.scopes
            .values()
            .map(|s| {
                s.terminal.map_or(s.until, |t| {
                    t + Duration::from_secs(wire::RETENTION_SECONDS)
                })
            })
            .min()
    })
}
impl Vault {
    pub(crate) fn enable_delegation(
        &self,
        policy: wire::DelegationPolicy,
    ) -> Result<(), varta_common::VartaError> {
        policy.validate()?;
        let fail = || {
            varta_common::VartaError::InvalidManifest(
                "native delegation control credential unavailable".into(),
            )
        };
        let token = std::env::var(&policy.control_token_env).map_err(|_| fail())?;
        if token.len() != 64
            || !token
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(fail());
        }
        let mut s = self.state.lock().map_err(|_| fail())?;
        s.delegation = Some(Delegations {
            policy,
            control: digest(&format!("Bearer {token}")),
            scopes: BTreeMap::new(),
            proofs: BTreeMap::new(),
        });
        Ok(())
    }
    pub(crate) fn delegation_enabled(&self) -> bool {
        self.state.lock().is_ok_and(|s| s.delegation.is_some())
    }
    pub(crate) fn delegation_auth(&self, authorization: &str) -> bool {
        self.state.lock().is_ok_and(|s| {
            s.delegation
                .as_ref()
                .is_some_and(|d| bool::from(d.control.ct_eq(&digest(authorization))))
        })
    }
    pub(crate) fn delegation_policy(&self) -> Option<wire::DelegationPolicy> {
        self.state
            .lock()
            .ok()?
            .delegation
            .as_ref()
            .map(|d| d.policy.clone())
    }
    pub(in crate::session) fn request_proof(
        self: &Arc<Self>,
        token: Option<&str>,
        scope: Option<&str>,
        owner: &crate::cancellation::Cancellation,
    ) -> Result<(String, Proof, Option<Lease>), &'static str> {
        let nonce = random()?;
        let key = digest(&nonce);
        let mut s = self.state.lock().map_err(|_| UNAVAILABLE)?;
        super::expire(&mut s, Instant::now());
        let d = s.delegation.as_ref().ok_or(UNAVAILABLE)?;
        if d.proofs.len() >= d.policy.max_request_proofs {
            return Err(FULL);
        }
        if d.proofs.contains_key(&key) {
            return Err(UNAVAILABLE);
        }
        let scope = scope.map(digest);
        if scope.is_some_and(|k| !live_scope(&s, &k, Instant::now())) {
            return Err(INVALID);
        }
        let parent = if let Some(scope) = scope {
            s.delegation
                .as_ref()
                .and_then(|d| d.scopes.get(&scope))
                .map(|v| v.parent)
        } else {
            token.map(digest)
        };
        let serial = s.serial()?;
        let lease = parent.and_then(|parent| {
            s.entries
                .get_mut(&parent)
                .filter(|e| alive(e, Instant::now()))
                .map(|e| {
                    e.leases.insert(serial, owner.clone());
                    Lease {
                        vault: Arc::downgrade(self),
                        token: parent,
                        serial,
                    }
                })
        });
        s.delegation.as_mut().ok_or(UNAVAILABLE)?.proofs.insert(
            key,
            ProofEntry {
                parent,
                scope,
                owner: owner.clone(),
            },
        );
        Ok((
            nonce,
            Proof {
                vault: Arc::downgrade(self),
                key,
            },
            lease,
        ))
    }
    pub(crate) fn lookup_proof(&self, proof: &str) -> Result<Lookup, &'static str> {
        let mut s = self.state.lock().map_err(|_| UNAVAILABLE)?;
        super::expire(&mut s, Instant::now());
        let p = s
            .delegation
            .as_ref()
            .and_then(|d| d.proofs.get(&digest(proof)))
            .ok_or(INVALID)?;
        let mode = if p.scope.is_some() {
            "delegated"
        } else {
            "ordinary"
        };
        let valid = p.owner.check().is_ok()
            && p.parent
                .is_some_and(|k| s.entries.get(&k).is_some_and(|e| alive(e, Instant::now())))
            && p.scope.is_none_or(|k| live_scope(&s, &k, Instant::now()));
        let context = if valid {
            p.scope
                .and_then(|k| s.delegation.as_ref()?.scopes.get(&k))
                .map(|v| Verified {
                    binding_id: v.binding.clone(),
                    context: v.context.clone(),
                    context_sha256: v.context_sha.clone(),
                    parent_generation: hex(v.parent),
                    expires_at_ms: v.expires_ms,
                })
        } else {
            None
        };
        let credential = if valid {
            p.parent
                .and_then(|k| s.entries.get(&k))
                .map(|e| e.secret.clone())
        } else {
            None
        };
        Ok(Lookup {
            schema_version: 1,
            mode,
            parent_generation: p.parent.map(hex),
            context,
            parent_credential: credential,
        })
    }
    pub(crate) fn issue_delegation(&self, issue: Issue) -> Result<Status, &'static str> {
        if !id(&issue.request_id)
            || !id(&issue.binding_id)
            || issue.context.len() > wire::MAX_CONTEXT_BYTES
            || issue.context.is_empty()
            || issue.context_sha256 != hex(digest(&issue.context))
            || !wire::prefix(&issue.path_prefix)
            || issue.methods.is_empty()
            || issue.methods.windows(2).any(|m| m[0] >= m[1])
        {
            return Err(INVALID);
        }
        let mut s = self.state.lock().map_err(|_| UNAVAILABLE)?;
        let now = Instant::now();
        super::expire(&mut s, now);
        let d = s.delegation.as_ref().ok_or(UNAVAILABLE)?;
        let proof = d.proofs.get(&digest(&issue.request_proof)).ok_or(INVALID)?;
        if proof.scope.is_some() || proof.owner.check().is_err() {
            return Err(INVALID);
        }
        let parent = proof
            .parent
            .filter(|p| s.entries.get(p).is_some_and(|e| alive(e, now)))
            .ok_or(INVALID)?;
        if !(1..=d.policy.max_age_seconds).contains(&issue.ttl_seconds)
            || !d
                .policy
                .path_prefixes
                .iter()
                .any(|p| wire::target_allowed(&issue.path_prefix, p))
            || issue.methods.iter().any(|m| !d.policy.methods.contains(m))
        {
            return Err(INVALID);
        }
        // The per-request carrier changes between retries; it is not binding identity.
        let identity = digest(
            &serde_json::to_string(&(
                issue.binding_id.as_str(),
                issue.context_sha256.as_str(),
                issue.path_prefix.as_str(),
                &issue.methods,
                issue.ttl_seconds,
            ))
            .map_err(|_| INVALID)?,
        );
        if let Some(v) = d
            .scopes
            .values()
            .find(|v| v.parent == parent && v.key == issue.request_id)
        {
            return if v.identity == identity {
                Ok(status(v))
            } else {
                Err(INVALID)
            };
        }
        if d.scopes.len() >= d.policy.max_bindings {
            return Err(FULL);
        }
        let entry = s.entries.get(&parent).ok_or(INVALID)?;
        let until = (now + Duration::from_secs(issue.ttl_seconds))
            .min(entry.expires)
            .min(entry.idle);
        let expires_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| UNAVAILABLE)?
            .as_millis() as u64
            + until.saturating_duration_since(now).as_millis() as u64;
        let scope = Scope {
            id: random()?,
            key: issue.request_id,
            identity,
            parent,
            binding: issue.binding_id,
            context: issue.context,
            context_sha: issue.context_sha256,
            prefix: issue.path_prefix,
            methods: issue.methods,
            until,
            expires_ms,
            terminal: None,
        };
        let result = status(&scope);
        let key = digest(&scope.id);
        if s.delegation
            .as_ref()
            .is_some_and(|d| d.scopes.contains_key(&key))
        {
            return Err(UNAVAILABLE);
        }
        s.delegation
            .as_mut()
            .ok_or(UNAVAILABLE)?
            .scopes
            .insert(key, scope);
        if s.next_expiration.is_none_or(|old| until < old) {
            s.next_expiration = Some(until);
            self.changed.notify_one();
        }
        Ok(result)
    }
    pub(crate) fn revoke_delegation(&self, proof: &str, id: &str) -> Result<Status, &'static str> {
        let mut s = self.state.lock().map_err(|_| UNAVAILABLE)?;
        super::expire(&mut s, Instant::now());
        let d = s.delegation.as_ref().ok_or(UNAVAILABLE)?;
        let p = d.proofs.get(&digest(proof)).ok_or(INVALID)?;
        if p.scope.is_some() || p.owner.check().is_err() {
            return Err(INVALID);
        }
        let scope = d.scopes.get(&digest(id)).ok_or(INVALID)?;
        if p.parent != Some(scope.parent)
            || !s
                .entries
                .get(&scope.parent)
                .is_some_and(|e| alive(e, Instant::now()))
        {
            return Err(INVALID);
        }
        let d = s.delegation.as_mut().ok_or(UNAVAILABLE)?;
        revoke_scope(d, digest(id), Instant::now());
        Ok(status(d.scopes.get(&digest(id)).ok_or(INVALID)?))
    }
    pub(crate) fn scope_descriptor(&self, id: &str) -> Result<serde_json::Value, &'static str> {
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE)?;
        super::expire(&mut state, Instant::now());
        let scope = state
            .delegation
            .as_ref()
            .and_then(|d| d.scopes.get(&digest(id)))
            .ok_or(INVALID)?;
        let mut value = serde_json::to_value(status(scope)).map_err(|_| UNAVAILABLE)?;
        let fields = value.as_object_mut().ok_or(UNAVAILABLE)?;
        fields.insert("pathPrefix".into(), serde_json::json!(scope.prefix));
        fields.insert("methods".into(), serde_json::json!(scope.methods));
        Ok(value)
    }
    pub(crate) fn delegation_live(&self, id: &str) -> bool {
        self.state
            .lock()
            .is_ok_and(|s| live_scope(&s, &digest(id), Instant::now()))
    }
    pub(crate) fn delegation_route(&self, id: &str, target: &str, method: &str) -> bool {
        self.state.lock().is_ok_and(|s| {
            live_scope(&s, &digest(id), Instant::now())
                && s.delegation
                    .as_ref()
                    .and_then(|d| d.scopes.get(&digest(id)))
                    .is_some_and(|v| {
                        v.methods.iter().any(|m| m == method)
                            && wire::target_allowed(target, &v.prefix)
                    })
        })
    }
    pub(crate) fn delegated_secret(&self, id: &str) -> Result<Option<String>, &'static str> {
        let s = self.state.lock().map_err(|_| UNAVAILABLE)?;
        if !live_scope(&s, &digest(id), Instant::now()) {
            return Ok(None);
        }
        let p = s
            .delegation
            .as_ref()
            .and_then(|d| d.scopes.get(&digest(id)))
            .ok_or(INVALID)?;
        Ok(s.entries.get(&p.parent).map(|e| e.secret.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Arc<Vault>, String, crate::cancellation::Cancellation) {
        let vault = super::super::tests::vault();
        let policy = wire::DelegationPolicy {
            control_token_env: "VARTA_DELEGATION_TEST".into(),
            max_bindings: 2,
            max_request_proofs: 8,
            max_age_seconds: 300,
            path_prefixes: vec!["/private".into()],
            methods: vec!["GET".into(), "PATCH".into(), "POST".into()],
        };
        vault.state.lock().unwrap().delegation = Some(Delegations {
            policy,
            control: digest("Bearer test-only"),
            scopes: BTreeMap::new(),
            proofs: BTreeMap::new(),
        });
        let parent = vault
            .rotate(None, "private-synthetic".into(), None)
            .unwrap();
        let owner = crate::cancellation::Cancellation::new(crate::VartaRuntime::new().engine);
        (vault, parent, owner)
    }
    fn issue(proof: &str) -> Issue {
        Issue {
            request_proof: proof.into(),
            request_id: "request-00000001".into(),
            binding_id: "binding-00000001".into(),
            context: "opaque UTF-8: Привіт".into(),
            context_sha256: hex(digest("opaque UTF-8: Привіт")),
            path_prefix: "/private/records".into(),
            methods: vec!["GET".into()],
            ttl_seconds: 300,
        }
    }
    #[test]
    fn delegation_proofs_are_request_owned_and_parent_revocation_never_erases_mode() {
        let (v, parent, o) = setup();
        let (p, guard, _lease) = v.request_proof(Some(&parent), None, &o).unwrap();
        assert!(v.delegation_auth("Bearer test-only"));
        assert!(!v.delegation_auth("Bearer wrong"));
        assert_eq!(v.lookup_proof(&p).unwrap().mode, "ordinary");
        let status = v.issue_delegation(issue(&p)).unwrap();
        let child = crate::cancellation::Cancellation::new(crate::VartaRuntime::new().engine);
        let (dp, dg, _dl) = v.request_proof(None, Some(&status.id), &child).unwrap();
        let read = v.lookup_proof(&dp).unwrap();
        assert_eq!(read.mode, "delegated");
        assert_eq!(read.parent_credential.as_deref(), Some("private-synthetic"));
        assert_eq!(read.context.unwrap().context, "opaque UTF-8: Привіт");
        assert!(v.issue_delegation(issue(&dp)).is_err());
        v.clear(&parent, None).unwrap();
        assert!(child.check().is_err());
        assert!(o.check().is_err());
        let read = v.lookup_proof(&dp).unwrap();
        assert_eq!(read.mode, "delegated");
        assert!(read.context.is_none() && read.parent_credential.is_none());
        // Even removal of retained scope rows never changes proof provenance.
        let mut state = v.state.lock().unwrap();
        super::expire(
            &mut state,
            Instant::now() + Duration::from_secs(wire::RETENTION_SECONDS - 1),
        );
        assert_eq!(state.delegation.as_ref().unwrap().scopes.len(), 1);
        super::expire(
            &mut state,
            Instant::now() + Duration::from_secs(wire::RETENTION_SECONDS + 1),
        );
        assert!(state.delegation.as_ref().unwrap().scopes.is_empty());
        drop(state);
        assert_eq!(v.lookup_proof(&dp).unwrap().mode, "delegated");
        drop(dg);
        assert!(v.lookup_proof(&dp).is_err());
        drop(guard);
        assert!(v.lookup_proof(&p).is_err());
    }
    #[test]
    fn delegation_replay_capacity_route_and_revoke_are_non_sliding() {
        let (v, parent, o) = setup();
        let (p, _g, _l) = v.request_proof(Some(&parent), None, &o).unwrap();
        let first = v.issue_delegation(issue(&p)).unwrap();
        let descriptor = v.scope_descriptor(&first.id).unwrap();
        assert_eq!(descriptor["pathPrefix"], "/private/records");
        assert_eq!(descriptor["contextSha256"], first.context_sha256);
        assert!(
            descriptor.get("parentCredential").is_none() && descriptor.get("context").is_none()
        );
        let repeated = v.issue_delegation(issue(&p)).unwrap();
        assert_eq!(first.id, repeated.id);
        assert_eq!(first.expires_at_ms, repeated.expires_at_ms);
        let mut conflict = issue(&p);
        conflict.methods.push("POST".into());
        assert!(v.issue_delegation(conflict).is_err());
        assert!(v.delegation_route(&first.id, "/private/records/item?cursor=x", "GET"));
        for (target, method) in [
            ("/private/recordsX", "GET"),
            ("/private/records/../escape", "GET"),
            ("/private/records/%2e%2e/escape", "GET"),
            ("/private/records", "POST"),
            ("/private/records", "DELETE"),
        ] {
            assert!(!v.delegation_route(&first.id, target, method));
        }
        let mut second = issue(&p);
        second.request_id = "request-00000002".into();
        v.issue_delegation(second).unwrap();
        let mut third = issue(&p);
        third.request_id = "request-00000003".into();
        assert!(v.issue_delegation(third).is_err());
        assert_eq!(v.revoke_delegation(&p, &first.id).unwrap().state, "revoked");
        assert_eq!(v.issue_delegation(issue(&p)).unwrap().state, "revoked");
        assert!(!v.delegation_live(&first.id));
        // A terminal receipt still occupies capacity and cannot resurrect on replay.
        assert_eq!(
            v.state
                .lock()
                .unwrap()
                .delegation
                .as_ref()
                .unwrap()
                .scopes
                .len(),
            2
        );
    }
    #[test]
    fn delegation_context_hash_ceiling_parent_and_request_liveness_fail_closed() {
        let (v, parent, o) = setup();
        let (p, _g, _l) = v.request_proof(Some(&parent), None, &o).unwrap();
        let mut bad = issue(&p);
        bad.context.push('x');
        assert!(v.issue_delegation(bad).is_err());
        let mut bad = issue(&p);
        bad.ttl_seconds = 301;
        assert!(v.issue_delegation(bad).is_err());
        let mut bad = issue(&p);
        bad.path_prefix = "/other".into();
        assert!(v.issue_delegation(bad).is_err());
        let mut bad = issue(&p);
        bad.methods = vec!["DELETE".into()];
        assert!(v.issue_delegation(bad).is_err());
        let mut bad = issue(&p);
        bad.context = "x".repeat(4097);
        bad.context_sha256 = hex(digest(&bad.context));
        assert!(v.issue_delegation(bad).is_err());
        let (anon, _ag, _al) = v.request_proof(None, None, &o).unwrap();
        assert!(v.issue_delegation(issue(&anon)).is_err());
        v.state
            .lock()
            .unwrap()
            .delegation
            .as_mut()
            .unwrap()
            .policy
            .max_request_proofs = 2;
        assert!(v.request_proof(None, None, &o).is_err());
        drop(_ag);
        let (_, replacement_guard, _) = v.request_proof(None, None, &o).unwrap();
        drop(replacement_guard);
        o.cancel();
        assert!(v.issue_delegation(issue(&p)).is_err());
        assert!(v.lookup_proof(&p).unwrap().parent_credential.is_none());
    }
    #[test]
    fn delegation_rotation_revokes_children_and_cross_parent_revoke_is_denied() {
        let (v, parent, o) = setup();
        let (p, _g, _l) = v.request_proof(Some(&parent), None, &o).unwrap();
        let first = v.issue_delegation(issue(&p)).unwrap();
        let replacement = v.rotate(Some(&parent), "replacement".into(), None).unwrap();
        assert!(!v.delegation_live(&first.id));
        let other = crate::cancellation::Cancellation::new(crate::VartaRuntime::new().engine);
        let (r, _rg, _rl) = v.request_proof(Some(&replacement), None, &other).unwrap();
        assert!(v.revoke_delegation(&r, &first.id).is_err());
        let live = v.issue_delegation(issue(&r)).unwrap();
        assert_ne!(first.id, live.id);
        assert_ne!(first.parent_generation, live.parent_generation);
    }
    #[tokio::test]
    async fn delegation_expiry_cancels_suspended_owner_without_another_guest_call() {
        let (v, parent, o) = setup();
        let (p, _g, _l) = v.request_proof(Some(&parent), None, &o).unwrap();
        v.state
            .lock()
            .unwrap()
            .entries
            .get_mut(&digest(&parent))
            .unwrap()
            .idle = Instant::now() + Duration::from_secs(1);
        let scope = v.issue_delegation(issue(&p)).unwrap();
        // A later parent idle refresh cannot extend the child deadline.
        let mut lease = None;
        assert!(v.read(&parent, &o, &mut lease).unwrap().is_some());
        assert_eq!(
            v.issue_delegation(issue(&p)).unwrap().expires_at_ms,
            scope.expires_at_ms
        );
        let child = crate::cancellation::Cancellation::new(crate::VartaRuntime::new().engine);
        let (cp, _cg, _cl) = v.request_proof(None, Some(&scope.id), &child).unwrap();
        let driver = v.clone();
        let running = tokio::spawn(async move { driver.expire_sessions().await });
        tokio::time::timeout(Duration::from_secs(3), child.cancelled())
            .await
            .unwrap();
        let read = v.lookup_proof(&cp).unwrap();
        assert_eq!(read.mode, "delegated");
        assert!(read.context.is_none());
        assert!(v.live(&parent));
        assert!(o.check().is_ok());
        running.abort();
    }
}
