//! GPU swapchain presentation through `wgpu` + `winit` (feature `gpu-present`).
//!
//! `wgpu` picks the native API per host: Vulkan on Linux and Windows (DX12 also
//! available there), Metal on macOS, GL as a last resort. Its public API is safe, so
//! this backend keeps the crate free of `unsafe` code.
//!
//! The guest frame is uploaded into one RGBA8 texture and drawn with a fullscreen
//! triangle into the swapchain image, letterboxed to keep the guest aspect ratio.
//! The event loop is pumped once per presented frame (`pump_app_events`), so the
//! runtime keeps driving presentation from its own loop as it does with `minifb`.
//!
//! Adapter selection honours wgpu's own environment variables: `WGPU_BACKEND`
//! (`vulkan`, `metal`, `dx12`, `gl`) and `WGPU_ADAPTER_NAME` (substring match).
//! `OUNCE_VSYNC=0` turns vsync off.

#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};
use winit::window::{Window, WindowId};

use super::{BackendConfig, Frame, HostKey, PresentBackend, PresentStatus, ScaleMode};

/// Fullscreen triangle sampling the guest frame. UV (0,0) is the top-left guest pixel.
const BLIT_SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VsOut;
    out.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var frame_tex: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(frame_tex, frame_sampler, in.uv).rgb, 1.0);
}
"#;

/// Translates a `winit` physical key into the runtime's own key type.
pub fn host_key_from_winit(code: KeyCode) -> Option<HostKey> {
    Some(match code {
        KeyCode::ArrowLeft => HostKey::Left,
        KeyCode::ArrowRight => HostKey::Right,
        KeyCode::ArrowUp => HostKey::Up,
        KeyCode::ArrowDown => HostKey::Down,
        KeyCode::KeyW => HostKey::W,
        KeyCode::KeyA => HostKey::A,
        KeyCode::KeyS => HostKey::S,
        KeyCode::KeyD => HostKey::D,
        KeyCode::Space => HostKey::Space,
        KeyCode::KeyJ => HostKey::J,
        KeyCode::KeyK => HostKey::K,
        KeyCode::KeyU => HostKey::U,
        KeyCode::KeyI => HostKey::I,
        KeyCode::KeyQ => HostKey::Q,
        KeyCode::KeyE => HostKey::E,
        KeyCode::KeyZ => HostKey::Z,
        KeyCode::KeyC => HostKey::C,
        KeyCode::Enter | KeyCode::NumpadEnter => HostKey::Enter,
        KeyCode::Backspace => HostKey::Backspace,
        KeyCode::Escape => HostKey::Escape,
        _ => return None,
    })
}

/// Where the guest frame lands inside a target of `target_w` x `target_h` pixels.
///
/// Returns `(x, y, width, height)` in target pixels. Pure, so it is tested without a GPU.
pub fn viewport_rect(frame_w: u32, frame_h: u32, target_w: u32, target_h: u32, mode: ScaleMode) -> (u32, u32, u32, u32) {
    if frame_w == 0 || frame_h == 0 || target_w == 0 || target_h == 0 {
        return (0, 0, target_w, target_h);
    }

    let (w, h) = match mode {
        ScaleMode::Native => (frame_w.min(target_w), frame_h.min(target_h)),
        ScaleMode::Integer => {
            let scale = (target_w / frame_w).min(target_h / frame_h).max(1);
            ((frame_w * scale).min(target_w), (frame_h * scale).min(target_h))
        }
        ScaleMode::FitAspect => {
            // Compare target_w / target_h against frame_w / frame_h without floats.
            if (target_w as u64) * (frame_h as u64) > (target_h as u64) * (frame_w as u64) {
                let w = ((target_h as u64 * frame_w as u64) / frame_h as u64) as u32;
                (w.max(1), target_h)
            } else {
                let h = ((target_w as u64 * frame_h as u64) / frame_w as u64) as u32;
                (target_w, h.max(1))
            }
        }
    };

    ((target_w - w) / 2, (target_h - h) / 2, w, h)
}

