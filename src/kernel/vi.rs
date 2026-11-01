//! Bounded Binder bootstrap for the public libnx parcel/relay interfaces.
//!
//! Sources: libnx display/parcel.h (16-byte parcel header, 0x400 payload cap)
//! and display/binder.h / binder.c (relay command IDs and argument layout).
//! The native-window parcel only exposes a registered producer object. Producer
//! transactions stay explicit and bounded; no queued buffer is presented until
//! its nvmap backing can be resolved.

use super::ipc::{IpcManager, ServiceType, RESULT_UNKNOWN_COMMAND};
use std::collections::BTreeMap;

const MAX_PARCEL_SIZE: usize = 0x1000;
const MAX_PAYLOAD_SIZE: usize = 0x400;
const MAX_OBJECTS: usize = 32;
const INVALID_SIZE: u32 = (101 << 9) | 1;
const INVALID_ADDRESS: u32 = (102 << 9) | 1;
const INVALID_HANDLE: u32 = (114 << 9) | 1;
const INVALID_ENUM: u32 = (120 << 9) | 1;
const INVALID_STATE: u32 = (125 << 9) | 1;
const PRODUCER_TOKEN_SIZE: usize = 0x50;
const PRODUCER_TOKEN: &str = "android.gui.IGraphicBufferProducer";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicBufferSlot {
    pub nvmap_id: u32,
    pub width: u32,
    pub height: u32,
    pub color_format: u64,
    pub layout: u32,
    pub pitch: u32,
    pub offset: u32,
    pub kind: u32,
    pub block_height_log2: u32,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedSurface {
    pub guest_addr: u64,
    pub width: u32,
    pub height: u32,
    pub color_format: u64,
    pub layout: u32,
    pub pitch: u32,
    pub kind: u32,
    pub block_height_log2: u32,
    /// Bytes available from `guest_addr` to the end of the nvmap allocation.
    pub allocation_size: u64,
    pub size: u64,
}

impl SubmittedSurface {
    /// Maps the public libnx `NvGraphicBuffer` fields into the shared raw-surface model.
    pub fn tiling_surface(&self) -> Option<crate::gpu::tiling::Surface> {
        use crate::gpu::tiling::{Format, Layout, Surface};
        let format = match self.color_format {
            0x0100532120 => Format::R8G8B8A8Unorm,
            0x010A881210 => Format::R5G6B5Unorm,
            _ => return None,
        };
        let layout = match self.layout {
            3 if self.block_height_log2 <= 5 => Layout::BlockLinear {
                height_log2: self.block_height_log2 as u8,
                depth_log2: 0,
            },
            1 if self.pitch != 0 => Layout::Pitch { stride: self.pitch },
            _ => return None,
        };
        let surface = Surface {
            width: self.width,
            height: self.height,
            depth: 1,
            format,
            layout,
        };
        let required_size = surface.size_bytes().ok()? as u64;
        (required_size <= self.allocation_size && required_size <= self.size).then_some(surface)
    }
}

#[derive(Debug, Default)]
struct ProducerObject {
    refs: [u32; 2],
    connected: bool,
    slots: BTreeMap<i32, GraphicBufferSlot>,
    next_slot: usize,
}

/// Minimal native-window parcel consumed by libnx: two opaque words followed by
/// the signed Binder object id. libnx does not interpret the opaque words.
pub fn native_window_parcel(object_id: u32) -> Vec<u8> {
    let payload_size = 12u32;
    let payload_off = 16u32;
    let objects_off = payload_off + payload_size;
    let mut out = Vec::with_capacity(objects_off as usize);
    for word in [payload_size, payload_off, 0, objects_off, 0, 0, object_id] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out
}

/// Validated regions, borrowed from the original bounded parcel.
#[derive(Debug, PartialEq, Eq)]
pub struct Parcel<'a> {
    pub payload: &'a [u8],
    pub object_offsets: &'a [u8],
}

