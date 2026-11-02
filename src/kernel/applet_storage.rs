//! Applet `IStorage` / `IStorageAccessor` holding launch parameters.
//!
//! `PopLaunchParameter(kind)` returns an `IStorage`; clients `Open` it for an `IStorageAccessor` and
//! `Read` the bytes. Command IDs follow libnx's public `applet.c`. The preselected-user parameter
//! (kind 2) tells the title which account the launcher already picked: here the one local user.

use super::account::USER_ID;
use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// `AppletLaunchParameterKind_PreselectedUser`.
pub const LAUNCH_PARAMETER_PRESELECTED_USER: u32 = 2;
/// Magic at the start of the preselected-user parameter.
const PRESELECTED_USER_MAGIC: u32 = 0xC794_97CA;
/// Size of the preselected-user parameter.
const PRESELECTED_USER_SIZE: usize = 0x88;

/// The preselected-user launch parameter: magic, "account selected" flag, then the user ID at +0x10.
pub fn preselected_user_parameter() -> Vec<u8> {
    let mut data = vec![0u8; PRESELECTED_USER_SIZE];
    data[0..4].copy_from_slice(&PRESELECTED_USER_MAGIC.to_le_bytes());
    data[4] = 1;
    data[0x10..0x20].copy_from_slice(&USER_ID);
    data
}

impl IpcManager {
    /// `IStorage` commands.
    pub(super) fn handle_applet_storage(&mut self, data: &[u8], cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // Open -> IStorageAccessor
            0 => self.reply_object(tls, mmu, ServiceType::AppletStorageAccessor(data.to_vec())),
            _ => self.reply_unimplemented("applet IStorage", cmd, tls, mmu),
        }
    }

    /// `IStorageAccessor` commands.
    pub(super) fn handle_applet_storage_accessor(&mut self, data: &[u8], cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // GetSize -> s64
            0 => self.reply_data(tls, mmu, &(data.len() as i64).to_le_bytes()),
            // Read(s64 offset), data into the B buffer
            11 => {
                let offset = mmu.read_u64(args).unwrap_or(0) as usize;
                let cap = self.request.recv_buffers.iter().chain(&self.request.recv_list).map(|b| b.size as usize).find(|&n| n > 0).unwrap_or(0);
                if offset > data.len() {
                    return self.reply_error(tls, mmu, super::applet::RESULT_OUT_OF_BOUNDS);
                }
                let end = data.len().min(offset + cap);
                self.write_out_buffer(mmu, &data[offset..end]);
                self.reply_ok(tls, mmu)
            }
            _ => self.reply_unimplemented("applet IStorageAccessor", cmd, tls, mmu),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preselected_user_parameter_names_the_local_user() {
        let p = preselected_user_parameter();
        assert_eq!(p.len(), 0x88);
        assert_eq!(u32::from_le_bytes(p[0..4].try_into().unwrap()), 0xC794_97CA);
        assert_eq!(p[4], 1);
        assert_eq!(p[0x10..0x20], USER_ID);
    }
}
