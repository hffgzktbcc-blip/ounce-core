//! Guest SASS execution through wgpu, with synchronous readback into guest memory.
//! This is an opt-in, bounded subset, not NVN or a native Tegra driver.
use super::*;
use std::sync::{Mutex, OnceLock};
use wgpu::util::DeviceExt;

struct Device {
    device: wgpu::Device,
    queue: wgpu::Queue,
    physical: bool,
}
static DEVICE: OnceLock<Result<Mutex<Device>, String>> = OnceLock::new();

fn device() -> Result<&'static Mutex<Device>, String> {
    DEVICE
        .get_or_init(|| {
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
            let adapter = pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(
                &instance, None,
            ))
            .map_err(|e| format!("no host adapter: {e}"))?;
            let info = adapter.get_info();
            let physical = matches!(
                info.device_type,
                wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu
            );
            eprintln!(
                "[guest-gpu] adapter={:?} backend={:?} type={:?} physical={physical}",
                info.name, info.backend, info.device_type
            );
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("ounce-guest-sass"),
                    ..Default::default()
                }))
                .map_err(|e| format!("open host device: {e}"))?;
            Ok(Mutex::new(Device {
                device,
                queue,
                physical,
            }))
        })
        .as_ref()
        .map_err(Clone::clone)
}

