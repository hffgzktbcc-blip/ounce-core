#![allow(dead_code)]

//! HIPC message layout, as written by libnx and documented publicly (switchbrew "IPC Marshalling"):
//! a two-word header, an optional special header (PID, copy and move handles), static (X), buffer
//! (A/B/W) descriptors, raw data words, and a receive list (C). CMIF payloads sit in the raw data,
//! aligned to 16 bytes from the start of the message.

use super::ipc::CMIF_OUT_MAGIC;
use crate::mmu::Memory;

/// HIPC message types in header word 0 (low 16 bits).
pub const TYPE_CLOSE: u16 = 2;
pub const TYPE_REQUEST: u16 = 4;
pub const TYPE_CONTROL: u16 = 5;
pub const TYPE_REQUEST_WITH_CONTEXT: u16 = 6;
pub const TYPE_CONTROL_WITH_CONTEXT: u16 = 7;

/// A guest buffer named by a descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Buffer {
    pub addr: u64,
    pub size: u64,
}

/// A parsed request. Offsets are from the start of the message (the thread's TLS).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    pub kind: u16,
    pub pid: Option<u64>,
    pub copy_handles: Vec<u32>,
    pub move_handles: Vec<u32>,
    /// Send statics (X): `(index, buffer)`.
    pub send_statics: Vec<(u8, Buffer)>,
    pub send_buffers: Vec<Buffer>,
    pub recv_buffers: Vec<Buffer>,
    pub exch_buffers: Vec<Buffer>,
    /// Receive list (C) entries.
    pub recv_list: Vec<Buffer>,
    pub data_offset: u64,
    pub num_data_words: u32,
    /// Start of the CMIF payload: `data_offset` aligned up to 16.
    pub cmif_offset: u64,
}

fn align16(v: u64) -> u64 {
    (v + 15) & !15
}

/// Parses the HIPC message at `base`. Unmapped memory reads as zero, which parses as an empty request.
pub fn parse_request(mmu: &Memory, base: u64) -> Request {
    let rd = |off: u64| mmu.read_u32(base + off).unwrap_or(0);
    let w0 = rd(0);
    let w1 = rd(4);
    let mut req = Request {
        kind: (w0 & 0xFFFF) as u16,
        num_data_words: w1 & 0x3FF,
        ..Default::default()
    };
    let num_statics = (w0 >> 16) & 0xF;
    let num_send = (w0 >> 20) & 0xF;
    let num_recv = (w0 >> 24) & 0xF;
    let num_exch = (w0 >> 28) & 0xF;
    let recv_static_mode = (w1 >> 10) & 0xF;
    let recv_list_offset = ((w1 >> 20) & 0x7FF) as u64;
    let has_special = (w1 >> 31) != 0;

    let mut off = 8u64;
    if has_special {
        let sh = rd(off);
        off += 4;
        if sh & 1 != 0 {
            req.pid = Some(mmu.read_u64(base + off).unwrap_or(0));
            off += 8;
        }
        for _ in 0..((sh >> 1) & 0xF) {
            req.copy_handles.push(rd(off));
            off += 4;
        }
        for _ in 0..((sh >> 5) & 0xF) {
            req.move_handles.push(rd(off));
            off += 4;
        }
    }
    for _ in 0..num_statics {
        let (a, b) = (rd(off), rd(off + 4));
        let addr = b as u64 | (((a >> 12) & 0xF) as u64) << 32 | (((a >> 6) & 0x3F) as u64) << 36;
        req.send_statics.push(((a & 0x3F) as u8, Buffer { addr, size: (a >> 16) as u64 }));
        off += 8;
    }
    let buffers = |n: u32, off: &mut u64| {
        (0..n)
            .map(|_| {
                let (size_lo, addr_lo, c) = (rd(*off), rd(*off + 4), rd(*off + 8));
                *off += 12;
                let addr = addr_lo as u64 | (((c >> 28) & 0xF) as u64) << 32 | (((c >> 2) & 0x3F_FFFF) as u64) << 36;
                let size = size_lo as u64 | (((c >> 24) & 0xF) as u64) << 32;
                Buffer { addr, size }
            })
            .collect::<Vec<_>>()
    };
    req.send_buffers = buffers(num_send, &mut off);
    req.recv_buffers = buffers(num_recv, &mut off);
    req.exch_buffers = buffers(num_exch, &mut off);
    req.data_offset = off;
    req.cmif_offset = align16(off);

    // Receive list: mode 2 = one entry, mode n > 2 = n - 2 entries.
    let num_recv_list = match recv_static_mode {
        0 | 1 => 0,
        2 => 1,
        n => n - 2,
    };
    let mut roff = if recv_list_offset != 0 { recv_list_offset * 4 } else { off + req.num_data_words as u64 * 4 };
    for _ in 0..num_recv_list {
        let (lo, hi) = (rd(roff), rd(roff + 4));
        req.recv_list.push(Buffer { addr: lo as u64 | ((hi & 0xFFFF) as u64) << 32, size: (hi >> 16) as u64 });
        roff += 8;
    }
    req
}

