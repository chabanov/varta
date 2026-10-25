#![forbid(unsafe_code)]
pub mod abi;
mod capability;
mod config;
mod diagnostic;
mod host;
mod invocation;
mod manifest;
mod module;
mod tool;
pub use capability::*;
pub use config::*;
pub use diagnostic::*;
pub use host::HostOperation;
pub use invocation::*;
pub use manifest::*;
pub use module::ModuleInfo;
pub use tool::*;

#[derive(Debug, thiserror::Error)]
pub enum VartaError {
    #[error("capability denied: {capability} for {target}")]
    CapabilityDenied { capability: String, target: String },
    #[error("invalid capability manifest: {0}")]
    InvalidManifest(String),
    #[error("{0}")]
    InvalidArtifact(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, VartaError>;

/// Host declarations for editors; semantic checking is implemented by the compiler.
pub const HOST_DECLARATIONS: &str = include_str!("varta.d.ts");
