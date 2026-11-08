#![allow(dead_code)]

//! HID shared memory in the layout libnx reads (`HidSharedMemory` in libnx `services/hid.h`), the
//! `hid` commands that configure it, and a host-side input feed. Offsets and command IDs were taken
//! from libnx's public `hid.h` / `hid.c`.
//!
//! Controllers come from [`InputFeed`]: players 1-8 and Handheld, each with one npad style (full key,
//! handheld, Joy-Con dual / left / right). A controller is visible to the guest only while its style is
//! in the set the guest declared with `SetSupportedNpadStyleSet` and its id is in
//! `SetSupportedNpadIdType` (until the guest declares them, everything is visible). Each `sync` appends
//! one state to the LIFO of every visible controller's style, and one to the touch screen, mouse and
//! keyboard LIFOs. Every LIFO keeps its own sampling numbers: libnx re-reads a LIFO whose stored states
//! are not consecutive.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;
use std::sync::{Arc, Mutex, MutexGuard};

/// Size of the HID shared memory block.
pub const HID_SHARED_MEMORY_SIZE: u64 = 0x40000;
/// `HidSharedMemory::touchscreen`, `mouse`, `keyboard`, `npad`.
pub const TOUCH_SCREEN_OFFSET: u64 = 0x400;
pub const MOUSE_OFFSET: u64 = 0x3400;
pub const KEYBOARD_OFFSET: u64 = 0x3800;
pub const NPAD_OFFSET: u64 = 0x9A00;
/// `sizeof(HidNpadSharedMemoryEntry)`.
pub const NPAD_ENTRY_SIZE: u64 = 0x5000;
/// `HidNpadInternalState` fields.
pub const STYLE_SET_OFFSET: u64 = 0x0;
pub const JOY_ASSIGNMENT_OFFSET: u64 = 0x4;
pub const FULL_KEY_COLOR_OFFSET: u64 = 0x8;
pub const JOY_COLOR_OFFSET: u64 = 0x14;
pub const FULL_KEY_LIFO_OFFSET: u64 = 0x28;
pub const HANDHELD_LIFO_OFFSET: u64 = 0x378;
pub const JOY_DUAL_LIFO_OFFSET: u64 = 0x6C8;
pub const JOY_LEFT_LIFO_OFFSET: u64 = 0xA18;
pub const JOY_RIGHT_LIFO_OFFSET: u64 = 0xD68;
pub const DEVICE_TYPE_OFFSET: u64 = 0x4188;
/// Every LIFO starts with a 0x20-byte `HidCommonLifoHeader` (`unused`, `buffer_count`, `tail`, `count`),
/// then 17 `{ u64 sampling_number; State state; }` slots. `tail` is the newest slot.
pub const LIFO_STORAGE_OFFSET: u64 = 0x20;
pub const LIFO_ENTRIES: u64 = 17;
/// `HidNpadCommonStateAtomicStorage`.
pub const LIFO_ENTRY_SIZE: u64 = 0x30;
/// `HidTouchScreenStateAtomicStorage`: sampling number, then `HidTouchScreenState` (sampling number,
/// count, reserved, 16 `HidTouchState` of 0x28 bytes).
pub const TOUCH_ENTRY_SIZE: u64 = 0x298;
pub const TOUCH_STATE_SIZE: u64 = 0x28;
pub const MAX_TOUCHES: usize = 16;
/// `HidMouseStateAtomicStorage`, `HidKeyboardStateAtomicStorage`.
pub const MOUSE_ENTRY_SIZE: u64 = 0x30;
pub const KEYBOARD_ENTRY_SIZE: u64 = 0x38;

/// npad entry indices: `HidNpadIdType_No1`..`No8` are 0..7, `HidNpadIdType_Handheld` (0x20) is 8.
pub const NPAD_NO1: u64 = 0;
pub const NPAD_HANDHELD: u64 = 8;
pub const NPAD_COUNT: usize = 9;
/// `HidNpadIdType_Handheld`.
pub const NPAD_ID_HANDHELD: u32 = 0x20;

/// `HidNpadStyleTag`.
pub const STYLE_FULL_KEY: u32 = 1 << 0;
pub const STYLE_HANDHELD: u32 = 1 << 1;
pub const STYLE_JOY_DUAL: u32 = 1 << 2;
pub const STYLE_JOY_LEFT: u32 = 1 << 3;
pub const STYLE_JOY_RIGHT: u32 = 1 << 4;
/// `HidNpadStyleSet_NpadStandard`.
pub const STYLE_SET_STANDARD: u32 = 0x1F;
/// `HidDeviceTypeBits`.
pub const DEVICE_FULL_KEY: u32 = 1 << 0;
pub const DEVICE_HANDHELD_LEFT: u32 = 1 << 2;
pub const DEVICE_HANDHELD_RIGHT: u32 = 1 << 3;
pub const DEVICE_JOY_LEFT: u32 = 1 << 4;
pub const DEVICE_JOY_RIGHT: u32 = 1 << 5;
/// `HidNpadAttribute`.
pub const ATTR_CONNECTED: u32 = 1 << 0;
pub const ATTR_WIRED: u32 = 1 << 1;
pub const ATTR_LEFT_CONNECTED: u32 = 1 << 2;
pub const ATTR_LEFT_WIRED: u32 = 1 << 3;
pub const ATTR_RIGHT_CONNECTED: u32 = 1 << 4;
pub const ATTR_RIGHT_WIRED: u32 = 1 << 5;
/// `HidNpadAttribute_IsConnected` | `IsWired`.
pub const ATTRIBUTES_CONNECTED: u32 = ATTR_CONNECTED | ATTR_WIRED;
/// `HidNpadJoyAssignmentMode`.
pub const JOY_ASSIGNMENT_DUAL: u32 = 0;
pub const JOY_ASSIGNMENT_SINGLE: u32 = 1;
/// `HidColorAttribute`.
pub const COLOR_OK: u32 = 0;
pub const COLOR_NO_CONTROLLER: u32 = 2;
/// Synthetic controller colors (body, buttons).
pub const COLOR_BODY: u32 = 0xFF32_3232;
pub const COLOR_BUTTONS: u32 = 0xFFE6_E6E6;
/// `HidTouchAttribute_Start`: set on the first sample of a finger.
pub const TOUCH_START: u32 = 1 << 0;
/// `HidMouseAttribute_IsConnected`.
pub const MOUSE_CONNECTED: u32 = 1 << 1;
/// `HidNpadJoyHoldType_Vertical` (the default) and `_Horizontal`.
pub const JOY_HOLD_VERTICAL: u64 = 0;
pub const JOY_HOLD_HORIZONTAL: u64 = 1;

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

