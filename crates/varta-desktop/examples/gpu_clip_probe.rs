//! Real offscreen GPU conformance for host-owned clipping, with bounded readback.
//! This does not launch a window or establish application/platform acceptance.
#[path = "../src/gpu_scene.rs"]
mod gpu_scene;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use varta_common::view::{Attribute as A, CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, PreparedFrame, Renderer, TextMeasurer, View};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 192;
const MAX_GPU_DIMENSION: u32 = 4096;
const GPU_ALLOCATION_LIMIT: u64 = 64 * 1024 * 1024;
const COPY_ROW_ALIGNMENT: u32 = 256;
const READBACK_DEADLINE: Duration = Duration::from_secs(10);
const CALLBACK_DEADLINE: Duration = Duration::from_secs(1);
const MAX_MEAN_RGB_DELTA: f64 = 2.0; // Same threshold as the existing GPU probe.
const PANEL: u32 = 2;
const FILL: u32 = 3;
const MARK: u32 = 4;
const INNER_FILL: u32 = 5;
const SIBLING: u32 = 6;
#[derive(Clone, Copy)]
enum Case {
    Plain,
    Rounded,
    Nested,
    Soft,
    Border,
    HugeInset,
    Fractional,
    Opacity,
    NestedOpacity,
    MixedBorder,
    MixedCorner,
    MixedCollision,
    Offset,
    Visible,
    Field,
    Password,
    Select,
    Button,
    Check,
    Radio,
}
impl Case {
    fn label(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Rounded => "rounded",
            Self::Nested => "nested",
            Self::Soft => "soft",
            Self::Border => "border",
            Self::HugeInset => "huge-inset",
            Self::Fractional => "fractional",
            Self::Opacity => "opacity",
            Self::NestedOpacity => "nested-opacity",
            Self::MixedBorder => "mixed-border",
            Self::MixedCorner => "mixed-corner",
            Self::MixedCollision => "mixed-collision",
            Self::Offset => "offset",
            Self::Visible => "visible",
            Self::Field => "field",
            Self::Password => "password",
            Self::Select => "select",
            Self::Button => "button",
            Self::Check => "check",
            Self::Radio => "radio",
        }
    }
    fn control(self) -> bool {
        matches!(
            self,
            Self::Field | Self::Password | Self::Select | Self::Button | Self::Check | Self::Radio
        )
    }
    fn software(self) -> bool {
        matches!(
            self,
            Self::Opacity
                | Self::NestedOpacity
                | Self::MixedBorder
                | Self::MixedCorner
                | Self::MixedCollision
        )
    }
    fn font(self) -> Result<Option<TextMeasurer>, Box<dyn std::error::Error>> {
        if !self.control() {
            return Ok(None);
        }
        Ok(Some(TextMeasurer::new(
            include_bytes!("../../varta-view/tests/fixtures/password.ttf").to_vec(),
            "Varta Password Fixture",
        )?))
    }
}
struct Denied;
impl Broker for Denied {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> Access {
        Access::Denied
    }
    fn clipboard_write(&mut self) -> Access {
        Access::Denied
    }
}
fn control_fixture(case: Case) -> Result<View, Box<dyn std::error::Error>> {
    let tag = match case {
        Case::Field => Tag::Textarea,
        Case::Select => Tag::Select,
        Case::Button => Tag::Button,
        _ => Tag::Input,
    };
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(PANEL, Tag::Div)
        .insert(1, PANEL, 0)
        .set_style(PANEL, P::Width, "120px")
        .set_style(PANEL, P::Height, "100px")
        .set_style(PANEL, P::Margin, "12px")
        .set_style(PANEL, P::BorderRadius, "32px")
        .set_style(PANEL, P::Overflow, "hidden")
        .set_style(PANEL, P::BackgroundColor, "#ffeeaa")
        .create_element(FILL, tag)
        .insert(PANEL, FILL, 0)
        .set_style(
            FILL,
            P::Display,
            if matches!(case, Case::Button) {
                "flex"
            } else {
                "block"
            },
        )
        .set_style(FILL, P::Width, "120px")
        .set_style(FILL, P::Height, "100px")
        .set_style(FILL, P::MinWidth, "0")
        .set_style(FILL, P::MinHeight, "0")
        .set_style(FILL, P::Padding, "0")
        .set_style(FILL, P::Border, "0 solid #000000")
        .set_style(FILL, P::BorderRadius, "50%")
        .set_style(FILL, P::Outline, "none")
        .set_style(FILL, P::BackgroundColor, "#eeeeff")
        .set_style(FILL, P::Color, "#ff0000")
        .set_style(FILL, P::FontFamily, "\"Varta Password Fixture\"")
        .set_style(FILL, P::FontSize, "20px")
        .set_style(FILL, P::FontWeight, "400")
        .set_style(FILL, P::FontStyle, "normal")
        .set_style(FILL, P::LineHeight, "20px");
    if matches!(case, Case::Button) {
        b.set_style(FILL, P::JustifyContent, "flex-start")
            .set_style(FILL, P::AlignItems, "flex-start");
    }
    match case {
        Case::Field => {
            b.set_value(FILL, "AAAA AA אבג AAAA", 0);
        }
        Case::Password => {
            b.set_attribute(FILL, A::Type, "password")
                .set_value(FILL, "Aé👨‍👩‍👧B", 0);
        }
        Case::Select => {
            b.create_element(MARK, Tag::Option)
                .create_text(INNER_FILL, "AAAA")
                .insert(MARK, INNER_FILL, 0)
                .insert(FILL, MARK, 0);
        }
        Case::Button => {
            b.create_text(MARK, "AAAA").insert(FILL, MARK, 0);
        }
        Case::Check | Case::Radio => {
            b.set_attribute(
                FILL,
                A::Type,
                if matches!(case, Case::Check) {
                    "checkbox"
                } else {
                    "radio"
                },
            )
            .set_checked(FILL, 1, 0);
        }
        _ => return Err("expected a native control fixture".into()),
    }
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Denied)?;
    Ok(view)
}
fn fixture(case: Case) -> Result<View, Box<dyn std::error::Error>> {
    if case.control() {
        return control_fixture(case);
    }
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(PANEL, Tag::Div)
        .insert(1, PANEL, 0)
        .set_style(PANEL, P::Width, "120px")
        .set_style(PANEL, P::Height, "100px")
        .set_style(PANEL, P::Margin, "12px")
        .set_style(PANEL, P::BorderRadius, "32px")
        .set_style(PANEL, P::Overflow, "hidden")
        .set_style(PANEL, P::BackgroundColor, "#ffeeaa")
        .create_element(FILL, Tag::Div)
        .insert(PANEL, FILL, 0)
        .set_style(FILL, P::Width, "120px")
        .set_style(FILL, P::Height, "100px")
        .set_style(FILL, P::BackgroundColor, "#0000ff")
        .create_element(MARK, Tag::Div)
        .insert(FILL, MARK, 0)
        .set_style(MARK, P::Width, "4px")
        .set_style(MARK, P::Height, "4px")
        .set_style(MARK, P::Margin, "32px")
        .set_style(MARK, P::BackgroundColor, "#ff0000")
        .create_element(SIBLING, Tag::Div)
        .insert(1, SIBLING, 0)
        .set_style(SIBLING, P::Width, "160px")
        .set_style(SIBLING, P::Height, "16px")
        .set_style(SIBLING, P::BackgroundColor, "#00ff00");
    match case {
        Case::Plain => {
            b.set_style(PANEL, P::BorderRadius, "0");
        }
        Case::Rounded => {}
        Case::Nested => {
            b.set_style(FILL, P::BorderRadius, "50%")
                .set_style(FILL, P::Overflow, "hidden")
                .create_element(INNER_FILL, Tag::Div)
                .insert(FILL, INNER_FILL, MARK)
                .set_style(INNER_FILL, P::Width, "120px")
                .set_style(INNER_FILL, P::Height, "100px")
                .set_style(INNER_FILL, P::MarginBottom, "-100px")
                .set_style(INNER_FILL, P::BackgroundColor, "#ff00ff60");
        }
        Case::Soft => {
            b.set_style(FILL, P::BackgroundColor, "#0000ff80")
                .set_style(PANEL, P::BorderRadius, "8px 32px 18px 44px");
        }
        Case::Border => {
            b.set_style(PANEL, P::Border, "6px solid #005000");
        }
        Case::HugeInset => {
            b.set_style(PANEL, P::Width, "70px")
                .set_style(PANEL, P::Height, "70px")
                .set_style(PANEL, P::Border, "4px solid #005000")
                .set_style(PANEL, P::BorderLeft, "60px solid #005000")
                .set_style(PANEL, P::BorderRadius, "0 100% 0 0");
        }
        Case::Fractional => {
            b.set_style(PANEL, P::Margin, "12.25px")
                .set_style(PANEL, P::Border, "3.5px solid #005000")
                .set_style(PANEL, P::BorderRadius, "7.25px 32.75px 18.5px 44.5px");
        }
        Case::Opacity => {
            b.set_style(PANEL, P::Opacity, "0.5");
        }
        Case::NestedOpacity => {
            b.set_style(PANEL, P::Opacity, "0.5")
                .set_style(FILL, P::Opacity, "0.375");
        }
        Case::MixedBorder | Case::MixedCollision => {
            let width = if matches!(case, Case::MixedCollision) {
                "30px"
            } else {
                "7px"
            };
            b.set_style(PANEL, P::Border, &format!("{width} solid #ff000080"))
                .set_style(PANEL, P::BorderRight, &format!("{width} solid #0000ff80"))
                .set_style(PANEL, P::BorderBottom, &format!("{width} solid #00ff0080"))
                .set_style(PANEL, P::BorderLeft, &format!("{width} solid #ffff0080"));
            if matches!(case, Case::MixedCollision) {
                b.set_style(PANEL, P::Width, "40px")
                    .set_style(PANEL, P::Height, "40px")
                    .set_style(PANEL, P::BorderRadius, "90px 0 90px 0");
            }
        }
        Case::MixedCorner => {
            b.set_style(PANEL, P::Width, "100px")
                .set_style(PANEL, P::Height, "40px")
                .set_style(PANEL, P::BorderTop, "30px solid #ff000080")
                .set_style(PANEL, P::BorderBottom, "30px solid #0000ff80")
                .set_style(PANEL, P::BorderRadius, "90px 0 0 0");
        }
        Case::Offset => {
            b.set_style(FILL, P::MarginTop, "-20px");
        }
        Case::Visible => {
            b.set_style(PANEL, P::Overflow, "visible");
        }
        _ => return Err("native control fixture was not dispatched".into()),
    }
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Denied)?;
    Ok(view)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 1 {
        return Err("usage: gpu_clip_probe OUTPUT_PREFIX".into());
    }
    if !Path::new(&args[0]).parent().is_some_and(Path::is_dir) {
        return Err("create output directory first".into());
    }
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits {
            max_texture_dimension_2d: MAX_GPU_DIMENSION,
            max_buffer_size: GPU_ALLOCATION_LIMIT,
            max_storage_buffer_binding_size: GPU_ALLOCATION_LIMIT,
            ..Default::default()
        },
        ..Default::default()
    }))?;
    eprintln!("GPU clip probe adapter: {:?}", adapter.get_info());
    println!(
        "{{\"boundary\":\"offscreen-with-readback; host box/control fixtures; no window or guest execution\"}}"
    );
    for scale in [1.0_f32, 2.0] {
        let (width, height) = (
            (WIDTH as f32 * scale) as u32,
            (HEIGHT as f32 * scale) as u32,
        );
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
            label: Some("Varta clip probe"),
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
        let target = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let row = (width * 4).next_multiple_of(COPY_ROW_ALIGNMENT);
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Varta clip readback"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut scene = vello_gpu::Scene::new(size.width, size.height);
        for case in [
            Case::Plain,
            Case::Rounded,
            Case::Nested,
            Case::Soft,
            Case::Border,
            Case::HugeInset,
            Case::Fractional,
            Case::Opacity,
            Case::NestedOpacity,
            Case::MixedBorder,
            Case::MixedCorner,
            Case::MixedCollision,
            Case::Offset,
            Case::Visible,
            Case::Field,
            Case::Password,
            Case::Select,
            Case::Button,
            Case::Check,
            Case::Radio,
        ] {
            let mut view = fixture(case)?;
            let mut cpu = Renderer::new(case.font()?, 16.0)?;
            let mut vector = Renderer::new(case.font()?, 16.0)?;
            let mut previous_pixels: Option<Vec<u8>> = None;
            for changed in [false, true] {
                if changed {
                    let mut b = B::new();
                    if matches!(case, Case::MixedCorner) {
                        b.set_style(PANEL, P::BorderBottom, "30px solid #ff00ff80");
                    } else if matches!(case, Case::MixedBorder | Case::MixedCollision) {
                        let width = if matches!(case, Case::MixedCollision) {
                            "30px"
                        } else {
                            "7px"
                        };
                        b.set_style(PANEL, P::BorderRight, &format!("{width} solid #ff00ff80"));
                    } else if case.control() {
                        b.set_style(FILL, P::Color, "#00ffff");
                    } else {
                        b.set_style(MARK, P::BackgroundColor, "#00ffff");
                    }
                    view.commit(&b.finish(), false, &mut Denied)?;
                }
                let snapshot = view.snapshot();
                let start = Instant::now();
                let reference = cpu.render(&view, width, height, scale)?;
                let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
                let dirty_pixels = cpu.stats().dirty_pixels;
                if changed {
                    assert!(
                        previous_pixels
                            .as_deref()
                            .is_some_and(|old| old != reference.pixels.as_slice()),
                        "mutation must be visible"
                    );
                    assert!(
                        dirty_pixels > 0 && dirty_pixels < (width as usize * height as usize),
                        "local change must reuse retained pixels"
                    );
                }
                previous_pixels = Some(reference.pixels.clone());
                let prepare_start = Instant::now();
                let prepared = vector.prepare(&view, width, height, scale)?;
                let prepare_ms = prepare_start.elapsed().as_secs_f64() * 1000.0;
                let prepare_dirty_pixels = vector.stats().dirty_pixels;
                let (metadata, list) = match prepared {
                    PreparedFrame::Vector { metadata, scene } => {
                        if case.software() {
                            return Err(
                                "isolation unexpectedly bypassed the old vector resource contract"
                                    .into(),
                            );
                        }
                        (metadata, scene)
                    }
                    PreparedFrame::Raster(frame) => {
                        if !case.software() {
                            return Err("ordinary fixture exceeded vector capacity".into());
                        }
                        assert_eq!(
                            frame, reference,
                            "explicit CPU fallback changed pixels or input geometry"
                        );
                        assert_eq!(view.snapshot(), snapshot, "fallback changed the view");
                        assert_eq!(
                            frame,
                            view.rasterize_scaled(width, height, scale, None)?,
                            "fallback must also match a fresh stateless frame"
                        );
                        assert_eq!(prepare_dirty_pixels, dirty_pixels);
                        let repeat_start = Instant::now();
                        let PreparedFrame::Raster(repeated) =
                            vector.prepare(&view, width, height, scale)?
                        else {
                            return Err("unchanged isolation lost raster fallback".into());
                        };
                        let repeat_ms = repeat_start.elapsed().as_secs_f64() * 1000.0;
                        assert_eq!(repeated, frame);
                        assert_eq!(vector.stats().dirty_pixels, 0);
                        assert_eq!(vector.damage(), None);
                        let label = case.label();
                        println!(
                            "{{\"case\":\"{label}\",\"scale\":{scale},\"changed\":{changed},\"cpu_prepare_ms\":{cpu_ms:.3},\"dirty_pixels\":{dirty_pixels},\"prepare_ms\":{prepare_ms:.3},\"prepare_dirty_pixels\":{prepare_dirty_pixels},\"unchanged_prepare_ms\":{repeat_ms:.3},\"unchanged_dirty_pixels\":0,\"mode\":\"cpu-fallback\",\"gpu_rendered\":false}}"
                        );
                        std::fs::write(
                            format!("{}-{label}-{scale}-{changed}.rgba", args[0]),
                            &frame.pixels,
                        )?;
                        continue;
                    }
                };
                assert_eq!(
                    &metadata, &*reference,
                    "GPU preparation changed input geometry"
                );
                assert_eq!(view.snapshot(), snapshot, "rendering changed the view");
                scene.reset_and_resize(size.width, size.height);
                gpu_scene::populate(&mut scene, &list, false);
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                gpu.render(
                    &scene,
                    &mut resources,
                    &device,
                    &queue,
                    &mut encoder,
                    &size,
                    &target,
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
                output
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let _ = send.send(result);
                    });
                device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(READBACK_DEADLINE),
                })?;
                receive.recv_timeout(CALLBACK_DEADLINE)??;
                let mapped = output.slice(..).get_mapped_range()?;
                let pixels = mapped
                    .chunks_exact(row as usize)
                    .flat_map(|r| r[..width as usize * 4].iter().copied())
                    .collect::<Vec<_>>();
                drop(mapped);
                output.unmap();
                let mut difference = 0_u64;
                let mut maximum = 0_u8;
                for (a, b) in reference
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(pixels.as_chunks::<4>().0)
                {
                    assert_eq!(b[3], 255);
                    for channel in 0..3 {
                        let delta = a[channel].saturating_add(255 - a[3]).abs_diff(b[channel]);
                        difference += u64::from(delta);
                        maximum = maximum.max(delta);
                    }
                }
                let mean = difference as f64 / (f64::from(width) * f64::from(height) * 3.0);
                let label = case.label();
                let clips = list
                    .commands()
                    .iter()
                    .filter(|draw| draw.clips.is_some())
                    .count();
                println!(
                    "{{\"case\":\"{label}\",\"scale\":{scale},\"changed\":{changed},\"cpu_prepare_ms\":{cpu_ms:.3},\"dirty_pixels\":{dirty_pixels},\"clipped_draws\":{clips},\"mode\":\"vector\",\"gpu_rendered\":true,\"mean_rgb_delta\":{mean:.4},\"max_rgb_delta\":{maximum}}}"
                );
                std::fs::write(
                    format!("{}-{label}-{scale}-{changed}.rgba", args[0]),
                    &pixels,
                )?;
                if mean > MAX_MEAN_RGB_DELTA {
                    return Err("GPU clip image differs substantially from CPU".into());
                }
            }
        }
    }
    Ok(())
}
