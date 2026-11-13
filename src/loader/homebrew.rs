#![allow(dead_code)]

//! Homebrew loader ABI: the `ConfigEntry` list a homebrew loader passes in `X0` to an NRO's `crt0`.
//!
//! Layout from the public homebrew ABI documentation: each entry is `{ u32 key; u32 flags; u64 value[2]; }`
//! (24 bytes) and the list ends with an `EndOfList` entry. `X1` must be `-1`; the main thread handle
//! travels in a `MainThreadHandle` entry instead. With a non-null list, libnx `crt0` returns to the
//! caller's `X30` on exit (with the exit code in `X0`) instead of calling `svcExitProcess`.

use crate::mmu::{Memory, MemoryError, Permission};

/// Bytes between the config block (`0x7FFF_0000`) and the NRO load address (`0x8000_0000`). The block must end
/// below the NRO: it is mapped in whole pages, so anything past this window would overwrite the program's code.
pub const CONFIG_WINDOW: u64 = 0x1_0000;

/// Longest argument string (the `sdmc:/<file>` program path, then the guest arguments, joined by spaces and quoted
/// where they contain spaces) that fits in `CONFIG_WINDOW` with the entries and next-load buffers `run-nro` passes:
/// 5 entries (120 bytes), the string and its NUL, 16-byte alignment, then 512 + 2048 bytes of next-load buffers.
/// `ounce-core run-nro` refuses longer strings with a non-zero exit before the program starts.
pub const MAX_ARGV_LEN: usize = 62_855;

/// Why the homebrew config block could not be written.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("argument string is {len} bytes, over the {max}-byte limit (the program path and the guest arguments, joined by spaces)")]
    ArgvTooLong { len: usize, max: usize },
    #[error(transparent)]
    Memory(#[from] MemoryError),
}

pub const ENTRY_SIZE: u64 = 24;
pub const FLAG_MANDATORY: u32 = 1;

pub const ENTRY_END_OF_LIST: u32 = 0;
pub const ENTRY_MAIN_THREAD_HANDLE: u32 = 1;
pub const ENTRY_NEXT_LOAD_PATH: u32 = 2;
pub const ENTRY_OVERRIDE_HEAP: u32 = 3;
pub const ENTRY_OVERRIDE_SERVICE: u32 = 4;
pub const ENTRY_ARGV: u32 = 5;
pub const ENTRY_SYSCALL_AVAILABLE_HINT: u32 = 6;
pub const ENTRY_APPLET_TYPE: u32 = 7;
pub const ENTRY_PROCESS_HANDLE: u32 = 10;
pub const ENTRY_LAST_LOAD_RESULT: u32 = 11;
pub const ENTRY_RANDOM_SEED: u32 = 14;

/// One `ConfigEntry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigEntry {
    pub key: u32,
    pub flags: u32,
    pub value: [u64; 2],
}

/// Builds the `ConfigEntry` list and the strings it points at.
#[derive(Debug, Default, Clone)]
pub struct HomebrewConfig {
    pub main_thread_handle: Option<u32>,
    /// `(address, size)` of a heap the loader has already mapped. Without it `crt0` calls `svcSetHeapSize`.
    pub heap: Option<(u64, u64)>,
    /// Space-separated argument string, as libnx expects for `Argv`.
    pub argv: Option<String>,
    pub applet_type: Option<u64>,
    pub random_seed: Option<[u64; 2]>,
    /// Give the program the `NextLoadPath` buffers hbloader always passes, so a menu knows it can chain-load.
    pub next_load: bool,
}

/// Sizes of the `NextLoadPath` buffers hbloader hands out (path, then argument string).
const NEXT_LOAD_PATH_SIZE: u64 = 512;
const NEXT_LOAD_ARGV_SIZE: u64 = 2048;

impl HomebrewConfig {
    /// The entries in list order, ending with `EndOfList`. `argv_addr` is where the argument string is written.
    pub fn entries(&self, argv_addr: u64) -> Vec<ConfigEntry> {
        let mut out = Vec::new();
        if let Some(handle) = self.main_thread_handle {
            out.push(ConfigEntry { key: ENTRY_MAIN_THREAD_HANDLE, flags: 0, value: [handle as u64, 0] });
        }
        if let Some((addr, size)) = self.heap {
            out.push(ConfigEntry { key: ENTRY_OVERRIDE_HEAP, flags: 0, value: [addr, size] });
        }
        if self.argv.is_some() {
            out.push(ConfigEntry { key: ENTRY_ARGV, flags: 0, value: [0, argv_addr] });
        }
        if let Some(kind) = self.applet_type {
            out.push(ConfigEntry { key: ENTRY_APPLET_TYPE, flags: 0, value: [kind, 0] });
        }
        if let Some(seed) = self.random_seed {
            out.push(ConfigEntry { key: ENTRY_RANDOM_SEED, flags: 0, value: seed });
        }
        if self.next_load {
            out.push(ConfigEntry { key: ENTRY_NEXT_LOAD_PATH, flags: 0, value: [0, 0] });
        }
        out.push(ConfigEntry { key: ENTRY_END_OF_LIST, flags: 0, value: [0, 0] });
        out
    }

