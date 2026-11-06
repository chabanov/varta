//! Small helpers ported from Tailwind's `utils/`.
pub mod brace_expansion;
pub mod compare;
pub mod compare_breakpoints;
pub mod decode_arbitrary_value;
pub mod dimensions;
pub mod escape;
pub mod infer_data_type;
pub mod is_color;
pub mod is_valid_arbitrary;
pub mod js_number;
pub mod math_operators;
pub mod replace_shadow_colors;
pub mod segment;
pub mod to_key_path;
pub mod topological_sort;
pub mod variables;

pub use escape::{escape, unescape};
pub use segment::segment;