/// The npad styles a host controller can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpadStyle {
    /// Pro Controller (or any host gamepad).
    FullKey,
    /// Joy-Con attached to the console. Only on the Handheld npad.
    Handheld,
    /// A pair of detached Joy-Con.
    JoyDual,
    /// One left Joy-Con held on its own.
    JoyLeft,
    /// One right Joy-Con held on its own.
    JoyRight,
}

impl NpadStyle {
    /// The `HidNpadStyleTag` bit.
    pub fn tag(self) -> u32 {
        match self {
            Self::FullKey => STYLE_FULL_KEY,
            Self::Handheld => STYLE_HANDHELD,
            Self::JoyDual => STYLE_JOY_DUAL,
            Self::JoyLeft => STYLE_JOY_LEFT,
            Self::JoyRight => STYLE_JOY_RIGHT,
        }
    }

    /// Offset of the style's LIFO in `HidNpadInternalState`.
    pub fn lifo_offset(self) -> u64 {
        match self {
            Self::FullKey => FULL_KEY_LIFO_OFFSET,
            Self::Handheld => HANDHELD_LIFO_OFFSET,
            Self::JoyDual => JOY_DUAL_LIFO_OFFSET,
            Self::JoyLeft => JOY_LEFT_LIFO_OFFSET,
            Self::JoyRight => JOY_RIGHT_LIFO_OFFSET,
        }
    }

    /// `HidDeviceTypeBits` of the devices that make up the controller.
    pub fn device_type(self) -> u32 {
        match self {
            Self::FullKey => DEVICE_FULL_KEY,
            Self::Handheld => DEVICE_HANDHELD_LEFT | DEVICE_HANDHELD_RIGHT,
            Self::JoyDual => DEVICE_JOY_LEFT | DEVICE_JOY_RIGHT,
            Self::JoyLeft => DEVICE_JOY_LEFT,
            Self::JoyRight => DEVICE_JOY_RIGHT,
        }
    }

    /// `HidNpadAttribute` bits of a state: handheld Joy-Con are wired through the rails, a host gamepad
    /// reads as a wired Pro Controller, detached Joy-Con are wireless.
    pub fn attributes(self) -> u32 {
        match self {
            Self::FullKey => ATTR_CONNECTED | ATTR_WIRED,
            Self::Handheld => {
                ATTR_CONNECTED | ATTR_WIRED | ATTR_LEFT_CONNECTED | ATTR_LEFT_WIRED | ATTR_RIGHT_CONNECTED | ATTR_RIGHT_WIRED
            }
            Self::JoyDual => ATTR_CONNECTED | ATTR_LEFT_CONNECTED | ATTR_RIGHT_CONNECTED,
            Self::JoyLeft => ATTR_CONNECTED | ATTR_LEFT_CONNECTED,
            Self::JoyRight => ATTR_CONNECTED | ATTR_RIGHT_CONNECTED,
        }
    }

    fn joy_assignment(self) -> u32 {
        match self {
            Self::JoyLeft | Self::JoyRight => JOY_ASSIGNMENT_SINGLE,
            _ => JOY_ASSIGNMENT_DUAL,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// npad entry index of a `HidNpadIdType` (No1..No8 = 0..7, Handheld = 0x20).
pub fn npad_index(id: u32) -> Option<usize> {
    match id {
        0..=7 => Some(id as usize),
        NPAD_ID_HANDHELD => Some(NPAD_HANDHELD as usize),
        _ => None,
    }
}

/// `HidNpadIdType` of an npad entry index.
pub fn npad_id(index: usize) -> u32 {
    if index == NPAD_HANDHELD as usize {
        NPAD_ID_HANDHELD
    } else {
        index as u32
    }
}

/// A connected host controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NpadController {
    pub style: NpadStyle,
    pub input: NpadInput,
}

/// One finger on the touch screen, in screen pixels (1280x720).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TouchPoint {
    /// Stays the same while the finger is down; a new id reads as a new touch.
    pub finger_id: u32,
    pub x: u32,
    pub y: u32,
    pub diameter_x: u32,
    pub diameter_y: u32,
    pub rotation_angle: u32,
}

/// Mouse state. The deltas and wheel deltas go into the next sample only, then reset to zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseInput {
    pub connected: bool,
    pub x: i32,
    pub y: i32,
    pub delta_x: i32,
    pub delta_y: i32,
    pub wheel_delta_x: i32,
    pub wheel_delta_y: i32,
    /// `HidMouseButton` bits.
    pub buttons: u32,
}