/// Uploads guest frames and draws them into any colour target. Shared by the window
/// path and the offscreen test, so the test exercises the same shader and pipeline.
pub struct FrameBlitter {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture_format: wgpu::TextureFormat,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl FrameBlitter {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat, width: u32, height: u32, scale: ScaleMode) -> Self {
        // Guest pixels are already display-encoded. With an sRGB target, sample them
        // through an sRGB view so the hardware decode and re-encode cancel out.
        let texture_format = if target_format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ounce-present-blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ounce-present-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ounce-present-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ounce-present-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // Pixel-exact modes sample with nearest filtering; fitted output is smoothed.
        let filter = match scale {
            ScaleMode::Native | ScaleMode::Integer => wgpu::FilterMode::Nearest,
            ScaleMode::FitAspect => wgpu::FilterMode::Linear,
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ounce-present-sampler"),
            mag_filter: filter,
            min_filter: filter,
            ..Default::default()
        });

        let (texture, bind_group) =
            Self::create_frame_texture(device, &bind_group_layout, &sampler, texture_format, width, height);

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            texture_format,
            texture,
            bind_group,
            width,
            height,
        }
    }

    fn create_frame_texture(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ounce-guest-frame"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ounce-present-bind-group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        (texture, bind_group)
    }

    /// Copies a frame into the GPU texture, recreating it if the guest resolution changed.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame) {
        let (w, h) = (frame.width as u32, frame.height as u32);
        if w == 0 || h == 0 {
            return;
        }
        if w != self.width || h != self.height {
            let (texture, bind_group) = Self::create_frame_texture(
                device,
                &self.bind_group_layout,
                &self.sampler,
                self.texture_format,
                w,
                h,
            );
            self.texture = texture;
            self.bind_group = bind_group;
            self.width = w;
            self.height = h;
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Records a pass that clears the target to black and draws the frame into its viewport.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, target_w: u32, target_h: u32, mode: ScaleMode) {
        let (x, y, w, h) = viewport_rect(self.width, self.height, target_w, target_h, mode);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ounce-present-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Swapchain, device and blitter for one open window.
struct SurfaceState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    blitter: FrameBlitter,
    scale: ScaleMode,
}

impl SurfaceState {
    fn new(instance: &wgpu::Instance, window: Arc<Window>, cfg: &BackendConfig) -> Result<Self, String> {
        let size = window.inner_size();
        let surface = instance
            .create_surface(window)
            .map_err(|e| format!("cannot create a GPU surface: {e}"))?;

        let adapter = pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(instance, Some(&surface)))
            .map_err(|e| format!("no usable GPU adapter: {e}"))?;
        let info = adapter.get_info();
        println!("[Display] GPU adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ounce-present-device"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| format!("cannot open the GPU device: {e}"))?;

        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| "surface is not supported by this adapter".to_string())?;
        // Prefer a non-sRGB swapchain format: guest pixels are already display-encoded.
        let caps = surface.get_capabilities(&adapter);
        if let Some(format) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = format;
        }
        let vsync_off = std::env::var("OUNCE_VSYNC").map_or(false, |v| v == "0");
        config.present_mode = if vsync_off {
            wgpu::PresentMode::AutoNoVsync
        } else {
            wgpu::PresentMode::Fifo
        };
        surface.configure(&device, &config);

        let blitter = FrameBlitter::new(&device, config.format, cfg.width as u32, cfg.height as u32, cfg.scale);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            blitter,
            scale: cfg.scale,
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return; // minimised; keep the old configuration until the window is visible again
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render(&mut self, frame: &Frame) {
        self.blitter.upload(&self.device, &self.queue, frame);

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                // Present this one, then reconfigure for the next frame.
                self.draw_and_present(t);
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            // Timeout, occluded window or a caught validation error: skip this frame.
            _ => return,
        };
        self.draw_and_present(surface_texture);
    }

    fn draw_and_present(&mut self, surface_texture: wgpu::SurfaceTexture) {
        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-present-encoder"),
        });
        self.blitter
            .draw(&mut encoder, &view, self.config.width, self.config.height, self.scale);
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(surface_texture);
    }
}

