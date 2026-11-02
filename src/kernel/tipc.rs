//! Tipc (tiny IPC) sessions of Switch 2 interfaces.
//!
//! A Tipc request has message type 16 + the command id and raw data right after the 8-byte header; there is
//! no CMIF header. A reply without handles is the header followed by the result at +8. A reply with handles
//! has a special header at +8, the handles after it, and the result as the first raw word after the handles.
//! The command meanings below come from the Nintendo SDK client code in the title's `sdk` module (the
//! calling sequence and which reply words it reads), not from public documentation.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// The unified GPU virtual address space: [128 MiB, 768 GiB).
pub const GPU_VA_START: u64 = 0x0800_0000;
pub const GPU_VA_END: u64 = 0xC0_0000_0000;
/// Name of the GPU memory session (command 22 of the GPU session).
pub const GPU_MEMORY_SESSION: &str = "GPU memory";
/// Concurrent sessions the server reports it supports.
pub const MAX_SESSIONS: u32 = 8;
/// Name of the interface `ISelfController` command 2000 returns.
pub const SELF_CONTROLLER_SESSION: &str = "ISelfController 2000";
/// Name of the session command 202 of that interface returns.
pub const SELF_CONTROLLER_202: &str = "ISelfController 2000 / 202";
/// Name of the session command 497 of that interface returns.
pub const SELF_CONTROLLER_497: &str = "ISelfController 2000 / 497";
/// Name of the session command 305 of that interface returns.
pub const SELF_CONTROLLER_305: &str = "ISelfController 2000 / 305";

/// Memory for a block-linear image whose packed size word holds width - 1 (low 16 bits) and height - 1 (high 16 bits),
/// at 4 bytes per pixel.
pub fn native_image_size(packed: u32) -> u64 {
    let (w, h) = ((packed & 0xFFFF) as u64 + 1, (packed >> 16) as u64 + 1);
    let pitch = (w * 4).div_ceil(64) * 64;
    let rows = h.div_ceil(128) * 128;
    (pitch * rows).div_ceil(0x1_0000) * 0x1_0000
}

impl IpcManager {
    /// Reply with a result only.
    fn tipc_reply_result(&self, tls: u64, mmu: &mut Memory, result: u32) -> u64 {
        let _ = mmu.write_u32(tls, 0);
        let _ = mmu.write_u32(tls + 4, 1);
        let _ = mmu.write_u32(tls + 8, result);
        0
    }

    /// Reply with a success result followed by `words` of output.
    fn tipc_reply_words(&self, tls: u64, mmu: &mut Memory, words: &[u32]) -> u64 {
        let _ = mmu.write_u32(tls, 0);
        let _ = mmu.write_u32(tls + 4, 1 + words.len() as u32);
        let _ = mmu.write_u32(tls + 8, 0);
        for (i, w) in words.iter().enumerate() {
            let _ = mmu.write_u32(tls + 12 + i as u64 * 4, *w);
        }
        0
    }

    /// Reply with a move handle and a success result.
    fn tipc_reply_move_handle(&self, tls: u64, mmu: &mut Memory, handle: u32) -> u64 {
        if std::env::var_os("OUNCE_TRACE_IPC").is_some() {
            eprintln!("[IPC] tipc reply: new handle 0x{handle:X} -> {:?}", self.handles.get(&handle));
        }
        let _ = mmu.write_u32(tls, 0);
        let _ = mmu.write_u32(tls + 4, 0x8000_0001);
        let _ = mmu.write_u32(tls + 8, 0x20); // one move handle
        let _ = mmu.write_u32(tls + 12, handle);
        let _ = mmu.write_u32(tls + 16, 0);
        0
    }

