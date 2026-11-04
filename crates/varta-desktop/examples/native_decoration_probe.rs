#[cfg(target_os = "macos")]
#[path = "../../varta-view/examples/decoration_probe/probe.rs"]
mod probe;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        probe::run(Some(|| {
            Box::new(varta_desktop::text_raster::CoreTextRasterizer::default())
        }))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("native decoration probe requires an implemented platform rasterizer".into())
    }
}
