//! `set` and `set:sys` in the shape libnx uses them (public `set.c` command IDs).
//!
//! Everything is synthetic: American English, region USA, firmware 23.0.1 for the Horizon OS 23.0.1
//! ABI, a placeholder serial number and device nickname.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// A language tag as the little-endian `u64` libnx's `SetLanguage` tables use.
pub const fn language_code(tag: &[u8]) -> u64 {
    let mut out = 0u64;
    let mut i = 0;
    while i < tag.len() {
        out |= (tag[i] as u64) << (8 * i);
        i += 1;
    }
    out
}

/// "en-US".
pub const DEFAULT_LANGUAGE_CODE: u64 = language_code(b"en-US");

/// Languages in `SetLanguage` order (libnx `set.h`).
pub const LANGUAGE_TAGS: [&[u8]; 18] = [
    b"ja", b"en-US", b"fr", b"de", b"it", b"es", b"zh-CN", b"ko", b"nl", b"pt", b"ru", b"zh-TW", b"en-GB", b"fr-CA",
    b"es-419", b"zh-Hans", b"zh-Hant", b"pt-BR",
];

/// `SetRegion_USA`.
pub const REGION_USA: u32 = 1;
/// The console model value the existing `set:sys` handler reports for T239.
pub const PRODUCT_MODEL_T239: u32 = 0x02;
/// `ColorSetId_BasicBlack`.
pub const COLOR_SET_BASIC_BLACK: u32 = 1;
pub const DEVICE_NICKNAME: &[u8] = b"Ounce-T239";
pub const SERIAL_NUMBER: &[u8] = b"XOUNCE0000000";
/// `SetSysPlatformRegion_Global`.
pub const PLATFORM_REGION_GLOBAL: u32 = 1;

/// The 0x100-byte `SetSysFirmwareVersion` for Horizon OS 23.0.1.
pub fn firmware_version() -> [u8; 0x100] {
    let mut out = [0u8; 0x100];
    out[..3].copy_from_slice(&[23, 0, 1]);
    let mut put = |off: usize, s: &[u8]| out[off..off + s.len()].copy_from_slice(s);
    put(8, b"NX"); // platform[0x20]
    put(0x28, b"ounce-synthetic"); // version_hash[0x40]
    put(0x68, b"23.0.1"); // display_version[0x18]
    put(0x80, b"Ounce Synthetic 23.0.1"); // display_title[0x80]
    out
}

impl IpcManager {
    /// `set`: language and region queries.
    pub(super) fn handle_set(&mut self, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        let total = LANGUAGE_TAGS.len() as u32;
        match cmd {
            // GetLanguageCode -> u64
            0 => self.reply_data(tls, mmu, &DEFAULT_LANGUAGE_CODE.to_le_bytes()),
            // GetAvailableLanguageCodes (pre-4.0.0 / 4.0.0+): out s32 total, language codes in the buffer
            1 | 5 => {
                let codes: Vec<u8> = LANGUAGE_TAGS.iter().flat_map(|t| language_code(t).to_le_bytes()).collect();
                let written = self.write_out_buffer(mmu, &codes) / 8;
                self.reply_data(tls, mmu, &(written as u32).to_le_bytes())
            }
            // MakeLanguageCode(s32 language) -> u64
            2 => match LANGUAGE_TAGS.get(mmu.read_u32(args).unwrap_or(u32::MAX) as usize) {
                Some(tag) => self.reply_data(tls, mmu, &language_code(tag).to_le_bytes()),
                None => self.reply_error(tls, mmu, RESULT_INVALID_LANGUAGE),
            },
            // GetAvailableLanguageCodeCount (pre-4.0.0 / 4.0.0+)
            3 | 6 => self.reply_data(tls, mmu, &total.to_le_bytes()),
            // GetRegionCode -> s32
            4 => self.reply_data(tls, mmu, &REGION_USA.to_le_bytes()),
            // GetQuestFlag -> bool
            8 => self.reply_data(tls, mmu, &[0]),
            // GetDeviceNickname -> SetSysDeviceNickName (0x80 bytes, buffer)
            11 => {
                let mut name = [0u8; 0x80];
                name[..DEVICE_NICKNAME.len()].copy_from_slice(DEVICE_NICKNAME);
                self.write_out_buffer(mmu, &name);
                self.reply_ok(tls, mmu)
            }
            _ => self.reply_unimplemented("set", cmd, tls, mmu),
        }
    }

