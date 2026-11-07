#![forbid(unsafe_code)]
//! Engine contracts, ABI, capability model and shared types.
//!
//! Default feature `std` builds the whole crate. Without it the crate is a
//! `no_std` + `alloc` core holding the value layout: the ABI constants and
//! static region ([`abi`]), bounded guest-memory reads ([`memory`]), the
//! value tags, layout constants, the cell, record, array, dictionary and
//! Error readers and the error classes ([`value`]), the closure envelope
//! readers ([`function`]), the BigInt transport ([`bigint`]), the data and
//! v38 program limits ([`limits`]), the UTF-16 helpers ([`text`]) and the
//! checked JSON schema model with its shared decoder ([`json`]). The linked
//! intrinsic library (`varta-intrinsics`, compiled to wasm32) reads guest
//! memory through that core, so the layout has one owner.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod abi;
pub mod bigint;
/// Generated browser-host registry, owned by the engine contracts.
#[cfg(feature = "std")]
pub mod browser;
/// Per-turn and per-instance budgets (v38 `turn_budgets`).
#[cfg(feature = "std")]
pub mod budget;
#[cfg(feature = "std")]
mod capability;
/// Idle and incremental collection (v38 `incremental_collection`).
#[cfg(feature = "std")]
pub mod collector;
/// Console output streams (v38 `console_methods`).
pub mod console;
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
/// Recorded host inputs of v38 local time: the zone and locale (K9 D4).
#[cfg(feature = "std")]
pub mod host_inputs;
#[cfg(feature = "std")]
pub mod http;
/// The v38 locale data format of `Intl` (K9 D4): blobs, segments and the
/// module manifest shared by the generator, the linker and linked code.
pub mod intl_data;
#[cfg(feature = "std")]
mod invocation;
/// Checked JSON schemas and decoding; without `std` the schema model, its
/// validation and the shared UTF-16 decoder ([`json::scan`]).
pub mod json;
/// Deno KV compatible storage (v38 `kv`).
#[cfg(feature = "std")]
pub mod kv;
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
/// The v38 RegExp program format shared by the compiler and the linked
/// backtracking matcher.
pub mod regexp;
#[cfg(feature = "std")]
pub mod scheduler;
pub mod text;
#[cfg(feature = "std")]
mod tool;
pub mod value;
/// L0 view protocol tables, unstable draft v0 (data only; no host operation).
/// The registry and the CSS engine (`view::css`, `view::stylesheet`,
/// `view::syntax`, `view::expansion`) are part of the `no_std` core, so the
/// browser host's Wasm style engine compiles the same code (feature
/// `view`, which `std` implies).
#[cfg(feature = "view")]
pub mod view;
/// Ephemeron tables and the checkpoint export of v38 weak references.
#[cfg(feature = "std")]
pub mod weak;
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
pub const HOST_FREE_EXPORTS: [&str; 10] = [
    TOOL_COPY_STRING,
    TOOL_RETAIN_STRING,
    value::VALUE_ALLOCATE,
    value::VALUE_RETAIN,
    http::COPY,
    http::RETAIN,
    http::STATE,
    http::VALUE,
    http::CHUNK_STATE,
    http::CHUNK_VALUE,
];

/// Host declarations for editors; semantic checking is implemented by the compiler.
#[cfg(feature = "std")]
pub const HOST_DECLARATIONS: &str = include_str!("varta.d.ts");

/// Opaque, capture-free function object layout for the checked function
/// profile. Without `std` only the layout constants and the closure envelope
/// readers (`function::ClosureView`) are compiled.
pub mod function;

#[cfg(feature = "std")]
pub mod native_math;

#[cfg(feature = "std")]
pub mod number_format;