/// `winit` application state, driven by `pump_app_events` from [`WgpuBackend::present`].
struct App {
    config: BackendConfig,
    instance: wgpu::Instance,
    window: Option<Arc<Window>>,
    gpu: Option<SurfaceState>,
    init_error: Option<String>,
    keys: HashSet<HostKey>,
    closed: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() || self.init_error.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(format!("{} [{} FPS Target]", self.config.title, self.config.target_fps))
            .with_inner_size(PhysicalSize::new(self.config.width as u32, self.config.height as u32));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                self.init_error = Some(format!("cannot create host window: {e}"));
                return;
            }
        };

        match SurfaceState::new(&self.instance, window.clone(), &self.config) {
            Ok(gpu) => {
                self.gpu = Some(gpu);
                self.window = Some(window);
            }
            Err(e) => self.init_error = Some(e),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.closed = true,
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let Some(key) = host_key_from_winit(code) else {
                    return;
                };
                match event.state {
                    ElementState::Pressed => {
                        self.keys.insert(key);
                        if key == HostKey::Escape {
                            self.closed = true;
                        }
                    }
                    ElementState::Released => {
                        self.keys.remove(&key);
                    }
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(), // no stuck keys after alt-tab
            _ => {}
        }
    }
}

pub struct WgpuBackend {
    event_loop: EventLoop<()>,
    app: App,
    frames: u64,
}

impl WgpuBackend {
    /// Opens a window and a GPU swapchain. Any failure is returned so the caller can
    /// fall back to the software window or headless presentation.
    ///
    /// Must be called from the main thread (a `winit` requirement on macOS and Linux).
    pub fn new(config: &BackendConfig) -> Result<Self, String> {
        let mut event_loop = EventLoop::new().map_err(|e| format!("cannot start the window event loop: {e}"))?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(
            event_loop.owned_display_handle(),
        )));

        let mut app = App {
            config: config.clone(),
            instance,
            window: None,
            gpu: None,
            init_error: None,
            keys: HashSet::new(),
            closed: false,
        };

        // Window creation happens in `resumed`, which arrives within the first few pumps.
        for _ in 0..100 {
            if let PumpStatus::Exit(code) = event_loop.pump_app_events(Some(Duration::from_millis(10)), &mut app) {
                return Err(format!("window event loop exited during start-up (code {code})"));
            }
            if let Some(e) = app.init_error.take() {
                return Err(e);
            }
            if app.gpu.is_some() {
                return Ok(Self {
                    event_loop,
                    app,
                    frames: 0,
                });
            }
        }
        Err("host window did not open within one second".to_string())
    }
}