impl<'a> Parcel<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, u32> {
        if !(16..=MAX_PARCEL_SIZE).contains(&bytes.len()) {
            return Err(INVALID_SIZE);
        }
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let (payload_size, payload_off, objects_size, objects_off) =
            (word(0), word(4), word(8), word(12));
        if payload_size > MAX_PAYLOAD_SIZE || objects_size % 4 != 0 {
            return Err(INVALID_SIZE);
        }
        let region = |off: usize, size: usize| -> Result<std::ops::Range<usize>, u32> {
            if off < 16 || off % 4 != 0 {
                return Err(INVALID_SIZE);
            }
            let end = off.checked_add(size).ok_or(INVALID_SIZE)?;
            if end > bytes.len() {
                return Err(INVALID_SIZE);
            }
            Ok(off..end)
        };
        let payload = region(payload_off, payload_size)?;
        let objects = region(objects_off, objects_size)?;
        if !payload.is_empty()
            && !objects.is_empty()
            && payload.start < objects.end
            && objects.start < payload.end
        {
            return Err(INVALID_SIZE);
        }
        // Object-table entries are byte offsets in the payload. Their concrete
        // flattened types remain unsupported, but even their first word must fit.
        let mut previous = None;
        for entry in bytes[objects.clone()].chunks_exact(4) {
            let offset = u32::from_le_bytes(entry.try_into().unwrap()) as usize;
            if offset % 4 != 0
                || offset.checked_add(4).is_none_or(|end| end > payload_size)
                || previous.is_some_and(|last| offset <= last)
            {
                return Err(INVALID_SIZE);
            }
            previous = Some(offset);
        }
        Ok(Self {
            payload: &bytes[payload],
            object_offsets: &bytes[objects],
        })
    }
}

/// A registered service object owns one lifetime reference independently from
/// clients' weak/strong refs. No object is registered just by opening a relay.
#[derive(Debug, Default)]
pub struct BinderObjects {
    objects: BTreeMap<u32, ProducerObject>,
    layers: BTreeMap<u64, u32>,
    next_id: u32,
    pub last_submission: Option<GraphicBufferSlot>,
    /// `QUEUE_BUFFER` transactions accepted so far; the i-th queue is frame i.
    pub queued: u64,
}

