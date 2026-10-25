#![allow(dead_code)]

use crate::mmu::{Memory, MemoryError, Permission};
use minifb::Key;

pub const HID_SHARED_MEM_ADDR: u64 = 0x10000000;
pub const HID_SHARED_MEM_SIZE: usize = 0x40000; // 256 KB standard Horizon HID pool

/// Nintendo Switch & Switch 2 Controller Button Bitmask (NpadButton).
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpadButton {
    A = 1 << 0,
    B = 1 << 1,
    X = 1 << 2,
    Y = 1 << 3,
    LStick = 1 << 4,
    RStick = 1 << 5,
    L = 1 << 6,
    R = 1 << 7,
    ZL = 1 << 8,
    ZR = 1 << 9,
    Plus = 1 << 10,
    Minus = 1 << 11,
    DLeft = 1 << 12,
    DUp = 1 << 13,
    DRight = 1 << 14,
    DDown = 1 << 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoyCon2AttachMode {
    /// Switch 2 Joy-Con magnetically latched to console body (Handheld Mode)
    MagneticallyLatchedHandheld,
    /// Joy-Con detached, operating wirelessly as a dual pair
    DualWirelessTabletop,
    /// Single Joy-Con Left (horizontal)
    SingleJoyConLeft,
    /// Single Joy-Con Right (horizontal)
    SingleJoyConRight,
    /// Switch 2 Pro Controller
    ProController,
}

/// Controller state for Player 1.
#[derive(Debug, Clone)]
pub struct ControllerState {
    pub buttons: u64,
    pub analog_lx: i32,
    pub analog_ly: i32,
    pub analog_rx: i32,
    pub analog_ry: i32,
    pub mode: JoyCon2AttachMode,
    pub is_magnetically_attached: bool,
}

impl Default for ControllerState {
    fn default() -> Self {
        Self {
            buttons: 0,
            analog_lx: 0,
            analog_ly: 0,
            analog_rx: 0,
            analog_ry: 0,
            mode: JoyCon2AttachMode::MagneticallyLatchedHandheld,
            is_magnetically_attached: true,
        }
    }
}

/// The Horizon Human Interface Device (HID) Service.
pub struct HidServer {
    pub shared_mem_addr: u64,
    pub state: ControllerState,
    pub sample_counter: u64,
}

impl Default for HidServer {
    fn default() -> Self {
        Self::new(HID_SHARED_MEM_ADDR)
    }
}

impl HidServer {
    pub fn new(shared_mem_addr: u64) -> Self {
        Self {
            shared_mem_addr,
            state: ControllerState::default(),
            sample_counter: 0,
        }
    }

    /// Initializes and maps the HID shared memory region in virtual memory.
    pub fn init_memory(&self, mmu: &mut Memory) {
        mmu.map_range(
            self.shared_mem_addr,
            HID_SHARED_MEM_SIZE,
            Permission::READ_WRITE,
        );
    }

    /// Translates host keyboard events into Nintendo Switch controller buttons.
    pub fn poll_keyboard(&mut self, keys: &[Key]) {
        let mut buttons: u64 = 0;
        let mut lx: i32 = 0;
        let mut ly: i32 = 0;

        for key in keys {
            match key {
                // D-Pad / Left Stick: Movement
                Key::Left | Key::A => {
                    buttons |= NpadButton::DLeft as u64;
                    lx = -32767;
                }
                Key::Right | Key::D => {
                    buttons |= NpadButton::DRight as u64;
                    lx = 32767;
                }
                Key::Up | Key::W => {
                    buttons |= NpadButton::DUp as u64;
                    ly = 32767;
                }
                Key::Down | Key::S => {
                    buttons |= NpadButton::DDown as u64;
                    ly = -32767;
                }

                // Action Buttons
                Key::Space | Key::J => buttons |= NpadButton::A as u64,
                Key::K => buttons |= NpadButton::B as u64,
                Key::U => buttons |= NpadButton::X as u64,
                Key::I => buttons |= NpadButton::Y as u64,

                // Shoulder & Triggers
                Key::Q => buttons |= NpadButton::L as u64,
                Key::E => buttons |= NpadButton::R as u64,
                Key::Z => buttons |= NpadButton::ZL as u64,
                Key::C => buttons |= NpadButton::ZR as u64,

                // System
                Key::Enter => buttons |= NpadButton::Plus as u64,
                Key::Backspace => buttons |= NpadButton::Minus as u64,
                _ => {}
            }
        }

        self.state.buttons = buttons;
        self.state.analog_lx = lx;
        self.state.analog_ly = ly;
    }

    /// Sets or clears a single controller button state directly.
    pub fn set_button_state(&mut self, button: NpadButton, pressed: bool) {
        if pressed {
            self.state.buttons |= button as u64;
        } else {
            self.state.buttons &= !(button as u64);
        }
    }

    /// Sets analog stick coordinates with deadzone filtering (-1.0..=1.0 mapped to -32767..=32767).
    pub fn apply_gamepad_axes(&mut self, lx: f32, ly: f32, rx: f32, ry: f32, deadzone: f32) {
        let filter_axis = |val: f32| -> i32 {
            if val.abs() < deadzone {
                0
            } else {
                (val.clamp(-1.0, 1.0) * 32767.0) as i32
            }
        };

        self.state.analog_lx = filter_axis(lx);
        self.state.analog_ly = filter_axis(ly);
        self.state.analog_rx = filter_axis(rx);
        self.state.analog_ry = filter_axis(ry);
    }

    /// Writes controller state into Horizon Shared Memory for the guest game to read.
    pub fn update_shared_memory(&mut self, mmu: &mut Memory) -> Result<(), MemoryError> {
        self.sample_counter += 1;

        // Shared Memory Layout:
        // Offset 0x00: Sampling Number (u64)
        // Offset 0x08: Buttons Bitmask (u64)
        // Offset 0x10: Analog Left X (i32)
        // Offset 0x14: Analog Left Y (i32)
        // Offset 0x18: Analog Right X (i32)
        // Offset 0x1C: Analog Right Y (i32)
        let base = self.shared_mem_addr;
        mmu.write_u64(base, self.sample_counter)?;
        mmu.write_u64(base + 8, self.state.buttons)?;
        mmu.write_u32(base + 16, self.state.analog_lx as u32)?;
        mmu.write_u32(base + 20, self.state.analog_ly as u32)?;
        mmu.write_u32(base + 24, self.state.analog_rx as u32)?;
        mmu.write_u32(base + 28, self.state.analog_ry as u32)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hid_shared_memory_gamepad_update() {
        let mut mmu = Memory::new();
        let hid_addr = 0x1000_0000u64;
        let mut hid = HidServer::new(hid_addr);
        hid.init_memory(&mut mmu);

        hid.set_button_state(NpadButton::A, true);
        hid.set_button_state(NpadButton::ZR, true);
        hid.apply_gamepad_axes(0.75, -0.5, 0.05, 0.0, 0.1); // RX is within 0.1 deadzone -> 0
        hid.update_shared_memory(&mut mmu).unwrap();

        assert_eq!(mmu.read_u64(hid_addr).unwrap(), 1); // Sample counter 1
        let btns = mmu.read_u64(hid_addr + 8).unwrap();
        assert_eq!(btns & (NpadButton::A as u64), NpadButton::A as u64);
        assert_eq!(btns & (NpadButton::ZR as u64), NpadButton::ZR as u64);

        let lx = mmu.read_u32(hid_addr + 16).unwrap() as i32;
        assert!(lx > 20000); // 0.75 * 32767
        let rx = mmu.read_u32(hid_addr + 24).unwrap() as i32;
        assert_eq!(rx, 0); // Filtered by deadzone!
    }
}