impl Ampere3d {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_host(
        &mut self,
        rec: &DrawRecord,
        ids: &[u32],
        tris: &[[usize; 3]],
        vs: &LoadedShader,
        ps: &LoadedShader,
        vs_cb: &Cbufs,
        ps_cb: &Cbufs,
        mem: &mut dyn GpuMemoryWriter,
    ) -> Result<(), String> {
        // Validate all state before submission; never use CPU shading as an implicit fallback.
        let targets = selected_targets(&self.regs);
        let [Some(ct)] = targets.as_slice() else {
            return Err("host draws require one enabled color target".into());
        };
        if ct.layer != 0 || ct.third_dimension > 1 || ct.format != ColorFormat::A8B8G8R8 {
            return Err("host draws require a single-layer RGBA8 target".into());
        }
        if self.reg_at(m::SET_DEPTH_TEST) & 1 != 0
            || self.reg_at(m::SET_DEPTH_WRITE) & 1 != 0
            || Stencil::from_regs(&self.regs)?.is_some()
            || depth::depth_bounds(&self.regs).is_some()
            || Blend::for_target(&self.regs, ct.index)?.is_some()
            || self.reg_at(m::FILL_RECTANGLE) == m::FILL_RECTANGLE_ENABLE
        {
            return Err("host depth/stencil/blend/fill-rectangle state is unsupported".into());
        }
        if rec.instance != 0
            || rec.base_instance != 0
            || rec.vertex_streams.iter().any(|s| s.instanced)
        {
            return Err("host instancing is unsupported".into());
        }
        if rec.shaders.iter().any(|s| {
            !matches!(
                s.kind,
                super::super::engine::ShaderType::Vertex
                    | super::super::engine::ShaderType::VertexCullBeforeFetch
                    | super::super::engine::ShaderType::Pixel
            )
        }) {
            return Err("host geometry/tessellation stages are unsupported".into());
        }
        let view = color_view(ct).ok_or("host target layout is unsupported")?;
        if view.width == 0
            || view.height == 0
            || view.width > 4096
            || view.height > 4096
            || view.size() > 128 * 1024 * 1024
        {
            return Err("host target exceeds bounded dimensions/storage".into());
        }
        let sizes = |cbufs: &Cbufs| {
            cbufs
                .0
                .iter()
                .map(|(b, bytes)| (*b, bytes.len() as u32))
                .collect::<Vec<_>>()
        };
        let (mut vsrc, vi) =
            crate::gpu::sass::wgsl::emit_with_cbuf_sizes(&vs.func, &vs.sph, &sizes(vs_cb))
                .map_err(|e| e.to_string())?;
        let (fsrc, fi) =
            crate::gpu::sass::wgsl::emit_with_cbuf_sizes(&ps.func, &ps.sph, &sizes(ps_cb))
                .map_err(|e| e.to_string())?;
        if !vi.writes_position
            || fi.color_targets != [0]
            || !vi.textures.is_empty()
            || !fi.textures.is_empty()
        {
            return Err("host shaders require position and color 0, without textures".into());
        }
        for varying in &fi.varyings {
            if !vi.varyings.iter().any(|v| v.location == varying.location) {
                return Err("fragment input has no vertex output".into());
            }
            let interpolation = match varying.interpolation {
                crate::gpu::sass::Interpolation::Perspective => "perspective, center",
                crate::gpu::sass::Interpolation::Linear => "linear, center",
                crate::gpu::sass::Interpolation::Flat => "flat",
            };
            let loc = varying.location;
            vsrc = vsrc.replace(
                &format!("@location({loc}) o{loc}:"),
                &format!("@location({loc}) @interpolate({interpolation}) o{loc}:"),
            );
        }
        // Indexed host draws preserve original guest vertex IDs. Sparse IDs and enormous
        // streams are rejected instead of allocating memory proportional to an untrusted ID.
        let max_id = ids.iter().copied().max().unwrap_or(0);
        if max_id > 1_048_575 || ids.len() > 1_048_576 || tris.len() > 1_048_576 {
            return Err("host draw exceeds vertex/index work limit".into());
        }
        if rec
            .vertex_streams
            .iter()
            .map(|s| s.size.min(1 << 24))
            .sum::<u64>()
            > 64 * 1024 * 1024
        {
            return Err("host vertex stream cache exceeds 64 MiB".into());
        }
        let stride = (vi.vertex_inputs.len() * 16).max(4);
        let size = (max_id as usize + 1)
            .checked_mul(stride)
            .ok_or("vertex size overflow")?;
        if size > 64 * 1024 * 1024 {
            return Err("host vertex staging exceeds 64 MiB".into());
        }
        let mut vertices = vec![0u8; size];
        let mut cache = Vec::new();
        for &id in ids {
            let attrs = fetch_attributes(rec, id, mem, &mut cache)?;
            for (i, &loc) in vi.vertex_inputs.iter().enumerate() {
                let words = attrs
                    .get(loc as usize)
                    .ok_or("host attribute location out of range")?;
                for (c, word) in words.iter().enumerate() {
                    let at = id as usize * stride + i * 16 + c * 4;
                    vertices[at..at + 4].copy_from_slice(&word.to_le_bytes());
                }
            }
        }
        let indices: Vec<u8> = tris
            .iter()
            .flat_map(|t| t.iter().flat_map(|&i| ids[i].to_le_bytes()))
            .collect();
        if indices.is_empty() {
            return Ok(());
        }
        let mut staged = Staged::load(view, mem).ok_or("host render target is not readable")?;
        let mut rgba = vec![0; (view.width * view.height * 4) as usize];
        for y in 0..view.height {
            for x in 0..view.width {
                let at = ((y * view.width + x) * 4) as usize;
                rgba[at..at + 4].copy_from_slice(staged.texel(x, y));
            }
        }
        // Use a full-target host viewport. Convert the guest viewport transform in homogeneous
        // coordinates, including its Y direction, rather than replaying CPU-shaded vertices.
        if rec
            .viewport0
            .scale
            .iter()
            .chain(rec.viewport0.offset.iter())
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite guest viewport".into());
        }
        let [sx, sy, _] = rec.viewport0.scale;
        let [ox, oy, _] = rec.viewport0.offset;
        let w = view.width as f32;
        let h = view.height as f32;
        let z = match ZClip::from_regs(&self.regs) {
            ZClip::Off => "p.w * 0.5",
            ZClip::Zero => "p.z",
            ZClip::MinusOne => "(p.z + p.w) * 0.5",
        };
        let original = "    return out;";
        let transform = format!(
            "    let p = out.position;\n    out.position = vec4<f32>(p.x * {ax:?} + p.w * {bx:?}, p.y * {ay:?} + p.w * {by:?}, {z}, p.w);\n    return out;",
            ax = 2.0 * sx / w,
            bx = 2.0 * ox / w - 1.0,
            ay = -2.0 * sy / h,
            by = 1.0 - 2.0 * oy / h
        );
        if vsrc.matches(original).count() != 1 {
            return Err("unexpected vertex entry-point structure".into());
        }
        vsrc = vsrc.replace(original, &transform);
        let mut bounds = Rect::new(view.width, view.height);
        let [x, y, w, h] = rec.viewport0.clip;
        if w > 0 && h > 0 {
            bounds = bounds.clip(x, y, x.saturating_add(w), y.saturating_add(h));
        }
        if rec.scissor0.enabled {
            let s = rec.scissor0;
            bounds = bounds.clip(s.xmin, s.ymin, s.xmax, s.ymax);
        }
        let sh = self.reg_at(m::SET_SURFACE_CLIP_HORIZONTAL);
        let sv = self.reg_at(m::SET_SURFACE_CLIP_VERTICAL);
        if sh >> 16 > 0 && sv >> 16 > 0 {
            let (x, y) = (sh & 0xffff, sv & 0xffff);
            bounds = bounds.clip(x, y, x + (sh >> 16), y + (sv >> 16));
        }
        if bounds.x0 >= bounds.x1 || bounds.y0 >= bounds.y1 {
            return Ok(());
        }
        let locked = device()?.lock().map_err(|_| "host GPU mutex poisoned")?;
        if std::env::var("OUNCE_GPU_REQUIRE_HARDWARE").is_ok_and(|s| s == "1") && !locked.physical {
            return Err(
                "physical GPU required; selected adapter is software or unclassified".into(),
            );
        }
        let device = &locked.device;
        let queue = &locked.queue;
        let facing = Facing::from_regs(&self.regs);
        let cull = match (facing.culled(true), facing.culled(false)) {
            (true, true) => return Ok(()),
            (true, false) => Some(wgpu::Face::Front),
            (false, true) => Some(wgpu::Face::Back),
            _ => None,
        };
        // Facing tests use the same framebuffer-space signed area as the CPU path.
        let front_face = if facing.is_front(-1.0) {
            wgpu::FrontFace::Ccw
        } else {
            wgpu::FrontFace::Cw
        };
        let attributes: Vec<_> = vi
            .vertex_inputs
            .iter()
            .enumerate()
            .map(|(i, &loc)| wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (i * 16) as u64,
                shader_location: loc,
            })
            .collect();
        let layouts = [Some(wgpu::VertexBufferLayout {
            array_stride: stride as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        let errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = (|| -> Result<Vec<u8>, String> {
            let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("guest-vertex-sass"),
                source: wgpu::ShaderSource::Wgsl(vsrc.into()),
            });
            let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("guest-fragment-sass"),
                source: wgpu::ShaderSource::Wgsl(fsrc.into()),
            });
            let mask = write_mask(&self.regs, ct.index).iter().enumerate().fold(
                wgpu::ColorWrites::empty(),
                |m, (i, &on)| {
                    if on {
                        m | wgpu::ColorWrites::from_bits_retain(1 << i)
                    } else {
                        m
                    }
                },
            );
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("guest-sass-draw"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &vertex,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers: if attributes.is_empty() { &[] } else { &layouts },
                },
                primitive: wgpu::PrimitiveState {
                    front_face,
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &fragment,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: mask,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
            let mut groups = Vec::new();
            for (group, iface, cbufs) in [(0, &vi, vs_cb), (1, &fi, ps_cb)] {
                let mut buffers = Vec::new();
                for cb in &iface.constant_buffers {
                    let bytes = &cbufs
                        .0
                        .iter()
                        .find(|(bank, _)| *bank as u32 == cb.binding)
                        .ok_or("missing host constant buffer")?
                        .1;
                    let bytes = bytes
                        .get(..cb.size_bytes as usize)
                        .ok_or("short host constant buffer")?;
                    buffers.push((
                        cb.binding,
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("guest-cbuf"),
                            contents: bytes,
                            usage: wgpu::BufferUsages::UNIFORM,
                        }),
                    ));
                }
                // Auto layouts include intermediate empty groups when fragment group 1 is used.
                if group == 0 && vi.constant_buffers.is_empty() && fi.constant_buffers.is_empty() {
                    continue;
                }
                if group == 1 && fi.constant_buffers.is_empty() {
                    continue;
                }
                let entries: Vec<_> = buffers
                    .iter()
                    .map(|(binding, b)| wgpu::BindGroupEntry {
                        binding: *binding,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                groups.push((
                    group,
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("guest-cbufs"),
                        layout: &pipeline.get_bind_group_layout(group),
                        entries: &entries,
                    }),
                ));
            }
            let extent = wgpu::Extent3d {
                width: view.width,
                height: view.height,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("guest-color-target"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(view.width * 4),
                    rows_per_image: Some(view.height),
                },
                extent,
            );
            let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("guest-attributes"),
                contents: &vertices,
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("guest-indices"),
                contents: &indices,
                usage: wgpu::BufferUsages::INDEX,
            });
            let row = (view.width * 4).div_ceil(256) * 256;
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("guest-readback"),
                size: row as u64 * view.height as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            let target = texture.create_view(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("guest-draw"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&pipeline);
                for (index, group) in &groups {
                    pass.set_bind_group(*index, group, &[]);
                }
                if !attributes.is_empty() {
                    pass.set_vertex_buffer(0, vb.slice(..));
                }
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_scissor_rect(
                    bounds.x0,
                    bounds.y0,
                    bounds.x1 - bounds.x0,
                    bounds.y1 - bounds.y0,
                );
                pass.draw_indexed(0..(indices.len() / 4) as u32, 0, 0..1);
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row),
                        rows_per_image: Some(view.height),
                    },
                },
                extent,
            );
            queue.submit(Some(encoder.finish()));
            let (tx, rx) = std::sync::mpsc::channel();
            readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .map_err(|e| e.to_string())?;
            rx.recv_timeout(std::time::Duration::from_secs(1))
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let mapped = readback
                .slice(..)
                .get_mapped_range()
                .map_err(|e| e.to_string())?;
            let mut pixels = Vec::with_capacity(rgba.len());
            for y in 0..view.height as usize {
                pixels.extend_from_slice(
                    &mapped[y * row as usize..y * row as usize + view.width as usize * 4],
                );
            }
            Ok(pixels)
        })();
        let validation = pollster::block_on(errors.pop());
        if let Some(error) = validation {
            return Err(format!("host validation: {error}"));
        }
        let pixels = result?;
        for y in 0..view.height {
            for x in 0..view.width {
                let at = ((y * view.width + x) * 4) as usize;
                staged.texel(x, y).copy_from_slice(&pixels[at..at + 4]);
            }
        }
        staged.store(mem);
        let mut stored = vec![0; staged.data.len()];
        if !mem.read(view.gpu_va, &mut stored) || stored != staged.data {
            return Err("host readback did not reach the guest render target".into());
        }
        self.stats.host_draws += 1;
        self.stats.hardware_draws += u32::from(locked.physical);
        self.stats.host_readback_bytes += pixels.len() as u64;
        self.stats.draws += 1;
        eprintln!(
            "[guest-gpu] completed class={:#x} host_draws={} hardware_draws={} readback_bytes={}",
            rec.class,
            self.stats.host_draws,
            self.stats.hardware_draws,
            self.stats.host_readback_bytes
        );
        Ok(())
    }
}
