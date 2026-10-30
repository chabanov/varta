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
    /// The declared profile (the legacy profile for undeclared modules): its
    /// structural limits bound the instance's readers, and under v38
    /// (`program_limits`) instantiation, whose data-segment copy Wasmtime
    /// meters per byte of the initialized image (including the gap before a
    /// static extension), is not charged to the guest's fuel.
    pub(crate) profile: varta_common::language::LanguageProfile,
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
    /// What the module's layout-bound readers accept: its static region and
    /// its profile's structural limits.
    pub fn bounds(&self) -> varta_common::value::ModuleBounds {
        varta_common::value::ModuleBounds::new(self.profile, self.static_region)
    }
}