    /// `set:sys`: firmware version, product model, serial number, nickname.
    pub(super) fn handle_setsys(&mut self, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        let _ = args;
        match cmd {
            // GetFirmwareVersion (< 3.0.0, zeroes revision) / GetFirmwareVersion2: 0x100 bytes via pointer buffer
            3 | 4 => {
                self.write_out_buffer(mmu, &firmware_version());
                self.reply_ok(tls, mmu)
            }
            // GetFirmwareVersionDigest -> 0x40 bytes
            5 => {
                let mut digest = [0u8; 0x40];
                digest[..15].copy_from_slice(b"ounce-synthetic");
                self.reply_data(tls, mmu, &digest)
            }
            // GetLockScreenFlag -> bool
            7 => self.reply_data(tls, mmu, &[0]),
            // GetColorSetId -> u32
            23 => self.reply_data(tls, mmu, &COLOR_SET_BASIC_BLACK.to_le_bytes()),
            // GetQuestFlag -> bool
            47 => self.reply_data(tls, mmu, &[0]),
            // GetSerialNumber -> char[0x18]
            68 => {
                let mut sn = [0u8; 0x18];
                sn[..SERIAL_NUMBER.len()].copy_from_slice(SERIAL_NUMBER);
                self.reply_data(tls, mmu, &sn)
            }
            // GetDeviceNickname -> 0x80 bytes via buffer
            77 => {
                let mut name = [0u8; 0x80];
                name[..DEVICE_NICKNAME.len()].copy_from_slice(DEVICE_NICKNAME);
                self.write_out_buffer(mmu, &name);
                self.reply_ok(tls, mmu)
            }
            // GetProductModel -> u32
            79 => self.reply_data(tls, mmu, &PRODUCT_MODEL_T239.to_le_bytes()),
            // GetPlatformRegion -> u32
            183 => self.reply_data(tls, mmu, &PLATFORM_REGION_GLOBAL.to_le_bytes()),
            _ => self.reply_unimplemented("set:sys", cmd, tls, mmu),
        }
    }
}

