//! `pctl` (parental controls) in the shape libnx uses it.
//!
//! The console model has no parental-control restrictions configured, which is the default state
//! of a retail unit: restriction checks report "disabled" and permission checks succeed. Command IDs
//! follow SwitchBrew's public `pctl` service page; commands outside this list report unimplemented.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

impl IpcManager {
    /// `IParentalControlServiceFactory` ("pctl", "pctl:a", "pctl:r", "pctl:s").
    pub(super) fn handle_pctl(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // CreateService(u64 pid) / CreateServiceWithoutInitialize(u64 pid) -> IParentalControlService
            0 | 1 => self.reply_object(tls, mmu, ServiceType::PctlService),
            _ => self.reply_unimplemented("pctl", cmd, tls, mmu),
        }
    }

    pub(super) fn handle_pctl_service(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // Initialize
            1 => self.reply_ok(tls, mmu),
            // CheckFreeCommunicationPermission / ConfirmLaunchApplicationPermission /
            // ConfirmResumeApplicationPermission / ConfirmSnsPostPermission: allowed, nothing is restricted
            1001..=1004 => self.reply_ok(tls, mmu),
            // IsRestrictionEnabled -> bool
            1031 => self.reply_data(tls, mmu, &[0]),
            _ => self.reply_unimplemented("pctl IParentalControlService", cmd, tls, mmu),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{Harness, Req};
    use crate::kernel::ipc::RESULT_UNKNOWN_COMMAND;

    #[test]
    fn factory_creates_an_unrestricted_service() {
        let mut h = Harness::new();
        let pctl = h.get_service("pctl");
        assert_eq!(h.ipc.handles.get(&pctl), Some(&ServiceType::Pctl));
        for cmd in [0, 1] {
            let s = h.call(pctl, Req { cmd, data: &[0; 8], pid: true, ..Default::default() }).object();
            assert_eq!(h.ipc.handles.get(&s), Some(&ServiceType::PctlService));
        }
        let s = h.call(pctl, Req { cmd: 1, data: &[0; 8], pid: true, ..Default::default() }).object();
        assert_eq!(h.call(s, Req { cmd: 1, ..Default::default() }).result, 0);
        assert_eq!(h.call_out(s, Req { cmd: 1031, ..Default::default() }, 1).u8(0), 0);
        assert_eq!(h.call(s, Req { cmd: 1001, ..Default::default() }).result, 0);
        assert_eq!(h.call(s, Req { cmd: 9999, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
        assert_eq!(h.call(pctl, Req { cmd: 99, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }
}