impl BinderObjects {
    /// For a future producer implementation. IDs never repeat after retirement.
    pub fn register(&mut self) -> Result<u32, u32> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err((103 << 9) | 1);
        }
        let id = self.next_id.checked_add(1).ok_or(INVALID_STATE)?;
        self.next_id = id;
        self.objects.insert(id, ProducerObject::default());
        Ok(id)
    }

    pub fn open_layer(&mut self, layer_id: u64) -> Result<u32, u32> {
        if let Some(&object_id) = self.layers.get(&layer_id) {
            return Ok(object_id);
        }
        let object_id = self.register()?;
        self.layers.insert(layer_id, object_id);
        Ok(object_id)
    }

    pub fn has_layer(&self, layer_id: u64) -> bool {
        self.layers.contains_key(&layer_id)
    }

    pub fn close_layer(&mut self, layer_id: u64) -> Result<(), u32> {
        let object_id = self.layers.remove(&layer_id).ok_or(INVALID_HANDLE)?;
        if let Err(result) = self.retire(object_id) {
            self.layers.insert(layer_id, object_id);
            return Err(result);
        }
        Ok(())
    }

    pub fn adjust(&mut self, id: u32, delta: i32, kind: u32) -> Result<(), u32> {
        if kind > 1 {
            return Err(INVALID_ENUM);
        }
        let counts = &mut self.objects.get_mut(&id).ok_or(INVALID_HANDLE)?.refs;
        let current = counts[kind as usize];
        let next = if delta >= 0 {
            current.checked_add(delta as u32)
        } else {
            current.checked_sub(delta.unsigned_abs())
        }
        .ok_or(INVALID_STATE)?;
        counts[kind as usize] = next;
        Ok(())
    }

    /// A registered object cannot be destroyed while clients hold references.
    pub fn retire(&mut self, id: u32) -> Result<(), u32> {
        let object = self.objects.get(&id).ok_or(INVALID_HANDLE)?;
        if object.refs != [0, 0] {
            return Err(INVALID_STATE);
        }
        self.objects.remove(&id);
        Ok(())
    }

    fn transact(&mut self, id: u32, code: u32, parcel: &Parcel<'_>) -> Result<Vec<u8>, u32> {
        let payload = parcel.payload;
        validate_producer_token(payload)?;
        let object = self.objects.get_mut(&id).ok_or(INVALID_HANDLE)?;
        let word = |off: usize| {
            payload
                .get(off..off + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                .ok_or(INVALID_SIZE)
        };
        match code {
            // REQUEST_BUFFER: preallocated buffers are already shared; libnx expects null.
            0x1 => {
                let slot = word(0x50)? as i32;
                if !object.slots.contains_key(&slot) {
                    return Err(INVALID_HANDLE);
                }
                Ok(reply_parcel(&[0, 0]))
            }
            // DEQUEUE_BUFFER: return one registered slot and an empty fence (num_fences = 0).
            0x3 => {
                if !object.connected || object.slots.is_empty() {
                    return Err(INVALID_STATE);
                }
                let slots: Vec<i32> = object.slots.keys().copied().collect();
                let slot = slots[object.next_slot % slots.len()];
                object.next_slot = (object.next_slot + 1) % slots.len();
                // Format: slot, has_fence (1), flattened object length (36), fd_count (0),
                // 36 bytes of flattened NvMultiFence (num_fences=0, 32 bytes reserved), status (0).
                let mut words = vec![slot as u32, 1, 36, 0];
                words.extend_from_slice(&[0u32; 9]); // 36 bytes of NvMultiFence with num_fences = 0
                words.push(0); // status / error code (0 = success)
                Ok(reply_parcel(&words))
            }
            // QUEUE_BUFFER: record the exact registered slot. Presentation/capture is separate.
            0x7 => {
                if !object.connected {
                    return Err(INVALID_STATE);
                }
                let slot = word(0x50)? as i32;
                let submitted = object.slots.get(&slot).cloned().ok_or(INVALID_HANDLE)?;
                self.last_submission = Some(submitted);
                self.queued += 1;
                Ok(buffer_output_parcel(1280, 720))
            }
            // CONNECT(NATIVE_WINDOW_API_CPU).
            0xA => {
                if word(0x50)? != 0 || word(0x54)? != 2 {
                    return Err(INVALID_ENUM);
                }
                object.connected = true;
                Ok(buffer_output_parcel(1280, 720))
            }
            // DISCONNECT(NATIVE_WINDOW_API_CPU).
            0xB => {
                if word(0x50)? != 2 {
                    return Err(INVALID_ENUM);
                }
                object.connected = false;
                Ok(reply_parcel(&[0]))
            }
            // SET_PREALLOCATED_BUFFER.
            0xE => {
                let slot = word(0x50)? as i32;
                if word(0x54)? != 1
                    || word(0x58)? != 364
                    || word(0x5C)? != 0
                    || word(0x60)? != 0x4742_4652
                    || word(0x80)? != 0
                    || word(0x84)? != 81
                {
                    return Err(INVALID_SIZE);
                }
                let u64_at = |off: usize| {
                    payload
                        .get(off..off + 8)
                        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                        .ok_or(INVALID_SIZE)
                };
                let buffer = GraphicBufferSlot {
                    nvmap_id: word(0x8C)?,
                    width: word(0xBC)?,
                    height: word(0xC0)?,
                    // NvSurface has a 64-bit NvColorFormat. The flattened
                    // native handle drops its 12-byte header, not struct padding.
                    color_format: u64_at(0xC4)?,
                    layout: word(0xCC)?,
                    pitch: word(0xD0)?,
                    offset: word(0xD8)?,
                    kind: word(0xDC)?,
                    block_height_log2: word(0xE0)?,
                    size: u64_at(0xF4)?,
                };
                if buffer.nvmap_id == 0
                    || buffer.width == 0
                    || buffer.height == 0
                    || buffer.size == 0
                {
                    return Err(INVALID_SIZE);
                }
                object.slots.insert(slot, buffer);
                Ok(reply_parcel(&[]))
            }
            _ => Err(RESULT_UNKNOWN_COMMAND),
        }
    }
}

fn validate_producer_token(payload: &[u8]) -> Result<(), u32> {
    if payload.len() < PRODUCER_TOKEN_SIZE {
        return Err(INVALID_SIZE);
    }
    let word = |off: usize| u32::from_le_bytes(payload[off..off + 4].try_into().unwrap());
    if word(0) != 0x100 || word(4) != PRODUCER_TOKEN.len() as u32 {
        return Err(INVALID_SIZE);
    }
    for (i, expected) in PRODUCER_TOKEN
        .encode_utf16()
        .chain(std::iter::once(0))
        .enumerate()
    {
        let off = 8 + i * 2;
        if u16::from_le_bytes(payload[off..off + 2].try_into().unwrap()) != expected {
            return Err(INVALID_SIZE);
        }
    }
    Ok(())
}