    /// Maps pages at `addr` (page aligned), writes the list there and the argument string after it.
    /// Returns the list address, which goes in `X0`.
    pub fn write(&self, mmu: &mut Memory, addr: u64) -> Result<u64, ConfigError> {
        // The argument string follows the list; point the Argv entry at it once the list length is known.
        let mut entries = self.entries(0);
        let argv_addr = addr + (entries.len() as u64) * ENTRY_SIZE;
        for e in entries.iter_mut().filter(|e| e.key == ENTRY_ARGV) {
            e.value[1] = argv_addr;
        }
        let argv_len = self.argv.as_ref().map_or(0, |a| a.len() as u64 + 1);
        let next_path_addr = (argv_addr + argv_len).next_multiple_of(16);
        let next_argv_addr = next_path_addr + NEXT_LOAD_PATH_SIZE;
        for e in entries.iter_mut().filter(|e| e.key == ENTRY_NEXT_LOAD_PATH) {
            e.value = [next_path_addr, next_argv_addr];
        }
        let end = if self.next_load { next_argv_addr + NEXT_LOAD_ARGV_SIZE } else { argv_addr + argv_len };
        let total = end - addr;
        // Refuse before mapping anything: a block past the window would map and overwrite the NRO's first page.
        if total > CONFIG_WINDOW {
            return Err(ConfigError::ArgvTooLong { len: self.argv.as_ref().map_or(0, |a| a.len()), max: MAX_ARGV_LEN });
        }
        // Map as many pages as the list and the string need (a long command line spills past one page).
        mmu.map_range(addr, total.max(1).div_ceil(0x1000) as usize * 0x1000, Permission::READ_WRITE);
        for (i, e) in entries.iter().enumerate() {
            let at = addr + i as u64 * ENTRY_SIZE;
            mmu.write_u32(at, e.key)?;
            mmu.write_u32(at + 4, e.flags)?;
            mmu.write_u64(at + 8, e.value[0])?;
            mmu.write_u64(at + 16, e.value[1])?;
        }
        if let Some(argv) = &self.argv {
            mmu.write_bytes(argv_addr, argv.as_bytes())?;
            mmu.write_u8(argv_addr + argv.len() as u64, 0)?;
        }
        Ok(addr)
    }

