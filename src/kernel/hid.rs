#![allow(dead_code)]

//! HID shared memory in the layout libnx reads (`HidSharedMemory` in libnx `services/hid.h`), so
//! `padUpdate` / `hidGetNpadStates*` see the host controller. Offsets were taken from those public
//! structure definitions.
//!
//! Only the npad section is written: player 1 (`No1`, full-key style) and `Handheld` carry the same
//! input, so pads set up with `padInitializeDefault` read it either way. Each `sync` appends one state
//! to both LIFOs with the next sampling number.

use crate::mmu::Memory;

/// Size of the HID shared memory block.
pub const HID_SHARED_MEMORY_SIZE: u64 = 0x40000;
/// `HidSharedMemory::npad`.
pub const NPAD_OFFSET: u64 = 0x9A00;
/// `sizeof(HidNpadSharedMemoryEntry)`.
pub const NPAD_ENTRY_SIZE: u64 = 0x5000;
/// `HidNpadInternalState` fields.
pub const STYLE_SET_OFFSET: u64 = 0x0;
pub const JOY_ASSIGNMENT_OFFSET: u64 = 0x4;
pub const FULL_KEY_LIFO_OFFSET: u64 = 0x28;
pub const HANDHELD_LIFO_OFFSET: u64 = 0x378;
pub const DEVICE_TYPE_OFFSET: u64 = 0x4188;
/// `HidNpadCommonLifo`: a 0x20-byte header (`unused`, `buffer_count`, `tail`, `count`), then 17
/// `{ u64 sampling_number; HidNpadCommonState state; }` slots of 0x30 bytes.
pub const LIFO_STORAGE_OFFSET: u64 = 0x20;
pub const LIFO_ENTRY_SIZE: u64 = 0x30;
pub const LIFO_ENTRIES: u64 = 17;

/// npad entry indices: `HidNpadIdType_No1` = 0, `HidNpadIdType_Handheld` (0x20) = 8.
pub const NPAD_NO1: u64 = 0;
pub const NPAD_HANDHELD: u64 = 8;

pub const STYLE_FULL_KEY: u32 = 1 << 0;
pub const STYLE_HANDHELD: u32 = 1 << 1;
pub const DEVICE_FULL_KEY: u32 = 1 << 0;
pub const DEVICE_HANDHELD_LEFT: u32 = 1 << 2;
pub const DEVICE_HANDHELD_RIGHT: u32 = 1 << 4;
/// `HidNpadAttribute_IsConnected` | `IsWired`.
pub const ATTRIBUTES_CONNECTED: u32 = 1 << 0 | 1 << 1;
/// `HidNpadJoyAssignmentMode_Dual`.
pub const JOY_ASSIGNMENT_DUAL: u32 = 0;

/// Controller input in libnx terms: `HidNpadButton` bits and stick positions (-0x7FFF..=0x7FFF).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NpadInput {
    pub buttons: u64,
    pub stick_l: (i32, i32),
    pub stick_r: (i32, i32),
}

impl NpadInput {
    /// The host controller state `HidServer` keeps (keyboard and gamepad, `NpadButton` bits and
    /// stick positions already in the -0x7FFF..=0x7FFF range).
    pub fn from_controller(state: &crate::hid::controller::ControllerState) -> Self {
        Self {
            buttons: state.buttons,
            stick_l: (state.analog_lx, state.analog_ly),
            stick_r: (state.analog_rx, state.analog_ry),
        }
    }
}

/// Writes `NpadInput` into the HID shared memory the guest mapped.
#[derive(Debug, Default)]
pub struct HidSharedMemory {
    /// Guest address of the mapped block, once the guest has mapped it.
    pub addr: Option<u64>,
    /// Handle of the shared memory object handed out by `IAppletResource::GetSharedMemoryHandle`.
    pub handle: Option<u32>,
    /// Current host input, written on the next `sync`.
    pub input: NpadInput,
    sampling: u64,
}

impl HidSharedMemory {
    /// The guest mapped the HID block at `addr`: describe the controllers and write the first state.
    pub fn mapped(&mut self, addr: u64, mmu: &mut Memory) {
        self.addr = Some(addr);
        self.sampling = 0;
        for (index, style, device) in [
            (NPAD_NO1, STYLE_FULL_KEY, DEVICE_FULL_KEY),
            (NPAD_HANDHELD, STYLE_HANDHELD, DEVICE_HANDHELD_LEFT | DEVICE_HANDHELD_RIGHT),
        ] {
            let npad = addr + NPAD_OFFSET + index * NPAD_ENTRY_SIZE;
            let _ = mmu.write_u32(npad + STYLE_SET_OFFSET, style);
            let _ = mmu.write_u32(npad + JOY_ASSIGNMENT_OFFSET, JOY_ASSIGNMENT_DUAL);
            let _ = mmu.write_u32(npad + DEVICE_TYPE_OFFSET, device);
            for lifo in [FULL_KEY_LIFO_OFFSET, HANDHELD_LIFO_OFFSET] {
                let header = npad + lifo;
                let _ = mmu.write_u64(header, 0);
                let _ = mmu.write_u64(header + 8, LIFO_ENTRIES);
                let _ = mmu.write_u64(header + 16, 0);
                let _ = mmu.write_u64(header + 24, 0);
            }
        }
        self.sync(mmu);
    }

