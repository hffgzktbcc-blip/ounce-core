//! Bounded Binder bootstrap for the public libnx parcel/relay interfaces.
//!
//! Sources: libnx display/parcel.h (16-byte parcel header, 0x400 payload cap)
//! and display/binder.h / binder.c (relay command IDs and argument layout).
//! No producer transaction or native-window wire object is implemented here.
//! In particular, OpenLayer must remain unsupported until its complete format
//! and backing producer queue are modeled.

use std::collections::BTreeMap;
use super::ipc::{IpcManager, ServiceType, RESULT_UNKNOWN_COMMAND};

const MAX_PARCEL_SIZE: usize = 0x1000;
const MAX_PAYLOAD_SIZE: usize = 0x400;
const MAX_OBJECTS: usize = 32;
const INVALID_SIZE: u32 = (101 << 9) | 1;
const INVALID_ADDRESS: u32 = (102 << 9) | 1;
const INVALID_HANDLE: u32 = (114 << 9) | 1;
const INVALID_ENUM: u32 = (120 << 9) | 1;
const INVALID_STATE: u32 = (125 << 9) | 1;

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
        if !payload.is_empty() && !objects.is_empty()
            && payload.start < objects.end && objects.start < payload.end
        {
            return Err(INVALID_SIZE);
        }
        // Object-table entries are byte offsets in the payload. Their concrete
        // flattened types remain unsupported, but even their first word must fit.
        let mut previous = None;
        for entry in bytes[objects.clone()].chunks_exact(4) {
            let offset = u32::from_le_bytes(entry.try_into().unwrap()) as usize;
            if offset % 4 != 0 || offset.checked_add(4).is_none_or(|end| end > payload_size)
                || previous.is_some_and(|last| offset <= last)
            {
                return Err(INVALID_SIZE);
            }
            previous = Some(offset);
        }
        Ok(Self { payload: &bytes[payload], object_offsets: &bytes[objects] })
    }
}

/// A registered service object owns one lifetime reference independently from
/// clients' weak/strong refs. No object is registered just by opening a relay.
#[derive(Debug, Default)]
pub struct BinderObjects {
    objects: BTreeMap<u32, [u32; 2]>,
    next_id: u32,
}

impl BinderObjects {
    /// For a future producer implementation. IDs never repeat after retirement.
    pub fn register(&mut self) -> Result<u32, u32> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err((103 << 9) | 1);
        }
        let id = self.next_id.checked_add(1).ok_or(INVALID_STATE)?;
        self.next_id = id;
        self.objects.insert(id, [0, 0]);
        Ok(id)
    }

    pub fn adjust(&mut self, id: u32, delta: i32, kind: u32) -> Result<(), u32> {
        if kind > 1 {
            return Err(INVALID_ENUM);
        }
        let counts = self.objects.get_mut(&id).ok_or(INVALID_HANDLE)?;
        let current = counts[kind as usize];
        let next = if delta >= 0 {
            current.checked_add(delta as u32)
        } else {
            current.checked_sub(delta.unsigned_abs())
        }.ok_or(INVALID_STATE)?;
        counts[kind as usize] = next;
        Ok(())
    }

    /// A registered object cannot be destroyed while clients hold references.
    pub fn retire(&mut self, id: u32) -> Result<(), u32> {
        let counts = self.objects.get(&id).ok_or(INVALID_HANDLE)?;
        if *counts != [0, 0] {
            return Err(INVALID_STATE);
        }
        self.objects.remove(&id);
        Ok(())
    }
}

