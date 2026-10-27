#![allow(dead_code)]

//! `nvdrv` device model for libnx-shaped requests: device files opened by path, and ioctls whose
//! argument block arrives in an X (or A) buffer and goes back through a C (or B) buffer.
//! Ioctl numbers and argument layouts follow the public Linux Tegra `nvmap` UAPI that the Switch
//! driver keeps (`_IOWR('\x01', nr, struct)`).

use std::collections::HashMap;

/// nvdrv status codes (libnx `NvError`).
pub const NV_OK: u32 = 0;
pub const NV_NOT_IMPLEMENTED: u32 = 1;
pub const NV_BAD_PARAMETER: u32 = 4;

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
        Self { next_fd: 1, next_handle: 1, next_id: 0x1001, ..Default::default() }
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
        if self.fds.remove(&fd).is_some() { NV_OK } else { NV_BAD_PARAMETER }
    }

    /// Runs an ioctl on `fd`. `data` is the argument block, updated in place.
    pub fn ioctl(&mut self, fd: u32, request: u32, data: &mut [u8]) -> u32 {
        let Some(path) = self.fds.get(&fd).cloned() else {
            return NV_BAD_PARAMETER;
        };
        if path == "/dev/nvmap" {
            if let Some(status) = self.nvmap_ioctl(request, data) {
                return status;
            }
        }
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
    fn ioctl_fields() {
        assert_eq!(ioctl_size(NVMAP_IOC_ALLOC), 0x20);
        assert!(ioctl_has_input(NVMAP_IOC_CREATE) && ioctl_has_output(NVMAP_IOC_CREATE));
    }
}