fn reply_parcel(words: &[u32]) -> Vec<u8> {
    let payload_size = (words.len() * 4) as u32;
    let mut out = Vec::with_capacity(16 + payload_size as usize);
    for word in [payload_size, 16, 0, 16 + payload_size] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    for word in words {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out
}

fn buffer_output_parcel(width: u32, height: u32) -> Vec<u8> {
    reply_parcel(&[width, height, 0, 0, 0])
}

impl IpcManager {
    pub fn vi_submission_diagnostic(&self) -> String {
        format!("slot={:?}; nvmap={:?}", self.vi_binder.last_submission, self.nvdrv.nvmap)
    }

    /// Number of `QUEUE_BUFFER` transactions accepted (frame i is the i-th queue).
    pub fn vi_frames_queued(&self) -> u64 {
        self.vi_binder.queued
    }

    /// Resolves the last queued producer slot through the nvmap registry. This
    /// describes raw guest memory only; it does not imply a valid rendered frame.
    pub fn submitted_vi_surface(&self) -> Option<SubmittedSurface> {
        let slot = self.vi_binder.last_submission.as_ref()?;
        let object = self
            .nvdrv
            .nvmap
            .values()
            .find(|object| object.id == slot.nvmap_id && object.addr != 0)?;
        let end = (slot.offset as u64).checked_add(slot.size)?;
        if end > object.size as u64 {
            return None;
        }
        Some(SubmittedSurface {
            guest_addr: object.addr.checked_add(slot.offset as u64)?,
            width: slot.width,
            height: slot.height,
            color_format: slot.color_format,
            layout: slot.layout,
            pitch: slot.pitch,
            kind: slot.kind,
            block_height_log2: slot.block_height_log2,
            allocation_size: object.size as u64 - slot.offset as u64,
            size: slot.size,
        })
    }

    pub(super) fn handle_vi_binder(
        &mut self,
        service: &ServiceType,
        cmd: u32,
        args: u64,
        tls: u64,
        mmu: &mut crate::mmu::Memory,
    ) -> Option<u64> {
        if !matches!(service, ServiceType::Generic(name)
            if name == "vi:binder-relay" || name == "vi:binder-indirect")
        {
            return None;
        }
        let result = match cmd {
            // AdjustRefcount(object_id, signed delta, weak/strong selector).
            1 => {
                let fields = mmu.read_u32(args).and_then(|id| {
                    mmu.read_u32(args + 4).and_then(|delta| {
                        mmu.read_u32(args + 8).map(|kind| (id, delta as i32, kind))
                    })
                });
                match fields {
                    Ok((id, delta, kind)) => {
                        self.vi_binder.adjust(id, delta, kind).err().unwrap_or(0)
                    }
                    Err(_) => INVALID_ADDRESS,
                }
            }
            // TransactParcel / TransactParcelAuto.
            0 | 3 => {
                let fields = mmu.read_u32(args).and_then(|id| {
                    mmu.read_u32(args + 4)
                        .and_then(|code| mmu.read_u32(args + 8).map(|flags| (id, code, flags)))
                });
                static TRACE_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                if std::env::var_os("OUNCE_TRACE_IPC").is_some()
                    && self.vi_binder.last_submission.is_none()
                    && TRACE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 64
                {
                    eprintln!("[VI transaction] fields={fields:?}");
                }
                let input = self
                    .request
                    .send_statics
                    .first()
                    .map(|(_, b)| *b)
                    .filter(|b| b.size != 0)
                    .or_else(|| self.request.send_buffers.first().copied());
                match (fields, input) {
                    (Ok((_, _, flags)), _) if flags != 0 => INVALID_ENUM,
                    (Ok((id, code, _)), Some(buffer))
                        if (16..=MAX_PARCEL_SIZE as u64).contains(&buffer.size)
                            && buffer.addr.checked_add(buffer.size).is_some() =>
                    {
                        let mut bytes = vec![0; buffer.size as usize];
                        match mmu.read_bytes(buffer.addr, &mut bytes) {
                            Ok(()) => match Parcel::parse(&bytes) {
                                Ok(parcel) => match self.vi_binder.transact(id, code, &parcel) {
                                    Ok(output)
                                        if self.write_out_buffer(mmu, &output) == output.len() =>
                                    {
                                        if code == 7 && std::env::var_os("OUNCE_TRACE_IPC").is_some() {
                                            eprintln!("[VI queue] result=0; {}", self.vi_submission_diagnostic());
                                        }
                                        0
                                    }
                                    Ok(_) => INVALID_ADDRESS,
                                    Err(result) => {
                                        if code == 7 && std::env::var_os("OUNCE_TRACE_IPC").is_some() {
                                            eprintln!("[VI queue] result={result:#x}; {}", self.vi_submission_diagnostic());
                                        }
                                        result
                                    },
                                },
                                Err(result) => result,
                            },
                            Err(_) => INVALID_ADDRESS,
                        }
                    }
                    (Err(_), _) => INVALID_ADDRESS,
                    (_, None) => RESULT_UNKNOWN_COMMAND,
                    (_, Some(_)) => INVALID_SIZE,
                }
            }
            // GetNativeHandle(object_id, inval=0x0f): the event is signaled while
            // this bounded producer has an immediately available slot.
            2 => {
                let fields = mmu
                    .read_u32(args)
                    .and_then(|id| mmu.read_u32(args + 4).map(|inval| (id, inval)));
                match fields {
                    Ok((id, 0x0f)) if self.vi_binder.objects.contains_key(&id) => {
                        let event =
                            self.allocate_handle(ServiceType::ServiceEvent("ViBufferQueue"));
                        self.reply(tls, mmu, 0, &[], &[event], None);
                        return Some(0);
                    }
                    Ok((id, _)) if !self.vi_binder.objects.contains_key(&id) => INVALID_HANDLE,
                    Ok(_) => INVALID_ENUM,
                    Err(_) => INVALID_ADDRESS,
                }
            }
            _ => RESULT_UNKNOWN_COMMAND,
        };
        self.reply(tls, mmu, result, &[], &[], None);
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parcel(payload: &[u8], offsets: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for word in [
            payload.len() as u32,
            16,
            (offsets.len() * 4) as u32,
            16 + payload.len() as u32,
        ] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(payload);
        for offset in offsets {
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn parcel_checks_all_regions_and_object_offsets() {
        let good = parcel(&[0; 16], &[0, 8]);
        let parsed = Parcel::parse(&good).unwrap();
        assert_eq!(parsed.payload.len(), 16);
        assert_eq!(parsed.object_offsets.len(), 8);
        assert!(Parcel::parse(&good[..15]).is_err());
        for (field, value) in [
            (0, 0x401),
            (4, u32::MAX),
            (8, 3),
            (12, u32::MAX),
            (12, 16),
            (4, 12),
        ] {
            let mut bad = good.clone();
            bad[field..field + 4].copy_from_slice(&value.to_le_bytes());
            assert!(Parcel::parse(&bad).is_err(), "field {field}, value {value}");
        }
        for offsets in [&[1][..], &[16][..], &[8, 0][..], &[0, 0][..]] {
            assert!(Parcel::parse(&parcel(&[0; 16], offsets)).is_err());
        }
        assert!(Parcel::parse(&vec![0; MAX_PARCEL_SIZE + 1]).is_err());
    }

    #[test]
    fn native_window_parcel_exposes_registered_producer_id() {
        let bytes = native_window_parcel(0x1234);
        let parsed = Parcel::parse(&bytes).unwrap();
        assert_eq!(parsed.payload.len(), 12);
        assert!(parsed.object_offsets.is_empty());
        assert_eq!(
            u32::from_le_bytes(parsed.payload[8..12].try_into().unwrap()),
            0x1234
        );
    }

    #[test]
    fn libnx_open_layer_and_native_handle_use_real_descriptors() {
        use super::super::cmif::test_util::{Harness, Req, BUF};
        let mut h = Harness::new();
        let display = h
            .ipc
            .allocate_handle(ServiceType::ApplicationDisplayService);
        let mut input = [0u8; 0x50];
        input[0x40..0x48].copy_from_slice(&1u64.to_le_bytes());
        let reply = h.call_out(
            display,
            Req {
                cmd: 2020,
                data: &input,
                pid: true,
                recv: Some((BUF, 0x100)),
                ..Default::default()
            },
            8,
        );
        assert_eq!(reply.result, 0);
        assert_eq!(reply.u64(0), 28);
        let parcel_bytes = h.read_buf(BUF, 28);
        let parcel = Parcel::parse(&parcel_bytes).unwrap();
        let object = u32::from_le_bytes(parcel.payload[8..12].try_into().unwrap());

        let mut scaling = [0u8; 16];
        scaling[0..4].copy_from_slice(&2u32.to_le_bytes());
        scaling[8..16].copy_from_slice(&1u64.to_le_bytes());
        assert_eq!(
            h.call(
                display,
                Req {
                    cmd: 2101,
                    data: &scaling,
                    ..Default::default()
                }
            )
            .result,
            0
        );

        let relay = h
            .ipc
            .allocate_handle(ServiceType::Generic("vi:binder-relay".into()));
        let mut args = [0u8; 8];
        args[0..4].copy_from_slice(&object.to_le_bytes());
        args[4..8].copy_from_slice(&0x0fu32.to_le_bytes());
        let reply = h.call(
            relay,
            Req {
                cmd: 2,
                data: &args,
                ..Default::default()
            },
        );
        assert_eq!(reply.result, 0);
        assert_eq!(reply.copy_handles.len(), 1);
        assert_eq!(
            h.ipc.service_event_signaled(reply.copy_handles[0]),
            Some(true)
        );

        assert_eq!(
            h.call(
                display,
                Req {
                    cmd: 2021,
                    data: &1u64.to_le_bytes(),
                    ..Default::default()
                }
            )
            .result,
            0
        );
        assert!(!h.ipc.vi_binder.has_layer(1));
    }

    #[test]
    fn relay_dispatch_validates_inputs_without_writing_output_parcels() {
        use super::super::hipc::Buffer;
        use crate::mmu::{Memory, Permission};
        use crate::{audio::AudioOutServer, fs::VirtualFileSystem, gpu::nvgpu::NvgpuChannel};
        let mut ipc = IpcManager::new();
        let mut mmu = Memory::new();
        mmu.map_range(0x1000, 0x4000, Permission::READ_WRITE);
        let session = ipc.allocate_handle(ServiceType::Generic("vi:binder-relay".into()));
        let object = ipc.vi_binder.register().unwrap();
        let (mut gpu, mut audio, mut vfs) = (
            NvgpuChannel::new(),
            AudioOutServer::new(),
            VirtualFileSystem::new(),
        );
        // Real HIPC/CMIF reference-count request: no descriptors, 3 input words.
        let request = |mmu: &mut Memory, id: u32, delta: i32, kind: u32| {
            for offset in (0..0x80u64).step_by(4) {
                mmu.write_u32(0x1000 + offset, 0).unwrap();
            }
            mmu.write_u32(0x1000, 4).unwrap();
            mmu.write_u32(0x1004, 11).unwrap();
            mmu.write_u32(0x1010, super::super::ipc::CMIF_IN_MAGIC)
                .unwrap();
            mmu.write_u32(0x1018, 1).unwrap();
            mmu.write_u32(0x1020, id).unwrap();
            mmu.write_u32(0x1024, delta as u32).unwrap();
            mmu.write_u32(0x1028, kind).unwrap();
        };
        request(&mut mmu, object, 1, 1);
        assert_eq!(
            ipc.handle_sync_request(session, 0x1000, &mut mmu, &mut gpu, &mut audio, &mut vfs),
            0
        );
        assert_eq!(mmu.read_u32(0x1018).unwrap(), 0);
        request(&mut mmu, object + 1, 1, 1);
        ipc.handle_sync_request(session, 0x1000, &mut mmu, &mut gpu, &mut audio, &mut vfs);
        assert_eq!(mmu.read_u32(0x1018).unwrap(), INVALID_HANDLE);

        let service = ServiceType::Generic("vi:binder-relay".into());
        let valid = parcel(&[0; 16], &[]);
        mmu.write_bytes(0x2000, &valid).unwrap();
        mmu.write_bytes(0x3000, &[0xA5; 64]).unwrap();
        ipc.request.send_buffers = vec![Buffer {
            addr: 0x2000,
            size: valid.len() as u64,
        }];
        ipc.request.recv_buffers = vec![Buffer {
            addr: 0x3000,
            size: 64,
        }];
        mmu.write_u32(0x1020, object).unwrap();
        mmu.write_u32(0x1024, 0x99).unwrap();
        mmu.write_u32(0x1028, 0).unwrap();
        ipc.handle_vi_binder(&service, 3, 0x1020, 0x1000, &mut mmu);
        assert_eq!(mmu.read_u32(0x1018).unwrap(), INVALID_SIZE);
        let mut output = [0; 64];
        mmu.read_bytes(0x3000, &mut output).unwrap();
        assert_eq!(output, [0xA5; 64]);
        for (addr, size, expected) in [
            (0x2000, 15, INVALID_SIZE),
            (u64::MAX - 4, 32, INVALID_SIZE),
            (0x9000, 32, INVALID_ADDRESS),
        ] {
            ipc.request.send_buffers = vec![Buffer { addr, size }];
            ipc.handle_vi_binder(&service, 0, 0x1020, 0x1000, &mut mmu);
            assert_eq!(mmu.read_u32(0x1018).unwrap(), expected);
            mmu.read_bytes(0x3000, &mut output).unwrap();
            assert_eq!(output, [0xA5; 64]);
        }
    }

    #[test]
    fn object_lifetime_and_refcounts_are_checked_and_bounded() {
        let mut objects = BinderObjects::default();
        assert_eq!(objects.adjust(1, 1, 0), Err(INVALID_HANDLE));
        let id = objects.register().unwrap();
        assert_eq!(objects.adjust(id, -1, 0), Err(INVALID_STATE));
        assert_eq!(objects.adjust(id, 1, 2), Err(INVALID_ENUM));
        objects.adjust(id, 1, 0).unwrap();
        objects.adjust(id, i32::MAX, 1).unwrap();
        objects.adjust(id, i32::MAX, 1).unwrap();
        assert_eq!(objects.adjust(id, 2, 1), Err(INVALID_STATE));
        assert_eq!(objects.retire(id), Err(INVALID_STATE));
        objects.adjust(id, -1, 0).unwrap();
        objects.adjust(id, -i32::MAX, 1).unwrap();
        objects.adjust(id, -i32::MAX, 1).unwrap();
        objects.retire(id).unwrap();
        assert_eq!(objects.adjust(id, 1, 0), Err(INVALID_HANDLE));
        assert_ne!(objects.register().unwrap(), id);
        for _ in 1..MAX_OBJECTS {
            objects.register().unwrap();
        }
        assert!(objects.register().is_err());
    }

    fn producer_payload(size: usize) -> Vec<u8> {
        let mut payload = vec![0; size.max(PRODUCER_TOKEN_SIZE)];
        payload[0..4].copy_from_slice(&0x100u32.to_le_bytes());
        payload[4..8].copy_from_slice(&(PRODUCER_TOKEN.len() as u32).to_le_bytes());
        for (i, unit) in PRODUCER_TOKEN
            .encode_utf16()
            .chain(std::iter::once(0))
            .enumerate()
        {
            payload[8 + i * 2..10 + i * 2].copy_from_slice(&unit.to_le_bytes());
        }
        payload
    }

    fn parsed(payload: &[u8]) -> Parcel<'_> {
        let bytes = parcel(payload, &[]);
        let leaked = Box::leak(bytes.into_boxed_slice());
        Parcel::parse(leaked).unwrap()
    }

    #[test]
    fn producer_preserves_u64_color_format_and_following_surface_fields() {
        let mut objects = BinderObjects::default();
        let id = objects.register().unwrap();

        let mut connect = producer_payload(0x5C);
        connect[0x54..0x58].copy_from_slice(&2u32.to_le_bytes());
        let reply = objects.transact(id, 0xA, &parsed(&connect)).unwrap();
        assert_eq!(Parcel::parse(&reply).unwrap().payload.len(), 20);

        let mut set = producer_payload(0x1CC);
        for (off, word) in [
            (0x50, 3),
            (0x54, 1),
            (0x58, 364),
            (0x60, 0x4742_4652),
            (0x84, 81),
            (0x8C, 0x1001),
            (0xBC, 1280),
            (0xC0, 720),
            (0xC4, 0x00532120),
            (0xC8, 1),
            (0xCC, 3),
            (0xD0, 5120),
            (0xD8, 0x2000),
            (0xDC, 0xFE),
            (0xE0, 4),
        ] as [(usize, u32); 15]
        {
            set[off..off + 4].copy_from_slice(&word.to_le_bytes());
        }
        // Includes the padded block-linear rows, not only visible pixels.
        set[0xF4..0xFC].copy_from_slice(&0x3C0000u64.to_le_bytes());
        objects.transact(id, 0xE, &parsed(&set)).unwrap();

        let dequeue = producer_payload(0x64);
        let reply = objects.transact(id, 0x3, &parsed(&dequeue)).unwrap();
        let payload = Parcel::parse(&reply).unwrap().payload;
        assert_eq!(u32::from_le_bytes(payload[0..4].try_into().unwrap()), 3);

        let mut queue = producer_payload(0xB0);
        queue[0x50..0x54].copy_from_slice(&3u32.to_le_bytes());
        objects.transact(id, 0x7, &parsed(&queue)).unwrap();
        let submitted = objects.last_submission.as_ref().unwrap();
        assert_eq!(submitted.nvmap_id, 0x1001);
        // libnx NvSurface: eight-byte color format followed by layout/pitch.
        // The high word must survive; reading it as layout was the regression.
        assert_eq!(submitted.color_format, 0x0100532120);
        assert_eq!((submitted.layout, submitted.kind), (3, 0xFE));
        assert_eq!(
            (submitted.width, submitted.height, submitted.pitch),
            (1280, 720, 5120)
        );
        assert_eq!((submitted.offset, submitted.block_height_log2), (0x2000, 4));

        let mut ipc = IpcManager::new();
        ipc.vi_binder = objects;
        ipc.nvdrv.nvmap.insert(
            7,
            super::super::nvdrv::NvMapObject {
                id: 0x1001,
                size: 0x400000,
                addr: 0x9000_0000,
                ..Default::default()
            },
        );
        let surface = ipc.submitted_vi_surface().unwrap();
        assert_eq!(surface.guest_addr, 0x9000_2000);
        assert_eq!(
            (surface.width, surface.height, surface.size),
            (1280, 720, 0x3C0000)
        );
        let tiled = surface.tiling_surface().unwrap();
        assert_eq!((tiled.width, tiled.height), (1280, 720));
        assert_eq!(
            tiled.layout,
            crate::gpu::tiling::Layout::BlockLinear {
                height_log2: 4,
                depth_log2: 0,
            }
        );
        let mut pitch = surface.clone();
        pitch.layout = 1;
        pitch.pitch = pitch.width * 4;
        assert_eq!(
            pitch.tiling_surface().unwrap().layout,
            crate::gpu::tiling::Layout::Pitch {
                stride: pitch.width * 4,
            }
        );
        let mut invalid = surface.clone();
        invalid.block_height_log2 = 254;
        assert!(invalid.tiling_surface().is_none());
        invalid = surface.clone();
        invalid.color_format = 0;
        assert!(invalid.tiling_surface().is_none());
        invalid = surface.clone();
        invalid.allocation_size = tiled.size_bytes().unwrap() as u64 - 1;
        assert!(invalid.tiling_surface().is_none());
        let required_size = tiled.size_bytes().unwrap() as u64;
        invalid = surface.clone();
        invalid.allocation_size = required_size * 2;
        invalid.size = required_size - 1;
        assert!(invalid.tiling_surface().is_none(),
            "a large nvmap allocation must not permit reads beyond the submitted plane");
        invalid.size = required_size;
        assert!(invalid.tiling_surface().is_some(),
            "an image fitting both the submitted plane and allocation is accepted");
        ipc.nvdrv.nvmap.get_mut(&7).unwrap().size = 0x1000;
        assert!(
            ipc.submitted_vi_surface().is_none(),
            "slot must fit its nvmap allocation"
        );
    }

    /// Mirrors the corpus runner: descriptor -> tiling surface -> frame -> centre pixel.
    fn extract_center(submitted: &SubmittedSurface) -> Option<(u8, u8, u8, u8)> {
        let mut mmu = crate::mmu::Memory::new();
        mmu.map_range(submitted.guest_addr, 0x1000, crate::mmu::Permission::READ_WRITE);
        let surface = submitted.tiling_surface()?;
        let frame = crate::gpu::present::surface::frame_from_surface(&mmu, submitted.guest_addr, &surface).ok()?;
        frame.pixel(frame.width / 2, frame.height / 2)
    }

    fn adversarial(width: u32, height: u32, layout: u32, pitch: u32) -> SubmittedSurface {
        SubmittedSurface {
            guest_addr: 0x9000_0000,
            width,
            height,
            color_format: 0x0100532120,
            layout,
            pitch,
            kind: 0xFE,
            block_height_log2: 4,
            allocation_size: 0x1000,
            size: 0x1000,
        }
    }

    /// adv_p5: pitch descriptor whose RGBA8 row size (width * 4) wraps u32 to 0.
    #[test]
    fn adv_p5_pitch_row_bytes_wrapping_to_zero_is_rejected() {
        let submitted = adversarial(0x4000_0000, 2, 1, 4);
        assert!(submitted.tiling_surface().is_none());
        assert_eq!(extract_center(&submitted), None);
    }

    /// adv_p5b: pitch descriptor whose row size wraps to a small non-zero value (4).
    #[test]
    fn adv_p5b_pitch_row_bytes_wrapping_to_small_value_is_rejected() {
        let submitted = adversarial(0x4000_0001, 16, 1, 4);
        assert!(submitted.tiling_surface().is_none());
        assert_eq!(extract_center(&submitted), None);
    }

    /// adv_p6: block-linear descriptor whose row size wraps to 0, giving a zero-byte plane.
    #[test]
    fn adv_p6_block_linear_row_bytes_wrapping_is_rejected() {
        let submitted = adversarial(0x4000_0000, 8, 3, 0);
        assert!(submitted.tiling_surface().is_none());
        assert_eq!(extract_center(&submitted), None);
    }

    /// Largest widths that do not wrap still have to fit the submitted plane.
    #[test]
    fn huge_but_non_wrapping_descriptors_fail_the_plane_bound() {
        for (layout, pitch) in [(1, 0xFFFF_FFFC), (3, 0)] {
            let submitted = adversarial(0x3FFF_FFFF, 1, layout, pitch);
            assert!(submitted.tiling_surface().is_none(), "layout {layout}");
        }
        // A small valid descriptor still extracts.
        let mut ok = adversarial(16, 16, 1, 64);
        ok.size = 16 * 64;
        assert!(ok.tiling_surface().is_some());
        assert_eq!(extract_center(&ok), Some((0, 0, 0, 0xFF)));
    }
}
