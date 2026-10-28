#![allow(dead_code)]

//! `nvdrv` device model for libnx-shaped requests: device files opened by path, and ioctls whose
//! argument block arrives in an X (or A) buffer and goes back through a C (or B) buffer.
//! Ioctl numbers and argument layouts follow the public Linux Tegra `nvmap` UAPI that the Switch
//! driver keeps (`_IOWR('\x01', nr, struct)`).

use std::collections::{BTreeMap, HashMap};

use crate::gpu::ampere::{self, Ampere3d, Diagnostic, GpuMemoryWriter};
use crate::mmu::Memory;

/// nvdrv status codes (libnx `NvError`).
pub const NV_OK: u32 = 0;
pub const NV_NOT_IMPLEMENTED: u32 = 1;
pub const NV_BAD_PARAMETER: u32 = 4;
pub const NV_TIMEOUT: u32 = 5;

/// Syncpoint used by the first GPFIFO channel; later channels take the next ones.
pub const FIRST_CHANNEL_SYNCPT: u32 = 18;
pub const NUM_SYNCPTS: usize = 192;
/// Start of the GPU virtual address range handed out by nvhost-as-gpu.
pub const GPU_VA_BASE: u64 = 0x4_0000_0000;

pub const NVMAP_IOC_CREATE: u32 = 0xC008_0101;
pub const NVMAP_IOC_FROM_ID: u32 = 0xC008_0103;
pub const NVMAP_IOC_ALLOC: u32 = 0xC020_0104;
pub const NVMAP_IOC_FREE: u32 = 0xC018_0105;
pub const NVMAP_IOC_PARAM: u32 = 0xC00C_0109;
pub const NVMAP_IOC_GET_ID: u32 = 0xC008_010E;

/// Bytes of argument data an ioctl carries (bits 29:16 of the request).
pub fn ioctl_size(request: u32) -> usize {
    ((request >> 16) & 0x3FFF) as usize
}

/// The ioctl reads its argument block from the caller (`_IOC_WRITE`).
pub fn ioctl_has_input(request: u32) -> bool {
    request & (1 << 30) != 0
}

/// The ioctl writes its argument block back (`_IOC_READ`).
pub fn ioctl_has_output(request: u32) -> bool {
    request & (1 << 31) != 0
}

/// An nvmap buffer object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NvMapObject {
    pub id: u32,
    pub size: u32,
    pub align: u32,
    pub kind: u8,
    pub flags: u32,
    pub heap_mask: u32,
    /// Guest address backing the object, 0 until `NVMAP_IOC_ALLOC`.
    pub addr: u64,
}

#[derive(Debug, Default)]
pub struct NvDrv {
    /// Open device files: fd -> device path.
    pub fds: HashMap<u32, String>,
    next_fd: u32,
    /// nvmap objects by handle.
    pub nvmap: HashMap<u32, NvMapObject>,
    next_handle: u32,
    next_id: u32,
    /// Ioctls answered with `NV_NOT_IMPLEMENTED`, for diagnostics.
    pub unhandled: Vec<(String, u32)>,
    /// Syncpoint values. Work completes when it is submitted, so values only move forward on submit.
    pub syncpts: Vec<u32>,
    /// GPU address space mappings: GPU VA -> mapping.
    pub gpu_mappings: std::collections::BTreeMap<u64, GpuMapping>,
    next_gpu_va: u64,
    /// GPFIFO channels by fd.
    pub channels: HashMap<u32, GpuChannel>,
    /// Pushbuffer segments submitted since the last `take_submissions`, as (channel fd, guest
    /// address, words).
    submissions: Vec<(u32, u64, usize)>,
    /// 3D engine state per channel fd, for channels that allocated MAXWELL_B or AMPERE_B.
    pub engines: HashMap<u32, Ampere3d>,
    /// Distinct 3D engine diagnostics seen so far (at most [`MAX_GPU_DIAGNOSTICS`]).
    pub gpu_diagnostics: Vec<Diagnostic>,
}

/// Cap on [`NvDrv::gpu_diagnostics`], so a long run cannot grow it without bound.
pub const MAX_GPU_DIAGNOSTICS: usize = 64;

/// Guest memory as one channel's GPU sees it: GPU VAs go through the nvhost-as-gpu mappings.
/// Ranges may span mappings; a byte outside a backed mapping fails the whole access.
pub struct GuestGpuMemory<'a> {
    pub mmu: &'a mut Memory,
    pub mappings: &'a BTreeMap<u64, GpuMapping>,
}

impl GuestGpuMemory<'_> {
    /// Splits `[va, va + len)` into (guest CPU address, offset into the access, length) pieces.
    fn pieces(&self, va: u64, len: usize) -> Option<Vec<(u64, usize, usize)>> {
        let mut out = Vec::new();
        let mut done = 0usize;
        while done < len {
            let cur = va.checked_add(done as u64)?;
            let (&base, m) = self.mappings.range(..=cur).next_back()?;
            let end = base.checked_add(m.size)?;
            if cur >= end || m.cpu_addr == 0 {
                return None;
            }
            let n = ((end - cur) as usize).min(len - done);
            out.push((m.cpu_addr + (cur - base), done, n));
            done += n;
        }
        Some(out)
    }
}

