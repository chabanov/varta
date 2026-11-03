//! Local diagnostic: separate guest input turns from repeated native CPU frames.
use std::{hint::black_box, io::Read, path::Path, time::Instant};
use varta_runtime::{ApplicationOptions, Step, VartaRuntime, ViewLayout};
use varta_view::{Input, Target, TextMeasurer};

#[path = "../src/pixels.rs"]
mod pixels;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;
const FONT_SIZE: f32 = 16.0;
const SAMPLES: usize = 5;
const MAX_SAMPLES: usize = 512;
const MAX_FONT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TURNS: usize = 100_000;

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err("invalid file size".into());
    }
    Ok(bytes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        return Err(
            "usage: frame_timing WASM FONT FAMILY TARGET [--retained] [--verify] [--samples=N] [--type=TEXT | --scroll=N | --pointer]"
                .into(),
        );
    }
    let mut text_input = None;
    let mut scroll_input = None;
    let mut pointer_input = false;
    let mut retained = false;
    let mut verify = false;
    let mut samples = SAMPLES;
    for arg in &args[4..] {
        if arg == "--pointer" {
            pointer_input = true;
        } else if arg == "--retained" {
            retained = true;
        } else if arg == "--verify" {
            verify = true;
        } else if let Some(number) = arg.strip_prefix("--samples=") {
            samples = number.parse()?;
            if !(1..=MAX_SAMPLES).contains(&samples) {
                return Err("sample count outside bounds".into());
            }
        } else if let Some(text) = arg.strip_prefix("--type=") {
            text_input = Some(text.to_owned());
        } else if let Some(top) = arg.strip_prefix("--scroll=") {
            let top: f64 = top.parse()?;
            if !top.is_finite() || top < 0.0 {
                return Err("invalid scroll offset".into());
            }
            scroll_input = Some(top);
        } else {
            return Err("unknown benchmark option".into());
        }
    }
    if usize::from(text_input.is_some())
        + usize::from(scroll_input.is_some())
        + usize::from(pointer_input)
        > 1
    {
        return Err("choose one input mode".into());
    }
    let target = if args[3].starts_with('#') {
        Target::Ident(args[3].clone())
    } else if let Ok(node) = args[3].parse::<u32>() {
        Target::Node(node)
    } else {
        Target::Ident(format!("#{}", args[3]))
    };
    let wasm = read_bounded(
        Path::new(&args[0]),
        varta_common::abi::MAX_WASM_BYTES as u64,
    )?;
    let font_bytes = read_bounded(Path::new(&args[1]), MAX_FONT_BYTES)?;
    let font = || TextMeasurer::new(font_bytes.clone(), &args[2]);
    let runtime = VartaRuntime::new();
    let mut app = runtime.start_application(
        &wasm,
        ApplicationOptions {
            view_layout: Some(ViewLayout::new(WIDTH, HEIGHT, Some(font()?), FONT_SIZE)?),
            ..ApplicationOptions::default()
        },
    )?;
    if let Some(error) = app.failure() {
        return Err(error.to_string().into());
    }
    let mut measurer = font()?;
    let mut renderer = varta_view::Renderer::new(Some(font()?), FONT_SIZE)?;

    let pointer_target = if pointer_input {
        let snapshot = app.view_snapshot();
        let mut pending = vec![&snapshot["tree"]];
        let mut found = None;
        while let Some(node) = pending.pop() {
            let matches = match &target {
                Target::Node(id) => node["id"].as_u64() == Some(u64::from(*id)),
                Target::Ident(name) => node["attributes"]["id"].as_str() == name.strip_prefix('#'),
            };
            if matches {
                found = node["id"].as_u64().and_then(|id| u32::try_from(id).ok());
                break;
            }
            if let Some(children) = node["children"].as_array() {
                pending.extend(children);
            }
        }
        Some(found.ok_or("pointer target not found")?)
    } else {
        None
    };
    let mut rgb = Vec::new();
    eprintln!("frame_timing: ready");
    for scale in [1.0, 2.0] {
        let mut frame_ms = Vec::new();
        let mut turn_ms = Vec::new();
        let mut packing_ms = Vec::new();
        for sample in 0..samples {
            let started = Instant::now();
            let input = if let Some(text) = &text_input {
                Input::Type {
                    target: target.clone(),
                    text: text.clone(),
                }
            } else if let Some(top) = scroll_input {
                Input::Scroll {
                    target: target.clone(),
                    top: if sample % 2 == 0 { 0.0 } else { top },
                }
            } else {
                Input::Click {
                    target: target.clone(),
                }
            };
            if let Some(node) = pointer_target {
                let hovered = (sample % 4 != 3).then_some(node);
                let pressed = (sample % 4 == 1).then_some(node);
                app.view_set_pointer_state(hovered, pressed);
            } else {
                app.view_input(&input)?;
            }
            for turn in 0..MAX_TURNS {
                if turn + 1 == MAX_TURNS {
                    return Err("input did not settle".into());
                }
                match app.step(Some(std::time::Duration::ZERO)) {
                    Step::Turn(_) => {}
                    Step::Idle | Step::Finished { .. } => break,
                    Step::Failed => {
                        return Err(app
                            .failure()
                            .map_or("failed".into(), ToString::to_string)
                            .into());
                    }
                }
            }
            turn_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            let frame = if retained {
                app.view_render(
                    &mut renderer,
                    (WIDTH * scale) as u32,
                    (HEIGHT * scale) as u32,
                    scale,
                )?
            } else {
                app.view_rasterize_scaled(
                    (WIDTH * scale) as u32,
                    (HEIGHT * scale) as u32,
                    scale,
                    Some((&mut measurer, FONT_SIZE)),
                )?
            };
            frame_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            let packing = Instant::now();
            pixels::update_rgb(
                &frame,
                &mut rgb,
                retained.then(|| renderer.damage()).flatten(),
                sample == 0 || !retained,
            )?;
            black_box(rgb.clone());
            packing_ms.push(packing.elapsed().as_secs_f64() * 1000.0);
            if verify {
                let reference = app.view_rasterize_scaled(
                    (WIDTH * scale) as u32,
                    (HEIGHT * scale) as u32,
                    scale,
                    Some((&mut measurer, FONT_SIZE)),
                )?;
                let diff = frame
                    .pixels
                    .iter()
                    .zip(&reference.pixels)
                    .position(|(a, b)| a != b);
                if let Some(byte) = diff {
                    return Err(format!(
                        "reference differs at byte {byte}: {} != {}",
                        frame.pixels[byte], reference.pixels[byte]
                    )
                    .into());
                }
                let mut a = frame.clone();
                let mut b = reference;
                a.pixels.clear();
                b.pixels.clear();
                assert_eq!(a, b);
            }
            black_box(frame);
        }
        println!(
            "retained={retained} scale={scale} input_ms={turn_ms:?} frame_ms={frame_ms:?} packing_ms={packing_ms:?} stats={:?}",
            renderer.stats()
        );
    }
    Ok(())
}
