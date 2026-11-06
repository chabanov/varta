//! Placeholder: port in progress (`@utility`, port of `createCssUtility`).
use crate::ast::Node;
use crate::design_system::DesignSystem;

/// Registers one `@utility` on a design system.
pub type CustomUtility = Box<dyn FnOnce(&mut DesignSystem)>;

/// The registration of a `@utility` rule, or `None` for an invalid name.
#[must_use]
pub fn create_css_utility(_node: &Node) -> Option<CustomUtility> {
    None
}