/// Keyboard state: `HidKeyboardModifier` bits and a 256-bit set of held keys by USB HID usage ID
/// (`HidKeyboardKey` in libnx).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyboardInput {
    pub modifiers: u64,
    pub keys: [u64; 4],
}

impl KeyboardInput {
    pub fn set_key(&mut self, usage: u8, down: bool) {
        let (word, bit) = (usage as usize / 64, usage % 64);
        if down {
            self.keys[word] |= 1 << bit;
        } else {
            self.keys[word] &= !(1 << bit);
        }
    }

    pub fn is_down(&self, usage: u8) -> bool {
        self.keys[usage as usize / 64] >> (usage % 64) & 1 != 0
    }
}

/// Everything the host feeds into HID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInput {
    /// Indexed like the npad entries: players 1-8 at 0..7, Handheld at 8.
    pub npads: [Option<NpadController>; NPAD_COUNT],
    /// At most `MAX_TOUCHES` are reported.
    pub touches: Vec<TouchPoint>,
    pub mouse: MouseInput,
    pub keyboard: KeyboardInput,
}

impl Default for HostInput {
    /// Player 1 as a full-key controller and the Handheld npad, both idle, so pads set up with
    /// `padInitializeDefault` see a controller either way.
    fn default() -> Self {
        let mut npads = [None; NPAD_COUNT];
        npads[NPAD_NO1 as usize] = Some(NpadController { style: NpadStyle::FullKey, input: NpadInput::default() });
        npads[NPAD_HANDHELD as usize] = Some(NpadController { style: NpadStyle::Handheld, input: NpadInput::default() });
        Self { npads, touches: Vec::new(), mouse: MouseInput::default(), keyboard: KeyboardInput::default() }
    }
}

/// Why the feed refused a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// Not `HidNpadIdType_No1`..`No8` or `Handheld`.
    BadNpadId(u32),
    /// The Handheld npad only takes the Handheld style, and the Handheld style only goes there.
    StyleNotAllowed { id: u32, style: NpadStyle },
    /// `set_npad` on an npad with no controller.
    NotConnected(u32),
}

/// Host-side input API. Clones share one state, so a window or GUI thread can keep a clone and push
/// input while the guest runs; the next `HidSharedMemory::sync` publishes it. Npad ids are
/// `HidNpadIdType` values: 0..=7 for players 1-8, `NPAD_ID_HANDHELD` for Handheld.
#[derive(Debug, Clone, Default)]
pub struct InputFeed(Arc<Mutex<HostInput>>);

impl InputFeed {
    fn lock(&self) -> MutexGuard<'_, HostInput> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn slot(id: u32, style: Option<NpadStyle>) -> Result<usize, InputError> {
        let index = npad_index(id).ok_or(InputError::BadNpadId(id))?;
        if let Some(style) = style {
            if (id == NPAD_ID_HANDHELD) != (style == NpadStyle::Handheld) {
                return Err(InputError::StyleNotAllowed { id, style });
            }
        }
        Ok(index)
    }

    /// Plugs a controller into npad `id` (or changes its style), keeping any input already set.
    pub fn connect(&self, id: u32, style: NpadStyle) -> Result<(), InputError> {
        let index = Self::slot(id, Some(style))?;
        let mut host = self.lock();
        let input = host.npads[index].map(|c| c.input).unwrap_or_default();
        host.npads[index] = Some(NpadController { style, input });
        Ok(())
    }

    /// Unplugs npad `id`.
    pub fn disconnect(&self, id: u32) -> Result<(), InputError> {
        let index = Self::slot(id, None)?;
        self.lock().npads[index] = None;
        Ok(())
    }

    /// Sets the buttons and sticks of the controller on npad `id`.
    pub fn set_npad(&self, id: u32, input: NpadInput) -> Result<(), InputError> {
        let index = Self::slot(id, None)?;
        match &mut self.lock().npads[index] {
            Some(c) => {
                c.input = input;
                Ok(())
            }
            None => Err(InputError::NotConnected(id)),
        }
    }

    /// Replaces the fingers on the touch screen (empty when nothing touches it).
    pub fn set_touches(&self, touches: &[TouchPoint]) {
        self.lock().touches = touches.iter().take(MAX_TOUCHES).copied().collect();
    }

    pub fn set_mouse(&self, mouse: MouseInput) {
        self.lock().mouse = mouse;
    }

    pub fn set_keyboard(&self, keyboard: KeyboardInput) {
        self.lock().keyboard = keyboard;
    }

    pub fn snapshot(&self) -> HostInput {
        self.lock().clone()
    }
}

/// Writes host input into the HID shared memory the guest mapped, and keeps the configuration the
/// guest sent through the `hid` service.
#[derive(Debug, Default)]
pub struct HidSharedMemory {
    /// Guest address of the mapped block, once the guest has mapped it.
    pub addr: Option<u64>,
    /// Handle of the shared memory object handed out by `IAppletResource::GetSharedMemoryHandle`.
    pub handle: Option<u32>,
    /// The runner's main host controller (window keyboard + gamepad). When it changes, `sync` copies it
    /// into player 1 and Handheld in `feed`; input set through `feed` stays until the next change here.
    pub input: NpadInput,
    /// Host-side input API (see [`InputFeed`]).
    pub feed: InputFeed,
    /// `SetSupportedNpadStyleSet`; `None` until the guest sends it (every style visible).
    pub supported_styles: Option<u32>,
    /// `SetSupportedNpadIdType`; `None` until the guest sends it (every npad visible).
    pub supported_ids: Option<Vec<u32>>,
    /// `SetNpadJoyHoldType`.
    pub joy_hold_type: u64,
    last_input: NpadInput,
    /// Style tag last written to each npad's `style_set` (0 = no controller).
    written_style: [u32; NPAD_COUNT],
    npad_sampling: [[u64; 5]; NPAD_COUNT],
    touch_sampling: u64,
    mouse_sampling: u64,
    keyboard_sampling: u64,
    touching: Vec<u32>,
}

