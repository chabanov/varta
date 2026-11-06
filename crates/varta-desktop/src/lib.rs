#![forbid(unsafe_code)]
//! Native desktop hosts over the I/O-free view engine: the `varta-shell`
//! packaged-application shell and the experimental `varta-preview`, which
//! share one window, input and render core ([`shell`]).
mod accessibility;
mod clipboard;
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "gpu")]
mod gpu_scene;
mod input_queue;
mod pixels;
mod sfnt;
pub mod shell;
#[cfg(target_os = "macos")]
pub mod text_raster;
mod timing;
mod wake;

/// The window's user event, named at the crate root by the platform
/// adapters (accessibility and GPU).
pub(crate) use shell::window::GuiEvent;
