//! Small helpers ported from Tailwind's `utils/`.
pub mod escape;
pub mod segment;
pub mod variables;

pub use escape::{escape, unescape};
pub use segment::segment;
