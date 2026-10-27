//! AMPERE_B / MAXWELL_B 3D method state and the records a draw, clear or semaphore release produces.
//!
//! The engine keeps a shadow of every class register (the last value written to each method), and
//! turns the trigger methods (`CLEAR_SURFACE`, `BEGIN`/`DRAW_VERTEX_ARRAY`/`END`, the
//! `DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_*` one-shots, `SET_REPORT_SEMAPHORE_D`) into records that
//! snapshot the state they used. No rasterisation happens here.

use super::methods as m;
use super::pushbuf::{self, MethodCall, PushbufError};
use super::surface::{self, addr40, ColorTarget, DepthTarget};

/// `BEGIN.OP` / `DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_*.TOPOLOGY` values (same encoding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    Points,
    Lines,
    LineLoop,
    LineStrip,
    Triangles,
    TriangleStrip,
    TriangleFan,
    Quads,
    QuadStrip,
    Polygon,
    LineListAdjacency,
    LineStripAdjacency,
    TriangleListAdjacency,
    TriangleStripAdjacency,
    Patch,
    Other(u32),
}

impl Topology {
    pub fn from_raw(v: u32) -> Self {
        use Topology::*;
        [Points, Lines, LineLoop, LineStrip, Triangles, TriangleStrip, TriangleFan, Quads, QuadStrip, Polygon,
         LineListAdjacency, LineStripAdjacency, TriangleListAdjacency, TriangleStripAdjacency, Patch]
            .get(v as usize)
            .copied()
            .unwrap_or(Other(v))
    }
}

/// An enabled vertex stream (`SET_VERTEX_STREAM_A_*`, `SET_VERTEX_STREAM_SIZE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexStream {
    pub index: u8,
    pub gpu_va: u64,
    pub stride: u32,
    pub size: u64,
    pub frequency: u32,
}

/// An active vertex attribute (`SET_VERTEX_ATTRIBUTE_A(i)`), fields kept as the class encodes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexAttribute {
    pub index: u8,
    pub stream: u8,
    pub offset: u32,
    /// `COMPONENT_BIT_WIDTHS`, e.g. `0x02` = R32_G32_B32, `0x01` = R32_G32_B32_A32.
    pub component_bit_widths: u32,
    /// `NUMERICAL_TYPE`, e.g. `7` = NUM_FLOAT, `2` = NUM_UNORM.
    pub numerical_type: u32,
    pub swap_r_and_b: bool,
}

/// `SET_PIPELINE_SHADER.TYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    VertexCullBeforeFetch,
    Vertex,
    TessellationInit,
    Tessellation,
    Geometry,
    Pixel,
    Other(u32),
}

impl ShaderType {
    fn from_raw(v: u32) -> Self {
        match v {
            0 => Self::VertexCullBeforeFetch,
            1 => Self::Vertex,
            2 => Self::TessellationInit,
            3 => Self::Tessellation,
            4 => Self::Geometry,
            5 => Self::Pixel,
            other => Self::Other(other),
        }
    }
}

/// An enabled shader pipeline slot. `program_va` is where the shader code starts; decoding it is
/// the shader thread's job (`src/gpu/shader/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderStage {
    pub pipeline: u8,
    pub kind: ShaderType,
    pub program_va: u64,
    pub register_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub scale: [f32; 3],
    pub offset: [f32; 3],
    /// `SET_VIEWPORT_CLIP_*`: x0, y0, width, height.
    pub clip: [u32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scissor {
    pub enabled: bool,
    pub xmin: u32,
    pub xmax: u32,
    pub ymin: u32,
    pub ymax: u32,
}

/// One draw: the vertex range plus the state it ran with.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawRecord {
    /// The 3D class the draw was issued on (`AMPERE_B` or `MAXWELL_B`).
    pub class: u32,
    pub topology: Topology,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub instance: u32,
    pub color_targets: Vec<ColorTarget>,
    pub depth_target: Option<DepthTarget>,
    pub vertex_streams: Vec<VertexStream>,
    pub vertex_attributes: Vec<VertexAttribute>,
    pub shaders: Vec<ShaderStage>,
    pub viewport0: Viewport,
    pub scissor0: Scissor,
}

/// One `CLEAR_SURFACE`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClearRecord {
    /// Raw `SET_COLOR_CLEAR_VALUE` words (floats for float/unorm targets).
    pub color: [u32; 4],
    pub depth: f32,
    pub stencil: u8,
    /// R, G, B, A write enables.
    pub color_mask: [bool; 4],
    pub clear_depth: bool,
    pub clear_stencil: bool,
    /// The color target selected by `MRT_SELECT`, if any channel is enabled.
    pub color_target: Option<ColorTarget>,
    pub depth_target: Option<DepthTarget>,
    pub layer: u32,
}

