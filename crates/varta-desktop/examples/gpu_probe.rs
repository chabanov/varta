//! Real GPU diagnostic. Readback is for verification, never the window path.
#[path = "../src/gpu_scene.rs"]
mod gpu_scene;
use std::{
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
use varta_runtime::{ApplicationOptions, Step, VartaRuntime, ViewLayout};
use varta_view::{Input, PreparedFrame, Renderer, Target, TextMeasurer};

fn read(path: &str, max: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > max {
        return Err("invalid probe file size".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("usage: gpu_probe WASM FONT FAMILY OUTPUT_PREFIX".into());
    }
    let wasm = read(&args[0], varta_common::abi::MAX_WASM_BYTES as u64)?;
    let font = read(&args[1], 4 * 1024 * 1024)?;
    if !Path::new(&args[3]).parent().is_some_and(Path::is_dir) {
        return Err("create output directory first".into());
    }
    let make_font = || TextMeasurer::new(font.clone(), &args[2]);
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits {
            max_texture_dimension_2d: 4096,
            max_buffer_size: 64 * 1024 * 1024,
            max_storage_buffer_binding_size: 64 * 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    }))?;
    eprintln!("GPU probe adapter: {:?}", adapter.get_info());
    println!(
        "{{\"wasm\":{:?},\"boundary\":\"offscreen-with-readback\"}}",
        varta_runtime::artifact_identity(&wasm)
    );
    for scale in [1.0_f32, 2.0] {
        let width = (800.0 * scale) as u32;
        let height = (600.0 * scale) as u32;
        let mut app = VartaRuntime::new().start_application(
            &wasm,
            ApplicationOptions {
                view_layout: Some(ViewLayout::new(800.0, 600.0, Some(make_font()?), 16.0)?),
                ..Default::default()
            },
        )?;
        if let Some(error) = app.failure() {
            return Err(error.to_string().into());
        }
        let mut cpu = Renderer::new(Some(make_font()?), 16.0)?;
        let mut vector = Renderer::new(Some(make_font()?), 16.0)?;
        let size = vello_gpu::RenderSize {
            width: width as u16,
            height: height as u16,
        };
        let mut settings = vello_gpu::RenderSettings::default();
        settings.memory_settings.layers_config.max_textures = Some(0);
        let (mut gpu, mut resources) = vello_gpu::Renderer::new_with(
            &device,
            &vello_gpu::RenderTargetConfig {
                format: wgpu::TextureFormat::Rgba8Unorm,
                width: size.width,
                height: size.height,
            },
            settings,
        );
        let depth = vello_gpu::Renderer::create_depth_texture_view(&device, &size);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Varta probe"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let row = (width * 4).next_multiple_of(256);
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Varta diagnostic readback"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut scene = vello_gpu::Scene::new(size.width, size.height);
        for case in [
            "initial",
            "counter",
            "check",
            "edit",
            "textarea",
            "selection",
            "preedit",
            "commit",
            "scroll",
            "select-open",
            "select-preview",
            "select-commit",
            "engine",
        ] {
            let input = match case {
                "counter" => Some(Input::Click {
                    target: Target::Ident("#increment".into()),
                }),
                "edit" => Some(Input::Type {
                    target: Target::Ident("#name".into()),
                    text: "Павло".into(),
                }),
                "check" => Some(Input::Click {
                    target: Target::Ident("#highlight".into()),
                }),
                "textarea" => Some(Input::Type {
                    target: Target::Ident("#editor".into()),
                    text: " Готово!".into(),
                }),
                "selection" => Some(Input::TextSelect {
                    target: Target::Ident("#editor".into()),
                    anchor: 0,
                    focus: "Украї".len(),
                    activate: true,
                }),
                "preedit" => Some(Input::ImePreedit {
                    target: Target::Ident("#editor".into()),
                    text: "Текст".into(),
                    cursor: Some((0, "Текст".len())),
                }),
                "commit" => Some(Input::ImeCommit {
                    target: Target::Ident("#editor".into()),
                    text: "Текст".into(),
                }),
                "scroll" => Some(Input::Scroll {
                    target: Target::Ident("#app".into()),
                    top: 80.0,
                }),
                "select-open" => Some(Input::Click {
                    target: Target::Ident("#topic".into()),
                }),
                "select-preview" | "select-commit" => Some(Input::Key {
                    target: Target::Ident("#topic".into()),
                    key: if case == "select-preview" {
                        "ArrowDown"
                    } else {
                        "Enter"
                    }
                    .into(),
                    shift: false,
                    ctrl: false,
                    alt: false,
                    meta: false,
                    composing: false,
                }),
                "engine" => Some(Input::Click {
                    target: Target::Ident("#run-engine".into()),
                }),
                _ => None,
            };
            if let Some(input) = input
                && let Err(error) = app.view_input(&input)
            {
                return Err(format!("{case}: {error}; {:?}", app.failure()).into());
            }
            for turn in 0..128 {
                match app.step(Some(Duration::ZERO)) {
                    Step::Turn(_) => {
                        if turn == 127 {
                            return Err("probe did not settle".into());
                        }
                    }
                    Step::Failed => return Err(app.failure().unwrap().to_string().into()),
                    _ => break,
                }
            }
            let t = Instant::now();
            let reference = app.view_render(&mut cpu, width, height, scale)?;
            let cpu_ms = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let PreparedFrame::Vector {
                metadata,
                scene: list,
            } = app.view_prepare(&mut vector, width, height, scale)?
            else {
                return Err("probe scene exceeded vector capacity".into());
            };
            let prepare_ms = t.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(
                &metadata, &*reference,
                "GPU preparation changed input geometry"
            );
            let t = Instant::now();
            scene.reset_and_resize(size.width, size.height);
            gpu_scene::populate(&mut scene, &list, false);
            let encode_ms = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            gpu.render(
                &scene,
                &mut resources,
                &device,
                &queue,
                &mut encoder,
                &size,
                &view,
                Some(&depth),
                &vello_gpu::TextureBindings::new(),
                vello_gpu::TargetInit::Clear(vello_gpu::ClearSettings::Viewport {
                    color: vello_gpu::peniko::Color::WHITE,
                }),
            )?;
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &output,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([encoder.finish()]);
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            output.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = send.send(r);
            });
            device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })?;
            receive.recv_timeout(Duration::from_secs(1))??;
            let mapped = output.slice(..).get_mapped_range()?;
            let pixels = mapped
                .chunks_exact(row as usize)
                .flat_map(|r| r[..width as usize * 4].iter().copied())
                .collect::<Vec<_>>();
            drop(mapped);
            output.unmap();
            let roundtrip_ms = t.elapsed().as_secs_f64() * 1000.0;
            let mut difference = 0u64;
            let mut max_delta = 0u8;
            for (a, b) in reference
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(pixels.as_chunks::<4>().0)
            {
                for c in 0..3 {
                    let expected = a[c].saturating_add(255 - a[3]);
                    let d = expected.abs_diff(b[c]);
                    difference += u64::from(d);
                    max_delta = max_delta.max(d);
                }
                assert_eq!(
                    b[3], 255,
                    "GPU background must match the CPU white compositor"
                );
            }
            let mean = difference as f64 / (f64::from(width) * f64::from(height) * 3.0);
            println!(
                "{{\"case\":\"{case}\",\"scale\":{scale},\"cpu_prepare_ms\":{cpu_ms:.3},\"vector_prepare_ms\":{prepare_ms:.3},\"scene_encode_ms\":{encode_ms:.3},\"gpu_readback_roundtrip_ms\":{roundtrip_ms:.3},\"mean_rgb_delta\":{mean:.4},\"max_rgb_delta\":{max_delta},\"draws\":{}}}",
                list.commands().len()
            );
            std::fs::write(format!("{}-{case}-{scale}.rgba", args[3]), &pixels)?;
            // AA implementations differ; preserve the complete output for review.
            if mean > 2.0 {
                return Err("GPU image differs substantially from the CPU reference".into());
            }
        }
    }
    Ok(())
}