impl GpuMemoryWriter for GuestGpuMemory<'_> {
    fn write(&mut self, gpu_va: u64, bytes: &[u8]) {
        if let Some(pieces) = self.pieces(gpu_va, bytes.len()) {
            for (cpu, off, n) in pieces {
                let _ = self.mmu.write_bytes(cpu, &bytes[off..off + n]);
            }
        }
    }

    fn read(&mut self, gpu_va: u64, buf: &mut [u8]) -> bool {
        let Some(pieces) = self.pieces(gpu_va, buf.len()) else {
            return false;
        };
        pieces
            .into_iter()
            .all(|(cpu, off, n)| self.mmu.read_bytes(cpu, &mut buf[off..off + n]).is_ok())
    }
}

/// A buffer mapped into the GPU address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuMapping {
    pub size: u64,
    /// Guest CPU address of the backing nvmap object (0 for reserved, unbacked space).
    pub cpu_addr: u64,
}

/// A GPFIFO channel opened on `/dev/nvhost-gpu`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GpuChannel {
    pub syncpt: u32,
    pub gpfifo_entries: u32,
    pub classes: Vec<u32>,
    pub submits: u64,
}

/// Extra buffers of `Ioctl2` (extra input) and `Ioctl3` (extra output).
#[derive(Debug, Default)]
pub struct IoctlExtra<'a> {
    pub inbuf: &'a [u8],
    pub outbuf: Option<&'a mut [u8]>,
}

fn rd32(b: &[u8], off: usize) -> u32 {
    b.get(off..off + 4).map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()))
}

fn wr32(b: &mut [u8], off: usize, v: u32) {
    if let Some(s) = b.get_mut(off..off + 4) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}

fn wr64(b: &mut [u8], off: usize, v: u64) {
    if let Some(s) = b.get_mut(off..off + 8) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}

impl NvDrv {
    pub fn new() -> Self {
        Self {
            next_fd: 1,
            next_handle: 1,
            next_id: 0x1001,
            syncpts: vec![0; NUM_SYNCPTS],
            next_gpu_va: GPU_VA_BASE,
            ..Default::default()
        }
    }

    /// Pushbuffer segments submitted through `SUBMIT_GPFIFO` since the last call.
    pub fn take_submissions(&mut self) -> Vec<(u64, usize)> {
        self.take_channel_submissions()
            .into_iter()
            .map(|(_, addr, words)| (addr, words))
            .collect()
    }

    /// As [`NvDrv::take_submissions`], with the channel fd each segment was submitted on.
    pub fn take_channel_submissions(&mut self) -> Vec<(u32, u64, usize)> {
        std::mem::take(&mut self.submissions)
    }

    /// Whether channel `fd` allocated a 3D class this model decodes (MAXWELL_B or AMPERE_B).
    pub fn has_3d_class(&self, fd: u32) -> bool {
        self.channels.get(&fd).is_some_and(|c| {
            c.classes
                .iter()
                .any(|&k| k == ampere::MAXWELL_B || k == ampere::AMPERE_B)
        })
    }

    /// Runs one submitted segment on channel `fd`'s 3D engine, with GPU VAs translated through the
    /// address space mappings. Returns false (nothing done) when the channel has no 3D class, so
    /// the caller can use the older synthetic-method path instead.
    pub fn run_3d(&mut self, mmu: &mut Memory, fd: u32, cpu_addr: u64, words: usize) -> bool {
        if !self.has_3d_class(fd) {
            return false;
        }
        let mut raw = vec![0u8; words * 4];
        if mmu.read_bytes(cpu_addr, &mut raw).is_err() {
            return true;
        }
        let words: Vec<u32> = raw
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let engine = self.engines.entry(fd).or_default();
        let mut mem = GuestGpuMemory {
            mmu,
            mappings: &self.gpu_mappings,
        };
        if engine.submit(&words, &mut mem).is_err() {
            Self::note(
                &mut self.gpu_diagnostics,
                Diagnostic::RenderSkipped {
                    reason: "pushbuffer segment does not decode",
                },
            );
        }
        // Records are for tests and tools; a running guest would grow them without bound.
        engine.take_events();
        for d in engine.take_diagnostics() {
            Self::note(&mut self.gpu_diagnostics, d);
        }
        true
    }

    fn note(list: &mut Vec<Diagnostic>, d: Diagnostic) {
        if list.len() < MAX_GPU_DIAGNOSTICS && !list.contains(&d) {
            list.push(d);
        }
    }

    /// Guest CPU address for a GPU virtual address, if it lies in a backed mapping.
    pub fn gpu_to_cpu(&self, va: u64) -> Option<u64> {
        let (&base, m) = self.gpu_mappings.range(..=va).next_back()?;
        (va < base + m.size && m.cpu_addr != 0).then(|| m.cpu_addr + (va - base))
    }

    fn alloc_gpu_va(&mut self, size: u64, align: u64) -> u64 {
        let align = align.max(0x1000);
        let va = (self.next_gpu_va + align - 1) & !(align - 1);
        self.next_gpu_va = va + size.max(0x1000);
        va
    }