/// Appends one entry to the LIFO at `header`: `write(mmu, slot, sampling)` fills the slot after its
/// leading sampling number, then the header's tail and count move.
fn lifo_push(mmu: &mut Memory, header: u64, entry_size: u64, sampling: &mut u64, write: impl FnOnce(&mut Memory, u64, u64)) {
    *sampling += 1;
    let s = *sampling;
    let tail = (mmu.read_u64(header + 16).unwrap_or(0) + 1) % LIFO_ENTRIES;
    let count = (mmu.read_u64(header + 24).unwrap_or(0) + 1).min(LIFO_ENTRIES);
    let slot = header + LIFO_STORAGE_OFFSET + tail * entry_size;
    let _ = mmu.write_u64(slot, s);
    write(mmu, slot + 8, s);
    let _ = mmu.write_u64(header + 16, tail);
    let _ = mmu.write_u64(header + 24, count);
}

fn lifo_reset(mmu: &mut Memory, header: u64) {
    let _ = mmu.write_u64(header, 0);
    let _ = mmu.write_u64(header + 8, LIFO_ENTRIES);
    let _ = mmu.write_u64(header + 16, 0);
    let _ = mmu.write_u64(header + 24, 0);
}

impl HidSharedMemory {
    /// The guest mapped the HID block at `addr`: reset every LIFO and write the first states.
    pub fn mapped(&mut self, addr: u64, mmu: &mut Memory) {
        self.addr = Some(addr);
        self.npad_sampling = Default::default();
        self.touch_sampling = 0;
        self.mouse_sampling = 0;
        self.keyboard_sampling = 0;
        self.touching.clear();
        for header in [addr + TOUCH_SCREEN_OFFSET, addr + MOUSE_OFFSET, addr + KEYBOARD_OFFSET] {
            lifo_reset(mmu, header);
        }
        for index in 0..NPAD_COUNT {
            let npad = Self::npad_base(addr, index);
            for style in [NpadStyle::FullKey, NpadStyle::Handheld, NpadStyle::JoyDual, NpadStyle::JoyLeft, NpadStyle::JoyRight] {
                lifo_reset(mmu, npad + style.lifo_offset());
            }
            Self::write_npad_header(mmu, npad, None);
            self.written_style[index] = 0;
        }
        self.sync(mmu);
    }

    pub fn unmapped(&mut self, addr: u64) {
        if self.addr == Some(addr) {
            self.addr = None;
        }
    }

    fn npad_base(addr: u64, index: usize) -> u64 {
        addr + NPAD_OFFSET + index as u64 * NPAD_ENTRY_SIZE
    }

    /// Whether the guest's configuration lets it see a `style` controller on npad entry `index`.
    pub fn visible(&self, index: usize, style: NpadStyle) -> bool {
        self.supported_styles.is_none_or(|s| s & style.tag() != 0)
            && self.supported_ids.as_ref().is_none_or(|ids| ids.contains(&npad_id(index)))
    }

    /// Style set, Joy-Con assignment, colors and device type of one npad.
    fn write_npad_header(mmu: &mut Memory, npad: u64, style: Option<NpadStyle>) {
        let _ = mmu.write_u32(npad + STYLE_SET_OFFSET, style.map_or(0, NpadStyle::tag));
        let _ = mmu.write_u32(npad + JOY_ASSIGNMENT_OFFSET, style.map_or(JOY_ASSIGNMENT_DUAL, NpadStyle::joy_assignment));
        let (attribute, body, buttons) = match style {
            Some(_) => (COLOR_OK, COLOR_BODY, COLOR_BUTTONS),
            None => (COLOR_NO_CONTROLLER, 0, 0),
        };
        // HidNpadFullKeyColorState { attribute, full_key { main, sub } }
        let _ = mmu.write_u32(npad + FULL_KEY_COLOR_OFFSET, attribute);
        let _ = mmu.write_u32(npad + FULL_KEY_COLOR_OFFSET + 4, body);
        let _ = mmu.write_u32(npad + FULL_KEY_COLOR_OFFSET + 8, buttons);
        // HidNpadJoyColorState { attribute, left { main, sub }, right { main, sub } }
        let _ = mmu.write_u32(npad + JOY_COLOR_OFFSET, attribute);
        for i in 0..2 {
            let _ = mmu.write_u32(npad + JOY_COLOR_OFFSET + 4 + i * 8, body);
            let _ = mmu.write_u32(npad + JOY_COLOR_OFFSET + 8 + i * 8, buttons);
        }
        let _ = mmu.write_u32(npad + DEVICE_TYPE_OFFSET, style.map_or(0, NpadStyle::device_type));
    }

