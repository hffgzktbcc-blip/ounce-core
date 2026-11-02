//! `aoc:u` (add-on content manager for applications): no add-on content is installed.
//!
//! Command IDs follow SwitchBrew's public `IAddOnContentManager` page.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

impl IpcManager {
    pub(super) fn handle_aoc(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // CountAddOnContent(pid) -> i32
            2 => self.reply_data(tls, mmu, &0i32.to_le_bytes()),
            // ListAddOnContent(offset, count, pid) -> i32 written; nothing to list
            3 => self.reply_data(tls, mmu, &0i32.to_le_bytes()),
            // GetAddOnContentBaseId(pid) -> u64
            5 => self.reply_data(tls, mmu, &0u64.to_le_bytes()),
            // GetAddOnContentListChangedEvent / ...WithProcessId
            8 | 10 => {
                let ev = self.allocate_handle(ServiceType::ServiceEvent("aoc list changed"));
                self.reply_event(tls, mmu, ev)
            }
            // CheckAddOnContentMountStatus(pid): nothing is installed, so nothing is half-mounted
            50 => self.reply_ok(tls, mmu),
            _ => self.reply_unimplemented("aoc:u", cmd, tls, mmu),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{Harness, Req};
    use crate::kernel::ipc::RESULT_UNKNOWN_COMMAND;

    #[test]
    fn no_add_on_content_is_installed() {
        let mut h = Harness::new();
        let aoc = h.get_service("aoc:u");
        assert_eq!(h.ipc.handles.get(&aoc), Some(&ServiceType::AddOnContent));
        assert_eq!(h.call_out(aoc, Req { cmd: 2, data: &[0; 8], pid: true, ..Default::default() }, 4).u32(0), 0);
        assert_eq!(h.call(aoc, Req { cmd: 50, data: &[0; 8], pid: true, ..Default::default() }).result, 0);
        assert_eq!(h.call(aoc, Req { cmd: 9999, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }
}