impl PresentBackend for WgpuBackend {
    fn name(&self) -> &'static str {
        "wgpu"
    }

    fn present(&mut self, frame: &Frame) -> PresentStatus {
        if let PumpStatus::Exit(_) = self.event_loop.pump_app_events(Some(Duration::ZERO), &mut self.app) {
            self.app.closed = true;
        }
        if self.app.closed {
            return PresentStatus::Closed;
        }

        if let Some(gpu) = self.app.gpu.as_mut() {
            gpu.render(frame);
        }
        if let Some(window) = self.app.window.as_ref() {
            window.request_redraw();
        }

        self.frames += 1;
        PresentStatus::Continue
    }

    fn pressed_keys(&mut self) -> Vec<HostKey> {
        self.app.keys.iter().copied().collect()
    }

    fn is_open(&self) -> bool {
        self.app.window.is_some() && !self.app.closed
    }

    fn set_title(&mut self, title: &str) {
        if let Some(window) = self.app.window.as_ref() {
            window.set_title(title);
        }
    }

    fn frames_presented(&self) -> u64 {
        self.frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_winit_key_mapping_covers_the_hid_bindings() {
        let bound = [
            (KeyCode::ArrowLeft, HostKey::Left),
            (KeyCode::ArrowRight, HostKey::Right),
            (KeyCode::ArrowUp, HostKey::Up),
            (KeyCode::ArrowDown, HostKey::Down),
            (KeyCode::KeyW, HostKey::W),
            (KeyCode::KeyA, HostKey::A),
            (KeyCode::KeyS, HostKey::S),
            (KeyCode::KeyD, HostKey::D),
            (KeyCode::Space, HostKey::Space),
            (KeyCode::KeyJ, HostKey::J),
            (KeyCode::KeyK, HostKey::K),
            (KeyCode::KeyU, HostKey::U),
            (KeyCode::KeyI, HostKey::I),
            (KeyCode::KeyQ, HostKey::Q),
            (KeyCode::KeyE, HostKey::E),
            (KeyCode::KeyZ, HostKey::Z),
            (KeyCode::KeyC, HostKey::C),
            (KeyCode::Enter, HostKey::Enter),
            (KeyCode::Backspace, HostKey::Backspace),
            (KeyCode::Escape, HostKey::Escape),
        ];
        for (code, expected) in bound {
            assert_eq!(host_key_from_winit(code), Some(expected));
        }
        assert_eq!(host_key_from_winit(KeyCode::F12), None);
    }

    #[test]
    fn test_viewport_letterboxing() {
        // Same aspect: fills the target.
        assert_eq!(viewport_rect(1280, 720, 1920, 1080, ScaleMode::FitAspect), (0, 0, 1920, 1080));
        // Wider target: pillarbox.
        assert_eq!(viewport_rect(1280, 720, 2000, 720, ScaleMode::FitAspect), (360, 0, 1280, 720));
        // Taller target: letterbox.
        assert_eq!(viewport_rect(1280, 720, 1280, 1000, ScaleMode::FitAspect), (0, 140, 1280, 720));
        // Integer scaling picks the largest whole multiple that fits.
        assert_eq!(viewport_rect(1280, 720, 2600, 1500, ScaleMode::Integer), (20, 30, 2560, 1440));
        // Integer scaling never goes below 1x, even if the window is smaller.
        assert_eq!(viewport_rect(1280, 720, 640, 360, ScaleMode::Integer), (0, 0, 640, 360));
        // Native is centred and unscaled.
        assert_eq!(viewport_rect(64, 32, 128, 64, ScaleMode::Native), (32, 16, 64, 32));
        // Degenerate sizes fall back to the whole target instead of dividing by zero.
        assert_eq!(viewport_rect(0, 0, 100, 50, ScaleMode::FitAspect), (0, 0, 100, 50));
    }

    /// Renders a frame through the real pipeline into an offscreen texture and reads it back.
    ///
    /// Needs a GPU adapter. On machines without one (most CI runners, unless a software
    /// Vulkan driver such as lavapipe is installed) the test prints why and returns.
    #[test]
    fn test_offscreen_blit_matches_the_guest_frame() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = match pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None)) {
            Ok(adapter) => adapter,
            Err(e) => {
                eprintln!("skipping offscreen GPU blit test: no adapter ({e})");
                return;
            }
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ounce-present-test"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("adapter found but device could not be opened");

        // 64 px * 4 bytes = 256-byte rows, which satisfies the copy alignment rule.
        let (w, h) = (64u32, 16u32);
        let mut frame = Frame::new(w as usize, h as usize);
        for y in 0..h as usize {
            for x in 0..w as usize {
                frame.set_pixel(x, y, (x * 4) as u8, (y * 16) as u8, ((x + y) * 3) as u8);
            }
        }

        let format = wgpu::TextureFormat::Rgba8Unorm;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ounce-present-test-target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-present-test-readback"),
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut blitter = FrameBlitter::new(&device, format, w, h, ScaleMode::Native);
        blitter.upload(&device, &queue, &frame);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        blitter.draw(&mut encoder, &view, w, h, ScaleMode::Native);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * w),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| result.expect("readback map failed"));
        device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        let pixels = slice.get_mapped_range().expect("readback not mapped").to_vec();

        assert_eq!(pixels.len(), frame.rgba.len());
        assert!(pixels == frame.rgba, "GPU output differs from the guest frame");
    }
}
