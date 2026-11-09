//! Explicit server-host sessions, separate from ordinary guest networking.
mod host;
mod request;
mod vault;
pub(crate) use host::register;
pub(crate) use request::RequestSession;
pub(crate) use vault::{Issue, Vault};
const UNAVAILABLE: &str = "session host unavailable";
const INVALID: &str = "invalid session operation";
const FULL: &str = "session capacity exhausted";
