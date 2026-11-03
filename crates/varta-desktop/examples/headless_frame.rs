//! Render a compiled module's initial native frame without opening a window.
//! Usage: headless_frame WASM MANIFEST FONT FAMILY OUTPUT_PREFIX [FONT_ALIAS...]
use std::{io::Read, path::Path};
use varta_common::{MAX_MANIFEST_BYTES, Manifest, abi};
use varta_runtime::{ApplicationOptions, RunOptions, VartaRuntime, ViewLayout, ViewSnapshotFormat};
use varta_view::TextMeasurer;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 768;
const FONT_SIZE: f32 = 16.0;
const MAX_FONT_BYTES: usize = 4 * 1024 * 1024;

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > limit {
        return Err(format!("{}: invalid file size", path.display()).into());
    }
    Ok(bytes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 5 {
        return Err(
            "usage: headless_frame WASM MANIFEST FONT FAMILY OUTPUT_PREFIX [FONT_ALIAS...]".into(),
        );
    }
    let family = args[3].to_str().ok_or("font family must be UTF-8")?;
    let aliases = args[5..]
        .iter()
        .map(|alias| alias.to_str().ok_or("font alias must be UTF-8"))
        .collect::<Result<Vec<_>, _>>()?;
    let wasm = read_bounded(Path::new(&args[0]), abi::MAX_WASM_BYTES)?;
    let manifest = Manifest::parse(&read_bounded(Path::new(&args[1]), MAX_MANIFEST_BYTES)?)?;
    let font_bytes = read_bounded(Path::new(&args[2]), MAX_FONT_BYTES)?;
    let make_font = || TextMeasurer::new_with_aliases(font_bytes.clone(), family, &aliases);
    let runtime = VartaRuntime::new();
    let app = runtime.start_application(
        &wasm,
        ApplicationOptions {
            run: RunOptions::from_manifests(&manifest, None)?,
            view_layout: Some(ViewLayout::new(
                WIDTH as f32,
                HEIGHT as f32,
                Some(make_font()?),
                FONT_SIZE,
            )?),
            ..ApplicationOptions::default()
        },
    )?;
    let prefix = Path::new(&args[4]);
    std::fs::write(
        prefix.with_extension("json"),
        app.view_snapshot_json(ViewSnapshotFormat::Pretty)?,
    )?;
    if let Some(error) = app.failure() {
        return Err(error.to_string().into());
    }
    let image = app.view_rasterize(WIDTH, HEIGHT, Some((&mut make_font()?, FONT_SIZE)))?;
    std::fs::write(prefix.with_extension("rgba"), &image.pixels)?;
    eprintln!(
        "initial native frame: {}x{}, {} bytes, {} commits",
        image.width,
        image.height,
        image.pixels.len(),
        app.view_commits()
    );
    Ok(())
}
