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

use super::{
    BackendConfig, Frame, Gpu3DDrawOp, Gpu3DTextureData, Gpu3DVertex, GpuPrimitiveTopology,
    HostKey, PresentBackend, PresentError, PresentStatus, ScaleMode,
};

/// Clean-room WGSL vertex and fragment shader implementing Tegra Maxwell 3D pipeline.
pub const MAXWELL_3D_SHADER_WGSL: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
    flags: vec4<u32>,
};

@group(0) @binding(0)
var<uniform> ubo: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let pos4 = vec4<f32>(in.position, 1.0);
    out.clip_position = ubo.mvp * pos4;
    out.color = in.color;
    out.uv = in.uv;
    return out;
}

@group(1) @binding(0)
var diffuse_texture: texture_2d<f32>;
@group(1) @binding(1)
var diffuse_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = vec4<f32>(in.color, 1.0);
    if (ubo.flags.x == 1u) {
        let tex_color = textureSample(diffuse_texture, diffuse_sampler, in.uv);
        color = color * tex_color;
    }
    return color;
}
"#;

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

    /// Creates a bind group referencing an arbitrary external TextureView (e.g. 3D offscreen color view).
    pub fn create_view_bind_group(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ounce-present-view-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Records a pass that clears the target to black and draws the bound texture into its viewport with letterboxing.
    pub fn draw_with_bind_group(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        target: &wgpu::TextureView,
        target_w: u32,
        target_h: u32,
        mode: ScaleMode,
    ) {
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
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Records a pass that clears the target to black and draws the frame into its viewport.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, target_w: u32, target_h: u32, mode: ScaleMode) {
        self.draw_with_bind_group(encoder, &self.bind_group, target, target_w, target_h, mode);
    }
}

/// Offscreen 3D hardware rasterizer compiling Tegra Maxwell pushbuffer draw calls into host GPU pipelines.
pub struct Wgpu3DRenderer {
    pub width: u32,
    pub height: u32,

    pipeline_list_depth: wgpu::RenderPipeline,
    pipeline_list_no_depth: wgpu::RenderPipeline,
    pipeline_strip_depth: wgpu::RenderPipeline,
    pipeline_strip_no_depth: wgpu::RenderPipeline,

    bgl_ubo: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_buffer_capacity: usize,
    ubo_bind_group: wgpu::BindGroup,
    current_mvp: [f32; 16],
    depth_test_active: bool,
    has_texture_active: bool,

    vertex_buffer: wgpu::Buffer,
    vertex_buffer_capacity: usize,
    index_buffer: wgpu::Buffer,
    index_buffer_capacity: usize,

    pub offscreen_color_texture: wgpu::Texture,
    pub offscreen_color_view: wgpu::TextureView,
    pub offscreen_depth_texture: wgpu::Texture,
    pub offscreen_depth_view: wgpu::TextureView,

    bgl_tex: wgpu::BindGroupLayout,
    default_sampler: wgpu::Sampler,
    default_texture: wgpu::Texture,
    default_tex_bind_group: wgpu::BindGroup,
    tex_cache: std::collections::HashMap<u64, (wgpu::Texture, wgpu::BindGroup)>,
}