    /// Opens a device file. Every `/dev/nv*` path opens; only `/dev/nvmap` has ioctls modelled.
    pub fn open(&mut self, path: &str) -> (u32, u32) {
        if !path.starts_with("/dev/nv") {
            return (0, NV_BAD_PARAMETER);
        }
        let fd = self.next_fd;
        self.next_fd += 1;
        self.fds.insert(fd, path.to_string());
        (fd, NV_OK)
    }

    pub fn close(&mut self, fd: u32) -> u32 {
        self.channels.remove(&fd);
        if self.fds.remove(&fd).is_some() { NV_OK } else { NV_BAD_PARAMETER }
    }

    /// Runs an ioctl on `fd`. `data` is the argument block, updated in place.
    pub fn ioctl(&mut self, fd: u32, request: u32, data: &mut [u8]) -> u32 {
        self.ioctl_ex(fd, request, data, IoctlExtra::default())
    }

    /// `ioctl` with the extra buffers of `Ioctl2`/`Ioctl3`.
    pub fn ioctl_ex(&mut self, fd: u32, request: u32, data: &mut [u8], extra: IoctlExtra) -> u32 {
        let Some(path) = self.fds.get(&fd).cloned() else {
            return NV_BAD_PARAMETER;
        };
        let status = match path.as_str() {
            "/dev/nvmap" => self.nvmap_ioctl(request, data),
            "/dev/nvhost-ctrl" => self.ctrl_ioctl(request, data),
            "/dev/nvhost-ctrl-gpu" => self.ctrl_gpu_ioctl(request, data, extra),
            "/dev/nvhost-as-gpu" => self.as_gpu_ioctl(request, data),
            "/dev/nvhost-gpu" => self.channel_ioctl(fd, request, data, extra),
            _ => None,
        };
        if let Some(status) = status {
            return status;
        }
        eprintln!("[nvdrv] ioctl 0x{request:08X} on {path} not implemented");
        self.unhandled.push((path, request));
        NV_NOT_IMPLEMENTED
    }

    fn nvmap_ioctl(&mut self, request: u32, data: &mut [u8]) -> Option<u32> {
        Some(match request {
            // { u32 size; u32 handle (out); }
            NVMAP_IOC_CREATE => {
                let size = rd32(data, 0);
                if size == 0 {
                    return Some(NV_BAD_PARAMETER);
                }
                let handle = self.next_handle;
                self.next_handle += 1;
                let id = self.next_id;
                self.next_id += 1;
                self.nvmap.insert(handle, NvMapObject { id, size, ..Default::default() });
                wr32(data, 4, handle);
                NV_OK
            }
            // { u32 id; u32 handle (out); }
            NVMAP_IOC_FROM_ID => {
                let id = rd32(data, 0);
                match self.nvmap.iter().find(|(_, o)| o.id == id) {
                    Some((&handle, _)) => {
                        wr32(data, 4, handle);
                        NV_OK
                    }
                    None => NV_BAD_PARAMETER,
                }
            }
            // { u32 handle; u32 heapmask; u32 flags; u32 align; u8 kind; u8 pad[7]; u64 addr; }
            NVMAP_IOC_ALLOC => {
                let Some(obj) = self.nvmap.get_mut(&rd32(data, 0)) else {
                    return Some(NV_BAD_PARAMETER);
                };
                obj.heap_mask = rd32(data, 4);
                obj.flags = rd32(data, 8);
                obj.align = rd32(data, 12).max(0x1000);
                obj.kind = data.get(16).copied().unwrap_or(0);
                obj.addr = u64::from_le_bytes(data.get(24..32)?.try_into().ok()?);
                NV_OK
            }
            // { u32 handle; u32 pad; u64 address (out); u32 size (out); u32 flags (out); }
            NVMAP_IOC_FREE => match self.nvmap.remove(&rd32(data, 0)) {
                Some(obj) => {
                    wr64(data, 8, obj.addr);
                    wr32(data, 16, obj.size);
                    wr32(data, 20, obj.flags);
                    NV_OK
                }
                None => NV_BAD_PARAMETER,
            },
            // { u32 handle; u32 param; u32 result (out); }: 1 size, 2 alignment, 3 base, 4 heap, 5 kind
            NVMAP_IOC_PARAM => {
                let Some(obj) = self.nvmap.get(&rd32(data, 0)) else {
                    return Some(NV_BAD_PARAMETER);
                };
                let value = match rd32(data, 4) {
                    1 => obj.size,
                    2 => obj.align,
                    3 => obj.addr as u32,
                    4 => obj.heap_mask,
                    5 => obj.kind as u32,
                    _ => return Some(NV_BAD_PARAMETER),
                };
                wr32(data, 8, value);
                NV_OK
            }
            // { u32 id (out); u32 handle; }
            NVMAP_IOC_GET_ID => match self.nvmap.get(&rd32(data, 4)) {
                Some(obj) => {
                    wr32(data, 0, obj.id);
                    NV_OK
                }
                None => NV_BAD_PARAMETER,
            },
            _ => return None,
        })
    }
}

