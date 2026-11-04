//! Same strict browser oracle, using the product's native glyph rasterizer.
#[cfg(target_os = "macos")]
#[path = "../../varta-view/examples/native_inline_probe/probe.rs"]
mod probe;
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::run(Some(|| {
        Box::new(varta_desktop::text_raster::CoreTextRasterizer::default())
    }))
}
#[cfg(not(target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("native text probe requires an implemented platform rasterizer".into())
}