    /// Publishes the current host input: one new state in each visible controller's LIFO and in the
    /// touch screen, mouse and keyboard LIFOs.
    pub fn sync(&mut self, mmu: &mut Memory) {
        let Some(addr) = self.addr else { return };
        let feed = self.feed.clone();
        let mut host = feed.lock();
        if self.input != self.last_input {
            self.last_input = self.input;
            for index in [NPAD_NO1 as usize, NPAD_HANDHELD as usize] {
                if let Some(c) = &mut host.npads[index] {
                    c.input = self.input;
                }
            }
        }

        for index in 0..NPAD_COUNT {
            let shown = host.npads[index].filter(|c| self.visible(index, c.style));
            let npad = Self::npad_base(addr, index);
            let tag = shown.map_or(0, |c| c.style.tag());
            if tag != self.written_style[index] {
                Self::write_npad_header(mmu, npad, shown.map(|c| c.style));
                self.written_style[index] = tag;
            }
            let Some(c) = shown else { continue };
            let sampling = &mut self.npad_sampling[index][c.style.index()];
            lifo_push(mmu, npad + c.style.lifo_offset(), LIFO_ENTRY_SIZE, sampling, |mmu, state, s| {
                // HidNpadCommonState { sampling_number; buttons; stick_l { x, y }; stick_r { x, y };
                // attributes; reserved }
                let _ = mmu.write_u64(state, s);
                let _ = mmu.write_u64(state + 8, c.input.buttons);
                let _ = mmu.write_u32(state + 16, c.input.stick_l.0 as u32);
                let _ = mmu.write_u32(state + 20, c.input.stick_l.1 as u32);
                let _ = mmu.write_u32(state + 24, c.input.stick_r.0 as u32);
                let _ = mmu.write_u32(state + 28, c.input.stick_r.1 as u32);
                let _ = mmu.write_u32(state + 32, c.style.attributes());
                let _ = mmu.write_u32(state + 36, 0);
            });
        }

        let touches = &host.touches[..host.touches.len().min(MAX_TOUCHES)];
        let touching = &self.touching;
        lifo_push(mmu, addr + TOUCH_SCREEN_OFFSET, TOUCH_ENTRY_SIZE, &mut self.touch_sampling, |mmu, state, s| {
            // HidTouchScreenState { sampling_number; count; reserved; touches[16] }
            let _ = mmu.write_u64(state, s);
            let _ = mmu.write_u32(state + 8, touches.len() as u32);
            let _ = mmu.write_u32(state + 12, 0);
            for (i, t) in touches.iter().enumerate() {
                // HidTouchState { delta_time; attributes; finger_id; x; y; diameter_x; diameter_y;
                // rotation_angle; reserved }
                let e = state + 16 + i as u64 * TOUCH_STATE_SIZE;
                let attributes = if touching.contains(&t.finger_id) { 0 } else { TOUCH_START };
                let _ = mmu.write_u64(e, 0);
                for (k, v) in [attributes, t.finger_id, t.x, t.y, t.diameter_x, t.diameter_y, t.rotation_angle, 0].into_iter().enumerate() {
                    let _ = mmu.write_u32(e + 8 + k as u64 * 4, v);
                }
            }
        });
        self.touching = touches.iter().map(|t| t.finger_id).collect();

        let m = host.mouse;
        lifo_push(mmu, addr + MOUSE_OFFSET, MOUSE_ENTRY_SIZE, &mut self.mouse_sampling, |mmu, state, s| {
            // HidMouseState { sampling_number; x; y; delta_x; delta_y; wheel_delta_x; wheel_delta_y;
            // buttons; attributes }
            let _ = mmu.write_u64(state, s);
            let attributes = if m.connected { MOUSE_CONNECTED } else { 0 };
            let words = [m.x, m.y, m.delta_x, m.delta_y, m.wheel_delta_x, m.wheel_delta_y, m.buttons as i32, attributes as i32];
            for (k, v) in words.into_iter().enumerate() {
                let _ = mmu.write_u32(state + 8 + k as u64 * 4, v as u32);
            }
        });
        let mouse = &mut host.mouse;
        (mouse.delta_x, mouse.delta_y, mouse.wheel_delta_x, mouse.wheel_delta_y) = (0, 0, 0, 0);

        let k = host.keyboard;
        lifo_push(mmu, addr + KEYBOARD_OFFSET, KEYBOARD_ENTRY_SIZE, &mut self.keyboard_sampling, |mmu, state, s| {
            // HidKeyboardState { sampling_number; modifiers; keys[4] }
            let _ = mmu.write_u64(state, s);
            let _ = mmu.write_u64(state + 8, k.modifiers);
            for (i, w) in k.keys.into_iter().enumerate() {
                let _ = mmu.write_u64(state + 16 + i as u64 * 8, w);
            }
        });
    }
}

