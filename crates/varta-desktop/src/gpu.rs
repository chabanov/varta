//! Optional native UI backend; consumes host-validated scenes, no guest GPU API.
use crate::shell::window::Console;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use varta_view::DisplayList;
use vello_gpu::peniko::Color;
use winit::window::Window;

pub(crate) struct Backend {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: vello_gpu::Renderer,
    resources: vello_gpu::Resources,
    scene: vello_gpu::Scene,
    depth: wgpu::TextureView,
    lost: Arc<AtomicBool>,
    completed: Arc<AtomicU64>,
    submitted: u64,
    submitted_at: std::time::Instant,
    occluded: bool,
    test_loss_after: Option<u64>,
    console: Console,
}
impl Backend {
    pub fn new(
        window: Arc<Window>,
        events: Option<winit::event_loop::EventLoopProxy<crate::GuiEvent>>,
        console: Console,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(window.clone()),
        ));
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let limits = wgpu::Limits {
            max_texture_dimension_2d: 4096,
            max_buffer_size: 64 * 1024 * 1024,
            max_storage_buffer_binding_size: 64 * 1024 * 1024,
            ..Default::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Varta UI"),
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let lost = Arc::new(AtomicBool::new(false));
        let notify = events.clone();
        let mark = lost.clone();
        device.set_device_lost_callback(move |_, _| {
            mark.store(true, Ordering::Release);
            if let Some(events) = &notify {
                let _ = events.send_event(crate::GuiEvent::GpuLost);
            }
        });
        let mark = lost.clone();
        device.on_uncaptured_error(Arc::new(move |_| {
            mark.store(true, Ordering::Release);
            if let Some(events) = &events {
                let _ = events.send_event(crate::GuiEvent::GpuLost);
            }
        }));
        let size = window.inner_size();
        let width = size.width.clamp(1, 4096);
        let height = size.height.clamp(1, 4096);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("GPU surface unsupported")?;
        config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .ok_or("GPU surface requires encoded 8-bit color output")?;
        if cfg!(target_os = "macos") {
            config.color_space = wgpu::SurfaceColorSpace::DisplayP3;
        }
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);
        let mut settings = vello_gpu::RenderSettings::default();
        // Current admitted UI uses path clips, no filters or isolated layers.
        settings.memory_settings.layers_config.max_textures = Some(0);
        let (renderer, resources) = vello_gpu::Renderer::new_with(
            &device,
            &vello_gpu::RenderTargetConfig {
                format: config.format,
                width: width as u16,
                height: height as u16,
            },
            settings,
        );
        let depth = vello_gpu::Renderer::create_depth_texture_view(
            &device,
            &vello_gpu::RenderSize {
                width: width as u16,
                height: height as u16,
            },
        );
        console.report(format_args!(
            "GPU {:?} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        ));
        Ok(Self {
            surface,
            device,
            queue,
            config,
            renderer,
            resources,
            scene: vello_gpu::Scene::new(width as u16, height as u16),
            depth,
            lost,
            completed: Arc::new(AtomicU64::new(0)),
            submitted: 0,
            submitted_at: std::time::Instant::now(),
            occluded: false,
            test_loss_after: std::env::var("VARTA_PREVIEW_GPU_TEST_LOSS_AFTER")
                .ok()
                .and_then(|v| v.parse().ok())
                .filter(|n| *n > 0 && *n <= 1_000_000),
            console,
        })
    }
    pub fn visible(&self) -> bool {
        !self.occluded
    }
    pub fn unocclude(&mut self) {
        self.occluded = false;
    }
    pub fn poll(&self) -> Result<bool, String> {
        self.device
            .poll(wgpu::PollType::Poll)
            .map_err(|e| e.to_string())?;
        if self.lost.load(Ordering::Acquire) {
            return Err("GPU device lost or validation failed".into());
        }
        let pending = self.completed.load(Ordering::Acquire) < self.submitted;
        if pending && self.submitted_at.elapsed() > std::time::Duration::from_secs(2) {
            return Err("GPU completion timed out".into());
        }
        Ok(pending)
    }
    /// False means no frame acquired; it must not become an input target.
    pub fn draw(&mut self, list: &DisplayList, width: u32, height: u32) -> Result<bool, String> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err("GPU viewport exceeds native bounds".into());
        }
        if self.lost.load(Ordering::Acquire) {
            return Err("GPU device lost or validation failed".into());
        }
        if self.poll()? {
            return Ok(false);
        }
        let size = vello_gpu::RenderSize {
            width: width as u16,
            height: height as u16,
        };
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.depth = vello_gpu::Renderer::create_depth_texture_view(&self.device, &size);
        }
        self.scene.reset_and_resize(size.width, size.height);
        crate::gpu_scene::populate(
            &mut self.scene,
            list,
            self.config.color_space == wgpu::SurfaceColorSpace::DisplayP3,
        );
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(f)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    wgpu::CurrentSurfaceTexture::Occluded => {
                        self.occluded = true;
                        return Ok(false);
                    }
                    _ => return Err("GPU surface recovery failed".into()),
                }
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.occluded = true;
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Timeout => return Err("GPU surface timed out".into()),
            _ => return Err("GPU surface lost or invalid".into()),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.renderer
            .render(
                &self.scene,
                &mut self.resources,
                &self.device,
                &self.queue,
                &mut encoder,
                &size,
                &view,
                Some(&self.depth),
                &vello_gpu::TextureBindings::new(),
                vello_gpu::TargetInit::Clear(vello_gpu::ClearSettings::Viewport {
                    color: Color::WHITE,
                }),
            )
            .map_err(|e| e.to_string())?;
        self.submitted = self
            .submitted
            .checked_add(1)
            .ok_or("GPU frame sequence exhausted")?;
        let sequence = self.submitted;
        let completed = self.completed.clone();
        self.submitted_at = std::time::Instant::now();
        self.queue.submit([encoder.finish()]);
        self.queue.on_submitted_work_done(move || {
            completed.fetch_max(sequence, Ordering::Release);
        });
        self.queue.present(frame);
        if self.test_loss_after == Some(sequence) {
            self.console.report(format_args!(
                "diagnostic GPU device loss after frame {sequence}"
            ));
            self.device.destroy();
        }
        Ok(true)
    }
}