/// A response to write back into the message buffer.
#[derive(Debug, Default, Clone)]
pub struct Response<'a> {
    pub result: u32,
    /// Raw CMIF output data after the `SFCO` header.
    pub data: &'a [u8],
    pub copy_handles: &'a [u32],
    pub move_handles: &'a [u32],
    /// For a domain session: the out object IDs. Adds the domain out header and appends the IDs after `data`.
    pub domain_objects: Option<&'a [u32]>,
}

/// Domain request header that precedes the CMIF payload on a domain session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainHeader {
    /// 1 = SendMessage, 2 = Close.
    pub kind: u8,
    pub num_in_objects: u8,
    pub data_size: u16,
    pub object_id: u32,
}

pub const DOMAIN_SEND_MESSAGE: u8 = 1;
pub const DOMAIN_CLOSE: u8 = 2;

/// Reads the domain header at `addr` (the aligned data start of a domain request).
pub fn read_domain_header(mmu: &Memory, addr: u64) -> DomainHeader {
    let w0 = mmu.read_u32(addr).unwrap_or(0);
    DomainHeader {
        kind: w0 as u8,
        num_in_objects: (w0 >> 8) as u8,
        data_size: (w0 >> 16) as u16,
        object_id: mmu.read_u32(addr + 4).unwrap_or(0),
    }
}