/// `set` ResultInvalidLanguage-style failure (module 105, description 1... synthetic).
pub const RESULT_INVALID_LANGUAGE: u32 = 1 << 9 | 105;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{Harness, Req, BUF};
    use crate::kernel::ipc::RESULT_UNKNOWN_COMMAND;

    #[test]
    fn language_codes_match_libnx_tables() {
        assert_eq!(DEFAULT_LANGUAGE_CODE.to_le_bytes(), *b"en-US\0\0\0");
        assert_eq!(LANGUAGE_TAGS[1], b"en-US");
        assert_eq!(LANGUAGE_TAGS[17], b"pt-BR");
        assert_eq!(language_code(b"ja"), 0x616A);
    }

    #[test]
    fn set_language_region_and_nickname() {
        let mut h = Harness::new();
        let set = h.get_service("set");
        assert_eq!(h.ipc.handles.get(&set), Some(&ServiceType::Set));
        assert_eq!(h.call_out(set, Req { cmd: 0, ..Default::default() }, 8).u64(0), DEFAULT_LANGUAGE_CODE);
        assert_eq!(h.call_out(set, Req { cmd: 6, ..Default::default() }, 4).u32(0), 18);
        assert_eq!(h.call_out(set, Req { cmd: 3, ..Default::default() }, 4).u32(0), 18);
        assert_eq!(h.call_out(set, Req { cmd: 4, ..Default::default() }, 4).u32(0), REGION_USA);
        assert_eq!(h.call_out(set, Req { cmd: 8, ..Default::default() }, 1).u8(0), 0);

        // GetAvailableLanguageCodes through a map-alias buffer; a short buffer truncates the count.
        let r = h.call_out(set, Req { cmd: 5, recv: Some((BUF, 18 * 8)), ..Default::default() }, 4);
        assert_eq!(r.u32(0), 18);
        let codes = h.read_buf(BUF, 18 * 8);
        assert_eq!(u64::from_le_bytes(codes[8..16].try_into().unwrap()), DEFAULT_LANGUAGE_CODE);
        let r = h.call_out(set, Req { cmd: 5, recv: Some((BUF, 3 * 8)), ..Default::default() }, 4);
        assert_eq!(r.u32(0), 3);

        // MakeLanguageCode
        assert_eq!(h.call_out(set, Req { cmd: 2, data: &1u32.to_le_bytes(), ..Default::default() }, 8).u64(0), DEFAULT_LANGUAGE_CODE);
        assert_eq!(h.call(set, Req { cmd: 2, data: &99u32.to_le_bytes(), ..Default::default() }).result, RESULT_INVALID_LANGUAGE);

        // Nickname
        assert_eq!(h.call(set, Req { cmd: 11, recv: Some((BUF + 0x400, 0x80)), ..Default::default() }).result, 0);
        assert_eq!(&h.read_buf(BUF + 0x400, 10), DEVICE_NICKNAME);
        assert_eq!(h.call(set, Req { cmd: 200, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }

    #[test]
    fn setsys_firmware_23_0_1_through_pointer_buffer() {
        let mut h = Harness::new();
        let sys = h.get_service("set:sys");
        assert_eq!(h.ipc.handles.get(&sys), Some(&ServiceType::SetSys));
        // libnx setsysGetFirmwareVersion: HipcPointer out buffer = receive list entry.
        let r = h.call(sys, Req { cmd: 4, recv_list: Some((BUF, 0x100)), ..Default::default() });
        assert_eq!(r.result, 0);
        let fw = h.read_buf(BUF, 0x100);
        assert_eq!(&fw[..8], &[23, 0, 1, 0, 0, 0, 0, 0]);
        assert_eq!(&fw[0x68..0x6E], b"23.0.1");
        assert_eq!(fw, firmware_version());
        // Pre-3.0.0 command ID gives the same data.
        h.call(sys, Req { cmd: 3, recv_list: Some((BUF + 0x200, 0x100)), ..Default::default() });
        assert_eq!(h.read_buf(BUF + 0x200, 0x100), fw);
    }

    #[test]
    fn setsys_model_serial_and_flags() {
        let mut h = Harness::new();
        let sys = h.get_service("set:sys");
        assert_eq!(h.call_out(sys, Req { cmd: 79, ..Default::default() }, 4).u32(0), PRODUCT_MODEL_T239);
        assert_eq!(h.call_out(sys, Req { cmd: 183, ..Default::default() }, 4).u32(0), PLATFORM_REGION_GLOBAL);
        assert_eq!(h.call_out(sys, Req { cmd: 23, ..Default::default() }, 4).u32(0), COLOR_SET_BASIC_BLACK);
        assert_eq!(&h.call_out(sys, Req { cmd: 68, ..Default::default() }, 0x18).data[..13], SERIAL_NUMBER);
        assert_eq!(h.call_out(sys, Req { cmd: 5, ..Default::default() }, 0x40).data.len(), 0x40);
        for cmd in [7, 47] {
            assert_eq!(h.call_out(sys, Req { cmd, ..Default::default() }, 1).u8(0), 0);
        }
        assert_eq!(h.call(sys, Req { cmd: 77, recv: Some((BUF, 0x80)), ..Default::default() }).result, 0);
        assert_eq!(&h.read_buf(BUF, 10), DEVICE_NICKNAME);
        assert_eq!(h.call(sys, Req { cmd: 9999, ..Default::default() }).result, RESULT_UNKNOWN_COMMAND);
    }
}
