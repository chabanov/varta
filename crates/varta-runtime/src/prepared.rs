//! Portable code and its immutable private-capture schema share one owner.
use std::sync::Arc;
use varta_common::function::VerifiedFunctionCatalog;

/// A validated module paired with its own capture metadata. Cloning keeps that
/// pairing intact; no metadata is recovered from mutable guest memory.
#[derive(Clone)]
pub struct PreparedModule {
    pub(crate) module: wasmtime::Module,
    pub(crate) function_catalog: Option<Arc<VerifiedFunctionCatalog>>,
    /// Declared by the module's `varta.static` section (v38), validated
    /// against its memory and data segments before compilation.
    pub(crate) static_region: varta_common::abi::StaticRegion,
    /// V38 (`program_limits`): instantiation, whose data-segment copy
    /// Wasmtime meters per byte of the initialized image (including the gap
    /// before a static extension), is not charged to the guest's fuel.
    pub(crate) uncharged_initialization: bool,
}
impl PreparedModule {
    pub fn module(&self) -> &wasmtime::Module {
        &self.module
    }
    pub fn function_catalog(&self) -> Option<&VerifiedFunctionCatalog> {
        self.function_catalog.as_deref()
    }
    /// The module's static data region: a single page before v38.
    pub fn static_region(&self) -> varta_common::abi::StaticRegion {
        self.static_region
    }
}
