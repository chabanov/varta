#![forbid(unsafe_code)]
//! Host-only rendering adapters, separate from the I/O-free view engine.
#[cfg(target_os = "macos")]
pub mod text_raster;
