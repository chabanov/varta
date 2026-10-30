//! Portable code and its immutable private-capture schema share one owner.
use std::sync::Arc;
use varta_common::function::VerifiedFunctionCatalog;

/// A validated module paired with its own capture metadata. Cloning keeps that
/// pairing intact; no metadata is recovered from mutable guest memory.
#[derive(Clone)]
pub struct PreparedModule {
    pub(crate) module: wasmtime::Module,
    pub(crate) function_catalog: Option<Arc<VerifiedFunctionCatalog>>,
    /// Per-value data limits of the module's declared profile.
    pub(crate) data: varta_common::limits::DataLimits,
}
impl PreparedModule {
    pub fn module(&self) -> &wasmtime::Module {
        &self.module
    }
    pub fn function_catalog(&self) -> Option<&VerifiedFunctionCatalog> {
        self.function_catalog.as_deref()
    }
}
