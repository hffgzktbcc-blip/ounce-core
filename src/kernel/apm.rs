//! `apm` (performance management) and `psm` (power supply) in the shape libnx uses them.
//!
//! Values are synthetic: a handheld unit at 100% battery with enough power supplied. Command IDs
//! follow libnx's public `apm.c` / `psm.c`.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// Synthetic performance configuration IDs for normal (handheld) and boost (docked) mode.
pub const CONFIG_NORMAL: u32 = 0x0001_0000;
pub const CONFIG_BOOST: u32 = 0x0002_0000;

/// `PsmChargerType_EnoughPower`.
pub const CHARGER_ENOUGH_POWER: u32 = 1;
/// Synthetic battery level.
pub const BATTERY_PERCENT: u32 = 100;

/// `apm` state: the configuration the guest selected per performance mode.
#[derive(Debug, Clone)]
pub struct ApmState {
    configs: [u32; 2],
    pub cpu_overclock: bool,
}

impl Default for ApmState {
    fn default() -> Self {
        Self { configs: [CONFIG_NORMAL, CONFIG_BOOST], cpu_overclock: false }
    }
}

impl ApmState {
    /// The configuration for `mode` (0 normal, 1 boost). Other modes read as normal.
    pub fn configuration(&self, mode: u32) -> u32 {
        self.configs.get(mode as usize).copied().unwrap_or(self.configs[0])
    }

    fn set_configuration(&mut self, mode: u32, config: u32) -> bool {
        match self.configs.get_mut(mode as usize) {
            Some(c) => {
                *c = config;
                true
            }
            None => false,
        }
    }
}

impl IpcManager {
    pub(super) fn handle_apm(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // OpenSession -> ISession
            0 => self.reply_object(tls, mmu, ServiceType::ApmSession),
            // GetPerformanceMode -> u32
            1 => self.reply_data(tls, mmu, &self.applet.performance_mode.to_le_bytes()),
            // IsCpuOverclockEnabled -> bool
            6 => self.reply_data(tls, mmu, &[self.apm.cpu_overclock as u8]),
            _ => self.reply_unimplemented("apm", cmd, tls, mmu),
        }
    }

    pub(super) fn handle_apm_session(&mut self, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        let (a, b) = (mmu.read_u32(args).unwrap_or(0), mmu.read_u32(args + 4).unwrap_or(0));
        match cmd {
            // SetPerformanceConfiguration(mode, config)
            0 if self.apm.set_configuration(a, b) => self.reply_ok(tls, mmu),
            0 => self.reply_error(tls, mmu, RESULT_INVALID_PERFORMANCE_MODE),
            // GetPerformanceConfiguration(mode) -> u32
            1 if a < 2 => self.reply_data(tls, mmu, &self.apm.configuration(a).to_le_bytes()),
            1 => self.reply_error(tls, mmu, RESULT_INVALID_PERFORMANCE_MODE),
            // SetCpuOverclockEnabled(bool)
            2 => {
                self.apm.cpu_overclock = a & 0xFF != 0;
                self.reply_ok(tls, mmu)
            }
            _ => self.reply_unimplemented("apm ISession", cmd, tls, mmu),
        }
    }

    pub(super) fn handle_psm(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // GetBatteryChargePercentage -> u32
            0 => self.reply_data(tls, mmu, &BATTERY_PERCENT.to_le_bytes()),
            // GetChargerType -> u32
            1 => self.reply_data(tls, mmu, &CHARGER_ENOUGH_POWER.to_le_bytes()),
            // Enable / DisableBatteryCharging, Acquire / ReleaseControllerPowerSupply,
            // Enable / DisableEnoughPowerChargeEmulation, Enable / DisableFastBatteryCharging
            2 | 3 | 5 | 6 | 8..=11 => self.reply_ok(tls, mmu),
            // IsBatteryChargingEnabled -> bool
            4 => self.reply_data(tls, mmu, &[0]),
            // OpenSession -> IPsmSession
            7 => self.reply_object(tls, mmu, ServiceType::PsmSession),
            // GetBatteryVoltageState -> u32 (good)
            12 => self.reply_data(tls, mmu, &2u32.to_le_bytes()),
            // GetRawBatteryChargePercentage / GetBatteryAgePercentage -> f64
            13 => self.reply_data(tls, mmu, &(BATTERY_PERCENT as f64).to_le_bytes()),
            15 => self.reply_data(tls, mmu, &100f64.to_le_bytes()),
            // IsEnoughPowerSupplied -> bool
            14 => self.reply_data(tls, mmu, &[1]),
            // GetBatteryChargeInfoEvent / GetBatteryChargeCalibratedEvent
            16 | 18 => {
                let ev = self.allocate_handle(ServiceType::ServiceEvent("PsmBatteryChargeInfo"));
                self.reply_event(tls, mmu, ev)
            }
            _ => self.reply_unimplemented("psm", cmd, tls, mmu),
        }
    }

    pub(super) fn handle_psm_session(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // BindStateChangeEvent
            0 => {
                let ev = self.allocate_handle(ServiceType::ServiceEvent("PsmStateChange"));
                self.reply_event(tls, mmu, ev)
            }
            // UnbindStateChangeEvent / SetChargerTypeChangeEventEnabled / SetPowerSupplyChangeEventEnabled /
            // SetBatteryVoltageStateChangeEventEnabled
            1..=4 => self.reply_ok(tls, mmu),
            _ => self.reply_unimplemented("psm ISession", cmd, tls, mmu),
        }
    }
}