    pub fn unmapped(&mut self, addr: u64) {
        if self.addr == Some(addr) {
            self.addr = None;
        }
    }

    /// Appends the current input to player 1's full-key LIFO and the handheld LIFO.
    pub fn sync(&mut self, mmu: &mut Memory) {
        let Some(addr) = self.addr else { return };
        self.sampling += 1;
        let s = self.sampling;
        for (index, lifo) in [(NPAD_NO1, FULL_KEY_LIFO_OFFSET), (NPAD_HANDHELD, HANDHELD_LIFO_OFFSET)] {
            let header = addr + NPAD_OFFSET + index * NPAD_ENTRY_SIZE + lifo;
            let tail = (mmu.read_u64(header + 16).unwrap_or(0) + 1) % LIFO_ENTRIES;
            let count = (mmu.read_u64(header + 24).unwrap_or(0) + 1).min(LIFO_ENTRIES);
            let slot = header + LIFO_STORAGE_OFFSET + tail * LIFO_ENTRY_SIZE;
            // HidNpadCommonStateAtomicStorage { sampling_number; state { sampling_number; buttons;
            // stick_l { x, y }; stick_r { x, y }; attributes; reserved } }
            let _ = mmu.write_u64(slot, s);
            let _ = mmu.write_u64(slot + 8, s);
            let _ = mmu.write_u64(slot + 16, self.input.buttons);
            let _ = mmu.write_u32(slot + 24, self.input.stick_l.0 as u32);
            let _ = mmu.write_u32(slot + 28, self.input.stick_l.1 as u32);
            let _ = mmu.write_u32(slot + 32, self.input.stick_r.0 as u32);
            let _ = mmu.write_u32(slot + 36, self.input.stick_r.1 as u32);
            let _ = mmu.write_u32(slot + 40, ATTRIBUTES_CONNECTED);
            let _ = mmu.write_u32(slot + 44, 0);
            let _ = mmu.write_u64(header + 16, tail);
            let _ = mmu.write_u64(header + 24, count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    #[test]
    fn lifo_entries_follow_libnx_layout() {
        let mut mmu = Memory::new();
        let base = 0x6000_0000;
        mmu.map_range(base, HID_SHARED_MEMORY_SIZE as usize, Permission::READ_WRITE);
        let mut hid = HidSharedMemory { input: NpadInput { buttons: 0x4001, stick_l: (100, -200), stick_r: (0, 0) }, ..Default::default() };
        hid.mapped(base, &mut mmu);
        let no1 = base + NPAD_OFFSET;
        assert_eq!(mmu.read_u32(no1).unwrap(), STYLE_FULL_KEY);
        let lifo = no1 + FULL_KEY_LIFO_OFFSET;
        assert_eq!(mmu.read_u64(lifo + 8).unwrap(), LIFO_ENTRIES);
        assert_eq!((mmu.read_u64(lifo + 16).unwrap(), mmu.read_u64(lifo + 24).unwrap()), (1, 1));
        let slot = lifo + LIFO_STORAGE_OFFSET + LIFO_ENTRY_SIZE;
        assert_eq!(mmu.read_u64(slot + 16).unwrap(), 0x4001);
        assert_eq!(mmu.read_u32(slot + 28).unwrap() as i32, -200);
        assert_eq!(mmu.read_u32(slot + 40).unwrap() & 1, 1);

        // The tail wraps after 17 states and the count stays at 17.
        for _ in 0..20 {
            hid.sync(&mut mmu);
        }
        assert_eq!((mmu.read_u64(lifo + 16).unwrap(), mmu.read_u64(lifo + 24).unwrap()), (21 % 17, 17));
        let handheld = base + NPAD_OFFSET + NPAD_HANDHELD * NPAD_ENTRY_SIZE;
        assert_eq!(mmu.read_u32(handheld).unwrap(), STYLE_HANDHELD);
        assert_eq!(mmu.read_u64(handheld + HANDHELD_LIFO_OFFSET + 16).unwrap(), 21 % 17);
    }
}
