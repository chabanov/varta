//! Portable outline reference for the native/browser geometry and paint oracle.
#[path = "native_inline_probe/probe.rs"]
mod probe;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::run(None)
}
