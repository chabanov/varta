#![forbid(unsafe_code)]
pub mod abi;
pub mod bigint;
mod capability;
mod diagnostic;
pub mod exceptions;
mod host;
pub mod http;
mod invocation;
pub mod json;
pub mod language;
pub mod launcher;
mod manifest;
mod module;
pub mod scheduler;
pub mod text;
mod tool;
pub mod value;
/// L0 view protocol tables, unstable draft v0 (data only; no host operation).
pub mod view;
pub use capability::*;
pub use diagnostic::*;
pub use host::{HostOperation, HostScalar};
pub use invocation::*;
pub use manifest::*;
pub use module::{ModuleInfo, WasmImport, WasmInspection};
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

/// Opaque, capture-free function object layout for the checked function profile.
pub mod function;

pub mod native_math;

pub mod number_format;