    /// Maps a page at `addr` holding `svc #0x07` (`svcExitProcess`) and returns its address.
    /// Point `X30` at it: with a `ConfigEntry` list, libnx `exit` returns there instead of exiting itself.
    pub fn install_exit_stub(mmu: &mut Memory, addr: u64) -> Result<u64, MemoryError> {
        mmu.map_range(addr, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.write_u32(addr, 0xD400_00E1)?;
        Ok(addr)
    }

    /// Writes the list at `addr` and sets the entry registers: `X0` = list, `X1` = `-1`.
    pub fn setup(&self, cpu: &mut crate::cpu::Cpu, mmu: &mut Memory, addr: u64) -> Result<(), ConfigError> {
        let list = self.write(mmu, addr)?;
        cpu.regs.write_x(0, list);
        cpu.regs.write_x(1, u64::MAX);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_list_layout() {
        let mut mmu = Memory::new();
        let mut cpu = crate::cpu::Cpu::new();
        let cfg = HomebrewConfig {
            main_thread_handle: Some(0xD000),
            heap: Some((0x2_0000_0000, 0x10_0000)),
            argv: Some("sdmc:/hello.nro".into()),
            ..Default::default()
        };
        cfg.setup(&mut cpu, &mut mmu, 0x7000_0000).unwrap();
        assert_eq!(cpu.regs.read_x(0), 0x7000_0000);
        assert_eq!(cpu.regs.read_x(1), u64::MAX);

        let at = |i: u64| 0x7000_0000 + i * ENTRY_SIZE;
        assert_eq!(mmu.read_u32(at(0)).unwrap(), ENTRY_MAIN_THREAD_HANDLE);
        assert_eq!(mmu.read_u64(at(0) + 8).unwrap(), 0xD000);
        assert_eq!(mmu.read_u32(at(1)).unwrap(), ENTRY_OVERRIDE_HEAP);
        assert_eq!(mmu.read_u64(at(1) + 8).unwrap(), 0x2_0000_0000);
        assert_eq!(mmu.read_u64(at(1) + 16).unwrap(), 0x10_0000);
        assert_eq!(mmu.read_u32(at(2)).unwrap(), ENTRY_ARGV);
        let argv_ptr = mmu.read_u64(at(2) + 16).unwrap();
        assert_eq!(argv_ptr, at(4));
        let mut s = [0u8; 16];
        mmu.read_bytes(argv_ptr, &mut s).unwrap();
        assert_eq!(&s, b"sdmc:/hello.nro\0");
        assert_eq!(mmu.read_u32(at(3)).unwrap(), ENTRY_END_OF_LIST);
    }

    #[test]
    fn long_argv_spills_into_more_pages() {
        let mut mmu = Memory::new();
        let argv: String = "x".repeat(0x1800);
        let cfg = HomebrewConfig { argv: Some(argv.clone()), ..Default::default() };
        cfg.write(&mut mmu, 0x7000_0000).unwrap();
        let ptr = mmu.read_u64(0x7000_0000 + 16).unwrap();
        assert_eq!(mmu.read_u8(ptr + argv.len() as u64).unwrap(), 0);
        assert_eq!(mmu.read_u8(ptr + argv.len() as u64 - 1).unwrap(), b'x');
    }

    /// Maps the NRO's first page at `0x8000_0000` with a sentinel and returns the config block address, which is
    /// `CONFIG_WINDOW` below it (the layout `run-nro` uses).
    fn map_nro_page_with_sentinel(mmu: &mut Memory) -> u64 {
        mmu.map_range(0x8000_0000, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.write_u32(0x8000_0000, 0xD400_00E1).unwrap();
        0x8000_0000 - CONFIG_WINDOW
    }

    fn run_nro_config(argv: String) -> HomebrewConfig {
        HomebrewConfig {
            main_thread_handle: Some(0x1000),
            heap: Some((0x2_0000_0000, 0x1000_0000)),
            argv: Some(argv),
            next_load: true,
            ..Default::default()
        }
    }

    #[test]
    fn argv_at_the_limit_fits_below_the_nro() {
        let mut mmu = Memory::new();
        let addr = map_nro_page_with_sentinel(&mut mmu);
        let argv = "x".repeat(MAX_ARGV_LEN);
        run_nro_config(argv.clone()).write(&mut mmu, addr).unwrap();
        // The block stops short of the NRO and the NRO's first instruction is untouched.
        assert_eq!(mmu.read_u32(0x8000_0000).unwrap(), 0xD400_00E1);
        // Argv is the third entry; its string follows the five entries of the run-nro list.
        let ptr = mmu.read_u64(addr + 2 * ENTRY_SIZE + 16).unwrap();
        assert_eq!(ptr, addr + 5 * ENTRY_SIZE);
        assert_eq!(mmu.read_u8(ptr + argv.len() as u64).unwrap(), 0);
        assert_eq!(mmu.read_u8(ptr + argv.len() as u64 - 1).unwrap(), b'x');
    }

    #[test]
    fn argv_one_byte_over_the_limit_is_refused_before_anything_is_mapped() {
        let mut mmu = Memory::new();
        let addr = map_nro_page_with_sentinel(&mut mmu);
        let err = run_nro_config("x".repeat(MAX_ARGV_LEN + 1)).write(&mut mmu, addr).unwrap_err();
        assert!(matches!(err, ConfigError::ArgvTooLong { len, max } if len == MAX_ARGV_LEN + 1 && max == MAX_ARGV_LEN));
        assert_eq!(mmu.read_u32(0x8000_0000).unwrap(), 0xD400_00E1);
        assert!(mmu.read_u32(addr).is_err(), "nothing was mapped for the refused block");
    }

    #[test]
    fn exit_stub_calls_exit_process() {
        let mut mmu = Memory::new();
        let mut cpu = crate::cpu::Cpu::new();
        cpu.regs.pc = HomebrewConfig::install_exit_stub(&mut mmu, 0x7FFE_0000).unwrap();
        assert!(matches!(cpu.step(&mut mmu), Ok(crate::cpu::StepResult::Svc(0x07))));
    }
}