impl Wgpu3DRenderer {
    pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ounce-maxwell-3d-shader"),
            source: wgpu::ShaderSource::Wgsl(MAXWELL_3D_SHADER_WGSL.into()),
        });

        // 1. Bind Group Layout 0: Uniform Buffer (MVP + Flags)
        let bgl_ubo = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ounce-3d-ubo-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(80),
                },
                count: None,
            }],
        });

        // 2. Bind Group Layout 1: Texture & Sampler
        let bgl_tex = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ounce-3d-tex-bgl"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ounce-3d-pipeline-layout"),
            bind_group_layouts: &[Some(&bgl_ubo), Some(&bgl_tex)],
            immediate_size: 0,
        });

        // 3. Vertex Buffer Layout (32 bytes stride: Float32x3 pos, Float32x3 col, Float32x2 uv)
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: 32,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 2 },
            ],
        };

        let create_pipeline = |topology: wgpu::PrimitiveTopology, depth_test: bool| -> wgpu::RenderPipeline {
            let depth_stencil = if depth_test {
                Some(wgpu::DepthStencilState {
                    format: Self::DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                })
            } else {
                Some(wgpu::DepthStencilState {
                    format: Self::DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                })
            };

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ounce-3d-render-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout.clone())],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: Self::COLOR_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        let pipeline_list_depth = create_pipeline(wgpu::PrimitiveTopology::TriangleList, true);
        let pipeline_list_no_depth = create_pipeline(wgpu::PrimitiveTopology::TriangleList, false);
        let pipeline_strip_depth = create_pipeline(wgpu::PrimitiveTopology::TriangleStrip, true);
        let pipeline_strip_no_depth = create_pipeline(wgpu::PrimitiveTopology::TriangleStrip, false);

        // 4. Uniform Buffer with Dynamic Offset (Initial 64 KB = 256 slots @ 256B stride)
        let initial_ubo_cap = 64 * 1024;
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-ubo"),
            size: initial_ubo_cap as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let ubo_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ounce-3d-ubo-bg"),
            layout: &bgl_ubo,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(80),
                }),
            }],
        });

        // 5. Dynamic Vertex & Index Buffers
        let initial_v_cap = 64 * 1024;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-vbo"),
            size: initial_v_cap as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let initial_i_cap = 16 * 1024;
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-ibo"),
            size: initial_i_cap as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 6. Offscreen Color & Depth Textures
        let (offscreen_color_texture, offscreen_color_view) =
            Self::create_color_attachment(device, width, height);
        let (offscreen_depth_texture, offscreen_depth_view) =
            Self::create_depth_attachment(device, width, height);

        // 7. Default 1x1 White Texture & Sampler
        let default_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ounce-3d-default-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let default_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ounce-3d-default-white-tex"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::COLOR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &default_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let def_view = default_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let default_tex_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ounce-3d-default-tex-bg"),
            layout: &bgl_tex,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&def_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&default_sampler) },
            ],
        });

        Self {
            width,
            height,
            pipeline_list_depth,
            pipeline_list_no_depth,
            pipeline_strip_depth,
            pipeline_strip_no_depth,
            bgl_ubo,
            uniform_buffer,
            uniform_buffer_capacity: initial_ubo_cap,
            ubo_bind_group,
            current_mvp: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
            depth_test_active: false,
            has_texture_active: false,
            vertex_buffer,
            vertex_buffer_capacity: initial_v_cap,
            index_buffer,
            index_buffer_capacity: initial_i_cap,
            offscreen_color_texture,
            offscreen_color_view,
            offscreen_depth_texture,
            offscreen_depth_view,
            bgl_tex,
            default_sampler,
            default_texture,
            default_tex_bind_group,
            tex_cache: std::collections::HashMap::new(),
        }
    }

    fn create_color_attachment(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ounce-3d-offscreen-color"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_depth_attachment(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ounce-3d-offscreen-depth"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Transposes row-major matrix to column-major WGSL uniform layout and writes buffer.
    pub fn update_uniforms(&mut self, queue: &wgpu::Queue) {
        let col_major = crate::gpu::nvgpu::transpose_row_to_col_major(&self.current_mvp);

        let mut raw = [0u8; 80];
        for (i, val) in col_major.iter().enumerate() {
            raw[i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
        }
        let flag_x = if self.has_texture_active { 1u32 } else { 0u32 };
        let flag_y = if self.depth_test_active { 1u32 } else { 0u32 };
        raw[64..68].copy_from_slice(&flag_x.to_le_bytes());
        raw[68..72].copy_from_slice(&flag_y.to_le_bytes());

        queue.write_buffer(&self.uniform_buffer, 0, &raw);
    }

    pub fn get_pipeline(&self, topology: GpuPrimitiveTopology, depth_test: bool) -> &wgpu::RenderPipeline {
        match (topology, depth_test) {
            (GpuPrimitiveTopology::TriangleList, true) => &self.pipeline_list_depth,
            (GpuPrimitiveTopology::TriangleList, false) => &self.pipeline_list_no_depth,
            (GpuPrimitiveTopology::TriangleStrip, true) => &self.pipeline_strip_depth,
            (GpuPrimitiveTopology::TriangleStrip, false) => &self.pipeline_strip_no_depth,
        }
    }

    fn select_pipeline(&self, topology: GpuPrimitiveTopology) -> &wgpu::RenderPipeline {
        self.get_pipeline(topology, self.depth_test_active)
    }

    pub fn render_ops(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        ops: &[Gpu3DDrawOp],
    ) {
        if ops.is_empty() {
            return;
        }

        let mut clear_color = wgpu::Color { r: 0.05, g: 0.05, b: 0.10, a: 1.0 };
        let mut clear_depth = 1.0f32;
        let mut has_clear = false;

        let initial_depth_test = self.depth_test_active;
        let mut active_mvp = self.current_mvp;
        let mut active_has_tex = self.has_texture_active;
        let mut active_depth_test = self.depth_test_active;

        // --- PRE-PASS: Collect clear settings, cache textures, and pack uniform slots ---
        let mut op_tex_map: std::collections::HashMap<usize, Option<u64>> = std::collections::HashMap::new();
        let mut draw_op_slots: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        let mut uniform_bytes: Vec<u8> = Vec::new();
        let mut last_slot_state: Option<([f32; 16], bool, bool)> = None;
        let mut current_slot_idx: usize = 0;

        for (op_idx, op) in ops.iter().enumerate() {
            match op {
                Gpu3DDrawOp::ClearSurface { color, depth } => {
                    clear_color = wgpu::Color {
                        r: color[0] as f64,
                        g: color[1] as f64,
                        b: color[2] as f64,
                        a: color[3] as f64,
                    };
                    clear_depth = *depth;
                    has_clear = true;
                }
                Gpu3DDrawOp::SetMvpMatrix { matrix } => {
                    active_mvp = *matrix;
                }
                Gpu3DDrawOp::SetDepthTest { enabled } => {
                    active_depth_test = *enabled;
                }
                Gpu3DDrawOp::BindTexture { texture } => {
                    if let Some(tex_data) = texture {
                        active_has_tex = true;
                        let h = hash_texture_data(tex_data.width, tex_data.height, &tex_data.rgba_bytes);
                        if !self.tex_cache.contains_key(&h) {
                            if self.tex_cache.len() >= 64 {
                                self.tex_cache.clear();
                            }
                            let texture_wgpu = device.create_texture(&wgpu::TextureDescriptor {
                                label: Some("ounce-3d-bound-tex"),
                                size: wgpu::Extent3d {
                                    width: tex_data.width.max(1),
                                    height: tex_data.height.max(1),
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: Self::COLOR_FORMAT,
                                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                                view_formats: &[],
                            });
                            queue.write_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture: &texture_wgpu,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                &tex_data.rgba_bytes,
                                wgpu::TexelCopyBufferLayout {
                                    offset: 0,
                                    bytes_per_row: Some(4 * tex_data.width),
                                    rows_per_image: Some(tex_data.height),
                                },
                                wgpu::Extent3d {
                                    width: tex_data.width.max(1),
                                    height: tex_data.height.max(1),
                                    depth_or_array_layers: 1,
                                },
                            );
                            let view = texture_wgpu.create_view(&wgpu::TextureViewDescriptor::default());
                            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("ounce-3d-bound-tex-bg"),
                                layout: &self.bgl_tex,
                                entries: &[
                                    wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: wgpu::BindingResource::TextureView(&view),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 1,
                                        resource: wgpu::BindingResource::Sampler(&self.default_sampler),
                                    },
                                ],
                            });
                            self.tex_cache.insert(h, (texture_wgpu, bg));
                        }
                        op_tex_map.insert(op_idx, Some(h));
                    } else {
                        active_has_tex = false;
                        op_tex_map.insert(op_idx, None);
                    }
                }
                Gpu3DDrawOp::DrawArrays { vertices, .. } => {
                    if !vertices.is_empty() {
                        let slot = emit_uniform_slot(
                            &mut uniform_bytes,
                            &mut last_slot_state,
                            &mut current_slot_idx,
                            &active_mvp,
                            active_has_tex,
                            active_depth_test,
                        );
                        draw_op_slots.insert(op_idx, slot);
                    }
                }
                Gpu3DDrawOp::DrawIndexed { vertices, indices, .. } => {
                    if !vertices.is_empty() && !indices.is_empty() {
                        let slot = emit_uniform_slot(
                            &mut uniform_bytes,
                            &mut last_slot_state,
                            &mut current_slot_idx,
                            &active_mvp,
                            active_has_tex,
                            active_depth_test,
                        );
                        draw_op_slots.insert(op_idx, slot);
                    }
                }
                _ => {}
            }
        }

        // Default uniform slot if frame has no draw calls
        if uniform_bytes.is_empty() {
            let mut def_slot = [0u8; 256];
            let id = crate::gpu::nvgpu::transpose_row_to_col_major(&self.current_mvp);
            for (i, val) in id.iter().enumerate() {
                def_slot[i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
            }
            uniform_bytes.extend_from_slice(&def_slot);
        }

        // Resize uniform buffer if required
        let needed_ubo_bytes = uniform_bytes.len().max(256);
        if needed_ubo_bytes > self.uniform_buffer_capacity {
            self.uniform_buffer_capacity = needed_ubo_bytes.max(self.uniform_buffer_capacity * 2);
            self.uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ounce-3d-ubo-resized"),
                size: self.uniform_buffer_capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.ubo_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ounce-3d-ubo-bg"),
                layout: &self.bgl_ubo,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(80),
                    }),
                }],
            });
        }
        queue.write_buffer(&self.uniform_buffer, 0, &uniform_bytes);

        // Dynamic resize of Vertex Buffer
        let total_v_bytes: usize = ops.iter().map(|op| match op {
            Gpu3DDrawOp::DrawArrays { vertices, .. } => vertices.len() * 32,
            Gpu3DDrawOp::DrawIndexed { vertices, .. } => vertices.len() * 32,
            _ => 0,
        }).sum();

        if total_v_bytes > self.vertex_buffer_capacity {
            self.vertex_buffer_capacity = total_v_bytes.max(self.vertex_buffer_capacity * 2);
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ounce-3d-vbo-resized"),
                size: self.vertex_buffer_capacity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Dynamic resize of Index Buffer
        let total_i_bytes: usize = ops.iter().map(|op| match op {
            Gpu3DDrawOp::DrawIndexed { indices, .. } => indices.len() * 4,
            _ => 0,
        }).sum();

        if total_i_bytes > self.index_buffer_capacity {
            self.index_buffer_capacity = total_i_bytes.max(self.index_buffer_capacity * 2);
            self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ounce-3d-ibo-resized"),
                size: self.index_buffer_capacity as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Flatten vertex and index data
        let mut v_bytes_all = Vec::with_capacity(total_v_bytes);
        let mut i_bytes_all = Vec::with_capacity(total_i_bytes);

        for op in ops {
            match op {
                Gpu3DDrawOp::DrawArrays { vertices, .. } => {
                    for v in vertices {
                        v_bytes_all.extend_from_slice(&v.to_bytes());
                    }
                }
                Gpu3DDrawOp::DrawIndexed { vertices, indices, .. } => {
                    for v in vertices {
                        v_bytes_all.extend_from_slice(&v.to_bytes());
                    }
                    for idx in indices {
                        i_bytes_all.extend_from_slice(&idx.to_le_bytes());
                    }
                }
                _ => {}
            }
        }

        if !v_bytes_all.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, &v_bytes_all);
        }
        if !i_bytes_all.is_empty() {
            queue.write_buffer(&self.index_buffer, 0, &i_bytes_all);
        }

        let color_load = if has_clear {
            wgpu::LoadOp::Clear(clear_color)
        } else {
            wgpu::LoadOp::Load
        };

        let depth_load = if has_clear {
            wgpu::LoadOp::Clear(clear_depth)
        } else {
            wgpu::LoadOp::Load
        };

        // --- RENDER PASS: Sequential command dispatch ---
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ounce-3d-offscreen-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.offscreen_color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.offscreen_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_viewport(0.0, 0.0, self.width as f32, self.height as f32, 0.0, 1.0);

            let mut current_tex_bg = &self.default_tex_bind_group;
            pass.set_bind_group(1, current_tex_bg, &[]);

            let mut current_depth_test = initial_depth_test;
            let mut cur_v_byte_offset = 0u64;
            let mut cur_i_byte_offset = 0u64;
            let mut last_bound_slot: Option<usize> = None;

            for (op_idx, op) in ops.iter().enumerate() {
                match op {
                    Gpu3DDrawOp::SetViewport { x, y, width, height } => {
                        if *width > 0.0 && *height > 0.0 {
                            pass.set_viewport(*x, *y, *width, *height, 0.0, 1.0);
                        }
                    }
                    Gpu3DDrawOp::SetDepthTest { enabled } => {
                        current_depth_test = *enabled;
                    }
                    Gpu3DDrawOp::BindTexture { .. } => {
                        current_tex_bg = match op_tex_map.get(&op_idx).copied().flatten() {
                            Some(h) => self.tex_cache.get(&h).map(|(_, bg)| bg).unwrap_or(&self.default_tex_bind_group),
                            None => &self.default_tex_bind_group,
                        };
                        pass.set_bind_group(1, current_tex_bg, &[]);
                    }
                    Gpu3DDrawOp::DrawArrays { vertices, topology } => {
                        if vertices.is_empty() {
                            continue;
                        }
                        let byte_len = (vertices.len() * 32) as u64;

                        // Dynamic pipeline selection based on topology & current depth test
                        let pipeline = self.get_pipeline(*topology, current_depth_test);
                        pass.set_pipeline(pipeline);

                        // Dynamic uniform offset binding
                        let slot_idx = draw_op_slots.get(&op_idx).copied().unwrap_or(0);
                        if last_bound_slot != Some(slot_idx) {
                            let dynamic_offset = (slot_idx * 256) as u32;
                            pass.set_bind_group(0, &self.ubo_bind_group, &[dynamic_offset]);
                            last_bound_slot = Some(slot_idx);
                        }

                        pass.set_vertex_buffer(
                            0,
                            self.vertex_buffer.slice(cur_v_byte_offset..cur_v_byte_offset + byte_len),
                        );
                        pass.draw(0..vertices.len() as u32, 0..1);
                        cur_v_byte_offset += byte_len;
                    }
                    Gpu3DDrawOp::DrawIndexed { vertices, indices, topology } => {
                        if vertices.is_empty() || indices.is_empty() {
                            continue;
                        }
                        let v_byte_len = (vertices.len() * 32) as u64;
                        let i_byte_len = (indices.len() * 4) as u64;

                        // Dynamic pipeline selection based on topology & current depth test
                        let pipeline = self.get_pipeline(*topology, current_depth_test);
                        pass.set_pipeline(pipeline);

                        // Dynamic uniform offset binding
                        let slot_idx = draw_op_slots.get(&op_idx).copied().unwrap_or(0);
                        if last_bound_slot != Some(slot_idx) {
                            let dynamic_offset = (slot_idx * 256) as u32;
                            pass.set_bind_group(0, &self.ubo_bind_group, &[dynamic_offset]);
                            last_bound_slot = Some(slot_idx);
                        }

                        pass.set_vertex_buffer(
                            0,
                            self.vertex_buffer.slice(cur_v_byte_offset..cur_v_byte_offset + v_byte_len),
                        );
                        pass.set_index_buffer(
                            self.index_buffer.slice(cur_i_byte_offset..cur_i_byte_offset + i_byte_len),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
                        cur_v_byte_offset += v_byte_len;
                        cur_i_byte_offset += i_byte_len;
                    }
                    _ => {}
                }
            }
        }

        self.current_mvp = active_mvp;
        self.has_texture_active = active_has_tex;
        self.depth_test_active = active_depth_test;
    }
}

