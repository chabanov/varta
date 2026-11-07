//! Immediate adapters of the shared yielding decode sequence.
use super::*;
impl<G: Guest> Guest for build::Synchronous<'_, G> {
    type Memory = G::Memory;
    fn memory(&self) -> &Self::Memory {
        self.0.memory()
    }
    fn ready(&mut self) -> Result<(), Self::Error> {
        self.0.ready()
    }
    fn deadline(&mut self) -> Result<(), Self::Error> {
        self.0.deadline()
    }
    fn charge(&mut self, fuel: u64) -> Result<(), Self::Error> {
        self.0.charge(fuel)
    }
    fn throw(&mut self, cell: u64) -> Result<(), Self::Error> {
        self.0.throw(cell)
    }
    fn result_length(&mut self, high: i32) -> Result<(), Self::Error> {
        self.0.result_length(high)
    }
    fn dispose(&mut self, value: JsonValue) -> Result<(), Self::Error> {
        self.0.dispose(value)
    }
    fn json_fatal(&self, message: String) -> Self::Error {
        self.0.json_fatal(message)
    }
    fn kv_fatal(&self, message: &'static str) -> Self::Error {
        self.0.kv_fatal(message)
    }
    fn abi_fatal(&self, error: ValueAbiError) -> Self::Error {
        self.0.abi_fatal(error)
    }
    fn deadline_error(&self) -> Self::Error {
        self.0.deadline_error()
    }
}
/// Synchronous entry points use the exact same checked sequence with an
/// immediate heap. Browser and ordinary native execution keep their API.
pub fn decode<G: Guest, D: Deadline + ?Sized>(
    guest: &mut G,
    module: ModuleBounds,
    deadline: &D,
    schema: (i32, i32),
    entry: Option<Entry<'_>>,
    read: impl FnOnce(&[u8], &ProgramLimits) -> Result<JsonSchema, String>,
) -> Result<u64, G::Error> {
    build::run_sync(decode_async(
        &mut build::Synchronous(guest),
        module,
        deadline,
        schema,
        entry,
        read,
    ))
}
pub fn materialize<G: Guest>(
    guest: &mut G,
    module: ModuleBounds,
    schema: &JsonSchema,
    value: JsonValue,
) -> Result<u64, G::Error> {
    build::run_sync(materialize_async(
        &mut build::Synchronous(guest),
        module,
        schema,
        value,
    ))
}
pub fn error<G: Guest>(
    guest: &mut G,
    module: ModuleBounds,
    schema: &JsonSchema,
    class: ErrorClass,
    message: &str,
) -> Result<u64, G::Error> {
    build::run_sync(error_async(
        &mut build::Synchronous(guest),
        module,
        schema,
        class,
        message,
    ))
}
pub fn type_error<G: Guest>(
    guest: &mut G,
    module: ModuleBounds,
    schema: &JsonSchema,
    message: &str,
) -> Result<u64, G::Error> {
    build::run_sync(type_error_async(
        &mut build::Synchronous(guest),
        module,
        schema,
        message,
    ))
}