impl NvDrv {
    /// `/dev/nvhost-ctrl` (ioctl type 0x00): syncpoints and their events.
    fn ctrl_ioctl(&mut self, request: u32, data: &mut [u8]) -> Option<u32> {
        let syncpt = |s: &Self, id: u32| s.syncpts.get(id as usize).copied();
        Some(match request & 0xFF {
            // SYNCPT_READ { u32 id; u32 value (out); }
            0x14 => match syncpt(self, rd32(data, 0)) {
                Some(v) => {
                    wr32(data, 4, v);
                    NV_OK
                }
                None => NV_BAD_PARAMETER,
            },
            // SYNCPT_INCR { u32 id; }
            0x15 => match self.syncpts.get_mut(rd32(data, 0) as usize) {
                Some(v) => {
                    *v = v.wrapping_add(1);
                    NV_OK
                }
                None => NV_BAD_PARAMETER,
            },
            // SYNCPT_WAIT { u32 id; u32 thresh; s32 timeout; } / SYNCPT_WAITEX (+ u32 value out)
            0x16 | 0x19 => match syncpt(self, rd32(data, 0)) {
                Some(v) if v >= rd32(data, 4) => {
                    wr32(data, 12, v);
                    NV_OK
                }
                Some(_) => NV_TIMEOUT,
                None => NV_BAD_PARAMETER,
            },
            // EVENT_WAIT / EVENT_WAIT_ASYNC { u32 syncpt_id; u32 thresh; s32 timeout; u32 value (inout); }
            0x1D | 0x1E => match syncpt(self, rd32(data, 0)) {
                Some(v) if v >= rd32(data, 4) => {
                    wr32(data, 12, v);
                    NV_OK
                }
                Some(_) => NV_TIMEOUT,
                None => NV_BAD_PARAMETER,
            },
            // EVENT_SIGNAL / EVENT_REGISTER / EVENT_UNREGISTER { u32 event_id; }: no state to keep.
            0x1C | 0x1F | 0x20 => NV_OK,
            _ => return None,
        })
    }

    /// `/dev/nvhost-ctrl-gpu` (type 'G'): GPU description. Values describe the Switch's GM20B,
    /// as libnx documents them in `nvioctl_gpu_characteristics`.
    fn ctrl_gpu_ioctl(&mut self, request: u32, data: &mut [u8], extra: IoctlExtra) -> Option<u32> {
        if (request >> 8) & 0xFF != 0x47 {
            return None;
        }
        Some(match request & 0xFF {
            // ZCULL_GET_CTX_SIZE { u32 size (out); }
            0x01 => {
                wr32(data, 0, ZCULL_CTX_SIZE);
                NV_OK
            }
            // ZCULL_GET_INFO: ten u32 fields
            0x02 => {
                for (i, v) in [0x20, 0x20, 0x400, 0x800, 0x20, 0x20, 0xC0, 0x20, 0x40, 0x10].iter().enumerate() {
                    wr32(data, i * 4, *v);
                }
                NV_OK
            }
            // ZBC_SET_TABLE (input only) / ZBC_QUERY_TABLE (leave the entry empty)
            0x03 | 0x04 => NV_OK,
            // GET_CHARACTERISTICS { u64 buf_size (inout); u64 buf_addr; characteristics }
            0x05 => {
                let gc = gpu_characteristics();
                wr64(data, 0, gc.len() as u64);
                if let Some(dst) = data.get_mut(0x10..0x10 + gc.len()) {
                    dst.copy_from_slice(&gc);
                }
                if let Some(out) = extra.outbuf {
                    let n = out.len().min(gc.len());
                    out[..n].copy_from_slice(&gc[..n]);
                }
                NV_OK
            }
            // GET_TPC_MASKS { u32 mask_buf_size; u32 pad[3]; u64 mask_buf_addr; }: GPC 0 has TPCs 0 and 1
            0x06 => {
                if let Some(out) = extra.outbuf {
                    if out.len() >= 4 {
                        out[..4].copy_from_slice(&3u32.to_le_bytes());
                    }
                }
                if data.len() >= 0x1C {
                    wr32(data, 0x18, 3);
                }
                NV_OK
            }
            // ZBC_GET_ACTIVE_SLOT_MASK { u32 slot; u32 mask; }
            0x14 => {
                wr32(data, 0, 7);
                wr32(data, 4, 0);
                NV_OK
            }
            // GET_GPU_TIME { u64 timestamp; u64 reserved; }: nanoseconds of submitted work, not wall time
            0x1C => {
                let total: u64 = self.channels.values().map(|c| c.submits).sum();
                wr64(data, 0, total * 1000);
                NV_OK
            }
            _ => return None,
        })
    }

