//! Memory-only host secrets. Token hashes, staged reservations and active
//! request leases are bounded by policy; no credential enters KV or a log.
mod delegation;
use super::*;
pub(crate) use delegation::Issue;
pub(super) use delegation::Proof;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
use varta_common::session as contract;
type Token = [u8; contract::TOKEN_BYTES];
struct Entry {
    secret: String,
    expires: Instant,
    idle: Instant,
    leases: BTreeMap<u64, crate::cancellation::Cancellation>,
}
struct Pending {
    token: String,
    hash: Token,
    previous: Option<Token>,
    secret: String,
    owner: Option<crate::cancellation::Cancellation>,
}
struct State {
    entries: BTreeMap<Token, Entry>,
    pending: BTreeMap<u64, Pending>,
    serial: u64,
    next_expiration: Option<Instant>,
    delegation: Option<delegation::Delegations>,
}
pub(crate) struct Vault {
    pub policy: contract::ServerPolicy,
    state: Mutex<State>,
    changed: tokio::sync::Notify,
}
pub(super) struct Lease {
    vault: Weak<Vault>,
    token: Token,
    serial: u64,
}
/// A reserved host slot. Dropping a failed request discards its staged secret.
pub(super) struct Staged {
    vault: Weak<Vault>,
    serial: u64,
}
impl Drop for Staged {
    fn drop(&mut self) {
        if let Some(vault) = self.vault.upgrade()
            && let Ok(mut state) = vault.state.lock()
        {
            state.pending.remove(&self.serial);
        }
    }
}
impl Staged {
    pub(super) fn publish(self) -> Result<String, &'static str> {
        let vault = self.vault.upgrade().ok_or(UNAVAILABLE)?;
        vault.publish(self.serial)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(vault) = self.vault.upgrade()
            && let Ok(mut state) = vault.state.lock()
            && let Some(entry) = state.entries.get_mut(&self.token)
        {
            entry.leases.remove(&self.serial);
        }
    }
}
fn digest(token: &str) -> Token {
    Sha256::digest(token.as_bytes()).into()
}
fn alive(entry: &Entry, now: Instant) -> bool {
    now < entry.expires && now < entry.idle
}
fn cancel(entry: Entry, except: Option<&crate::cancellation::Cancellation>) {
    for lease in entry.leases.into_values() {
        if except.is_none_or(|current| !current.same(&lease)) {
            lease.cancel();
        }
    }
}
fn revoke(state: &mut State, token: Token, except: Option<&crate::cancellation::Cancellation>) {
    delegation::revoke_parent(state, token);
    if let Some(entry) = state.entries.remove(&token) {
        cancel(entry, except);
    }
    let staged: Vec<_> = state
        .pending
        .iter()
        .filter_map(|(id, p)| (p.previous == Some(token)).then_some(*id))
        .collect();
    for id in staged {
        if let Some(pending) = state.pending.remove(&id)
            && let Some(owner) = pending.owner
            && except.is_none_or(|except| !except.same(&owner))
        {
            owner.cancel();
        }
    }
}
fn expire(state: &mut State, now: Instant) {
    let expired: Vec<_> = state
        .entries
        .iter()
        .filter_map(|(id, e)| (!alive(e, now)).then_some(*id))
        .collect();
    for id in expired {
        revoke(state, id, None);
    }
    delegation::expire(state, now);
}
impl State {
    fn serial(&mut self) -> Result<u64, &'static str> {
        self.serial = self.serial.checked_add(1).ok_or(UNAVAILABLE)?;
        Ok(self.serial)
    }
    fn occupied(&self) -> usize {
        let replaced = self
            .pending
            .values()
            .filter(|p| p.previous.is_some_and(|id| self.entries.contains_key(&id)))
            .count();
        self.entries.len() + self.pending.len() - replaced
    }
}
impl Vault {
    pub(crate) fn new(
        policy: contract::ServerPolicy,
    ) -> Result<Arc<Self>, varta_common::VartaError> {
        policy.validate()?;
        Ok(Arc::new(Self {
            policy,
            state: Mutex::new(State {
                entries: BTreeMap::new(),
                pending: BTreeMap::new(),
                serial: 0,
                next_expiration: None,
                delegation: None,
            }),
            changed: tokio::sync::Notify::new(),
        }))
    }
    /// One server-owned deadline future, with no per-session task or polling.
    /// Reads may postpone an idle deadline; the earlier timer then recomputes
    /// the minimum once. New earlier deadlines wake this future immediately.
    pub(crate) async fn expire_sessions(&self) -> Result<(), &'static str> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let deadline = {
                let mut state = self.state.lock().map_err(|_| UNAVAILABLE)?;
                let now = Instant::now();
                if state.next_expiration.is_some_and(|due| due <= now) {
                    expire(&mut state, now);
                    state.next_expiration = state
                        .entries
                        .values()
                        .map(|entry| entry.expires.min(entry.idle))
                        .chain(delegation::deadline(&state))
                        .min();
                }
                state.next_expiration
            };
            let due = async {
                match deadline {
                    Some(deadline) => {
                        tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await
                    }
                    None => std::future::pending().await,
                }
            };
            tokio::select! { () = changed => {}, () = due => {} }
        }
    }
    pub(super) fn live(&self, token: &str) -> bool {
        self.state
            .lock()
            .ok()
            .and_then(|state| {
                state
                    .entries
                    .get(&digest(token))
                    .map(|e| alive(e, Instant::now()))
            })
            .unwrap_or(false)
    }
    pub(super) fn read(
        self: &Arc<Self>,
        token: &str,
        owner: &crate::cancellation::Cancellation,
        lease: &mut Option<Lease>,
    ) -> Result<Option<String>, &'static str> {
        let hash = digest(token);
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE)?;
        let now = Instant::now();
        if state.entries.get(&hash).is_some_and(|e| !alive(e, now)) {
            revoke(&mut state, hash, None);
        }
        let serial = state.serial()?;
        let Some(entry) = state.entries.get_mut(&hash) else {
            return Ok(None);
        };
        entry.idle = now + Duration::from_secs(self.policy.idle_seconds);
        if lease.is_none() {
            entry.leases.insert(serial, owner.clone());
            *lease = Some(Lease {
                vault: Arc::downgrade(self),
                token: hash,
                serial,
            });
        }
        Ok(Some(entry.secret.clone()))
    }
    /// Reserve before the application returns, so capacity refusal is catchable.
    /// A live token can have only one staged replacement; it keeps working until
    /// a validated successful response publishes that replacement.
    pub(super) fn stage(
        self: &Arc<Self>,
        previous: Option<&str>,
        secret: String,
        owner: Option<&crate::cancellation::Cancellation>,
    ) -> Result<Staged, &'static str> {
        if secret.is_empty() || secret.len() > contract::MAX_SECRET_BYTES {
            return Err(INVALID);
        }
        let mut entropy = [0u8; contract::TOKEN_BYTES];
        getrandom::fill(&mut entropy).map_err(|_| UNAVAILABLE)?;
        use std::fmt::Write;
        let mut token = String::with_capacity(contract::TOKEN_HEX_BYTES);
        for byte in entropy {
            write!(token, "{byte:02x}").map_err(|_| UNAVAILABLE)?;
        }
        let hash = digest(&token);
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE)?;
        expire(&mut state, Instant::now());
        if state.entries.contains_key(&hash) || state.pending.values().any(|p| p.hash == hash) {
            return Err(UNAVAILABLE);
        }
        let previous = previous
            .map(digest)
            .filter(|id| state.entries.contains_key(id));
        if previous.is_some() && state.pending.values().any(|p| p.previous == previous) {
            return Err(FULL);
        }
        if state.occupied() >= self.policy.max_sessions && previous.is_none() {
            return Err(FULL);
        }
        if let Some(owner) = owner {
            owner.check().map_err(|_| UNAVAILABLE)?;
        }
        let serial = state.serial()?;
        state.pending.insert(
            serial,
            Pending {
                token,
                hash,
                previous,
                secret,
                owner: owner.cloned(),
            },
        );
        Ok(Staged {
            vault: Arc::downgrade(self),
            serial,
        })
    }
    fn publish(&self, serial: u64) -> Result<String, &'static str> {
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE)?;
        let now = Instant::now();
        expire(&mut state, now);
        let pending = state.pending.remove(&serial).ok_or(INVALID)?;
        if let Some(owner) = &pending.owner {
            owner.check().map_err(|_| UNAVAILABLE)?;
        }
        if let Some(previous) = pending.previous {
            revoke(&mut state, previous, pending.owner.as_ref());
        }
        let deadline =
            now + Duration::from_secs(self.policy.max_age_seconds.min(self.policy.idle_seconds));
        if state.next_expiration.is_none_or(|next| deadline < next) {
            state.next_expiration = Some(deadline);
            self.changed.notify_one();
        }
        state.entries.insert(
            pending.hash,
            Entry {
                secret: pending.secret,
                expires: now + Duration::from_secs(self.policy.max_age_seconds),
                idle: now + Duration::from_secs(self.policy.idle_seconds),
                leases: BTreeMap::new(),
            },
        );
        Ok(pending.token)
    }
    pub(super) fn clear(
        &self,
        token: &str,
        owner: Option<&crate::cancellation::Cancellation>,
    ) -> Result<(), &'static str> {
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE)?;
        revoke(&mut state, digest(token), owner);
        Ok(())
    }
    #[cfg(test)]
    fn rotate(
        self: &Arc<Self>,
        previous: Option<&str>,
        secret: String,
        owner: Option<&crate::cancellation::Cancellation>,
    ) -> Result<String, &'static str> {
        self.stage(previous, secret, owner)?.publish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn vault() -> Arc<Vault> {
        Vault::new(contract::ServerPolicy {
            origin: "https://api.test".into(),
            client_origin: "https://studio.test".into(),
            max_age_seconds: 3600,
            idle_seconds: 900,
            max_sessions: 1,
            same_site: contract::CookieSite::Strict,
            allow_insecure_loopback: false,
        })
        .unwrap()
    }
    #[test]
    fn protected_session_pending_slots_release_and_revocation_fences_publication() {
        let vault = vault();
        let engine = crate::VartaRuntime::new().engine;
        let owner = crate::cancellation::Cancellation::new(engine.clone());
        let logout = crate::cancellation::Cancellation::new(engine);
        let pending = vault.stage(None, "new".into(), Some(&owner)).unwrap();
        assert!(vault.stage(None, "full".into(), None).is_err());
        drop(pending);
        assert_eq!(vault.state.lock().unwrap().occupied(), 0);
        let token = vault.rotate(None, "live".into(), None).unwrap();
        let pending = vault
            .stage(Some(&token), "replace".into(), Some(&owner))
            .unwrap();
        assert!(vault.live(&token));
        assert!(vault.stage(Some(&token), "duplicate".into(), None).is_err());
        drop(pending);
        assert!(vault.live(&token));
        let pending = vault
            .stage(Some(&token), "replace".into(), Some(&owner))
            .unwrap();
        vault.clear(&token, Some(&logout)).unwrap();
        assert!(owner.check().is_err());
        assert!(pending.publish().is_err());
        assert_eq!(vault.state.lock().unwrap().occupied(), 0);
    }
    #[tokio::test]
    async fn protected_session_idle_refresh_rearms_expiry_without_extending_absolute_age() {
        let vault = vault();
        let owner = crate::cancellation::Cancellation::new(crate::VartaRuntime::new().engine);
        let token = vault.rotate(None, "synthetic".into(), None).unwrap();
        let mut lease = None;
        vault.read(&token, &owner, &mut lease).unwrap();
        let now = Instant::now();
        let initial_idle = now + Duration::from_secs(1);
        {
            let mut state = vault.state.lock().unwrap();
            let entry = state.entries.get_mut(&digest(&token)).unwrap();
            entry.idle = initial_idle;
            entry.expires = now + Duration::from_secs(2);
            state.next_expiration = Some(initial_idle);
        }
        vault.changed.notify_one();
        let running = vault.clone();
        let task = tokio::spawn(async move { running.expire_sessions().await });
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            vault.read(&token, &owner, &mut lease).unwrap().as_deref(),
            Some("synthetic")
        );
        tokio::time::sleep_until(tokio::time::Instant::from_std(
            initial_idle + Duration::from_millis(100),
        ))
        .await;
        assert!(vault.live(&token));
        assert!(owner.check().is_ok());
        tokio::time::timeout(Duration::from_secs(2), owner.cancelled())
            .await
            .unwrap();
        assert!(!vault.live(&token));
        assert!(vault.state.lock().unwrap().entries.is_empty());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }

    #[test]
    fn protected_session_soak_releases_failed_rotation_leases_and_secrets() {
        const CYCLES: usize = 1024;
        const REPEATED_READS: usize = 16;
        let vault = vault();
        let engine = crate::VartaRuntime::new().engine;
        for _ in 0..CYCLES {
            let owner = crate::cancellation::Cancellation::new(engine.clone());
            let pending = vault.stage(None, "discarded".into(), Some(&owner)).unwrap();
            drop(pending);
            let token = vault
                .rotate(None, "synthetic".into(), Some(&owner))
                .unwrap();
            let mut lease = None;
            for _ in 0..REPEATED_READS {
                assert_eq!(
                    vault.read(&token, &owner, &mut lease).unwrap().as_deref(),
                    Some("synthetic")
                );
            }
            let replacement = vault
                .stage(Some(&token), "replacement".into(), Some(&owner))
                .unwrap();
            assert_eq!(
                vault.state.lock().unwrap().entries[&digest(&token)]
                    .leases
                    .len(),
                1
            );
            vault.clear(&token, None).unwrap();
            assert!(replacement.publish().is_err());
            drop(lease);
            let state = vault.state.lock().unwrap();
            assert!(state.entries.is_empty() && state.pending.is_empty());
            assert_eq!(state.occupied(), 0);
        }
    }

    #[test]
    fn protected_session_vault_bounds_expiry_revocation_and_request_lease_cleanup() {
        let vault = vault();
        let engine = crate::VartaRuntime::new().engine;
        let first = crate::cancellation::Cancellation::new(engine.clone());
        let other = crate::cancellation::Cancellation::new(engine);
        let token = vault
            .rotate(None, "private-synthetic".into(), None)
            .unwrap();
        assert_eq!(token.len(), contract::TOKEN_HEX_BYTES);
        assert!(vault.rotate(None, "another".into(), None).is_err());
        let mut lease = None;
        assert_eq!(
            vault.read(&token, &first, &mut lease).unwrap().as_deref(),
            Some("private-synthetic")
        );
        let mut second = None;
        vault.read(&token, &other, &mut second).unwrap();
        drop(second);
        assert_eq!(
            vault.state.lock().unwrap().entries[&digest(&token)]
                .leases
                .len(),
            1
        );
        vault.clear(&token, Some(&other)).unwrap();
        assert!(first.check().is_err());
        assert!(other.check().is_ok());
        assert!(!vault.live(&token));
        drop(lease);
        let token = vault.rotate(None, "new".into(), None).unwrap();
        vault
            .state
            .lock()
            .unwrap()
            .entries
            .get_mut(&digest(&token))
            .unwrap()
            .idle = Instant::now();
        assert!(!vault.live(&token));
        assert!(vault.read(&token, &other, &mut None).unwrap().is_none());
        assert!(vault.state.lock().unwrap().entries.is_empty());
    }
}
