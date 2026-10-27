//! `time:u` / `time:s` in the shape libnx's `time.c` uses them: the clock sessions, the time zone
//! service (UTC only) and the 0x1000-byte shared memory libnx reads clocks from (6.0.0+).
//!
//! Command IDs and the shared memory layout follow libnx's public source. The steady clock is the
//! host monotonic clock since start; the system clocks are host UTC.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// Source ID shared by the steady clock and every system clock context (libnx compares them).
pub const STEADY_CLOCK_SOURCE_ID: [u8; 16] = *b"ounce-steady-clk";
pub const SHARED_MEMORY_SIZE: u64 = 0x1000;
/// `TimeLocationName` is 0x24 bytes.
pub const LOCATION_NAME: &[u8] = b"UTC";
/// `TimeZoneRule` is 0x4000 opaque bytes; UTC needs none of it.
pub const TIME_ZONE_RULE_SIZE: usize = 0x4000;

/// Shared memory offsets (libnx `_timeReadSharedmemObj` users).
const SHMEM_STEADY: u64 = 0x00;
const SHMEM_LOCAL_CLOCK: u64 = 0x38;
const SHMEM_NETWORK_CLOCK: u64 = 0x80;

#[derive(Debug, Clone, Default)]
pub struct TimeState {
    pub shm_handle: Option<u32>,
    /// Pins "now" (UTC seconds) for tests; `None` reads the host clock.
    pub fixed_unix_secs: Option<u64>,
}

impl TimeState {
    pub fn unix_secs(&self) -> u64 {
        self.fixed_unix_secs.unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs())
    }
}

/// Days since 1970-01-01 -> (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

/// (year, month, day) -> days since 1970-01-01.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = y - (m <= 2) as i64;
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `TimeCalendarTime` (8 bytes) + `TimeCalendarAdditionalInfo` (24 bytes) for `secs` in UTC.
pub fn calendar_time(secs: u64) -> [u8; 32] {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    let yday = (days - days_from_civil(y, 1, 1)) as u32;
    let wday = (days + 4).rem_euclid(7) as u32; // 1970-01-01 was a Thursday
    let mut out = [0u8; 32];
    out[0..2].copy_from_slice(&(y as u16).to_le_bytes());
    out[2] = m as u8;
    out[3] = d as u8;
    out[4] = (rem / 3600) as u8;
    out[5] = (rem % 3600 / 60) as u8;
    out[6] = (rem % 60) as u8;
    out[8..12].copy_from_slice(&wday.to_le_bytes());
    out[12..16].copy_from_slice(&yday.to_le_bytes());
    out[16..19].copy_from_slice(b"UTC"); // timezoneName[8]; DST = 0 and offset = 0 follow as zeros
    out
}

/// Inverse of [`calendar_time`] for the first 8 bytes of a `TimeCalendarTime`.
pub fn posix_time(cal: &[u8]) -> u64 {
    let year = u16::from_le_bytes([cal[0], cal[1]]) as i64;
    let days = days_from_civil(year, cal[2].clamp(1, 12) as u32, cal[3].max(1) as u32);
    (days * 86_400) as u64 + cal[4] as u64 * 3600 + cal[5] as u64 * 60 + cal[6] as u64
}

impl IpcManager {
    fn steady_secs(&self) -> i64 {
        self.start_instant.elapsed().as_secs() as i64
    }

    /// Writes the clock block libnx reads once the guest maps the shared memory at `addr`.
    pub(super) fn time_shared_memory_mapped(&mut self, handle: u32, addr: u64, mmu: &mut Memory) {
        if self.time.shm_handle != Some(handle) {
            return;
        }
        // Each block: u32 counter, pad, two {data} slots; libnx reads slot (counter & 1). Counter 0 = slot 0.
        let steady = [0i64.to_le_bytes().as_slice(), &STEADY_CLOCK_SOURCE_ID].concat();
        let _ = mmu.write_bytes(addr + SHMEM_STEADY + 8, &steady);
        let context = self.system_clock_context();
        for off in [SHMEM_LOCAL_CLOCK, SHMEM_NETWORK_CLOCK] {
            let _ = mmu.write_bytes(addr + off + 8, &context);
        }
    }

    /// `TimeSystemClockContext`: offset so that `offset + steady seconds` is UTC now, then the steady
    /// clock time point it was taken at.
    fn system_clock_context(&self) -> Vec<u8> {
        let steady = self.steady_secs();
        let offset = self.time.unix_secs() as i64 - steady;
        [offset.to_le_bytes().as_slice(), &steady.to_le_bytes(), &STEADY_CLOCK_SOURCE_ID].concat()
    }