    /// `/dev/nvhost-as-gpu` (type 'A'): the GPU virtual address space.
    fn as_gpu_ioctl(&mut self, request: u32, data: &mut [u8]) -> Option<u32> {
        if (request >> 8) & 0xFF != 0x41 {
            return None;
        }
        let rd64 = |b: &[u8], off: usize| b.get(off..off + 8).map_or(0, |s| u64::from_le_bytes(s.try_into().unwrap()));
        Some(match request & 0xFF {
            // BIND_CHANNEL { u32 channel_fd; } / INITIALIZE_EX: one shared address space here
            0x01 | 0x09 => NV_OK,
            // ALLOC_SPACE { u32 pages; u32 page_size; u32 flags; u32 pad; u64 align_or_offset (inout); }
            0x02 => {
                let size = rd32(data, 0) as u64 * rd32(data, 4) as u64;
                let va = if rd32(data, 8) & 1 != 0 { rd64(data, 16) } else { self.alloc_gpu_va(size, rd64(data, 16)) };
                self.gpu_mappings.insert(va, GpuMapping { size, cpu_addr: 0 });
                wr64(data, 16, va);
                NV_OK
            }
            // FREE_SPACE { u64 offset; u32 pages; u32 page_size; } / UNMAP_BUFFER { u64 offset; }
            0x03 | 0x05 => {
                self.gpu_mappings.remove(&rd64(data, 0));
                NV_OK
            }
            // MAP_BUFFER_EX { u32 flags; u32 kind; u32 nvmap_handle; u32 page_size; u64 buffer_offset;
            //                 u64 mapping_size; u64 offset (inout); }
            0x06 => {
                let flags = rd32(data, 0);
                let Some(obj) = self.nvmap.get(&rd32(data, 8)).cloned() else {
                    return Some(NV_BAD_PARAMETER);
                };
                let buffer_offset = rd64(data, 16);
                let size = match rd64(data, 24) {
                    0 => obj.size as u64 - buffer_offset.min(obj.size as u64),
                    n => n,
                };
                let cpu_addr = obj.addr + buffer_offset;
                let va = if flags & 1 != 0 { rd64(data, 32) } else { self.alloc_gpu_va(size, rd32(data, 12) as u64) };
                self.gpu_mappings.insert(va, GpuMapping { size, cpu_addr });
                wr64(data, 32, va);
                NV_OK
            }
            // GET_VA_REGIONS { u64 unused; u32 buf_size (inout); u32 pad; va_region regions[2]; }
            0x08 => {
                wr32(data, 8, 0x30);
                // Small pages (4 KiB) and big pages (128 KiB), each { u64 offset; u32 page_size; u32 pad; u64 pages; }
                for (i, (offset, page, pages)) in [(GPU_VA_BASE, 0x1000u32, 0x3FFF_Fu64), (0x10_0000_0000, 0x2_0000, 0x7_FFFF)].iter().enumerate() {
                    let at = 16 + i * 24;
                    wr64(data, at, *offset);
                    wr32(data, at + 8, *page);
                    wr64(data, at + 16, *pages);
                }
                NV_OK
            }
            _ => return None,
        })
    }

    /// `/dev/nvhost-gpu` (type 'H'): a GPFIFO channel.
    fn channel_ioctl(&mut self, fd: u32, request: u32, data: &mut [u8], extra: IoctlExtra) -> Option<u32> {
        if (request >> 8) & 0xFF != 0x48 {
            return None;
        }
        let next_syncpt = FIRST_CHANNEL_SYNCPT + self.channels.len() as u32;
        let ch = self.channels.entry(fd).or_insert_with(|| GpuChannel { syncpt: next_syncpt, ..Default::default() });
        let syncpt = ch.syncpt;
        Some(match request & 0xFF {
            // SET_NVMAP_FD, SET_TIMEOUT, ZCULL_BIND, SET_ERROR_NOTIFIER, SET_PRIORITY, SET_USER_DATA,
            // SET_SUBMIT_TIMEOUT: settings with no effect on this model
            0x01 | 0x03 | 0x0B | 0x0C | 0x0D | 0x14 | 0x1B => NV_OK,
            // SUBMIT_GPFIFO { u64 gpfifo; u32 num_entries; u32 flags; fence (inout) { u32 id; u32 value; };
            //                 entries[] } (entries inline, or in the Ioctl2 input buffer)
            0x08 => {
                let num = rd32(data, 8) as usize;
                let inline = data.get(0x18..).unwrap_or(&[]);
                let entries = if extra.inbuf.len() >= num * 8 { extra.inbuf } else { inline };
                let mut segments: Vec<(u32, u64, usize)> = Vec::new();
                for i in 0..num {
                    let Some(e) = entries.get(i * 8..i * 8 + 8) else { break };
                    let desc = u64::from_le_bytes(e.try_into().unwrap());
                    let va = desc & 0xFF_FFFF_FFFC;
                    let words = ((desc >> 42) & 0x1F_FFFF) as usize;
                    if let Some(cpu) = self.gpu_to_cpu(va) {
                        segments.push((fd, cpu, words));
                    }
                }
                self.submissions.extend(segments);
                let ch = self.channels.get_mut(&fd).unwrap();
                ch.submits += 1;
                // The work runs as it is submitted: advance the channel's syncpoint and report it as the fence.
                let value = self.syncpts[syncpt as usize].wrapping_add(1);
                self.syncpts[syncpt as usize] = value;
                wr32(data, 16, syncpt);
                wr32(data, 20, value);
                NV_OK
            }
            // ALLOC_OBJ_CTX { u32 class_num; u32 flags; u64 obj_id (out); }
            0x09 => {
                let class = rd32(data, 0);
                ch.classes.push(class);
                wr64(data, 8, ch.classes.len() as u64);
                NV_OK
            }
            // GET_ERROR_INFO / GET_ERROR_NOTIFICATION: no errors happen here, so report zeros
            0x16 | 0x17 => {
                data.fill(0);
                NV_OK
            }
            // ALLOC_GPFIFO_EX2 { u32 num_entries; u32 flags; u32 unk0; fence (out) { u32 id; u32 value; }; u32 unk[3]; }
            0x1A => {
                ch.gpfifo_entries = rd32(data, 0);
                wr32(data, 12, syncpt);
                wr32(data, 16, self.syncpts[syncpt as usize]);
                NV_OK
            }
            _ => return None,
        })
    }
}