impl ClearRecord {
    pub fn color_f32(&self) -> [f32; 4] {
        self.color.map(f32::from_bits)
    }
}

/// `SET_REPORT_SEMAPHORE_D.OPERATION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemaphoreOp {
    Release,
    Acquire,
    ReportOnly,
    Trap,
}

/// One `SET_REPORT_SEMAPHORE_D` trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemaphoreRecord {
    pub op: SemaphoreOp,
    pub gpu_va: u64,
    pub payload: u32,
    /// `STRUCTURE_SIZE`: true = ONE_WORD (payload only), false = FOUR_WORDS (payload, 0, 64-bit
    /// timestamp).
    pub one_word: bool,
    pub awaken: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Clear(ClearRecord),
    Draw(DrawRecord),
    Semaphore(SemaphoreRecord),
}

/// Things the state model noticed but did not act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diagnostic {
    /// A method on a subchannel whose bound class is neither AMPERE_B nor MAXWELL_B (or nothing is
    /// bound).
    ForeignSubchannel { subchannel: u8, class: Option<u32>, method: u32 },
    /// `DRAW_VERTEX_ARRAY` or `END` outside `BEGIN`/`END`.
    DrawOutsideBegin { method: u32 },
    /// `BEGIN` while a previous `BEGIN` was still open.
    NestedBegin,
}

/// Where semaphore releases land. The routing hook in `nvgpu.rs` maps GPU VAs to guest memory.
pub trait GpuMemoryWriter {
    fn write(&mut self, gpu_va: u64, bytes: &[u8]);
}

#[derive(Debug, Clone)]
struct OpenDraw {
    topology: Topology,
    instance: u32,
}

/// 3D engine state for one channel; decodes AMPERE_B and MAXWELL_B through one method table.
#[derive(Debug, Clone)]
pub struct Ampere3d {
    regs: Vec<u32>,
    bound: [Option<u32>; 8],
    open: Option<OpenDraw>,
    instance: u32,
    events: Vec<Event>,
    diagnostics: Vec<Diagnostic>,
    /// Monotonic counter used for the FOUR_WORDS timestamp, so tests stay deterministic.
    ticks: u64,
}

impl Default for Ampere3d {
    fn default() -> Self {
        Self::new()
    }
}

impl Ampere3d {
    pub fn new() -> Self {
        Self {
            regs: vec![0; (m::METHOD_SPACE / 4) as usize],
            bound: [None; 8],
            open: None,
            instance: 0,
            events: Vec::new(),
            diagnostics: Vec::new(),
            ticks: 0,
        }
    }

    /// Class bound to `subchannel` by `SET_OBJECT`.
    pub fn bound_class(&self, subchannel: u8) -> Option<u32> {
        self.bound[(subchannel & 7) as usize]
    }

