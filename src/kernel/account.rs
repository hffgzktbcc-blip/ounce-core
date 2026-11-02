//! `acc:u0` / `acc:u1` (account service for applications): one local user, no network account.
//!
//! Command IDs follow SwitchBrew's public `IAccountServiceForApplication` page. Values are synthetic: a
//! single user whose 128-bit ID is `USER_ID`.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// The single user's 128-bit ID (little-endian halves).
pub const USER_ID: [u8; 16] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

impl IpcManager {
    pub(super) fn handle_account(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // GetUserCount -> i32
            0 => self.reply_data(tls, mmu, &1i32.to_le_bytes()),
            // GetUserExistence(uid) -> bool
            1 => self.reply_data(tls, mmu, &[1]),
            // ListAllUsers / ListOpenUsers -> array of up to 8 user IDs in the output buffer
            2 | 3 => {
                let mut ids = [0u8; 8 * 16];
                ids[..16].copy_from_slice(&USER_ID);
                self.write_out_buffer(mmu, &ids);
                self.reply_ok(tls, mmu)
            }
            // GetLastOpenedUser / TrySelectUserWithoutInteraction -> uid
            4 | 51 => self.reply_data(tls, mmu, &USER_ID),
            // IsUserRegistrationRequestPermitted(pid) -> bool
            50 => self.reply_data(tls, mmu, &[0]),
            // InitializeApplicationInfo(pid) / InitializeApplicationInfoV2(pid)
            140 | 160 => self.reply_ok(tls, mmu),
            // GetUserRegistrationNotifier / GetUserStateChangeNotifier -> INotifier: an event nothing signals
            100 | 101 => {
                let ev = self.allocate_handle(ServiceType::ServiceEvent("account notifier"));
                self.reply_event(tls, mmu, ev)
            }
            // IsUserAccountSwitchLocked -> bool
            150 => self.reply_data(tls, mmu, &[0]),
            _ => self.reply_unimplemented("acc:u0", cmd, tls, mmu),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{Harness, Req};
    use crate::kernel::ipc::RESULT_UNKNOWN_COMMAND;

    #[test]
    fn single_local_user() {
        let mut h = Harness::new();
        let acc = h.get_service("acc:u0");
        assert_eq!(h.ipc.handles.get(&acc), Some(&ServiceType::Account));
        assert_eq!(h.call_out(acc, Req { cmd: 0, ..Default::default() }, 4).u32(0), 1);
        assert_eq!(h.call_out(acc, Req { cmd: 4, ..Default::default() }, 16).data[..16], USER_ID);
        assert_eq!(h.call(acc, Req { cmd: 160, data: &[0; 8], pid: true, ..Default::default() }).result, 0);
        assert_eq!(h.call(acc, Req { cmd: 9999, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }
}
