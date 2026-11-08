use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CookieSite {
    #[default]
    Strict,
    None,
}
impl CookieSite {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Strict => "Strict",
            Self::None => "None",
        }
    }
}
fn age() -> u64 {
    DEFAULT_AGE_SECONDS
}
fn idle() -> u64 {
    DEFAULT_IDLE_SECONDS
}
fn capacity() -> usize {
    DEFAULT_SESSIONS
}

/// Host policy only. Neither the manifest nor the TS program grants an origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ServerPolicy {
    pub origin: String,
    pub client_origin: String,
    #[serde(default = "age")]
    pub max_age_seconds: u64,
    #[serde(default = "idle")]
    pub idle_seconds: u64,
    #[serde(default = "capacity")]
    pub max_sessions: usize,
    #[serde(default)]
    pub same_site: CookieSite,
    #[serde(default)]
    pub allow_insecure_loopback: bool,
}
impl ServerPolicy {
    pub fn validate(&self) -> crate::Result<()> {
        let invalid =
            || crate::VartaError::InvalidManifest("invalid host HTTP session policy".into());
        if !origin(&self.origin, self.allow_insecure_loopback)
            || !origin(&self.client_origin, self.allow_insecure_loopback)
            || !(1..=MAX_AGE_SECONDS).contains(&self.max_age_seconds)
            || !(1..=self.max_age_seconds).contains(&self.idle_seconds)
            || !(1..=MAX_SESSIONS).contains(&self.max_sessions)
            || self.same_site == CookieSite::None
                && (!self.origin.starts_with("https://")
                    || !self.client_origin.starts_with("https://"))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

/// Trusted browser-host binding, independent of the application's request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ClientBinding {
    pub api_origin: String,
    pub client_origin: String,
    #[serde(default)]
    pub allow_insecure_loopback: bool,
}
impl ClientBinding {
    pub fn validate(&self) -> crate::Result<()> {
        if !origin(&self.api_origin, self.allow_insecure_loopback)
            || !origin(&self.client_origin, self.allow_insecure_loopback)
        {
            return Err(crate::VartaError::InvalidManifest(
                "invalid protected browser session binding".into(),
            ));
        }
        Ok(())
    }
}