    /// Last value written to `method` (byte offset).
    pub fn reg(&self, method: u32) -> u32 {
        self.regs[((method % m::METHOD_SPACE) / 4) as usize]
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Decodes and runs one pushbuffer segment.
    pub fn submit(&mut self, words: &[u32], mem: &mut dyn GpuMemoryWriter) -> Result<(), PushbufError> {
        for call in pushbuf::decode(words)? {
            self.execute(call, mem);
        }
        Ok(())
    }

    pub fn execute(&mut self, call: MethodCall, mem: &mut dyn GpuMemoryWriter) {
        let sc = (call.subchannel & 7) as usize;
        if call.method == m::SET_OBJECT {
            self.bound[sc] = Some(call.data & 0xffff);
            return;
        }
        if call.method < m::FIRST_CLASS_METHOD {
            return; // Host methods: the channel model handles them.
        }
        let class = match self.bound[sc] {
            Some(c @ (m::AMPERE_B | m::MAXWELL_B)) => c,
            _ => {
                self.diagnostics.push(Diagnostic::ForeignSubchannel {
                    subchannel: call.subchannel,
                    class: self.bound[sc],
                    method: call.method,
                });
                return;
            }
        };
        let method = call.method % m::METHOD_SPACE;
        self.regs[(method / 4) as usize] = call.data;

        match method {
            m::CLEAR_SURFACE => self.clear(call.data, class),
            m::BEGIN => {
                if self.open.is_some() {
                    self.diagnostics.push(Diagnostic::NestedBegin);
                }
                // INSTANCE_ID 27:26: FIRST resets, SUBSEQUENT advances, UNCHANGED keeps.
                match (call.data >> 26) & 3 {
                    0 => self.instance = 0,
                    1 => self.instance += 1,
                    _ => {}
                }
                self.open = Some(OpenDraw { topology: Topology::from_raw(call.data & 0xffff), instance: self.instance });
            }
            m::DRAW_VERTEX_ARRAY => match self.open.clone() {
                Some(open) => {
                    let first = self.reg(m::SET_VERTEX_ARRAY_START);
                    self.draw(class, open.topology, first, call.data, open.instance);
                }
                None => self.diagnostics.push(Diagnostic::DrawOutsideBegin { method }),
            },
            m::END => {
                if self.open.take().is_none() {
                    self.diagnostics.push(Diagnostic::DrawOutsideBegin { method });
                }
            }
            m::DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST | m::DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT => {
                if method == m::DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST {
                    self.instance = 0;
                } else {
                    self.instance += 1;
                }
                let topology = Topology::from_raw(call.data >> 28);
                self.draw(class, topology, call.data & 0xffff, (call.data >> 16) & 0xfff, self.instance);
            }
            m::SET_REPORT_SEMAPHORE_D => self.semaphore(call.data, mem),
            _ => {}
        }
    }

    fn color_targets(&self) -> Vec<ColorTarget> {
        let select = self.reg(m::SET_CT_SELECT);
        let count = (select & 0xf).min(m::COLOR_TARGET_COUNT as u32);
        (0..count)
            .map(|n| ((select >> (4 + 3 * n)) & 7) as usize)
            .map(|slot| surface::color_target(&self.regs, slot))
            .filter(|ct| ct.format != surface::ColorFormat::Disabled)
            .collect()
    }

    fn depth_target(&self, class: u32) -> Option<DepthTarget> {
        (self.reg(m::SET_ZT_SELECT) & 1 == 1).then(|| surface::depth_target(&self.regs, class == m::MAXWELL_B))
    }

    fn clear(&mut self, data: u32, class: u32) {
        let color_mask = [2, 3, 4, 5].map(|bit| (data >> bit) & 1 == 1);
        let clear_depth = data & 1 == 1;
        let clear_stencil = (data >> 1) & 1 == 1;
        let mrt = ((data >> 6) & 0xf) as usize;
        let color_target = (color_mask.iter().any(|&b| b) && mrt < m::COLOR_TARGET_COUNT)
            .then(|| surface::color_target(&self.regs, mrt));
        let depth_target = if clear_depth || clear_stencil { self.depth_target(class) } else { None };
        let record = ClearRecord {
            color: [0, 1, 2, 3].map(|i| self.reg(m::SET_COLOR_CLEAR_VALUE + 4 * i)),
            depth: f32::from_bits(self.reg(m::SET_Z_CLEAR_VALUE)),
            stencil: self.reg(m::SET_STENCIL_CLEAR_VALUE) as u8,
            color_mask,
            clear_depth,
            clear_stencil,
            color_target,
            depth_target,
            layer: (data >> 10) & 0xffff,
        };
        self.events.push(Event::Clear(record));
    }

    fn draw(&mut self, class: u32, topology: Topology, first_vertex: u32, vertex_count: u32, instance: u32) {
        let vertex_streams = (0..m::VERTEX_STREAM_COUNT as u32)
            .filter_map(|j| {
                let fmt = self.reg(m::SET_VERTEX_STREAM_A_FORMAT + j * m::VERTEX_STREAM_A_STRIDE);
                let gpu_va = addr40(
                    self.reg(m::SET_VERTEX_STREAM_A_LOCATION_A + j * m::VERTEX_STREAM_A_STRIDE),
                    self.reg(m::SET_VERTEX_STREAM_A_LOCATION_B + j * m::VERTEX_STREAM_A_STRIDE),
                );
                let size = if class == m::MAXWELL_B {
                    // Read as the last valid byte address (`address + size - 1`). The header gives
                    // no unit; this is the usual nouveau convention, unverified here.
                    let limit = addr40(
                        self.reg(m::B197_SET_VERTEX_STREAM_LIMIT_A_A + j * m::B197_VERTEX_STREAM_LIMIT_STRIDE),
                        self.reg(m::B197_SET_VERTEX_STREAM_LIMIT_A_B + j * m::B197_VERTEX_STREAM_LIMIT_STRIDE),
                    );
                    if limit >= gpu_va { limit - gpu_va + 1 } else { 0 }
                } else {
                    addr40(
                        self.reg(m::SET_VERTEX_STREAM_SIZE_A + j * m::VERTEX_STREAM_SIZE_STRIDE),
                        self.reg(m::SET_VERTEX_STREAM_SIZE_A + 4 + j * m::VERTEX_STREAM_SIZE_STRIDE),
                    )
                };
                ((fmt >> 12) & 1 == 1).then(|| VertexStream {
                    index: j as u8,
                    gpu_va,
                    stride: fmt & 0xfff,
                    size,
                    frequency: self.reg(m::SET_VERTEX_STREAM_A_FREQUENCY + j * m::VERTEX_STREAM_A_STRIDE),
                })
            })
            .collect();
        let vertex_attributes = (0..m::VERTEX_ATTRIBUTE_COUNT as u32)
            .filter_map(|i| {
                let a = self.reg(m::SET_VERTEX_ATTRIBUTE_A + 4 * i);
                // SOURCE 6:6 = INACTIVE, and an all-zero word is a never-programmed slot.
                (a != 0 && (a >> 6) & 1 == 0).then(|| VertexAttribute {
                    index: i as u8,
                    stream: (a & 0x1f) as u8,
                    offset: (a >> 7) & 0x3fff,
                    component_bit_widths: (a >> 21) & 0x3f,
                    numerical_type: (a >> 27) & 7,
                    swap_r_and_b: a >> 31 == 1,
                })
            })
            .collect();
        let shaders = (0..m::PIPELINE_COUNT as u32)
            .filter_map(|j| {
                let off = j * m::PIPELINE_STRIDE;
                let s = self.reg(m::SET_PIPELINE_SHADER + off);
                (s & 1 == 1).then(|| ShaderStage {
                    pipeline: j as u8,
                    kind: ShaderType::from_raw((s >> 4) & 0xf),
                    program_va: if class == m::MAXWELL_B {
                        let region = addr40(self.reg(m::B197_SET_PROGRAM_REGION_A), self.reg(m::B197_SET_PROGRAM_REGION_B));
                        region + self.reg(m::B197_SET_PIPELINE_PROGRAM + off) as u64
                    } else {
                        addr40(
                            self.reg(m::SET_PIPELINE_PROGRAM_ADDRESS_A + off),
                            self.reg(m::SET_PIPELINE_PROGRAM_ADDRESS_B + off),
                        )
                    },
                    register_count: self.reg(m::SET_PIPELINE_REGISTER_COUNT + off)
                        & if class == m::MAXWELL_B { 0xff } else { 0x1ff },
                })
            })
            .collect();
        let f = |method: u32| f32::from_bits(self.reg(method));
        let vp = m::SET_VIEWPORT_SCALE_X;
        let clip_h = self.reg(m::SET_VIEWPORT_CLIP_HORIZONTAL);
        let clip_v = self.reg(m::SET_VIEWPORT_CLIP_VERTICAL);
        let sc_h = self.reg(m::SET_SCISSOR_HORIZONTAL);
        let sc_v = self.reg(m::SET_SCISSOR_VERTICAL);
        let record = DrawRecord {
            class,
            topology,
            first_vertex,
            vertex_count,
            instance,
            color_targets: self.color_targets(),
            depth_target: self.depth_target(class),
            vertex_streams,
            vertex_attributes,
            shaders,
            viewport0: Viewport {
                scale: [f(vp), f(vp + 4), f(vp + 8)],
                offset: [f(vp + 12), f(vp + 16), f(vp + 20)],
                clip: [clip_h & 0xffff, clip_v & 0xffff, clip_h >> 16, clip_v >> 16],
            },
            scissor0: Scissor {
                enabled: self.reg(m::SET_SCISSOR_ENABLE) & 1 == 1,
                xmin: sc_h & 0xffff,
                xmax: sc_h >> 16,
                ymin: sc_v & 0xffff,
                ymax: sc_v >> 16,
            },
        };
        self.events.push(Event::Draw(record));
    }

    fn semaphore(&mut self, d: u32, mem: &mut dyn GpuMemoryWriter) {
        let op = match d & 3 {
            0 => SemaphoreOp::Release,
            1 => SemaphoreOp::Acquire,
            2 => SemaphoreOp::ReportOnly,
            _ => SemaphoreOp::Trap,
        };
        let record = SemaphoreRecord {
            op,
            gpu_va: addr40(self.reg(m::SET_REPORT_SEMAPHORE_A), self.reg(m::SET_REPORT_SEMAPHORE_B)),
            payload: self.reg(m::SET_REPORT_SEMAPHORE_C),
            one_word: (d >> 28) & 1 == 1,
            awaken: (d >> 20) & 1 == 1,
        };
        if op == SemaphoreOp::Release {
            // Work completes synchronously here, so the release can land at once.
            if record.one_word {
                mem.write(record.gpu_va, &record.payload.to_le_bytes());
            } else {
                self.ticks += 1;
                let mut bytes = [0u8; 16];
                bytes[0..4].copy_from_slice(&record.payload.to_le_bytes());
                bytes[8..16].copy_from_slice(&self.ticks.to_le_bytes());
                mem.write(record.gpu_va, &bytes);
            }
        }
        self.events.push(Event::Semaphore(record));
    }
}
