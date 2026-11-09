use super::*;
use std::sync::Arc;
use varta_common::{
    http::{HttpHeader, HttpResponse},
    session as contract,
};

pub(crate) struct RequestSession {
    vault: Arc<Vault>,
    _proof: Option<super::vault::Proof>,
    delegated: Option<String>,
    token: Option<String>,
    channel: bool,
    mutable: bool,
    pending: Option<super::vault::Staged>,
    cleared: bool,
    sealed: bool,
    lease: Option<super::vault::Lease>,
    owner: Option<crate::cancellation::Cancellation>,
}
fn field<'a>(headers: &'a [HttpHeader], name: &str) -> Result<Option<&'a str>, &'static str> {
    let mut fields = headers.iter().filter(|h| h.name.eq_ignore_ascii_case(name));
    let value = fields.next().map(|h| h.value.as_str());
    if fields.next().is_some() {
        return Err(INVALID);
    }
    Ok(value)
}
impl RequestSession {
    pub(crate) fn new(
        vault: Arc<Vault>,
        headers: &mut Vec<HttpHeader>,
        method: &str,
        owner: Option<crate::cancellation::Cancellation>,
    ) -> Result<Self, &'static str> {
        if vault.delegation_enabled()
            && field(headers, contract::delegation::REQUEST_PROOF_HEADER)?.is_some()
        {
            return Err(INVALID);
        }
        let marker = field(headers, contract::HEADER)?;
        let origin = field(headers, "origin")?;
        let same_origin = vault.policy.origin == vault.policy.client_origin
            && origin.is_none()
            && field(headers, "sec-fetch-site")? == Some("same-origin");
        let channel = marker == Some(contract::HEADER_VALUE)
            && (origin == Some(vault.policy.client_origin.as_str()) || same_origin);
        let name = if vault.policy.origin.starts_with("https://") {
            contract::COOKIE
        } else {
            contract::DEVELOPMENT_COOKIE
        };
        let mut token = None;
        for cookie in headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case("cookie"))
        {
            for part in cookie.value.split(';') {
                if let Some(value) = part
                    .trim()
                    .strip_prefix(name)
                    .and_then(|s| s.strip_prefix('='))
                {
                    if token.is_some()
                        || value.len() != contract::TOKEN_HEX_BYTES
                        || !value
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        return Err(INVALID);
                    }
                    token = Some(value.to_owned());
                }
            }
        }
        if (token.is_some() || marker.is_some()) && field(headers, "authorization")?.is_some() {
            return Err(INVALID);
        }
        if token.is_some() && !channel || marker.is_some() && !channel {
            return Err(INVALID);
        }
        headers.retain(|h| {
            !h.name.eq_ignore_ascii_case("cookie") && !h.name.eq_ignore_ascii_case(contract::HEADER)
        });
        let mut proof = None;
        let mut lease = None;
        if vault.delegation_enabled()
            && let Some(owner) = &owner
        {
            let (nonce, guard, parent) = vault.request_proof(token.as_deref(), None, owner)?;
            headers.push(HttpHeader {
                name: contract::delegation::REQUEST_PROOF_HEADER.into(),
                value: nonce,
            });
            proof = Some(guard);
            lease = parent;
        }
        Ok(Self {
            _proof: proof,
            delegated: None,
            vault,
            token,
            channel,
            mutable: matches!(method, "POST" | "PUT" | "PATCH" | "DELETE"),
            pending: None,
            cleared: false,
            sealed: false,
            lease,
            owner,
        })
    }
    pub(crate) fn delegated(
        vault: Arc<Vault>,
        headers: &mut Vec<HttpHeader>,
        scope: String,
        owner: Option<crate::cancellation::Cancellation>,
    ) -> Result<Self, &'static str> {
        let owner = owner.ok_or(INVALID)?;
        if !vault.delegation_live(&scope)
            || headers.iter().any(|h| {
                h.name
                    .eq_ignore_ascii_case(contract::delegation::REQUEST_PROOF_HEADER)
                    || h.name.eq_ignore_ascii_case("cookie")
                    || h.name.eq_ignore_ascii_case("authorization")
                    || h.name.eq_ignore_ascii_case(contract::HEADER)
            })
        {
            return Err(INVALID);
        }
        let (nonce, proof, lease) = vault.request_proof(None, Some(&scope), &owner)?;
        headers.push(HttpHeader {
            name: contract::delegation::REQUEST_PROOF_HEADER.into(),
            value: nonce,
        });
        Ok(Self {
            vault,
            _proof: Some(proof),
            delegated: Some(scope),
            token: None,
            channel: false,
            mutable: false,
            pending: None,
            cleared: false,
            sealed: false,
            lease,
            owner: Some(owner),
        })
    }
    pub(crate) fn check(&self) -> Result<(), wasmtime::Error> {
        let invalid = match &self.delegated {
            Some(scope) => !self.vault.delegation_live(scope),
            None => {
                self.lease.is_some() && self.token.as_deref().is_none_or(|t| !self.vault.live(t))
            }
        };
        if invalid {
            return Err(wasmtime::Error::new(crate::cancellation::Cancelled));
        }
        Ok(())
    }
    pub(crate) fn get(&mut self) -> Result<Option<String>, &'static str> {
        if let Some(scope) = &self.delegated {
            return self.vault.delegated_secret(scope);
        }
        if !self.channel || self.owner.is_none() {
            return Err(INVALID);
        }
        let Some(token) = self.token.as_deref() else {
            return Ok(None);
        };
        let owner = self.owner.as_ref().ok_or(INVALID)?;
        self.vault.read(token, owner, &mut self.lease)
    }
    pub(crate) fn set(&mut self, secret: String) -> Result<(), &'static str> {
        if !self.channel
            || !self.mutable
            || self.owner.is_none()
            || self.sealed
            || secret.is_empty()
            || secret.len() > contract::MAX_SECRET_BYTES
        {
            return Err(INVALID);
        }
        // Replacing a staged write releases its previous reservation first.
        self.pending = None;
        self.pending = Some(self.vault.stage(
            self.token.as_deref(),
            secret,
            self.owner.as_ref(),
        )?);
        Ok(())
    }
    pub(crate) fn clear(&mut self) -> Result<(), &'static str> {
        if !self.channel || !self.mutable || self.owner.is_none() || self.sealed {
            return Err(INVALID);
        }
        if let Some(token) = self.token.take() {
            self.vault.clear(&token, self.owner.as_ref())?;
        }
        self.lease = None;
        self.pending = None;
        self.cleared = true;
        Ok(())
    }
    /// Validate the entire outgoing head before publishing a staged secret.
    /// Revocation by clear is immediate; a failed handler never rotates a token.
    pub(crate) fn finish(&mut self, response: &mut HttpResponse) -> Result<(), wasmtime::Error> {
        if self.sealed {
            return Ok(());
        }
        if response.headers.iter().any(|h| {
            h.name.eq_ignore_ascii_case("set-cookie")
                || h.name.to_ascii_lowercase().starts_with("access-control-")
        }) {
            return Err(wasmtime::Error::msg(
                "session response uses host-owned headers",
            ));
        }
        let secure = self.vault.policy.origin.starts_with("https://");
        let rotating = (200..300).contains(&response.status) && self.pending.is_some();
        let cookie = if rotating {
            Some(contract::cookie(
                &"0".repeat(contract::TOKEN_HEX_BYTES),
                self.vault.policy.max_age_seconds,
                secure,
                self.vault.policy.same_site.name(),
            ))
        } else if self.cleared {
            Some(contract::cookie(
                "",
                0,
                secure,
                self.vault.policy.same_site.name(),
            ))
        } else {
            None
        };
        if let Some(value) = cookie {
            response.headers.push(HttpHeader {
                name: "set-cookie".into(),
                value,
            });
        }
        if self.channel {
            response.headers.extend([
                HttpHeader {
                    name: "access-control-allow-origin".into(),
                    value: self.vault.policy.client_origin.clone(),
                },
                HttpHeader {
                    name: "access-control-allow-credentials".into(),
                    value: "true".into(),
                },
                HttpHeader {
                    name: "vary".into(),
                    value: "Origin".into(),
                },
                HttpHeader {
                    name: "cache-control".into(),
                    value: "no-store".into(),
                },
            ]);
        }
        crate::server::validate_response(response)?;
        if rotating {
            let staged = self
                .pending
                .take()
                .ok_or_else(|| wasmtime::Error::msg(INVALID))?;
            let token = staged.publish().map_err(wasmtime::Error::msg)?;
            let value = contract::cookie(
                &token,
                self.vault.policy.max_age_seconds,
                secure,
                self.vault.policy.same_site.name(),
            );
            if let Some(header) = response.headers.iter_mut().find(|h| h.name == "set-cookie") {
                header.value = value;
            }
            self.lease = None;
            self.token = Some(token);
            let _ = self.get().map_err(wasmtime::Error::msg)?;
        } else {
            self.pending = None;
        }
        self.sealed = true;
        Ok(())
    }
}
