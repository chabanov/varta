#![forbid(unsafe_code)]
//! Engine contracts, ABI, capability model and shared types.
//!
//! Default feature `std` builds the whole crate. Without it the crate is a
//! `no_std` + `alloc` core holding the value layout: the ABI constants and
//! static region ([`abi`]), bounded guest-memory reads ([`memory`]), the
//! value tags, layout constants, primitive-cell reader and error classes
//! ([`value`]), the BigInt transport ([`bigint`]), the data limits
//! ([`limits`]), the UTF-16 helpers ([`text`]) and the checked JSON schema
//! model with its shared decoder ([`json`]). The linked
//! intrinsic library (`varta-intrinsics`, compiled to wasm32) reads guest
//! memory through that core, so the layout has one owner.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod abi;
pub mod bigint;
/// Per-turn and per-instance budgets (v38 `turn_budgets`).
#[cfg(feature = "std")]
pub mod budget;
#[cfg(feature = "std")]
mod capability;
#[cfg(feature = "std")]
mod diagnostic;
/// Host event sources and host tasks (v38 `turn_budgets`).
#[cfg(feature = "std")]
pub mod events;
/// The exception channel's exports and layout; its diagnostics need `std`.
pub mod exceptions;
/// Virtual file roots (v38 `file_roots`).
#[cfg(feature = "std")]
pub mod files;
/// Fair host fuel rates (v38): the published charge of host work.
pub mod fuel;
#[cfg(feature = "std")]
mod host;
#[cfg(feature = "std")]
pub mod http;
#[cfg(feature = "std")]
mod invocation;
/// Checked JSON schemas and decoding; without `std` the schema model, its
/// validation and the shared UTF-16 decoder ([`json::scan`]).
pub mod json;
#[cfg(feature = "std")]
pub mod language;
#[cfg(feature = "std")]
pub mod launcher;
/// Program and data limits by profile (v38 `program_limits`, `data_limits`).
pub mod limits;
#[cfg(feature = "std")]
mod manifest;
/// Bounded read access to guest linear memory.
pub mod memory;
#[cfg(feature = "std")]
mod module;
/// Asynchronous network requests (v37 `net_request`).
#[cfg(feature = "std")]
pub mod net;
/// Permission broker v1 (v38 `turn_budgets`).
#[cfg(feature = "std")]
pub mod permission;
/// Development fuel profiling (`varta dev --profile-fuel`).
#[cfg(feature = "std")]
pub mod profile;
#[cfg(feature = "std")]
pub mod scheduler;
pub mod text;
#[cfg(feature = "std")]
mod tool;
pub mod value;
/// L0 view protocol tables, unstable draft v0 (data only; no host operation).
#[cfg(feature = "std")]
pub mod view;
#[cfg(feature = "std")]
pub use capability::*;
#[cfg(feature = "std")]
pub use diagnostic::*;
#[cfg(feature = "std")]
pub use host::{HostOperation, HostScalar};
#[cfg(feature = "std")]
pub use invocation::*;
#[cfg(feature = "std")]
pub use manifest::*;
#[cfg(feature = "std")]
pub use module::{ModuleInfo, WasmImport, WasmInspection};
#[cfg(feature = "std")]
pub use tool::*;

#[cfg(feature = "std")]
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
#[cfg(feature = "std")]
pub type Result<T> = std::result::Result<T, VartaError>;

/// Guest exports the host calls to copy or retain values in guest memory
/// (tool, value and HTTP string ABIs). Runtime validation rejects a module in
/// which any function they reach through direct calls calls a host import;
/// fuel profile builds leave that call graph uninstrumented.
#[cfg(feature = "std")]
pub const HOST_FREE_EXPORTS: [&str; 6] = [
    TOOL_COPY_STRING,
    TOOL_RETAIN_STRING,
    value::VALUE_ALLOCATE,
    value::VALUE_RETAIN,
    http::COPY,
    http::RETAIN,
];

/// Host declarations for editors; semantic checking is implemented by the compiler.
#[cfg(feature = "std")]
pub const HOST_DECLARATIONS: &str = include_str!("varta.d.ts");

/// Opaque, capture-free function object layout for the checked function profile.
#[cfg(feature = "std")]
pub mod function;

#[cfg(feature = "std")]
pub mod native_math;

#[cfg(feature = "std")]
pub mod number_format;