    /// `time:u` / `time:s` root.
    pub(super) fn handle_time(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // GetStandardUserSystemClock / GetStandardNetworkSystemClock / GetStandardLocalSystemClock
            0 | 1 | 4 => self.reply_object(tls, mmu, ServiceType::SystemClock),
            // GetStandardSteadyClock
            2 => self.reply_object(tls, mmu, ServiceType::SteadyClock),
            // GetTimeZoneService
            3 => self.reply_object(tls, mmu, ServiceType::TimeZoneService),
            // GetSharedMemoryNativeHandle
            20 => {
                let h = match self.time.shm_handle {
                    Some(h) => h,
                    None => {
                        let h = self.allocate_handle(ServiceType::SharedMemory { size: SHARED_MEMORY_SIZE });
                        self.time.shm_handle = Some(h);
                        h
                    }
                };
                self.reply_event(tls, mmu, h)
            }
            _ => self.reply_unimplemented("time", cmd, tls, mmu),
        }
    }

    /// `ISystemClock`.
    pub(super) fn handle_system_clock(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // GetCurrentTime -> u64
            0 => self.reply_data(tls, mmu, &self.time.unix_secs().to_le_bytes()),
            // SetCurrentTime(u64): the guest may not move the host clock; accepted and ignored.
            1 => self.reply_ok(tls, mmu),
            // GetSystemClockContext
            2 => self.reply_data(tls, mmu, &self.system_clock_context()),
            _ => self.reply_unimplemented("ISystemClock", cmd, tls, mmu),
        }
    }

    /// `ISteadyClock`.
    pub(super) fn handle_steady_clock(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // GetCurrentTimePoint -> {s64 seconds, Uuid source}
            0 => {
                let tp = [self.steady_secs().to_le_bytes().as_slice(), &STEADY_CLOCK_SOURCE_ID].concat();
                self.reply_data(tls, mmu, &tp)
            }
            // GetInternalOffset -> s64
            200 => self.reply_data(tls, mmu, &0i64.to_le_bytes()),
            _ => self.reply_unimplemented("ISteadyClock", cmd, tls, mmu),
        }
    }

    /// `ITimeZoneService`, with UTC as the only location.
    pub(super) fn handle_time_zone(&mut self, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // GetDeviceLocationName -> TimeLocationName (0x24)
            0 => {
                let mut name = [0u8; 0x24];
                name[..LOCATION_NAME.len()].copy_from_slice(LOCATION_NAME);
                self.reply_data(tls, mmu, &name)
            }
            // SetDeviceLocationName: only UTC exists.
            1 => {
                let mut name = [0u8; 0x24];
                let _ = mmu.read_bytes(args, &mut name);
                if name.starts_with(LOCATION_NAME) && name[LOCATION_NAME.len()] == 0 {
                    self.reply_ok(tls, mmu)
                } else {
                    self.reply_error(tls, mmu, RESULT_TIME_NOT_FOUND)
                }
            }
            // GetTotalLocationNameCount -> s32
            2 => self.reply_data(tls, mmu, &1u32.to_le_bytes()),
            // LoadLocationNameList(s32 index) -> s32 count, names in the buffer
            3 => {
                let mut name = [0u8; 0x24];
                name[..LOCATION_NAME.len()].copy_from_slice(LOCATION_NAME);
                let index = mmu.read_u32(args).unwrap_or(0);
                let n = if index == 0 { self.write_out_buffer(mmu, &name) / 0x24 } else { 0 };
                self.reply_data(tls, mmu, &(n as u32).to_le_bytes())
            }
            // LoadTimeZoneRule(name) -> TimeZoneRule in the buffer
            4 => {
                self.write_out_buffer(mmu, &vec![0u8; TIME_ZONE_RULE_SIZE]);
                self.reply_ok(tls, mmu)
            }
            // ToCalendarTime(u64, rule in buffer) / ToCalendarTimeWithMyRule(u64)
            100 | 101 => {
                let secs = mmu.read_u64(args).unwrap_or(0);
                self.reply_data(tls, mmu, &calendar_time(secs))
            }
            // ToPosixTime(caltime, rule in buffer) / ToPosixTimeWithMyRule(caltime): one result, UTC has no ambiguity
            201 | 202 => {
                let mut cal = [0u8; 8];
                let _ = mmu.read_bytes(args, &mut cal);
                self.write_out_buffer(mmu, &posix_time(&cal).to_le_bytes());
                self.reply_data(tls, mmu, &1u32.to_le_bytes())
            }
            _ => self.reply_unimplemented("ITimeZoneService", cmd, tls, mmu),
        }
    }
}

