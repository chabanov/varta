use super::*;
use crate::application::{ChannelStart, NativeScope, POLICY_ID as APPLICATION_POLICY};
impl Registry {
    pub fn authorize_browser(&self, id: &str, cookie: &str) -> Api<()> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        Self::browser_run(&mut s, id, cookie)?;
        Ok(())
    }
    pub fn cookie_name(&self) -> &'static str {
        if self.application.is_some() {
            "varta_application"
        } else {
            COOKIE_NAME
        }
    }
    pub async fn start_application(&self, request: ChannelStart, key: &str) -> Api<RunStatus> {
        if !key_valid(key)
            || request.schema_version != 1
            || request.policy_id != APPLICATION_POLICY
            || request.target != TARGET
            || !digest(&request.delegation_id)
            || !key_valid(&request.binding_id)
            || !digest(&request.context_sha256)
        {
            return Err("invalid_request");
        }
        let sha = identity(&request).map_err(|_| "invalid_request")?;
        {
            let mut s = lock(&self.state);
            self.expire_locked(&mut s);
            if s.tombstones.contains_key(key) {
                return Err("cancelled_request");
            }
            if let Some(r) = s.runs.values().find(|r| r.key == key) {
                return if r.status.request_sha256 == sha {
                    Ok(r.status.clone())
                } else {
                    Err("idempotency_conflict")
                };
            }
        }
        let upstream = self.application.as_ref().ok_or("invalid_request")?;
        let native = upstream.scope(&request.delegation_id).await?;
        if native.binding_id != request.binding_id
            || native.context_sha256 != request.context_sha256
        {
            return Err("invalid_scope");
        }
        let app = {
            let s = lock(&self.state);
            let catalog = s
                .catalogs
                .get(&request.bundle_sha256)
                .ok_or("bundle_not_found")?;
            let manifest = catalog
                .files
                .get("app.manifest.json")
                .ok_or("invalid_bundle")?;
            let manifest = varta_common::Manifest::parse(manifest).map_err(|_| "invalid_bundle")?;
            crate::application::artifact_request(&manifest, &self.origin)
                .map_err(|_| "invalid_bundle")?
        };
        if app
            .access
            .methods()
            .iter()
            .any(|m| !native.methods.contains(m))
        {
            return Err("invalid_scope");
        }
        self.start_inner(
            StartRequest {
                schema_version: request.schema_version,
                host_sha256: request.host_sha256,
                bundle_sha256: request.bundle_sha256,
                target: request.target,
                policy_id: POLICY_ID.into(),
                ttl_seconds: request.ttl_seconds,
            },
            key,
            sha,
            Some((native, app)),
        )
    }
    pub async fn check_application(&self, id: &str) -> Api<()> {
        let Some(upstream) = &self.application else {
            return Ok(());
        };
        let expected = {
            let mut s = lock(&self.state);
            self.expire_locked(&mut s);
            let run = s.runs.get(id).ok_or("not_found")?;
            if run.status.state.terminal() {
                return Err("ended");
            }
            run.scope
                .as_ref()
                .map(|(scope, _)| scope.clone())
                .ok_or("invalid_scope")?
        };
        let valid = upstream
            .scope(&expected.id)
            .await
            .is_ok_and(|scope| scope == expected);
        if !valid {
            let mut s = lock(&self.state);
            if let Some(run) = s.runs.get_mut(id)
                && !run.status.state.terminal()
            {
                Self::end(run, RunState::Stopped, Some("scope_lost"));
            }
            return Err("ended");
        }
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        if s.runs.get(id).is_none_or(|r| r.status.state.terminal()) {
            return Err("ended");
        }
        Ok(())
    }
    pub fn ticket_run(&self, ticket: &str) -> Api<String> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        s.runs
            .values()
            .find(|r| {
                r.ticket
                    .as_ref()
                    .is_some_and(|(h, until)| *until > Instant::now() && matches_secret(ticket, h))
            })
            .map(|r| r.status.id.clone())
            .ok_or("unauthorized")
    }
    pub fn channel_status(&self, id: &str) -> Api<serde_json::Value> {
        let s = lock(&self.state);
        let run = s.runs.get(id).ok_or("not_found")?;
        let (native, app) = run.scope.as_ref().ok_or("invalid_scope")?;
        Ok(
            serde_json::json!({"delegationId":native.id,"bindingId":native.binding_id,"contextSha256":native.context_sha256,"parentGeneration":native.parent_generation,"access":app.access,"methods":app.access.methods(),"dataUrl":format!("{}/runs/{id}/data",self.origin)}),
        )
    }
    pub fn application_inputs(&self, id: &str) -> Api<serde_json::Value> {
        let s = lock(&self.state);
        let r = s.runs.get(id).ok_or("not_found")?;
        let (_, app) = r.scope.as_ref().ok_or("invalid_scope")?;
        let manifest = app.ceiling(id);
        let grants = manifest.grants().map_err(|_| "invalid_scope")?;
        let scope = grants
            .iter()
            .find_map(|g| match &g.value {
                varta_common::CapabilityValue::Requests(scope) => Some(scope),
                _ => None,
            })
            .ok_or("invalid_scope")?;
        let mut budgets = serde_json::Map::new();
        for kind in varta_common::budget::BudgetKind::ALL {
            budgets.insert(
                kind.wire_name().into(),
                serde_json::json!(
                    self.ceiling
                        .budgets
                        .value(kind)
                        .unwrap_or(kind.production())
                ),
            );
        }
        Ok(
            serde_json::json!({"permissionCeiling":{"env.get":{"patterns":[crate::application::DATA_URL_ENV]},"net.session":{"patterns":scope.urls,"methods":scope.methods,"responseHeaders":scope.response_headers}},"sessionBinding":manifest.browser_session,"environmentInputs":[[crate::application::DATA_URL_ENV,format!("{}/runs/{id}/data",self.origin)]],"budgetCeiling":budgets,"pollMs":POLL_MS,"application":true}),
        )
    }
    pub fn data_scope(
        &self,
        id: &str,
        cookie: &str,
        method: &str,
    ) -> Api<(NativeScope, watch::Receiver<bool>)> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let r = Self::browser_run(&mut s, id, cookie)?;
        if r.client.is_none() {
            return Err("unauthorized");
        }
        let (native, app) = r.scope.as_ref().ok_or("invalid_scope")?;
        if !app.access.methods().iter().any(|m| m == method) {
            return Err("invalid_request");
        }
        Ok((native.clone(), r.cancel.subscribe()))
    }
}
