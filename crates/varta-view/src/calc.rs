//! Layout-time `calc()` (CSS Values 4 §10.10): a length summed with a
//! percentage only has a used value once layout knows the percentage basis.
//! The style engine lowers such a value to a math function of `px`,
//! percentage and number leaves; layout interns it here and hands Taffy an
//! opaque handle, an index into this thread's table (never a pointer), which
//! [`resolve`] evaluates against the basis Taffy supplies.
use std::cell::RefCell;
use std::collections::BTreeMap;
use varta_common::view::css::{self, math};

/// Distinct layout-time expressions one thread keeps. Interned texts are
/// never dropped, so a handle stays valid for every cached style; past this
/// many, a new expression is refused as unsupported.
pub(crate) const MAX_CALC_EXPRESSIONS: usize = 4096;
/// Taffy keeps three tag bits in a calc handle.
const HANDLE_SHIFT: u32 = 3;

#[derive(Default)]
struct Table {
    expressions: Vec<math::Expression>,
    index: BTreeMap<String, usize>,
}

thread_local! {
    static TABLE: RefCell<Table> = RefCell::new(Table::default());
}

/// No context units remain in a lowered expression.
const NO_REFERENCES: math::Resolution = math::Resolution {
    em: None,
    rem: 0.0,
    lh: None,
    rlh: None,
    viewport_width: 0.0,
    viewport_height: 0.0,
};

/// The handle of a lowered layout-time length, or `None` when `value` is
/// not one or the table is full.
pub(crate) fn handle(value: &str) -> Option<*const ()> {
    TABLE.with(|table| {
        let mut table = table.borrow_mut();
        let index = match table.index.get(value) {
            Some(index) => *index,
            None => {
                if table.expressions.len() >= MAX_CALC_EXPRESSIONS {
                    return None;
                }
                let expression = css::layout_math(value)?;
                table.expressions.push(expression);
                let index = table.expressions.len() - 1;
                table.index.insert(value.to_owned(), index);
                index
            }
        };
        let tagged = (index + 1).checked_shl(HANDLE_SHIFT)?;
        // An integer handle, never dereferenced (Taffy keeps it opaque).
        Some(std::ptr::without_provenance(tagged))
    })
}

/// A handle as a plain integer, for values kept outside Taffy's styles.
pub(crate) fn key(handle: *const ()) -> usize {
    handle.addr()
}

/// The used value of the expression of a [`key`] against `basis`.
pub(crate) fn resolve_key(key: usize, basis: f32) -> f32 {
    resolve(std::ptr::without_provenance(key), basis)
}

/// The used value of the expression behind `handle` against the
/// percentage `basis`, in CSS pixels; zero for an unknown handle.
pub(crate) fn resolve(handle: *const (), basis: f32) -> f32 {
    let index = (handle.addr() >> HANDLE_SHIFT).wrapping_sub(1);
    TABLE.with(|table| {
        table
            .borrow()
            .expressions
            .get(index)
            .and_then(|expression| expression.resolve_percentages(&NO_REFERENCES, f64::from(basis)))
            .map_or(0.0, |value| {
                let value = value as f32;
                if value.is_finite() {
                    value.clamp(-crate::units::MAX_LENGTH, crate::units::MAX_LENGTH)
                } else {
                    0.0
                }
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_interned_indices_and_resolve_against_the_basis() {
        let a = handle("calc(50% - 16px)").unwrap();
        assert_eq!(handle("calc(50% - 16px)"), Some(a));
        assert_eq!(a.addr() & 0b111, 0);
        assert_eq!(resolve(a, 200.0), 84.0);
        let b = handle("max(0px, calc(10% - 50px))").unwrap();
        assert_ne!(a, b);
        assert_eq!(resolve(b, 200.0), 0.0);
        assert_eq!(resolve(b, 1000.0), 50.0);
        let c = handle("max(0px, calc(70% + 16px))").unwrap();
        assert_eq!(resolve(c, 300.0), 226.0);
        assert_eq!(handle("calc(1em + 10%)"), None);
        assert_eq!(handle("10px"), None);
        assert_eq!(resolve(std::ptr::null(), 10.0), 0.0);
    }
}