/// `time` ResultTimeZoneNotFound (module 116, description 200).
pub const RESULT_TIME_NOT_FOUND: u32 = 200 << 9 | 116;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{Harness, Req, BUF};
    use crate::kernel::ipc::RESULT_UNKNOWN_COMMAND;
    use crate::mmu::Permission;

    /// 2026-10-27 12:34:56 UTC, a Tuesday.
    const NOW: u64 = 1_793_104_496;

    fn harness() -> (Harness, u32) {
        let mut h = Harness::new();
        h.ipc.time.fixed_unix_secs = Some(NOW);
        let time = h.get_service("time:u");
        assert_eq!(h.ipc.handles.get(&time), Some(&ServiceType::Time));
        (h, time)
    }

    #[test]
    fn calendar_math_round_trips() {
        let c = calendar_time(NOW);
        assert_eq!(u16::from_le_bytes([c[0], c[1]]), 2026);
        assert_eq!(&c[2..7], &[10, 27, 12, 34, 56]);
        assert_eq!(u32::from_le_bytes(c[8..12].try_into().unwrap()), 2); // Tuesday
        assert_eq!(u32::from_le_bytes(c[12..16].try_into().unwrap()), 299); // 0-based day of year
        assert_eq!(posix_time(&c[..8]), NOW);
        // Epoch, a leap day and the end of a year.
        assert_eq!(&calendar_time(0)[2..4], &[1, 1]);
        assert_eq!(&calendar_time(951_782_400)[2..4], &[2, 29]); // 2000-02-29
        assert_eq!(posix_time(&calendar_time(951_782_400)), 951_782_400);
        assert_eq!(u32::from_le_bytes(calendar_time(1_735_603_199)[12..16].try_into().unwrap()), 364); // 2024-12-30
    }

    #[test]
    fn root_hands_out_the_five_sessions_libnx_opens() {
        let (mut h, time) = harness();
        for (cmd, svc) in [
            (0, ServiceType::SystemClock),
            (1, ServiceType::SystemClock),
            (2, ServiceType::SteadyClock),
            (3, ServiceType::TimeZoneService),
            (4, ServiceType::SystemClock),
        ] {
            let obj = h.call(time, Req { cmd, ..Default::default() }).object();
            assert_eq!(h.ipc.handles.get(&obj), Some(&svc), "cmd {cmd}");
        }
        assert_eq!(h.call(time, Req { cmd: 77, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }

    #[test]
    fn system_and_steady_clock_commands() {
        let (mut h, time) = harness();
        let clock = h.call(time, Req { cmd: 0, ..Default::default() }).object();
        assert_eq!(h.call_out(clock, Req { cmd: 0, ..Default::default() }, 8).u64(0), NOW);
        assert_eq!(h.call(clock, Req { cmd: 1, data: &5u64.to_le_bytes(), ..Default::default() }).result, 0);
        assert_eq!(h.call_out(clock, Req { cmd: 0, ..Default::default() }, 8).u64(0), NOW); // not moved
        let ctx = h.call_out(clock, Req { cmd: 2, ..Default::default() }, 32);
        assert_eq!(&ctx.data[16..], &STEADY_CLOCK_SOURCE_ID);
        let offset = ctx.u64(0) as i64;
        let steady = ctx.u64(8) as i64;
        assert_eq!(offset + steady, NOW as i64);
        assert_eq!(h.call(clock, Req { cmd: 9, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);

        let steady_clock = h.call(time, Req { cmd: 2, ..Default::default() }).object();
        let tp = h.call_out(steady_clock, Req { cmd: 0, ..Default::default() }, 24);
        assert!(tp.u64(0) < 5);
        assert_eq!(&tp.data[8..], &STEADY_CLOCK_SOURCE_ID);
        assert_eq!(h.call_out(steady_clock, Req { cmd: 200, ..Default::default() }, 8).u64(0), 0);
        assert_eq!(h.call(steady_clock, Req { cmd: 9, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }

    #[test]
    fn shared_memory_gives_libnx_a_consistent_clock() {
        let (mut h, time) = harness();
        let r = h.call(time, Req { cmd: 20, ..Default::default() });
        let shm = r.copy_handles[0];
        assert_eq!(h.ipc.handles.get(&shm), Some(&ServiceType::SharedMemory { size: SHARED_MEMORY_SIZE }));
        assert_eq!(h.call(time, Req { cmd: 20, ..Default::default() }).copy_handles[0], shm);

        // svcMapSharedMemory
        let map = 0x9000_0000u64;
        h.mmu.map_range(map, 0x1000, Permission::READ_WRITE);
        h.ipc.shared_memory_mapped(shm, map, &mut h.mmu);

        // libnx: steady {base_time, source} at +8, local clock context at 0x38+8, network at 0x80+8.
        let steady = h.read_buf(map + 8, 24);
        let base = i64::from_le_bytes(steady[..8].try_into().unwrap());
        for off in [0x38u64, 0x80] {
            let ctx = h.read_buf(map + off + 8, 32);
            assert_eq!(&ctx[16..], &steady[8..], "source ids must match or libnx returns 0x7456 2");
            let offset = i64::from_le_bytes(ctx[..8].try_into().unwrap());
            // timeGetCurrentTime = offset + (base_time + ticks_ns) / 1e9; a few ticks in.
            assert_eq!(offset + base, NOW as i64 - i64::from_le_bytes(ctx[8..16].try_into().unwrap()));
        }
        assert_eq!(h.read_buf(map, 4), [0; 4]); // sequence counter: slot 0
    }

    #[test]
    fn time_zone_service_is_utc() {
        let (mut h, time) = harness();
        let tz = h.call(time, Req { cmd: 3, ..Default::default() }).object();
        let name = h.call_out(tz, Req { cmd: 0, ..Default::default() }, 0x24);
        assert_eq!(&name.data[..4], b"UTC\0");
        assert_eq!(h.call_out(tz, Req { cmd: 2, ..Default::default() }, 4).u32(0), 1);

        let mut utc = [0u8; 0x24];
        utc[..3].copy_from_slice(b"UTC");
        assert_eq!(h.call(tz, Req { cmd: 1, data: &utc, ..Default::default() }).result, 0);
        let mut other = [0u8; 0x24];
        other[..16].copy_from_slice(b"Europe/Amsterdam");
        assert_eq!(h.call(tz, Req { cmd: 1, data: &other, ..Default::default() }).result, RESULT_TIME_NOT_FOUND);

        // LoadLocationNameList(0) -> 1 name; index 1 -> none.
        let r = h.call_out(tz, Req { cmd: 3, data: &0u32.to_le_bytes(), recv: Some((BUF, 0x24 * 4)), ..Default::default() }, 4);
        assert_eq!(r.u32(0), 1);
        assert_eq!(&h.read_buf(BUF, 4), b"UTC\0");
        let r = h.call_out(tz, Req { cmd: 3, data: &1u32.to_le_bytes(), recv: Some((BUF, 0x24 * 4)), ..Default::default() }, 4);
        assert_eq!(r.u32(0), 0);

        // LoadTimeZoneRule fills the buffer.
        h.mmu.write_bytes(BUF + 0x100, &[0xFF; 16]).unwrap();
        assert_eq!(h.call(tz, Req { cmd: 4, data: &utc, recv: Some((BUF + 0x100, 0x4000)), ..Default::default() }).result, 0);
        assert_eq!(h.read_buf(BUF + 0x100, 16), [0; 16]);

        // __libnx_init_time: ToCalendarTimeWithMyRule(boottime) -> caltime + info
        let r = h.call_out(tz, Req { cmd: 101, data: &NOW.to_le_bytes(), ..Default::default() }, 32);
        assert_eq!(r.data, calendar_time(NOW));
        assert_eq!(&r.data[16..20], b"UTC\0");
        assert_eq!(h.call_out(tz, Req { cmd: 100, data: &NOW.to_le_bytes(), send: Some((BUF, 0x4000)), ..Default::default() }, 32).data, calendar_time(NOW));

        // ToPosixTime(WithMyRule): calendar time in, one timestamp out via the receive list.
        let cal = calendar_time(NOW);
        for cmd in [201, 202] {
            let r = h.call_out(tz, Req { cmd, data: &cal[..8], recv_list: Some((BUF + 0x800, 64)), ..Default::default() }, 4);
            assert_eq!(r.u32(0), 1);
            assert_eq!(u64::from_le_bytes(h.read_buf(BUF + 0x800, 8).try_into().unwrap()), NOW);
        }
        assert_eq!(h.call(tz, Req { cmd: 300, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }
}
