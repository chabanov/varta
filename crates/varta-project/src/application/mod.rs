//! Explicit app-scoped browser authority, separate from deny-all preview.
mod contract;
pub use contract::*;

mod upstream;
pub(crate) use upstream::Upstream;