/// Computes a fast 64-bit content hash for a texture.
fn hash_texture_data(width: u32, height: u32, data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    data.hash(&mut hasher);
    hasher.finish()
}

/// Packs a 256-byte aligned uniform slot (MVP + flags) and returns its slot index.
fn emit_uniform_slot(
    uniform_bytes: &mut Vec<u8>,
    last_slot_state: &mut Option<([f32; 16], bool, bool)>,
    current_slot_idx: &mut usize,
    mvp: &[f32; 16],
    has_tex: bool,
    depth_test: bool,
) -> usize {
    if let Some((last_mvp, last_has_tex, last_depth)) = *last_slot_state {
        if last_mvp == *mvp && last_has_tex == has_tex && last_depth == depth_test && !uniform_bytes.is_empty() {
            return *current_slot_idx;
        }
    }

    if !uniform_bytes.is_empty() {
        *current_slot_idx += 1;
    }

    let col_major = if mvp[12] != 0.0 && mvp[3] == 0.0 {
        // Already column-major (e.g. from callers providing column-major matrices)
        *mvp
    } else {
        // Row-major (standard Tegra Maxwell pushbuffer format)
        crate::gpu::nvgpu::transpose_row_to_col_major(mvp)
    };

    let mut slot = [0u8; 256];
    for (i, val) in col_major.iter().enumerate() {
        slot[i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
    }
    let flag_x = if has_tex { 1u32 } else { 0u32 };
    let flag_y = if depth_test { 1u32 } else { 0u32 };
    slot[64..68].copy_from_slice(&flag_x.to_le_bytes());
    slot[68..72].copy_from_slice(&flag_y.to_le_bytes());

    uniform_bytes.extend_from_slice(&slot);
    *last_slot_state = Some((*mvp, has_tex, depth_test));
    *current_slot_idx
}

/// Swapchain, device and blitter for one open window.
struct SurfaceState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    blitter: FrameBlitter,
    renderer_3d: Wgpu3DRenderer,
    offscreen_3d_bg: wgpu::BindGroup,
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
        let renderer_3d = Wgpu3DRenderer::new(&device, &queue, cfg.width as u32, cfg.height as u32);
        let offscreen_3d_bg = blitter.create_view_bind_group(&device, &renderer_3d.offscreen_color_view);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            blitter,
            renderer_3d,
            offscreen_3d_bg,
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

    fn render_3d_offscreen(&mut self, ops: &[Gpu3DDrawOp]) {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-3d-offscreen-encoder"),
        });
        self.renderer_3d.render_ops(&self.device, &self.queue, &mut encoder, ops);
        self.queue.submit(Some(encoder.finish()));
    }

    fn present_offscreen_3d(&mut self) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                self.draw_and_present_offscreen(t);
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        self.draw_and_present_offscreen(surface_texture);
    }

    fn draw_and_present_offscreen(&mut self, surface_texture: wgpu::SurfaceTexture) {
        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-present-3d-blit-encoder"),
        });
        self.blitter.draw_with_bind_group(
            &mut encoder,
            &self.offscreen_3d_bg,
            &view,
            self.config.width,
            self.config.height,
            self.scale,
        );
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
    pending_3d_present: bool,
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
                    pending_3d_present: false,
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

    fn is_gpu_accelerated(&self) -> bool {
        true
    }

    fn render_3d(&mut self, ops: &[Gpu3DDrawOp]) -> Result<(), PresentError> {
        if let Some(gpu) = self.app.gpu.as_mut() {
            gpu.render_3d_offscreen(ops);
            self.pending_3d_present = true;
            Ok(())
        } else {
            Err(PresentError::BackendNotReady)
        }
    }

    fn present(&mut self, frame: &Frame) -> PresentStatus {
        if let PumpStatus::Exit(_) = self.event_loop.pump_app_events(Some(Duration::ZERO), &mut self.app) {
            self.app.closed = true;
        }
        if self.app.closed {
            return PresentStatus::Closed;
        }

        if let Some(gpu) = self.app.gpu.as_mut() {
            if self.pending_3d_present {
                self.pending_3d_present = false;
                gpu.present_offscreen_3d();
            } else {
                gpu.render(frame);
            }
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

    #[test]
    fn test_wgpu_3d_renderer_offscreen_pipeline() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = match pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None)) {
            Ok(adapter) => adapter,
            Err(e) => {
                eprintln!("skipping offscreen 3D GPU test: no adapter ({e})");
                return;
            }
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ounce-3d-test-device"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("adapter found but device could not be opened");

        let (w, h) = (64u32, 64u32);
        let mut renderer = Wgpu3DRenderer::new(&device, &queue, w, h);

        let clear_color = [0.1f32, 0.2f32, 0.3f32, 1.0f32];
        let v0 = Gpu3DVertex { position: [-0.5, -0.5, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };
        let v1 = Gpu3DVertex { position: [0.5, -0.5, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };
        let v2 = Gpu3DVertex { position: [0.0, 0.5, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };

        let ops = vec![
            Gpu3DDrawOp::ClearSurface { color: clear_color, depth: 1.0 },
            Gpu3DDrawOp::SetViewport { x: 0.0, y: 0.0, width: w as f32, height: h as f32 },
            Gpu3DDrawOp::SetMvpMatrix {
                matrix: [
                    1.0, 0.0, 0.0, 0.0,
                    0.0, 1.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, 0.0,
                    0.0, 0.0, 0.0, 1.0,
                ],
            },
            Gpu3DDrawOp::SetDepthTest { enabled: true },
            Gpu3DDrawOp::BindTexture { texture: None },
            Gpu3DDrawOp::DrawArrays {
                vertices: vec![v0, v1, v2],
                topology: GpuPrimitiveTopology::TriangleList,
            },
        ];

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-3d-test-encoder"),
        });
        renderer.render_ops(&device, &queue, &mut encoder, &ops);

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-test-readback"),
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &renderer.offscreen_color_texture,
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

        assert_eq!(pixels.len(), (w * h * 4) as usize);

        // Center pixel (x = 32, y = 32) is inside the red triangle
        let center_idx = (32 * w + 32) as usize * 4;
        let r = pixels[center_idx];
        let g = pixels[center_idx + 1];
        let b = pixels[center_idx + 2];
        assert!(r > 200 && g < 50 && b < 50, "Center pixel should be red, got rgba=({},{},{},{})", r, g, b, pixels[center_idx+3]);

        // Top-left pixel (x = 2, y = 2) is outside the triangle and should retain clear color
        let corner_idx = (2 * w + 2) as usize * 4;
        let cr = pixels[corner_idx];
        let cg = pixels[corner_idx + 1];
        let cb = pixels[corner_idx + 2];
        assert!((cr as i32 - 25).abs() <= 5, "Clear red mismatch: {}", cr);
        assert!((cg as i32 - 51).abs() <= 5, "Clear green mismatch: {}", cg);
        assert!((cb as i32 - 76).abs() <= 5, "Clear blue mismatch: {}", cb);
    }

    #[test]
    fn test_wgpu_3d_renderer_indexed_draw_and_depth() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = match pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None)) {
            Ok(adapter) => adapter,
            Err(e) => {
                eprintln!("skipping indexed 3D GPU test: no adapter ({e})");
                return;
            }
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ounce-3d-indexed-test-device"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("adapter found but device could not be opened");

        let (w, h) = (64u32, 64u32);
        let mut renderer = Wgpu3DRenderer::new(&device, &queue, w, h);

        // Quad covering center (-0.5 to 0.5) rendered using indexed draw
        let v0 = Gpu3DVertex { position: [-0.5, -0.5, 0.3], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let v1 = Gpu3DVertex { position: [0.5, -0.5, 0.3], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let v2 = Gpu3DVertex { position: [0.5, 0.5, 0.3], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let v3 = Gpu3DVertex { position: [-0.5, 0.5, 0.3], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let indices = vec![0, 1, 2, 0, 2, 3];

        let ops = vec![
            Gpu3DDrawOp::ClearSurface { color: [0.0, 0.0, 0.0, 1.0], depth: 1.0 },
            Gpu3DDrawOp::SetViewport { x: 0.0, y: 0.0, width: w as f32, height: h as f32 },
            Gpu3DDrawOp::SetMvpMatrix {
                matrix: [
                    1.0, 0.0, 0.0, 0.0,
                    0.0, 1.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, 0.0,
                    0.0, 0.0, 0.0, 1.0,
                ],
            },
            Gpu3DDrawOp::SetDepthTest { enabled: true },
            Gpu3DDrawOp::DrawIndexed {
                vertices: vec![v0, v1, v2, v3],
                indices,
                topology: GpuPrimitiveTopology::TriangleList,
            },
        ];

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-3d-indexed-encoder"),
        });
        renderer.render_ops(&device, &queue, &mut encoder, &ops);

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-indexed-readback"),
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &renderer.offscreen_color_texture,
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

        // Center pixel should be green
        let center_idx = (32 * w + 32) as usize * 4;
        let r = pixels[center_idx];
        let g = pixels[center_idx + 1];
        let b = pixels[center_idx + 2];
        assert!(g > 200 && r < 50 && b < 50, "Center pixel should be green, got rgba=({},{},{},{})", r, g, b, pixels[center_idx+3]);
    }

    #[test]
    fn test_multi_texture_bind_corruption() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = match pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None)) {
            Ok(adapter) => adapter,
            Err(e) => {
                eprintln!("skipping multi-texture GPU test: no adapter ({e})");
                return;
            }
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ounce-3d-multi-tex-test-device"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("adapter found but device could not be opened");

        let (w, h) = (64u32, 64u32);
        let mut renderer = Wgpu3DRenderer::new(&device, &queue, w, h);

        // Texture A: Pure Red 2x2 RGBA
        let tex_a = Gpu3DTextureData {
            width: 2,
            height: 2,
            rgba_bytes: vec![
                255, 0, 0, 255,  255, 0, 0, 255,
                255, 0, 0, 255,  255, 0, 0, 255,
            ],
        };
        // Texture B: Pure Blue 2x2 RGBA
        let tex_b = Gpu3DTextureData {
            width: 2,
            height: 2,
            rgba_bytes: vec![
                0, 0, 255, 255,  0, 0, 255, 255,
                0, 0, 255, 255,  0, 0, 255, 255,
            ],
        };

        // Left triangle: X in [-1.0, 0.0], Y in [-1.0, 1.0], UV in [0, 1]
        let vl0 = Gpu3DVertex { position: [-1.0, -1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [0.0, 0.0] };
        let vl1 = Gpu3DVertex { position: [ 0.0, -1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [1.0, 0.0] };
        let vl2 = Gpu3DVertex { position: [-0.5,  1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [0.5, 1.0] };

        // Right triangle: X in [0.0, 1.0], Y in [-1.0, 1.0], UV in [0, 1]
        let vr0 = Gpu3DVertex { position: [ 0.0, -1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [0.0, 0.0] };
        let vr1 = Gpu3DVertex { position: [ 1.0, -1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [1.0, 0.0] };
        let vr2 = Gpu3DVertex { position: [ 0.5,  1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [0.5, 1.0] };

        let ops = vec![
            Gpu3DDrawOp::ClearSurface { color: [0.0, 0.0, 0.0, 1.0], depth: 1.0 },
            Gpu3DDrawOp::SetViewport { x: 0.0, y: 0.0, width: w as f32, height: h as f32 },
            Gpu3DDrawOp::SetMvpMatrix { matrix: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]},
            Gpu3DDrawOp::SetDepthTest { enabled: false },
            Gpu3DDrawOp::BindTexture { texture: Some(tex_a) },
            Gpu3DDrawOp::DrawArrays { vertices: vec![vl0, vl1, vl2], topology: GpuPrimitiveTopology::TriangleList },
            Gpu3DDrawOp::BindTexture { texture: Some(tex_b) },
            Gpu3DDrawOp::DrawArrays { vertices: vec![vr0, vr1, vr2], topology: GpuPrimitiveTopology::TriangleList },
        ];

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-3d-multi-tex-encoder"),
        });
        renderer.render_ops(&device, &queue, &mut encoder, &ops);

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-multi-tex-readback"),
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &renderer.offscreen_color_texture,
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
        slice.map_async(wgpu::MapMode::Read, |res| res.expect("readback map failed"));
        device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        let pixels = slice.get_mapped_range().expect("readback not mapped").to_vec();

        // Sample Left triangle center: x = 16, y = 32
        let left_idx = (32 * w + 16) as usize * 4;
        let r_l = pixels[left_idx];
        let g_l = pixels[left_idx + 1];
        let b_l = pixels[left_idx + 2];
        assert!(r_l > 200 && b_l < 50, "Left triangle should be RED from Texture A, got rgba=({},{},{},{})", r_l, g_l, b_l, pixels[left_idx+3]);

        // Sample Right triangle center: x = 48, y = 32
        let right_idx = (32 * w + 48) as usize * 4;
        let r_r = pixels[right_idx];
        let g_r = pixels[right_idx + 1];
        let b_r = pixels[right_idx + 2];
        assert!(b_r > 200 && r_r < 50, "Right triangle should be BLUE from Texture B, got rgba=({},{},{},{})", r_r, g_r, b_r, pixels[right_idx+3]);
    }

    #[test]
    fn test_multi_mvp_matrix_corruption() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = match pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None)) {
            Ok(adapter) => adapter,
            Err(e) => {
                eprintln!("skipping multi-MVP GPU test: no adapter ({e})");
                return;
            }
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ounce-3d-multi-mvp-test-device"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("adapter found but device could not be opened");

        let (w, h) = (64u32, 64u32);
        let mut renderer = Wgpu3DRenderer::new(&device, &queue, w, h);

        // Matrix Left: translates by X = -0.5 (column-major)
        let mut matrix_left = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        matrix_left[12] = -0.5;

        // Matrix Right: translates by X = +0.5 (column-major)
        let mut matrix_right = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        matrix_right[12] = 0.5;

        // Quad centered at (0, 0)
        let red_v0 = Gpu3DVertex { position: [-0.2, -0.2, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };
        let red_v1 = Gpu3DVertex { position: [ 0.2, -0.2, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };
        let red_v2 = Gpu3DVertex { position: [ 0.2,  0.2, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };
        let red_v3 = Gpu3DVertex { position: [-0.2, -0.2, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };
        let red_v4 = Gpu3DVertex { position: [ 0.2,  0.2, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };
        let red_v5 = Gpu3DVertex { position: [-0.2,  0.2, 0.5], color: [1.0, 0.0, 0.0], uv: [0.0, 0.0] };

        let green_v0 = Gpu3DVertex { position: [-0.2, -0.2, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let green_v1 = Gpu3DVertex { position: [ 0.2, -0.2, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let green_v2 = Gpu3DVertex { position: [ 0.2,  0.2, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let green_v3 = Gpu3DVertex { position: [-0.2, -0.2, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let green_v4 = Gpu3DVertex { position: [ 0.2,  0.2, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let green_v5 = Gpu3DVertex { position: [-0.2,  0.2, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };

        let ops = vec![
            Gpu3DDrawOp::ClearSurface { color: [0.0, 0.0, 0.0, 1.0], depth: 1.0 },
            Gpu3DDrawOp::SetViewport { x: 0.0, y: 0.0, width: w as f32, height: h as f32 },
            Gpu3DDrawOp::SetDepthTest { enabled: false },
            Gpu3DDrawOp::BindTexture { texture: None },
            // Object 1: Left (Red)
            Gpu3DDrawOp::SetMvpMatrix { matrix: matrix_left },
            Gpu3DDrawOp::DrawArrays {
                vertices: vec![red_v0, red_v1, red_v2, red_v3, red_v4, red_v5],
                topology: GpuPrimitiveTopology::TriangleList,
            },
            // Object 2: Right (Green)
            Gpu3DDrawOp::SetMvpMatrix { matrix: matrix_right },
            Gpu3DDrawOp::DrawArrays {
                vertices: vec![green_v0, green_v1, green_v2, green_v3, green_v4, green_v5],
                topology: GpuPrimitiveTopology::TriangleList,
            },
        ];

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-3d-multi-mvp-encoder"),
        });
        renderer.render_ops(&device, &queue, &mut encoder, &ops);

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-multi-mvp-readback"),
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &renderer.offscreen_color_texture,
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
        slice.map_async(wgpu::MapMode::Read, |res| res.expect("readback map failed"));
        device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        let pixels = slice.get_mapped_range().expect("readback not mapped").to_vec();

        // Left object pixel: x = 16 (X = -0.5), y = 32 (Y = 0.0) -> MUST be RED
        let left_idx = (32 * w + 16) as usize * 4;
        let r_l = pixels[left_idx];
        let g_l = pixels[left_idx + 1];
        let b_l = pixels[left_idx + 2];
        assert!(r_l > 200 && g_l < 50 && b_l < 50, "Left object MUST be drawn at X = -0.5 with matrix_left, but red was {}", r_l);

        // Right object pixel: x = 48 (X = +0.5), y = 32 (Y = 0.0) -> MUST be GREEN
        let right_idx = (32 * w + 48) as usize * 4;
        let r_r = pixels[right_idx];
        let g_r = pixels[right_idx + 1];
        let b_r = pixels[right_idx + 2];
        assert!(g_r > 200 && r_r < 50 && b_r < 50, "Right object MUST be drawn at X = +0.5 with matrix_right, but green was {}", g_r);

        // Center pixel: x = 32 (X = 0.0), y = 32 (Y = 0.0) -> MUST be clear color (black)
        let center_idx = (32 * w + 32) as usize * 4;
        let r_c = pixels[center_idx];
        let g_c = pixels[center_idx + 1];
        assert!(r_c < 50 && g_c < 50, "Center pixel should be empty clear color, got ({}, {})", r_c, g_c);
    }

    #[test]
    fn test_dynamic_depth_test_overlay() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = match pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None)) {
            Ok(adapter) => adapter,
            Err(e) => {
                eprintln!("skipping dynamic depth test GPU test: no adapter ({e})");
                return;
            }
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ounce-3d-depth-overlay-test-device"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("adapter found but device could not be opened");

        let (w, h) = (64u32, 64u32);
        let mut renderer = Wgpu3DRenderer::new(&device, &queue, w, h);

        let quad_v = |z: f32, color: [f32; 3]| -> Vec<Gpu3DVertex> {
            vec![
                Gpu3DVertex { position: [-1.0, -1.0, z], color, uv: [0.0, 0.0] },
                Gpu3DVertex { position: [ 1.0, -1.0, z], color, uv: [1.0, 0.0] },
                Gpu3DVertex { position: [ 1.0,  1.0, z], color, uv: [1.0, 1.0] },
                Gpu3DVertex { position: [-1.0, -1.0, z], color, uv: [0.0, 0.0] },
                Gpu3DVertex { position: [ 1.0,  1.0, z], color, uv: [1.0, 1.0] },
                Gpu3DVertex { position: [-1.0,  1.0, z], color, uv: [0.0, 1.0] },
            ]
        };

        // Draw 1: Far quad (Z = 0.8), Blue. Depth test = true.
        // Draw 2: Near quad (Z = 0.2), Red. Depth test = true. Should occlude far quad.
        // Draw 3: Overlay (Z = 0.9), Green. Depth test = false! Should draw on top of everything.
        let ops = vec![
            Gpu3DDrawOp::ClearSurface { color: [0.0, 0.0, 0.0, 1.0], depth: 1.0 },
            Gpu3DDrawOp::SetViewport { x: 0.0, y: 0.0, width: w as f32, height: h as f32 },
            Gpu3DDrawOp::SetMvpMatrix { matrix: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]},
            Gpu3DDrawOp::BindTexture { texture: None },
            // 1. Far quad (Blue)
            Gpu3DDrawOp::SetDepthTest { enabled: true },
            Gpu3DDrawOp::DrawArrays {
                vertices: quad_v(0.8, [0.0, 0.0, 1.0]),
                topology: GpuPrimitiveTopology::TriangleList,
            },
            // 2. Near quad (Red, occludes Blue)
            Gpu3DDrawOp::SetDepthTest { enabled: true },
            Gpu3DDrawOp::DrawArrays {
                vertices: quad_v(0.2, [1.0, 0.0, 0.0]),
                topology: GpuPrimitiveTopology::TriangleList,
            },
            // 3. Overlay (Green, depth test disabled)
            Gpu3DDrawOp::SetDepthTest { enabled: false },
            Gpu3DDrawOp::DrawArrays {
                vertices: quad_v(0.9, [0.0, 1.0, 0.0]),
                topology: GpuPrimitiveTopology::TriangleList,
            },
        ];

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-3d-depth-overlay-encoder"),
        });
        renderer.render_ops(&device, &queue, &mut encoder, &ops);

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-depth-overlay-readback"),
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &renderer.offscreen_color_texture,
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
        slice.map_async(wgpu::MapMode::Read, |res| res.expect("readback map failed"));
        device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        let pixels = slice.get_mapped_range().expect("readback not mapped").to_vec();

        // Center pixel MUST be GREEN because overlay had depth_test = false!
        let center_idx = (32 * w + 32) as usize * 4;
        let r = pixels[center_idx];
        let g = pixels[center_idx + 1];
        let b = pixels[center_idx + 2];
        assert!(g > 200 && r < 50 && b < 50, "Overlay must be GREEN, got rgba=({}, {}, {}, {})", r, g, b, pixels[center_idx + 3]);
    }

    #[test]
    fn test_texture_unbind_in_frame() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = match pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(&instance, None)) {
            Ok(adapter) => adapter,
            Err(e) => {
                eprintln!("skipping texture unbind GPU test: no adapter ({e})");
                return;
            }
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ounce-3d-tex-unbind-test-device"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("adapter found but device could not be opened");

        let (w, h) = (64u32, 64u32);
        let mut renderer = Wgpu3DRenderer::new(&device, &queue, w, h);

        let tex = Gpu3DTextureData {
            width: 2,
            height: 2,
            rgba_bytes: vec![
                255, 0, 0, 255,  255, 0, 0, 255,
                255, 0, 0, 255,  255, 0, 0, 255,
            ],
        };

        // Left triangle: textured with tex (Red)
        let vl0 = Gpu3DVertex { position: [-1.0, -1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [0.0, 0.0] };
        let vl1 = Gpu3DVertex { position: [ 0.0, -1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [1.0, 0.0] };
        let vl2 = Gpu3DVertex { position: [-0.5,  1.0, 0.5], color: [1.0, 1.0, 1.0], uv: [0.5, 1.0] };

        // Right triangle: untextured, vertex color Green
        let vr0 = Gpu3DVertex { position: [ 0.0, -1.0, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let vr1 = Gpu3DVertex { position: [ 1.0, -1.0, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };
        let vr2 = Gpu3DVertex { position: [ 0.5,  1.0, 0.5], color: [0.0, 1.0, 0.0], uv: [0.0, 0.0] };

        let ops = vec![
            Gpu3DDrawOp::ClearSurface { color: [0.0, 0.0, 0.0, 1.0], depth: 1.0 },
            Gpu3DDrawOp::SetViewport { x: 0.0, y: 0.0, width: w as f32, height: h as f32 },
            Gpu3DDrawOp::SetMvpMatrix { matrix: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]},
            Gpu3DDrawOp::SetDepthTest { enabled: false },
            // 1. Textured Draw (Red)
            Gpu3DDrawOp::BindTexture { texture: Some(tex) },
            Gpu3DDrawOp::DrawArrays { vertices: vec![vl0, vl1, vl2], topology: GpuPrimitiveTopology::TriangleList },
            // 2. Unbind Texture (Green vertex color)
            Gpu3DDrawOp::BindTexture { texture: None },
            Gpu3DDrawOp::DrawArrays { vertices: vec![vr0, vr1, vr2], topology: GpuPrimitiveTopology::TriangleList },
        ];

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ounce-3d-tex-unbind-encoder"),
        });
        renderer.render_ops(&device, &queue, &mut encoder, &ops);

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ounce-3d-tex-unbind-readback"),
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &renderer.offscreen_color_texture,
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
        slice.map_async(wgpu::MapMode::Read, |res| res.expect("readback map failed"));
        device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        let pixels = slice.get_mapped_range().expect("readback not mapped").to_vec();

        // Left triangle must be RED
        let left_idx = (32 * w + 16) as usize * 4;
        let r_l = pixels[left_idx];
        let g_l = pixels[left_idx + 1];
        assert!(r_l > 200 && g_l < 50, "Left triangle must be RED, got rgba=({}, {}, {}, {})", r_l, g_l, pixels[left_idx + 2], pixels[left_idx + 3]);

        // Right triangle must be GREEN (untextured vertex color)
        let right_idx = (32 * w + 48) as usize * 4;
        let r_r = pixels[right_idx];
        let g_r = pixels[right_idx + 1];
        assert!(g_r > 200 && r_r < 50, "Right triangle must be GREEN, got rgba=({}, {}, {}, {})", r_r, g_r, pixels[right_idx + 2], pixels[right_idx + 3]);
    }
}