/// `ZCULL_GET_CTX_SIZE` result.
pub const ZCULL_CTX_SIZE: u32 = 0x8000;

/// `nvioctl_gpu_characteristics` for the Switch's GM20B, with the values libnx documents.
pub fn gpu_characteristics() -> Vec<u8> {
    let mut b = Vec::with_capacity(0xA0);
    let w32 = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
    let w64 = |b: &mut Vec<u8>, v: u64| b.extend_from_slice(&v.to_le_bytes());
    for v in [0x120, 0xB, 0xA1, 0x1] {
        w32(&mut b, v); // arch, impl, rev, num_gpc
    }
    w64(&mut b, 0x40000); // L2 cache size
    w64(&mut b, 0); // on-board video memory
    for v in [0x2, 0x20, 0x20000, 0x20000, 0x1B, 0x30000, 0x1, 0x503, 0x503, 0x80, 0x28, 0] {
        w32(&mut b, v); // num_tpc_per_gpc .. reserved
    }
    w64(&mut b, 0x55); // flags
    for v in [0x902D, 0xB197, 0xB1C0, 0xB06F, 0xA140, 0xB0B5, 0x1, 0x0, 0x2, 0x1, 0x0, 0x1, 0x21D70, 0x0] {
        w32(&mut b, v); // classes .. rop_l2_en_mask_1
    }
    w64(&mut b, 0x62_3032_6D67); // chipname "gm20b"
    w64(&mut b, 0); // gr_compbit_store_base_hw
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvmap_lifecycle() {
        let mut nv = NvDrv::new();
        let (fd, err) = nv.open("/dev/nvmap");
        assert_eq!(err, NV_OK);

        let mut create = [0u8; 8];
        create[..4].copy_from_slice(&0x10000u32.to_le_bytes());
        assert_eq!(nv.ioctl(fd, NVMAP_IOC_CREATE, &mut create), NV_OK);
        let handle = rd32(&create, 4);
        assert_ne!(handle, 0);

        let mut alloc = [0u8; 32];
        wr32(&mut alloc, 0, handle);
        wr32(&mut alloc, 12, 0x1000);
        wr64(&mut alloc, 24, 0x2_0000_0000);
        assert_eq!(nv.ioctl(fd, NVMAP_IOC_ALLOC, &mut alloc), NV_OK);

        let mut get_id = [0u8; 8];
        wr32(&mut get_id, 4, handle);
        assert_eq!(nv.ioctl(fd, NVMAP_IOC_GET_ID, &mut get_id), NV_OK);
        let id = rd32(&get_id, 0);
        let mut from_id = [0u8; 8];
        wr32(&mut from_id, 0, id);
        assert_eq!(nv.ioctl(fd, NVMAP_IOC_FROM_ID, &mut from_id), NV_OK);
        assert_eq!(rd32(&from_id, 4), handle);

        let mut param = [0u8; 12];
        wr32(&mut param, 0, handle);
        wr32(&mut param, 4, 1);
        assert_eq!(nv.ioctl(fd, NVMAP_IOC_PARAM, &mut param), NV_OK);
        assert_eq!(rd32(&param, 8), 0x10000);

        let mut free = [0u8; 24];
        wr32(&mut free, 0, handle);
        assert_eq!(nv.ioctl(fd, NVMAP_IOC_FREE, &mut free), NV_OK);
        assert_eq!(u64::from_le_bytes(free[8..16].try_into().unwrap()), 0x2_0000_0000);
        assert_eq!(nv.ioctl(fd, NVMAP_IOC_GET_ID, &mut get_id), NV_BAD_PARAMETER);

        assert_eq!(nv.ioctl(fd, 0xC004_0199, &mut [0u8; 4]), NV_NOT_IMPLEMENTED);
        assert_eq!(nv.close(fd), NV_OK);
        assert_eq!(nv.ioctl(fd, NVMAP_IOC_CREATE, &mut create), NV_BAD_PARAMETER);
    }

    #[test]
    fn gpu_channel_submit_and_fence() {
        let mut nv = NvDrv::new();
        let (map_fd, _) = nv.open("/dev/nvmap");
        let (as_fd, _) = nv.open("/dev/nvhost-as-gpu");
        let (ch_fd, _) = nv.open("/dev/nvhost-gpu");
        let (ctrl_fd, _) = nv.open("/dev/nvhost-ctrl");

        // nvmap object backed at guest 0x5000_0000, mapped into the GPU address space.
        let mut create = [0u8; 8];
        wr32(&mut create, 0, 0x1000);
        nv.ioctl(map_fd, NVMAP_IOC_CREATE, &mut create);
        let handle = rd32(&create, 4);
        let mut alloc = [0u8; 32];
        wr32(&mut alloc, 0, handle);
        wr64(&mut alloc, 24, 0x5000_0000);
        nv.ioctl(map_fd, NVMAP_IOC_ALLOC, &mut alloc);
        let mut map = [0u8; 40];
        wr32(&mut map, 8, handle);
        assert_eq!(nv.ioctl(as_fd, 0xC028_4106, &mut map), NV_OK);
        let va = u64::from_le_bytes(map[32..40].try_into().unwrap());
        assert!(va >= GPU_VA_BASE);
        assert_eq!(nv.gpu_to_cpu(va + 0x10), Some(0x5000_0010));

        // ALLOC_GPFIFO_EX2 hands back the channel's syncpoint as the fence.
        let mut gpfifo = [0u8; 32];
        wr32(&mut gpfifo, 0, 0x800);
        assert_eq!(nv.ioctl(ch_fd, 0xC020_481A, &mut gpfifo), NV_OK);
        let syncpt = rd32(&gpfifo, 12);
        assert_eq!(syncpt, FIRST_CHANNEL_SYNCPT);

        // SUBMIT_GPFIFO with one inline entry of 3 words at va + 0x40.
        let mut submit = vec![0u8; 0x20];
        wr32(&mut submit, 8, 1);
        wr64(&mut submit, 0x18, (va + 0x40) | 3 << 42);
        assert_eq!(nv.ioctl(ch_fd, 0xC020_4808, &mut submit), NV_OK);
        assert_eq!(nv.take_submissions(), vec![(0x5000_0040, 3)]);
        assert_eq!((rd32(&submit, 16), rd32(&submit, 20)), (syncpt, 1));

        // The fence is already reached; a later threshold is not.
        let mut wait = [0u8; 12];
        wr32(&mut wait, 0, syncpt);
        wr32(&mut wait, 4, 1);
        assert_eq!(nv.ioctl(ctrl_fd, 0xC00C_0016, &mut wait), NV_OK);
        wr32(&mut wait, 4, 2);
        assert_eq!(nv.ioctl(ctrl_fd, 0xC00C_0016, &mut wait), NV_TIMEOUT);

        // GET_CHARACTERISTICS reports GM20B classes.
        let (gpu_fd, _) = nv.open("/dev/nvhost-ctrl-gpu");
        let mut gc = vec![0u8; 0xB0];
        assert_eq!(nv.ioctl(gpu_fd, 0xC0B0_4705, &mut gc), NV_OK);
        assert_eq!(rd32(&gc, 0x10), 0x120);
        assert_eq!(gpu_characteristics().len(), 0xA0);
    }

    /// One nvmap object of `size` bytes backed at guest `cpu`, mapped into the GPU address space.
    fn map_object(nv: &mut NvDrv, map_fd: u32, as_fd: u32, cpu: u64, size: u32) -> u64 {
        let mut create = [0u8; 8];
        wr32(&mut create, 0, size);
        nv.ioctl(map_fd, NVMAP_IOC_CREATE, &mut create);
        let handle = rd32(&create, 4);
        let mut alloc = [0u8; 32];
        wr32(&mut alloc, 0, handle);
        wr64(&mut alloc, 24, cpu);
        nv.ioctl(map_fd, NVMAP_IOC_ALLOC, &mut alloc);
        let mut map = [0u8; 40];
        wr32(&mut map, 8, handle);
        assert_eq!(nv.ioctl(as_fd, 0xC028_4106, &mut map), NV_OK);
        u64::from_le_bytes(map[32..40].try_into().unwrap())
    }

    /// A libnx-shaped channel that allocated `class`: submitted segments reach the 3D engine,
    /// which reads and writes guest memory through the GPU mappings.
    fn submit_3d(class: u32) -> (NvDrv, Memory, u64, u64, bool) {
        use crate::gpu::ampere::methods as m;
        use crate::gpu::ampere::pushbuf::{immd_header, inc_header};
        use crate::mmu::Permission;

        let mut mmu = Memory::new();
        let (rt_cpu, pb_cpu) = (0x6000_0000u64, 0x6100_0000u64);
        mmu.map_range(rt_cpu, 0x10000, Permission::READ_WRITE);
        mmu.map_range(pb_cpu, 0x1000, Permission::READ_WRITE);
        let mut nv = NvDrv::new();
        let (map_fd, _) = nv.open("/dev/nvmap");
        let (as_fd, _) = nv.open("/dev/nvhost-as-gpu");
        let (ch_fd, _) = nv.open("/dev/nvhost-gpu");
        let rt_va = map_object(&mut nv, map_fd, as_fd, rt_cpu, 0x10000);
        let pb_va = map_object(&mut nv, map_fd, as_fd, pb_cpu, 0x1000);
        let mut obj = [0u8; 16];
        wr32(&mut obj, 0, class);
        assert_eq!(nv.ioctl(ch_fd, 0xC010_4809, &mut obj), NV_OK);

        // 64x64 A8B8G8R8 target, block linear with 8-GOB blocks (4 GOBs wide x 1 block high =
        // 16 KiB), cleared to opaque green, then a one-word semaphore release into the same buffer
        // past the surface.
        let hi = |va: u64| (va >> 32) as u32;
        let lo = |va: u64| va as u32;
        let sem_va = rt_va + 0x8000;
        let mut pb = vec![inc_header(0, m::SET_OBJECT, 1), class];
        pb.extend([
            inc_header(0, m::SET_COLOR_TARGET_A, 9),
            hi(rt_va),
            lo(rt_va),
            64,
            64,
            0xd5,
            3 << 4,
            1,
            0,
            0,
        ]);
        pb.push(immd_header(0, m::SET_CT_SELECT, 1));
        pb.push(inc_header(0, m::SET_COLOR_CLEAR_VALUE, 4));
        pb.extend([0.0f32, 1.0, 0.0, 1.0].map(f32::to_bits));
        pb.push(immd_header(0, m::CLEAR_SURFACE, 0x3c));
        pb.extend([
            inc_header(0, m::SET_REPORT_SEMAPHORE_A, 4),
            hi(sem_va),
            lo(sem_va),
            0xfeed,
            1 << 28,
        ]);
        let bytes: Vec<u8> = pb.iter().flat_map(|w| w.to_le_bytes()).collect();
        mmu.write_bytes(pb_cpu, &bytes).unwrap();

        let mut submit = vec![0u8; 0x20];
        wr32(&mut submit, 8, 1);
        wr64(&mut submit, 0x18, pb_va | (pb.len() as u64) << 42);
        assert_eq!(nv.ioctl(ch_fd, 0xC020_4808, &mut submit), NV_OK);
        let subs = nv.take_channel_submissions();
        assert_eq!(subs, vec![(ch_fd, pb_cpu, pb.len())]);
        let ran = nv.run_3d(&mut mmu, ch_fd, pb_cpu, pb.len());
        (nv, mmu, rt_cpu, ch_fd as u64, ran)
    }

    #[test]
    fn submitted_3d_work_renders_into_guest_memory() {
        use crate::gpu::tiling::{Format, Layout, Surface, detile};
        for class in [crate::gpu::ampere::AMPERE_B, crate::gpu::ampere::MAXWELL_B] {
            let (nv, mmu, rt_cpu, ch_fd, ran) = submit_3d(class);
            assert!(ran);
            assert!(nv.gpu_diagnostics.is_empty(), "{:?}", nv.gpu_diagnostics);
            assert_eq!(nv.engines[&(ch_fd as u32)].render_stats().clears, 1);
            let mut raw = vec![0u8; 64 * 64 * 4];
            mmu.read_bytes(rt_cpu, &mut raw).unwrap();
            let surface = Surface::new_2d(
                64,
                64,
                Format::R8G8B8A8Unorm,
                Layout::BlockLinear {
                    height_log2: 3,
                    depth_log2: 0,
                },
            );
            let linear = detile(&surface, &raw).unwrap();
            assert!(linear.chunks(4).all(|p| p == [0, 255, 0, 255]));
            assert_eq!(mmu.read_u32(rt_cpu + 0x8000).unwrap(), 0xfeed);
        }
    }

    #[test]
    fn channels_without_a_3d_class_keep_the_old_path() {
        let (nv, mmu, rt_cpu, _, ran) = submit_3d(0xB1C0); // compute class only
        assert!(!ran);
        assert!(nv.engines.is_empty());
        assert_eq!(mmu.read_u32(rt_cpu).unwrap(), 0);
    }

    #[test]
    fn gpu_memory_spans_mappings_and_rejects_holes() {
        use crate::mmu::Permission;
        let mut mmu = Memory::new();
        mmu.map_range(0x7000_0000, 0x2000, Permission::READ_WRITE);
        let mut maps = BTreeMap::new();
        maps.insert(
            0x4_0000_0000,
            GpuMapping {
                size: 0x1000,
                cpu_addr: 0x7000_1000,
            },
        );
        maps.insert(
            0x4_0000_1000,
            GpuMapping {
                size: 0x1000,
                cpu_addr: 0x7000_0000,
            },
        );
        maps.insert(
            0x4_0000_2000,
            GpuMapping {
                size: 0x1000,
                cpu_addr: 0,
            },
        );
        let mut mem = GuestGpuMemory {
            mmu: &mut mmu,
            mappings: &maps,
        };
        mem.write(0x4_0000_0ffe, &[1, 2, 3, 4]);
        let mut buf = [0u8; 4];
        assert!(mem.read(0x4_0000_0ffe, &mut buf));
        assert_eq!(buf, [1, 2, 3, 4]);
        assert!(mem.read(0, &mut []));
        assert!(!mem.read(0x4_0000_1ffe, &mut buf), "unbacked reservation");
        assert!(!mem.read(0x3_ffff_fffe, &mut buf), "below every mapping");
        // The first two bytes land at the end of the first mapping's backing, the rest at the start
        // of the second's.
        let mut two = [0u8; 2];
        mmu.read_bytes(0x7000_1ffe, &mut two).unwrap();
        assert_eq!(two, [1, 2]);
        mmu.read_bytes(0x7000_0000, &mut two).unwrap();
        assert_eq!(two, [3, 4]);
    }

    #[test]
    fn ioctl_fields() {
        assert_eq!(ioctl_size(NVMAP_IOC_ALLOC), 0x20);
        assert!(ioctl_has_input(NVMAP_IOC_CREATE) && ioctl_has_output(NVMAP_IOC_CREATE));
    }
}