    pub(super) fn handle_tipc(&mut self, service: &ServiceType, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        // The request's words, read before the reply poisons or overwrites the message area.
        let mut raw = [0u8; 48];
        let _ = mmu.read_bytes(tls + 8, &mut raw);
        let words: Vec<u32> = raw.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        super::hipc::poison_reply_area(mmu, tls);
        match (service, cmd) {
            // The client's setup sequence: command 4 carries the process handle, a 64-bit id and the process's
            // remote memory usage and peak; 11 and 101 carry the session handle. Each answers with a result only.
            (ServiceType::Tipc(SELF_CONTROLLER_SESSION), 4 | 11 | 101 | 201) => self.tipc_reply_result(tls, mmu, 0),
            // Command 1402 takes the session handle and returns the GPU driver session (`nvdrv`) as a move handle:
            // its first request is Open("/dev/nvhost-ctrl-gpu").
            (ServiceType::Tipc(SELF_CONTROLLER_SESSION), 1402) => {
                let h = self.allocate_handle(ServiceType::Nvdrv);
                self.tipc_reply_move_handle(tls, mmu, h)
            }
            // Command 202 takes the session handle and returns a handle (move) the client keeps.
            (ServiceType::Tipc(SELF_CONTROLLER_SESSION), 202) => {
                let h = self.allocate_handle(ServiceType::Tipc(SELF_CONTROLLER_202));
                self.tipc_reply_move_handle(tls, mmu, h)
            }
            // Command 0 is a handshake: the client sends a 64-bit token and checks that the reply carries the same one.
            (ServiceType::Tipc(_), 0) => {
                let token = u64::from(words[0]) | u64::from(words[1]) << 32;
                let _ = mmu.write_u32(tls, 0);
                let _ = mmu.write_u32(tls + 4, 3);
                let _ = mmu.write_u32(tls + 8, 0);
                let _ = mmu.write_u64(tls + 12, token);
                0
            }
            // Commands several sessions share: 0 handshake, 1 a u32 below 0x400, 3 an enumeration value below 8.
            // Command 1 returns a u32 the client requires to be below 0x400.
            (ServiceType::Tipc(_), 1) => {
                let _ = mmu.write_u32(tls, 0);
                let _ = mmu.write_u32(tls + 4, 2);
                let _ = mmu.write_u32(tls + 8, 0);
                let _ = mmu.write_u32(tls + 12, 1);
                0
            }
            // Command 115 takes a u64 and returns a u64 the client keeps in a global.
            (ServiceType::Tipc(SELF_CONTROLLER_SESSION), 115) => {
                let _ = mmu.write_u32(tls, 0);
                let _ = mmu.write_u32(tls + 4, 3);
                let _ = mmu.write_u32(tls + 8, 0);
                let _ = mmu.write_u64(tls + 12, 0);
                0
            }
            // Command 497 takes a u64 and returns a session (move handle) the client swaps into one of its objects.
            (ServiceType::Tipc(SELF_CONTROLLER_SESSION), 497) => {
                let h = self.allocate_handle(ServiceType::Tipc(SELF_CONTROLLER_497));
                self.tipc_reply_move_handle(tls, mmu, h)
            }
            // Command 11 returns (count, capacity); the client requires count <= min(capacity, 0x40).
            (ServiceType::Tipc(SELF_CONTROLLER_497), 11) => {
                let _ = mmu.write_u32(tls, 0);
                let _ = mmu.write_u32(tls + 4, 3);
                let _ = mmu.write_u32(tls + 8, 0);
                let _ = mmu.write_u32(tls + 12, 0);
                let _ = mmu.write_u32(tls + 16, 0x40);
                0
            }
            // Command 12 takes a slot index below the capacity and returns one event handle (move) per slot. The
            // client waits on slot events; this model completes every operation at once, so they read as signaled.
            (ServiceType::Tipc(SELF_CONTROLLER_497), 12) => {
                let h = self.allocate_handle(ServiceType::ServiceEvent("tipc slot"));
                self.tipc_reply_move_handle(tls, mmu, h)
            }
            // Command 305 returns a session (move handle) that the client stores in one of its objects.
            (ServiceType::Tipc(SELF_CONTROLLER_SESSION), 305) => {
                let h = self.allocate_handle(ServiceType::Tipc(SELF_CONTROLLER_305));
                self.tipc_reply_move_handle(tls, mmu, h)
            }
            // Command 3 returns a u32 the client requires to be below 8 (an enumeration value).
            (ServiceType::Tipc(_), 3) => {
                let _ = mmu.write_u32(tls, 0);
                let _ = mmu.write_u32(tls + 4, 2);
                let _ = mmu.write_u32(tls + 8, 0);
                let _ = mmu.write_u32(tls + 12, 0);
                0
            }
            // On this session command 11 returns an event (move handle) the client polls with a zero timeout.
            (ServiceType::Tipc(SELF_CONTROLLER_305), 11) => {
                let h = self.allocate_handle(ServiceType::ServiceEvent("tipc event"));
                self.tipc_reply_move_handle(tls, mmu, h)
            }
            // Command 5 returns the session limit minus one (the client requires it below 8 and sizes a pool of
            // that many sessions from it; with fewer than 8 slots its second session allocation fails).
            (ServiceType::Tipc(SELF_CONTROLLER_305), 5) => {
                let _ = mmu.write_u32(tls, 0);
                let _ = mmu.write_u32(tls + 4, 2);
                let _ = mmu.write_u32(tls + 8, 0);
                let _ = mmu.write_u32(tls + 12, MAX_SESSIONS - 1);
                0
            }
            // Command 212 takes a name-sized argument block and returns a u64 the client stores.
            (ServiceType::Tipc(SELF_CONTROLLER_305), 212) => {
                let _ = mmu.write_u32(tls, 0);
                let _ = mmu.write_u32(tls + 4, 3);
                let _ = mmu.write_u32(tls + 8, 0);
                let _ = mmu.write_u64(tls + 12, 0);
                0
            }
            // Command 22 returns the GPU memory session (move handle): the client sends it memory commands
            // (101 create GPU-accessible memory, 211 map, ...).
            (ServiceType::Tipc(SELF_CONTROLLER_497), 22) => {
                let h = self.allocate_handle(ServiceType::Tipc(GPU_MEMORY_SESSION));
                self.tipc_reply_move_handle(tls, mmu, h)
            }
            // GPU memory session: the client builds a table of up to 8 partitions. Command 33 creates one and returns its id;
            // 201 and 202 return two (u64, u64) ranges for it. Here one partition covers the unified GPU VA space
            // [128 MiB, 768 GiB) (matching the address space reported by `nvdrv`).
            (ServiceType::Tipc(SELF_CONTROLLER_497), 33) => {
                self.gpu_partitions += 1;
                let id = self.gpu_partitions;
                self.tipc_reply_words(tls, mmu, &[id])
            }
            (ServiceType::Tipc(SELF_CONTROLLER_497), 201) => {
                let (start, size) = (GPU_VA_START, GPU_VA_END - GPU_VA_START);
                self.tipc_reply_words(tls, mmu, &[start as u32, (start >> 32) as u32, size as u32, (size >> 32) as u32])
            }
            (ServiceType::Tipc(SELF_CONTROLLER_497), 202) => self.tipc_reply_words(tls, mmu, &[0, 0, 0, 0]),
            // GPU memory session. 101 creates a GPU-visible memory object for a CPU range (u64 first, u64 last address, ...)
            // and returns its id; 211 maps an object; 102 releases one. CPU and GPU share memory in this model, so the
            // object only needs an id.
            (ServiceType::Tipc(GPU_MEMORY_SESSION), 101) => {
                self.gpu_memory_objects += 1;
                let id = self.gpu_memory_objects;
                self.tipc_reply_words(tls, mmu, &[id])
            }
            (ServiceType::Tipc(GPU_MEMORY_SESSION), 211 | 102) => self.tipc_reply_result(tls, mmu, 0),
            // Command 435 takes a 64-byte image description (word 2 packs width - 1 and height - 1) and returns the memory
            // the image needs as a u64: rows of 4-byte pixels padded to 64 bytes, height padded to a 128-row block, the
            // total rounded up to 64 KiB.
            (ServiceType::Tipc(SELF_CONTROLLER_SESSION), 435) => {
                let size = native_image_size(words[2]);
                self.tipc_reply_words(tls, mmu, &[size as u32, (size >> 32) as u32])
            }
            _ => {
                if self.reported_tipc.insert((format!("{service:?}"), cmd)) {
                    eprintln!("[HIPC] tipc command {cmd} on {service:?} not modelled (replied with success, no output); words after header: {words:08X?}");
                }
                self.tipc_reply_result(tls, mmu, 0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_image_size_pads_rows_and_height() {
        // 2880 x 1620: pitch 11520 bytes, 1664 rows, rounded up to 64 KiB.
        assert_eq!(native_image_size(0x0653_0B3F), (11520u64 * 1664).div_ceil(0x1_0000) * 0x1_0000);
        assert_eq!(native_image_size(0), 0x1_0000);
    }
}