/// `apm` ResultInvalidParameter-style failure for an unknown performance mode (module 148, desc 2).
pub const RESULT_INVALID_PERFORMANCE_MODE: u32 = 2 << 9 | 148;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{Harness, Req};
    use crate::kernel::ipc::RESULT_UNKNOWN_COMMAND;

    fn words(a: u32, b: u32) -> Vec<u8> {
        [a.to_le_bytes(), b.to_le_bytes()].concat()
    }

    #[test]
    fn apm_root_and_session() {
        let mut h = Harness::new();
        let apm = h.get_service("apm");
        assert_eq!(h.ipc.handles.get(&apm), Some(&ServiceType::Apm));
        assert_eq!(h.call_out(apm, Req { cmd: 1, ..Default::default() }, 4).u32(0), 0);
        assert_eq!(h.call_out(apm, Req { cmd: 6, ..Default::default() }, 1).u8(0), 0);
        let s = h.call(apm, Req { cmd: 0, ..Default::default() }).object();
        assert_eq!(h.ipc.handles.get(&s), Some(&ServiceType::ApmSession));

        // Defaults, then libnx's appletInitialize apmSetPerformanceConfiguration(mode, cfg).
        assert_eq!(h.call_out(s, Req { cmd: 1, data: &words(0, 0), ..Default::default() }, 4).u32(0), CONFIG_NORMAL);
        assert_eq!(h.call_out(s, Req { cmd: 1, data: &words(1, 0), ..Default::default() }, 4).u32(0), CONFIG_BOOST);
        assert_eq!(h.call(s, Req { cmd: 0, data: &words(1, 0x9222_0009), ..Default::default() }).result, 0);
        assert_eq!(h.call_out(s, Req { cmd: 1, data: &words(1, 0), ..Default::default() }, 4).u32(0), 0x9222_0009);
        // The same configuration shows through ICommonStateGetter after a mode switch.
        assert_eq!(h.ipc.apm.configuration(1), 0x9222_0009);

        // Bad mode: error, not success.
        assert_eq!(h.call(s, Req { cmd: 0, data: &words(5, 1), ..Default::default() }).result, RESULT_INVALID_PERFORMANCE_MODE);
        assert_eq!(h.call_out(s, Req { cmd: 1, data: &words(5, 0), ..Default::default() }, 4).result, RESULT_INVALID_PERFORMANCE_MODE);

        assert_eq!(h.call(s, Req { cmd: 2, data: &words(1, 0), ..Default::default() }).result, 0);
        assert!(h.ipc.apm.cpu_overclock);
        assert_eq!(h.call_out(apm, Req { cmd: 6, ..Default::default() }, 1).u8(0), 1);
        assert_eq!(h.call(s, Req { cmd: 77, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
        assert_eq!(h.call(apm, Req { cmd: 77, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);

        // Boost mode reports mode 1.
        h.ipc.applet.set_operation_mode(crate::kernel::applet::OPERATION_MODE_CONSOLE);
        assert_eq!(h.call_out(apm, Req { cmd: 1, ..Default::default() }, 4).u32(0), 1);
    }

    #[test]
    fn psm_battery_and_session() {
        let mut h = Harness::new();
        let psm = h.get_service("psm");
        assert_eq!(h.ipc.handles.get(&psm), Some(&ServiceType::Psm));
        assert_eq!(h.call_out(psm, Req { cmd: 0, ..Default::default() }, 4).u32(0), 100);
        assert_eq!(h.call_out(psm, Req { cmd: 1, ..Default::default() }, 4).u32(0), CHARGER_ENOUGH_POWER);
        assert_eq!(h.call_out(psm, Req { cmd: 4, ..Default::default() }, 1).u8(0), 0);
        assert_eq!(h.call_out(psm, Req { cmd: 12, ..Default::default() }, 4).u32(0), 2);
        let raw = h.call_out(psm, Req { cmd: 13, ..Default::default() }, 8);
        assert_eq!(f64::from_le_bytes(raw.data[..8].try_into().unwrap()), 100.0);
        let age = h.call_out(psm, Req { cmd: 15, ..Default::default() }, 8);
        assert_eq!(f64::from_le_bytes(age.data[..8].try_into().unwrap()), 100.0);
        assert_eq!(h.call_out(psm, Req { cmd: 14, ..Default::default() }, 1).u8(0), 1);
        for cmd in [2, 3, 5, 6, 8, 9, 10, 11] {
            assert_eq!(h.call(psm, Req { cmd, ..Default::default() }).result, 0, "psm {cmd}");
        }
        for cmd in [16, 18] {
            assert_eq!(h.call(psm, Req { cmd, ..Default::default() }).copy_handles.len(), 1);
        }
        assert_eq!(h.call(psm, Req { cmd: 99, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);

        // psmBindStateChangeEvent: open a session, enable three notifications, bind, unbind.
        let s = h.call(psm, Req { cmd: 7, ..Default::default() }).object();
        assert_eq!(h.ipc.handles.get(&s), Some(&ServiceType::PsmSession));
        for cmd in [2, 3, 4] {
            assert_eq!(h.call(s, Req { cmd, data: &[1, 0, 0, 0], ..Default::default() }).result, 0);
        }
        let ev = h.call(s, Req { cmd: 0, ..Default::default() }).copy_handles[0];
        assert_eq!(h.ipc.service_event_signaled(ev), Some(false));
        assert_eq!(h.call(s, Req { cmd: 1, ..Default::default() }).result, 0);
        assert_eq!(h.call(s, Req { cmd: 9, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }
}
