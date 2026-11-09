//! Fresh startup evidence, not a claim on future application health.
use super::*;

impl Registry {
    pub async fn readiness(&self, id: &str) -> Api<serde_json::Value> {
        if self.application.is_none() {
            return Err("not_found");
        }
        // Never serve idempotency replay or a cached native authorization here.
        self.check_application(id).await?;
        self.readiness_snapshot(id)
    }

    fn readiness_snapshot(&self, id: &str) -> Api<serde_json::Value> {
        let mut state = lock(&self.state);
        self.expire_locked(&mut state);
        let run = state.runs.get(id).ok_or("not_found")?;
        let now = Instant::now();
        if run.status.state != RunState::Active
            || run.status.guest_state != GuestState::Ready
            || run.client.is_none()
            || run.lease.is_none_or(|lease| lease <= now)
        {
            return Err("not_ready");
        }
        let (native, app) = run.scope.as_ref().ok_or("invalid_scope")?;
        let observed = now_ms();
        let remaining = run
            .lease
            .unwrap()
            .min(run.until)
            .saturating_duration_since(now);
        let valid_until = observed
            .saturating_add(remaining.as_millis().min(1000) as u64)
            .min(run.status.expires_at_ms)
            .min(native.expires_at_ms);
        if valid_until <= observed {
            return Err("not_ready");
        }
        let mut status = serde_json::to_value(&run.status).map_err(|_| "unavailable")?;
        status["channel"] = serde_json::json!({"delegationId":native.id,"bindingId":native.binding_id,"contextSha256":native.context_sha256,"parentGeneration":native.parent_generation,"access":app.access,"methods":app.access.methods(),"dataUrl":format!("{}/runs/{id}/data",self.origin)});
        Ok(
            serde_json::json!({"schemaVersion":1,"hostInstanceId":self.host_instance,"observedAtMs":observed,"validUntilMs":valid_until,"run":status}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{Access, ApplicationRequest, NativeScope};

    #[test]
    fn readiness_requires_live_claim_ready_checkpoint_and_returns_bounded_secret_free_pins() {
        let registry = Registry::new(
            super::super::super::tests::ceiling(),
            "a".repeat(64),
            "http://localhost:8810".into(),
            PathBuf::from("/unused"),
        );
        let other = Registry::new(
            super::super::super::tests::ceiling(),
            "a".repeat(64),
            "http://localhost:8810".into(),
            PathBuf::from("/unused"),
        );
        assert_ne!(registry.host_instance, other.host_instance);
        assert!(digest(&registry.host_instance));
        let id = "b".repeat(64);
        let native = NativeScope {
            schema_version: 1,
            id: "c".repeat(64),
            binding_id: "binding-00000001".into(),
            context_sha256: "d".repeat(64),
            parent_generation: "e".repeat(64),
            state: "active".into(),
            expires_at_ms: now_ms() + 60_000,
            path_prefix: "/private".into(),
            methods: vec!["GET".into()],
        };
        let (cancel, _) = watch::channel(false);
        lock(&registry.state).runs.insert(
            id.clone(),
            Run {
                scope: Some((
                    native,
                    ApplicationRequest {
                        public_origin: registry.origin.clone(),
                        access: Access::Read,
                    },
                )),
                cancel,
                key: "request-00000001".into(),
                status: RunStatus {
                    id: id.clone(),
                    host_sha256: registry.host.clone(),
                    request_sha256: "f".repeat(64),
                    bundle_sha256: "0".repeat(64),
                    state: RunState::Ready,
                    guest_state: GuestState::Pending,
                    error: None,
                    expires_at_ms: now_ms() + 60_000,
                },
                until: Instant::now() + Duration::from_secs(60),
                terminal: None,
                ticket: None,
                session: Some("secret-cookie-hash".into()),
                client: None,
                lease: Some(Instant::now() + Duration::from_secs(3)),
            },
        );
        assert_eq!(registry.readiness_snapshot(&id).unwrap_err(), "not_ready");
        {
            let mut state = lock(&registry.state);
            let run = state.runs.get_mut(&id).unwrap();
            run.status.state = RunState::Active;
            run.client = Some("secret-client-hash".into());
        }
        assert_eq!(registry.readiness_snapshot(&id).unwrap_err(), "not_ready");
        lock(&registry.state)
            .runs
            .get_mut(&id)
            .unwrap()
            .status
            .guest_state = GuestState::Ready;
        let value = registry.readiness_snapshot(&id).unwrap();
        assert_eq!(value["hostInstanceId"], registry.host_instance);
        assert_eq!(value["run"]["id"], id);
        assert_eq!(value["run"]["channel"]["bindingId"], "binding-00000001");
        let observed = value["observedAtMs"].as_u64().unwrap();
        let until = value["validUntilMs"].as_u64().unwrap();
        assert!(until > observed && until <= observed + 1000);
        assert!(!value.to_string().contains("secret-"));
        lock(&registry.state).runs.get_mut(&id).unwrap().lease =
            Some(Instant::now() - Duration::from_millis(1));
        assert_eq!(registry.readiness_snapshot(&id).unwrap_err(), "not_ready");
        assert_eq!(registry.status(&id).unwrap().state, RunState::Stopped);
        assert_eq!(
            registry.readiness_snapshot(&"9".repeat(64)).unwrap_err(),
            "not_found"
        );
    }
}