impl IpcManager {
    /// libnx-shaped `hid` commands (IDs from libnx `hid.c`). Commands not listed here succeed with no
    /// effect, as before: libnx sends many configuration commands this model does not need.
    pub(super) fn handle_hid(&mut self, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // SetSupportedNpadStyleSet(u32 style_set, aruid)
            100 => {
                self.hid.supported_styles = Some(mmu.read_u32(args).unwrap_or(0));
                // The guest may read shared memory before its next SVC.
                self.hid.sync(mmu);
                self.reply_ok(tls, mmu)
            }
            // GetSupportedNpadStyleSet(aruid) -> u32 (0 until the guest sets it)
            101 => self.reply_data(tls, mmu, &self.hid.supported_styles.unwrap_or(0).to_le_bytes()),
            // SetSupportedNpadIdType(aruid, X buffer of u32 HidNpadIdType)
            102 => {
                let ids = match self.request.send_statics.first() {
                    Some(&(_, buf)) => {
                        let mut bytes = vec![0u8; buf.size.min(0x100) as usize];
                        let _ = mmu.read_bytes(buf.addr, &mut bytes);
                        bytes.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect()
                    }
                    None => Vec::new(),
                };
                self.hid.supported_ids = Some(ids);
                self.hid.sync(mmu);
                self.reply_ok(tls, mmu)
            }
            // AcquireNpadStyleSetUpdateEventHandle(u32 id, aruid, u64) -> copy handle. Never signaled:
            // homebrew polls the style set in shared memory.
            106 => {
                let event = self.allocate_handle(ServiceType::ServiceEvent("NpadStyleSetUpdate"));
                self.reply_event(tls, mmu, event)
            }
            // SetNpadJoyHoldType(u64, aruid), GetNpadJoyHoldType(aruid) -> u64
            120 => {
                self.hid.joy_hold_type = mmu.read_u64(args).unwrap_or(0);
                self.reply_ok(tls, mmu)
            }
            121 => self.reply_data(tls, mmu, &self.hid.joy_hold_type.to_le_bytes()),
            _ => self.reply_data(tls, mmu, &[0; 4]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{Harness, Req, BUF};
    use crate::mmu::Permission;

    const BASE: u64 = 0x6000_0000;

    fn mapped(input: NpadInput) -> (Memory, HidSharedMemory) {
        let mut mmu = Memory::new();
        mmu.map_range(BASE, HID_SHARED_MEMORY_SIZE as usize, Permission::READ_WRITE);
        let mut hid = HidSharedMemory { input, ..Default::default() };
        hid.mapped(BASE, &mut mmu);
        (mmu, hid)
    }

    /// libnx `_hidGetStates` for one state: the newest entry of the LIFO at `header`, or `None` if the
    /// LIFO is empty. Also checks the slot's two sampling numbers agree, as libnx does.
    fn newest(mmu: &Memory, header: u64, entry_size: u64) -> Option<u64> {
        if mmu.read_u64(header + 24).unwrap() == 0 {
            return None;
        }
        let tail = mmu.read_u64(header + 16).unwrap();
        let slot = header + LIFO_STORAGE_OFFSET + tail * entry_size;
        assert_eq!(mmu.read_u64(slot).unwrap(), mmu.read_u64(slot + 8).unwrap());
        Some(slot + 8)
    }

    /// What libnx `padUpdate` reads for one npad id: `(style_set, buttons, stick_l, attributes)`.
    fn read_npad(mmu: &Memory, id: u32) -> (u32, u64, (i32, i32), u32) {
        let npad = HidSharedMemory::npad_base(BASE, npad_index(id).unwrap());
        let style_set = mmu.read_u32(npad).unwrap();
        let lifo = match style_set {
            STYLE_FULL_KEY => FULL_KEY_LIFO_OFFSET,
            STYLE_HANDHELD => HANDHELD_LIFO_OFFSET,
            STYLE_JOY_DUAL => JOY_DUAL_LIFO_OFFSET,
            STYLE_JOY_LEFT => JOY_LEFT_LIFO_OFFSET,
            STYLE_JOY_RIGHT => JOY_RIGHT_LIFO_OFFSET,
            _ => return (style_set, 0, (0, 0), 0),
        };
        let state = newest(mmu, npad + lifo, LIFO_ENTRY_SIZE).expect("connected npad with an empty LIFO");
        let stick = (mmu.read_u32(state + 16).unwrap() as i32, mmu.read_u32(state + 20).unwrap() as i32);
        (style_set, mmu.read_u64(state + 8).unwrap(), stick, mmu.read_u32(state + 32).unwrap())
    }

    #[test]
    fn lifo_entries_follow_libnx_layout() {
        let (mut mmu, mut hid) = mapped(NpadInput { buttons: 0x4001, stick_l: (100, -200), stick_r: (0, 0) });
        let no1 = BASE + NPAD_OFFSET;
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
        let handheld = BASE + NPAD_OFFSET + NPAD_HANDHELD * NPAD_ENTRY_SIZE;
        assert_eq!(mmu.read_u32(handheld).unwrap(), STYLE_HANDHELD);
        assert_eq!(mmu.read_u32(handheld + DEVICE_TYPE_OFFSET).unwrap(), DEVICE_HANDHELD_LEFT | DEVICE_HANDHELD_RIGHT);
        assert_eq!(mmu.read_u64(handheld + HANDHELD_LIFO_OFFSET + 16).unwrap(), 21 % 17);
    }

    /// libnx retries a read while two stored neighbours are not consecutive, so every LIFO counts on
    /// its own, including across a style change.
    #[test]
    fn sampling_numbers_are_consecutive_per_lifo() {
        let (mut mmu, mut hid) = mapped(NpadInput::default());
        hid.feed.connect(1, NpadStyle::JoyDual).unwrap();
        for _ in 0..3 {
            hid.sync(&mut mmu);
        }
        hid.feed.connect(1, NpadStyle::JoyLeft).unwrap();
        for _ in 0..30 {
            hid.sync(&mut mmu);
        }
        let npad = HidSharedMemory::npad_base(BASE, 1);
        for header in [BASE + NPAD_OFFSET + FULL_KEY_LIFO_OFFSET, npad + JOY_DUAL_LIFO_OFFSET, npad + JOY_LEFT_LIFO_OFFSET, BASE + TOUCH_SCREEN_OFFSET] {
            let entry = if header == BASE + TOUCH_SCREEN_OFFSET { TOUCH_ENTRY_SIZE } else { LIFO_ENTRY_SIZE };
            let (tail, count) = (mmu.read_u64(header + 16).unwrap(), mmu.read_u64(header + 24).unwrap());
            let at = |i: u64| mmu.read_u64(header + LIFO_STORAGE_OFFSET + ((tail + LIFO_ENTRIES - i) % LIFO_ENTRIES) * entry + 8).unwrap();
            for i in 1..count {
                assert_eq!(at(i - 1), at(i) + 1, "LIFO at {header:#x}");
            }
        }
        assert_eq!(mmu.read_u64(npad + JOY_DUAL_LIFO_OFFSET + 24).unwrap(), 3);
        assert_eq!(read_npad(&mmu, 1).0, STYLE_JOY_LEFT);
        assert_eq!(mmu.read_u32(npad + JOY_ASSIGNMENT_OFFSET).unwrap(), JOY_ASSIGNMENT_SINGLE);
    }

    #[test]
    fn feed_drives_players_styles_and_disconnects() {
        let (mut mmu, mut hid) = mapped(NpadInput::default());
        let feed = hid.feed.clone();
        feed.connect(1, NpadStyle::JoyDual).unwrap();
        feed.set_npad(1, NpadInput { buttons: 1 << 3, stick_l: (-5, 7), stick_r: (0, 0) }).unwrap();
        feed.connect(7, NpadStyle::JoyRight).unwrap();
        assert_eq!(feed.connect(NPAD_ID_HANDHELD, NpadStyle::FullKey), Err(InputError::StyleNotAllowed { id: NPAD_ID_HANDHELD, style: NpadStyle::FullKey }));
        assert_eq!(feed.connect(2, NpadStyle::Handheld), Err(InputError::StyleNotAllowed { id: 2, style: NpadStyle::Handheld }));
        assert_eq!(feed.set_npad(3, NpadInput::default()), Err(InputError::NotConnected(3)));
        assert_eq!(feed.connect(8, NpadStyle::FullKey), Err(InputError::BadNpadId(8)));
        hid.sync(&mut mmu);

        assert_eq!(read_npad(&mmu, 1), (STYLE_JOY_DUAL, 1 << 3, (-5, 7), ATTR_CONNECTED | ATTR_LEFT_CONNECTED | ATTR_RIGHT_CONNECTED));
        assert_eq!(read_npad(&mmu, 7).0, STYLE_JOY_RIGHT);
        let p8 = HidSharedMemory::npad_base(BASE, 7);
        assert_eq!(mmu.read_u32(p8 + DEVICE_TYPE_OFFSET).unwrap(), DEVICE_JOY_RIGHT);
        assert_eq!(mmu.read_u32(p8 + JOY_COLOR_OFFSET).unwrap(), COLOR_OK);
        assert_eq!(read_npad(&mmu, 2).0, 0);
        assert_eq!(mmu.read_u32(HidSharedMemory::npad_base(BASE, 2) + FULL_KEY_COLOR_OFFSET).unwrap(), COLOR_NO_CONTROLLER);

        // The runner's main controller still lands on player 1 and Handheld when it changes.
        hid.input = NpadInput { buttons: 1, ..Default::default() };
        hid.sync(&mut mmu);
        assert_eq!(read_npad(&mmu, 0).1, 1);
        assert_eq!(read_npad(&mmu, NPAD_ID_HANDHELD).1, 1);

        feed.disconnect(1).unwrap();
        hid.sync(&mut mmu);
        let p2 = HidSharedMemory::npad_base(BASE, 1);
        assert_eq!((mmu.read_u32(p2).unwrap(), mmu.read_u32(p2 + DEVICE_TYPE_OFFSET).unwrap()), (0, 0));
    }

    #[test]
    fn guest_configuration_hides_unsupported_controllers() {
        let (mut mmu, mut hid) = mapped(NpadInput::default());
        hid.feed.connect(2, NpadStyle::JoyLeft).unwrap();
        hid.supported_styles = Some(STYLE_FULL_KEY | STYLE_JOY_DUAL);
        hid.supported_ids = Some(vec![0, 1, 2]);
        hid.sync(&mut mmu);
        assert_eq!(read_npad(&mmu, 0).0, STYLE_FULL_KEY);
        assert_eq!(read_npad(&mmu, 2).0, 0, "JoyLeft is not a supported style");
        assert_eq!(read_npad(&mmu, NPAD_ID_HANDHELD).0, 0, "Handheld is neither a supported id nor style");
        hid.supported_styles = Some(STYLE_SET_STANDARD);
        hid.sync(&mut mmu);
        assert_eq!(read_npad(&mmu, 2).0, STYLE_JOY_LEFT);
        assert_eq!(read_npad(&mmu, NPAD_ID_HANDHELD).0, 0);
    }

    #[test]
    fn touch_mouse_and_keyboard_lifos() {
        let (mut mmu, mut hid) = mapped(NpadInput::default());
        let feed = hid.feed.clone();
        feed.set_touches(&[TouchPoint { finger_id: 4, x: 640, y: 360, diameter_x: 10, diameter_y: 12, rotation_angle: 0 }]);
        feed.set_mouse(MouseInput { connected: true, x: 10, y: 20, delta_x: 3, delta_y: -1, buttons: 1, ..Default::default() });
        let mut keys = KeyboardInput { modifiers: 1 << 1, ..Default::default() };
        keys.set_key(0x04, true); // HidKeyboardKey_A
        keys.set_key(0xE1, true); // HidKeyboardKey_LeftShift
        feed.set_keyboard(keys);
        hid.sync(&mut mmu);

        let touch = newest(&mmu, BASE + TOUCH_SCREEN_OFFSET, TOUCH_ENTRY_SIZE).unwrap();
        assert_eq!(mmu.read_u32(touch + 8).unwrap(), 1);
        let t = touch + 16;
        let word = |off: u64| mmu.read_u32(t + off).unwrap();
        assert_eq!((word(8), word(12), word(16), word(20), word(24)), (TOUCH_START, 4, 640, 360, 10));
        hid.sync(&mut mmu);
        let touch = newest(&mmu, BASE + TOUCH_SCREEN_OFFSET, TOUCH_ENTRY_SIZE).unwrap();
        assert_eq!(mmu.read_u32(touch + 16 + 8).unwrap(), 0, "Start only on the first sample");
        feed.set_touches(&[]);
        hid.sync(&mut mmu);
        let touch = newest(&mmu, BASE + TOUCH_SCREEN_OFFSET, TOUCH_ENTRY_SIZE).unwrap();
        assert_eq!(mmu.read_u32(touch + 8).unwrap(), 0);

        let mouse = newest(&mmu, BASE + MOUSE_OFFSET, MOUSE_ENTRY_SIZE).unwrap();
        let m = |off: u64| mmu.read_u32(mouse + off).unwrap();
        // Deltas were reported once, on the first sync.
        assert_eq!((m(8), m(12), m(16), m(32), m(36)), (10, 20, 0, 1, MOUSE_CONNECTED));
        let first = BASE + MOUSE_OFFSET + LIFO_STORAGE_OFFSET + MOUSE_ENTRY_SIZE * 2 + 8;
        assert_eq!((mmu.read_u32(first + 16).unwrap() as i32, mmu.read_u32(first + 20).unwrap() as i32), (3, -1));

        let kb = newest(&mmu, BASE + KEYBOARD_OFFSET, KEYBOARD_ENTRY_SIZE).unwrap();
        assert_eq!(mmu.read_u64(kb + 8).unwrap(), 1 << 1);
        assert_eq!(mmu.read_u64(kb + 16).unwrap(), 1 << 4);
        assert_eq!(mmu.read_u64(kb + 16 + 3 * 8).unwrap(), 1 << (0xE1 - 0xC0));
    }

    /// `hid` commands as libnx sends them: `padConfigureInput(1, NpadStandard)` (ActivateNpad,
    /// SetSupportedNpadIdType with [No1, Handheld], SetSupportedNpadStyleSet), the joy hold type and the
    /// style-set update event.
    #[test]
    fn libnx_hid_configuration_commands() {
        let mut h = Harness::new();
        let base = 0x6000_0000;
        h.mmu.map_range(base, HID_SHARED_MEMORY_SIZE as usize, Permission::READ_WRITE);
        h.ipc.hid.feed.connect(3, NpadStyle::FullKey).unwrap();
        h.ipc.hid.mapped(base, &mut h.mmu);
        let hid = h.ipc.allocate_handle(ServiceType::Hid);
        let npad_style = |h: &Harness, index: u64| h.mmu.read_u32(base + NPAD_OFFSET + index * NPAD_ENTRY_SIZE).unwrap();
        assert_eq!(npad_style(&h, 3), STYLE_FULL_KEY);

        let aruid = 0u64.to_le_bytes();
        assert_eq!(h.call(hid, Req { cmd: 103, data: &aruid, pid: true, ..Default::default() }).result, 0);
        let ids: Vec<u8> = [0u32, NPAD_ID_HANDHELD].iter().flat_map(|i| i.to_le_bytes()).collect();
        h.mmu.write_bytes(BUF, &ids).unwrap();
        let r = h.call(hid, Req { cmd: 102, data: &aruid, pid: true, send_static: Some((BUF, ids.len() as u64)), ..Default::default() });
        assert_eq!(r.result, 0);
        assert_eq!(h.ipc.hid.supported_ids, Some(vec![0, NPAD_ID_HANDHELD]));
        // Applied at once: player 4 is not a supported id any more.
        assert_eq!(npad_style(&h, 3), 0);

        let mut style = [0u8; 16];
        style[..4].copy_from_slice(&STYLE_SET_STANDARD.to_le_bytes());
        assert_eq!(h.call(hid, Req { cmd: 100, data: &style, pid: true, ..Default::default() }).result, 0);
        let r = h.call_out(hid, Req { cmd: 101, data: &aruid, pid: true, ..Default::default() }, 4);
        assert_eq!(r.u32(0), STYLE_SET_STANDARD);
        assert_eq!(npad_style(&h, 0), STYLE_FULL_KEY);
        assert_eq!(npad_style(&h, NPAD_HANDHELD), STYLE_HANDHELD);

        let mut hold = [0u8; 16];
        hold[..8].copy_from_slice(&JOY_HOLD_HORIZONTAL.to_le_bytes());
        assert_eq!(h.call(hid, Req { cmd: 120, data: &hold, pid: true, ..Default::default() }).result, 0);
        assert_eq!(h.call_out(hid, Req { cmd: 121, data: &aruid, pid: true, ..Default::default() }, 8).u64(0), JOY_HOLD_HORIZONTAL);

        let r = h.call(hid, Req { cmd: 106, data: &[0; 24], pid: true, ..Default::default() });
        assert_eq!(r.result, 0);
        assert_eq!(h.ipc.handles.get(&r.copy_handles[0]), Some(&ServiceType::ServiceEvent("NpadStyleSetUpdate")));
    }
}
