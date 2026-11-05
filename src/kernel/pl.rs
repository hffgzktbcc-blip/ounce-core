//! `pl:u` (`nn::pl::detail::IPlatformServiceManager`): the shared font.
//!
//! libnx requests every font type to load, then reads the list of fonts with command 5 and
//! addresses them inside the shared memory block (`plGetSharedmemAddr() + offset`). Real firmware
//! serves Nintendo's fonts; this serves one bundled open-licensed font (`assets/fonts`) as the
//! standard font.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// Size libnx maps the shared memory block with.
const SHARED_MEMORY_SIZE: u64 = 0x110_0000;
/// Where the font sits inside the shared memory block.
const FONT_OFFSET: u32 = 0x1000;
/// `PlSharedFontType_Standard`.
const FONT_TYPE_STANDARD: u32 = 0;
/// Load state libnx waits for.
const LOAD_STATE_LOADED: u32 = 1;

static FONT: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");

impl IpcManager {
    /// Writes the font into the shared memory block once the guest maps it at `addr`.
    pub(super) fn pl_shared_memory_mapped(&mut self, handle: u32, addr: u64, mmu: &mut Memory) {
        if self.pl_shm_handle == Some(handle) {
            let _ = mmu.write_bytes(addr + FONT_OFFSET as u64, FONT);
        }
    }

    pub(super) fn handle_pl(&mut self, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // RequestLoad(type)
            0 => self.reply(tls, mmu, 0, &[], &[], None),
            // GetLoadState(type) -> state
            1 => self.write_cmif_response(tls, mmu, 0, &[LOAD_STATE_LOADED], None),
            // GetSize(type) -> size
            2 => self.write_cmif_response(tls, mmu, 0, &[FONT.len() as u32], None),
            // GetSharedMemoryAddressOffset(type) -> offset
            3 => self.write_cmif_response(tls, mmu, 0, &[FONT_OFFSET], None),
            // GetSharedMemoryNativeHandle -> copy handle
            4 => {
                let h = match self.pl_shm_handle {
                    Some(h) => h,
                    None => {
                        let h = self.allocate_handle(ServiceType::SharedMemory { size: SHARED_MEMORY_SIZE });
                        self.pl_shm_handle = Some(h);
                        h
                    }
                };
                return self.reply_event(tls, mmu, h);
            }
            // GetSharedFontInOrderOfPriority(language code) -> (loaded, count), types[], offsets[], sizes[]
            5 => {
                let _language = mmu.read_u64(args).unwrap_or(0);
                let arrays = [FONT_TYPE_STANDARD, FONT_OFFSET, FONT.len() as u32];
                for (i, value) in arrays.into_iter().enumerate() {
                    if let Some(buf) = self.request.recv_buffers.get(i).copied() {
                        let _ = mmu.write_bytes(buf.addr, &value.to_le_bytes()[..(buf.size as usize).min(4)]);
                    }
                }
                self.write_cmif_response(tls, mmu, 0, &[1, 1], None);
            }
            _ => return self.reply_unimplemented("pl:u", cmd, tls, mmu),
        }
        0
    }
}
