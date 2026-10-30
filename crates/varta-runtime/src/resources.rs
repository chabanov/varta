//! Typed resource failures of v38 (`data_limits`) guests. A guest that
//! exhausts a real resource stores its code and request in two exported
//! globals immediately before an `unreachable` trap; only such a trap of a
//! module whose profile reports resources becomes a [`ResourceExhausted`]
//! error. The globals select a diagnostic and carry no authority.
use crate::{RuntimeContext, RuntimeError};
use varta_common::limits::{self, Resource, ResourceExhausted};
use wasmtime::{Instance, Store, Trap, Val};

pub(crate) fn classify(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
    error: RuntimeError,
) -> RuntimeError {
    if !store.data().module.limits.data.reports_resources {
        return error;
    }
    let RuntimeError::Execution(source) = &error else {
        return error;
    };
    if source.downcast_ref::<Trap>() != Some(&Trap::UnreachableCodeReached) {
        return error;
    }
    let mut read = |name| match instance
        .get_global(&mut *store, name)
        .map(|global| global.get(&mut *store))
    {
        Some(Val::I32(value)) => Some(value),
        _ => None,
    };
    let (Some(code), Some(requested)) = (
        read(limits::RESOURCE_FAILURE),
        read(limits::RESOURCE_REQUEST),
    ) else {
        return error;
    };
    match Resource::from_code(code) {
        Some(resource) => RuntimeError::Resource(ResourceExhausted {
            resource,
            requested: requested as u32,
        }),
        None => error,
    }
}