impl IpcManager {
    pub(super) fn handle_vi_binder(
        &mut self, service: &ServiceType, cmd: u32, args: u64, tls: u64,
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
                let fields = mmu.read_u32(args).and_then(|id|
                    mmu.read_u32(args + 4).and_then(|delta|
                    mmu.read_u32(args + 8).map(|kind| (id, delta as i32, kind))));
                match fields {
                    Ok((id, delta, kind)) => self.vi_binder.adjust(id, delta, kind).err().unwrap_or(0),
                    Err(_) => INVALID_ADDRESS,
                }
            }
            // TransactParcel / TransactParcelAuto: validate the borrowed input
            // before reporting the unsupported producer transaction. Never write
            // an output parcel, mutate refs or claim a transaction succeeded.
            0 | 3 => {
                let input = self.request.send_statics.first().map(|(_, b)| *b)
                    .filter(|b| b.size != 0)
                    .or_else(|| self.request.send_buffers.first().copied());
                match input {
                    Some(buffer) if (16..=MAX_PARCEL_SIZE as u64).contains(&buffer.size)
                        && buffer.addr.checked_add(buffer.size).is_some() => {
                        let mut bytes = vec![0; buffer.size as usize];
                        match mmu.read_bytes(buffer.addr, &mut bytes) {
                            Ok(()) => match Parcel::parse(&bytes) {
                                Ok(_) => RESULT_UNKNOWN_COMMAND,
                                Err(result) => result,
                            },
                            Err(_) => INVALID_ADDRESS,
                        }
                    }
                    None => RESULT_UNKNOWN_COMMAND,
                    Some(_) => INVALID_SIZE,
                }
            }
            // GetNativeHandle requires a modeled queue event, not a dummy handle.
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
        for word in [payload.len() as u32, 16, (offsets.len() * 4) as u32, 16 + payload.len() as u32] {
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
        for (field, value) in [(0, 0x401), (4, u32::MAX), (8, 3), (12, u32::MAX), (12, 16), (4, 12)] {
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
    fn relay_dispatch_validates_inputs_without_writing_output_parcels() {
        use crate::{audio::AudioOutServer, fs::VirtualFileSystem, gpu::nvgpu::NvgpuChannel};
        use crate::mmu::{Memory, Permission};
        use super::super::hipc::Buffer;
        let mut ipc = IpcManager::new();
        let mut mmu = Memory::new();
        mmu.map_range(0x1000, 0x4000, Permission::READ_WRITE);
        let session = ipc.allocate_handle(ServiceType::Generic("vi:binder-relay".into()));
        let object = ipc.vi_binder.register().unwrap();
        let (mut gpu, mut audio, mut vfs) =
            (NvgpuChannel::new(), AudioOutServer::new(), VirtualFileSystem::new());
        // Real HIPC/CMIF reference-count request: no descriptors, 3 input words.
        let request = |mmu: &mut Memory, id: u32, delta: i32, kind: u32| {
            for offset in (0..0x80u64).step_by(4) {
                mmu.write_u32(0x1000 + offset, 0).unwrap();
            }
            mmu.write_u32(0x1000, 4).unwrap();
            mmu.write_u32(0x1004, 11).unwrap();
            mmu.write_u32(0x1010, super::super::ipc::CMIF_IN_MAGIC).unwrap();
            mmu.write_u32(0x1018, 1).unwrap();
            mmu.write_u32(0x1020, id).unwrap();
            mmu.write_u32(0x1024, delta as u32).unwrap();
            mmu.write_u32(0x1028, kind).unwrap();
        };
        request(&mut mmu, object, 1, 1);
        assert_eq!(ipc.handle_sync_request(session, 0x1000, &mut mmu, &mut gpu, &mut audio, &mut vfs), 0);
        assert_eq!(mmu.read_u32(0x1018).unwrap(), 0);
        request(&mut mmu, object + 1, 1, 1);
        ipc.handle_sync_request(session, 0x1000, &mut mmu, &mut gpu, &mut audio, &mut vfs);
        assert_eq!(mmu.read_u32(0x1018).unwrap(), INVALID_HANDLE);

        let service = ServiceType::Generic("vi:binder-relay".into());
        let valid = parcel(&[0; 16], &[]);
        mmu.write_bytes(0x2000, &valid).unwrap();
        mmu.write_bytes(0x3000, &[0xA5; 64]).unwrap();
        ipc.request.send_buffers = vec![Buffer { addr: 0x2000, size: valid.len() as u64 }];
        ipc.request.recv_buffers = vec![Buffer { addr: 0x3000, size: 64 }];
        ipc.handle_vi_binder(&service, 3, 0x1020, 0x1000, &mut mmu);
        assert_eq!(mmu.read_u32(0x1018).unwrap(), RESULT_UNKNOWN_COMMAND);
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
}