/// Writes a CMIF response at `base`. Returns the offset of the `SFCO` header.
pub fn write_response(mmu: &mut Memory, base: u64, resp: &Response) -> u64 {
    let special = !resp.copy_handles.is_empty() || !resp.move_handles.is_empty();
    let mut off = 8u64;
    if special {
        let sh = (resp.copy_handles.len() as u32 & 0xF) << 1 | (resp.move_handles.len() as u32 & 0xF) << 5;
        let _ = mmu.write_u32(base + off, sh);
        off += 4;
        for &h in resp.copy_handles.iter().chain(resp.move_handles) {
            let _ = mmu.write_u32(base + off, h);
            off += 4;
        }
    }
    let start = align16(off);
    let objects = resp.domain_objects.unwrap_or(&[]);
    let cmif = if let Some(objs) = resp.domain_objects {
        let _ = mmu.write_u32(base + start, objs.len() as u32);
        for i in 1..4 {
            let _ = mmu.write_u32(base + start + i * 4, 0);
        }
        start + 16
    } else {
        start
    };
    // Data words cover the alignment padding, the domain and CMIF headers, the output data and object IDs.
    let num_data_words = ((cmif - off) + 16 + resp.data.len() as u64 + objects.len() as u64 * 4).div_ceil(4) as u32;
    let _ = mmu.write_u32(base, 0);
    let _ = mmu.write_u32(base + 4, num_data_words | (special as u32) << 31);
    let _ = mmu.write_u32(base + cmif, CMIF_OUT_MAGIC);
    let _ = mmu.write_u32(base + cmif + 4, 0);
    let _ = mmu.write_u32(base + cmif + 8, resp.result);
    let _ = mmu.write_u32(base + cmif + 12, 0);
    let _ = mmu.write_bytes(base + cmif + 16, resp.data);
    for (i, &id) in objects.iter().enumerate() {
        let _ = mmu.write_u32(base + cmif + 16 + resp.data.len() as u64 + i as u64 * 4, id);
    }
    cmif
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    const TLS: u64 = 0x4000_0000;

    fn tls() -> Memory {
        let mut mmu = Memory::new();
        mmu.map_range(TLS, 0x1000, Permission::READ_WRITE);
        mmu
    }

    #[test]
    fn zero_header_puts_cmif_at_0x10() {
        let mmu = tls();
        let req = parse_request(&mmu, TLS);
        assert_eq!(req.cmif_offset, 0x10);
        assert!(req.pid.is_none() && req.copy_handles.is_empty());
    }

    #[test]
    fn pid_handles_and_buffers() {
        let mut mmu = tls();
        // Request, 1 send static, 1 send buffer, 1 recv buffer; special header; 8 data words.
        mmu.write_u32(TLS, 4 | 1 << 16 | 1 << 20 | 1 << 24).unwrap();
        mmu.write_u32(TLS + 4, 8 | 1 << 31).unwrap();
        mmu.write_u32(TLS + 8, 1 | 1 << 1 | 1 << 5).unwrap(); // pid, 1 copy, 1 move
        mmu.write_u64(TLS + 12, 0x52).unwrap();
        mmu.write_u32(TLS + 20, 0xC0).unwrap();
        mmu.write_u32(TLS + 24, 0xD0).unwrap();
        // X: index 3, size 0x40, address 0x12_3456_7000 (mid = 2, high = 1)
        mmu.write_u32(TLS + 28, 3 | 1 << 6 | 2 << 12 | 0x40 << 16).unwrap();
        mmu.write_u32(TLS + 32, 0x3456_7000).unwrap();
        // A: size 0x1_0000_0100, address 0x5_8000_0000
        mmu.write_u32(TLS + 36, 0x100).unwrap();
        mmu.write_u32(TLS + 40, 0x8000_0000).unwrap();
        mmu.write_u32(TLS + 44, 1 << 24 | 5 << 28).unwrap();
        // B: size 0x20, address 0x9000
        mmu.write_u32(TLS + 48, 0x20).unwrap();
        mmu.write_u32(TLS + 52, 0x9000).unwrap();
        mmu.write_u32(TLS + 56, 0).unwrap();

        let req = parse_request(&mmu, TLS);
        assert_eq!(req.kind, TYPE_REQUEST);
        assert_eq!(req.pid, Some(0x52));
        assert_eq!((req.copy_handles.as_slice(), req.move_handles.as_slice()), (&[0xC0][..], &[0xD0][..]));
        assert_eq!(req.send_statics, vec![(3, Buffer { addr: 0x12_3456_7000, size: 0x40 })]);
        assert_eq!(req.send_buffers, vec![Buffer { addr: 0x5_8000_0000, size: 0x1_0000_0100 }]);
        assert_eq!(req.recv_buffers, vec![Buffer { addr: 0x9000, size: 0x20 }]);
        assert_eq!(req.data_offset, 60);
        assert_eq!(req.cmif_offset, 64);
    }

    #[test]
    fn response_with_copy_and_move_handles() {
        let mut mmu = tls();
        let cmif = write_response(
            &mut mmu,
            TLS,
            &Response { result: 0, data: &7u32.to_le_bytes(), copy_handles: &[0x11], move_handles: &[0x22], ..Default::default() },
        );
        assert_eq!(cmif, 0x20); // 8 header + 4 special + 8 handles = 20 -> 32
        assert_eq!(mmu.read_u32(TLS + 4).unwrap() >> 31, 1);
        assert_eq!(mmu.read_u32(TLS + 8).unwrap(), 1 << 1 | 1 << 5);
        assert_eq!(mmu.read_u32(TLS + 12).unwrap(), 0x11);
        assert_eq!(mmu.read_u32(TLS + 16).unwrap(), 0x22);
        assert_eq!(mmu.read_u32(TLS + 0x20).unwrap(), CMIF_OUT_MAGIC);
        assert_eq!(mmu.read_u32(TLS + 0x30).unwrap(), 7);
        // A plain response keeps the old fixed layout: SFCO at 0x10, data at 0x20.
        let cmif = write_response(&mut mmu, TLS, &Response { move_handles: &[0x33], ..Default::default() });
        assert_eq!(cmif, 0x10);
    }

    #[test]
    fn domain_response_layout() {
        let mut mmu = tls();
        let cmif = write_response(
            &mut mmu,
            TLS,
            &Response { data: &[1, 0, 0, 0], domain_objects: Some(&[0x55]), ..Default::default() },
        );
        assert_eq!(cmif, 0x20); // domain out header at 0x10, SFCO at 0x20
        assert_eq!(mmu.read_u32(TLS + 0x10).unwrap(), 1);
        assert_eq!(mmu.read_u32(TLS + 0x20).unwrap(), CMIF_OUT_MAGIC);
        assert_eq!(mmu.read_u32(TLS + 0x30).unwrap(), 1);
        assert_eq!(mmu.read_u32(TLS + 0x34).unwrap(), 0x55); // object ID right after the data
        mmu.write_u32(TLS + 0x40, DOMAIN_CLOSE as u32 | 0x10 << 16).unwrap();
        mmu.write_u32(TLS + 0x44, 9).unwrap();
        let h = read_domain_header(&mmu, TLS + 0x40);
        assert_eq!((h.kind, h.data_size, h.object_id), (DOMAIN_CLOSE, 0x10, 9));
    }
}
