//! A classified tree owned by a suspendable materialization. Cancellation
//! must not recursively drop a deep tree on Wasmtime's small fiber stack.
use super::*;
pub(super) struct Owned(pub(super) Option<JsonValue>);
impl Drop for Owned {
    fn drop(&mut self) {
        let Some(value) = self.0.take() else {
            return;
        };
        let mut pending = alloc::vec![value];
        while let Some(value) = pending.pop() {
            match value {
                JsonValue::Union { value, .. } => pending.push(*value),
                JsonValue::Array { values, .. } | JsonValue::Record { fields: values, .. } => {
                    pending.extend(values)
                }
                JsonValue::Dictionary { entries, .. } => {
                    pending.extend(entries.into_iter().map(|(_, value)| value))
                }
                _ => {}
            }
        }
    }
}
