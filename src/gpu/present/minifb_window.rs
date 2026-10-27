//! Software window presentation through `minifb`.
//!
//! This is the behaviour the runtime had before presentation backends existed: the
//! composited frame is converted to the 0RGB words `minifb` expects and blitted to a
//! native window (X11/Wayland, Win32, Cocoa).

#![allow(dead_code)]

use minifb::{Key, Scale, Window, WindowOptions};

use super::{BackendConfig, Frame, HostKey, PresentBackend, PresentStatus, ScaleMode};

/// Translates a `minifb` key into the runtime's own key type.
///
/// Keys the HID service does not map are dropped here instead of travelling further.
pub fn host_key_from_minifb(key: Key) -> Option<HostKey> {
    Some(match key {
        Key::Left => HostKey::Left,
        Key::Right => HostKey::Right,
        Key::Up => HostKey::Up,
        Key::Down => HostKey::Down,
        Key::W => HostKey::W,
        Key::A => HostKey::A,
        Key::S => HostKey::S,
        Key::D => HostKey::D,
        Key::Space => HostKey::Space,
        Key::J => HostKey::J,
        Key::K => HostKey::K,
        Key::U => HostKey::U,
        Key::I => HostKey::I,
        Key::Q => HostKey::Q,
        Key::E => HostKey::E,
        Key::Z => HostKey::Z,
        Key::C => HostKey::C,
        Key::Enter => HostKey::Enter,
        Key::Backspace => HostKey::Backspace,
        Key::Escape => HostKey::Escape,
        _ => return None,
    })
}

fn minifb_scale(mode: ScaleMode) -> Scale {
    match mode {
        ScaleMode::Native => Scale::X1,
        // minifb has no aspect-preserving mode of its own; FitScreen is the closest,
        // and the GPU backend is where proper letterboxing lands.
        ScaleMode::Integer | ScaleMode::FitAspect => Scale::FitScreen,
    }
}

pub struct MinifbBackend {
    window: Window,
    /// Scratch 0RGB buffer, reused every frame so presenting allocates nothing.
    blit: Vec<u32>,
    width: usize,
    height: usize,
    frames: u64,
}

impl MinifbBackend {
    pub fn new(config: &BackendConfig) -> Result<Self, String> {
        let mut opts = WindowOptions::default();
        opts.scale = minifb_scale(config.scale);
        opts.resize = true;

        let mut window = Window::new(
            &format!("{} [{} FPS Target]", config.title, config.target_fps),
            config.width,
            config.height,
            opts,
        )
        .map_err(|e| format!("failed to create host window: {e}"))?;

        window.set_target_fps(config.target_fps as usize);

        Ok(Self {
            window,
            blit: vec![0u32; config.width * config.height],
            width: config.width,
            height: config.height,
            frames: 0,
        })
    }
}

impl PresentBackend for MinifbBackend {
    fn name(&self) -> &'static str {
        "minifb"
    }

    fn present(&mut self, frame: &Frame) -> PresentStatus {
        if !self.window.is_open() || self.window.is_key_down(Key::Escape) {
            return PresentStatus::Closed;
        }

        if frame.width != self.width || frame.height != self.height {
            self.width = frame.width;
            self.height = frame.height;
            self.blit.clear();
            self.blit.resize(frame.pixel_count(), 0);
        }

        for (dst, src) in self.blit.iter_mut().zip(frame.rgba.chunks_exact(4)) {
            *dst = ((src[0] as u32) << 16) | ((src[1] as u32) << 8) | src[2] as u32;
        }

        if let Err(e) = self.window.update_with_buffer(&self.blit, self.width, self.height) {
            eprintln!("[Display Error] Failed to update frame: {e}");
            return PresentStatus::Closed;
        }

        self.frames += 1;
        PresentStatus::Continue
    }

    fn pump_events(&mut self) -> PresentStatus {
        self.window.update();
        if self.window.is_open() && !self.window.is_key_down(minifb::Key::Escape) {
            PresentStatus::Continue
        } else {
            PresentStatus::Closed
        }
    }

    fn pressed_keys(&mut self) -> Vec<HostKey> {
        self.window.get_keys().into_iter().filter_map(host_key_from_minifb).collect()
    }

    fn is_open(&self) -> bool {
        self.window.is_open()
    }

    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    fn frames_presented(&self) -> u64 {
        self.frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minifb_key_mapping_covers_the_hid_bindings() {
        // Every key `HidServer::poll_keyboard` acts on must survive translation.
        let bound = [
            (Key::Left, HostKey::Left),
            (Key::Right, HostKey::Right),
            (Key::Up, HostKey::Up),
            (Key::Down, HostKey::Down),
            (Key::W, HostKey::W),
            (Key::A, HostKey::A),
            (Key::S, HostKey::S),
            (Key::D, HostKey::D),
            (Key::Space, HostKey::Space),
            (Key::J, HostKey::J),
            (Key::K, HostKey::K),
            (Key::U, HostKey::U),
            (Key::I, HostKey::I),
            (Key::Q, HostKey::Q),
            (Key::E, HostKey::E),
            (Key::Z, HostKey::Z),
            (Key::C, HostKey::C),
            (Key::Enter, HostKey::Enter),
            (Key::Backspace, HostKey::Backspace),
            (Key::Escape, HostKey::Escape),
        ];
        for (key, expected) in bound {
            assert_eq!(host_key_from_minifb(key), Some(expected));
        }

        // Unmapped keys are dropped rather than forwarded.
        assert_eq!(host_key_from_minifb(Key::F12), None);
    }

    #[test]
    fn test_scale_mode_translation() {
        // `minifb::Scale` implements neither PartialEq nor Debug, so match on it.
        assert!(matches!(minifb_scale(ScaleMode::Native), Scale::X1));
        assert!(matches!(minifb_scale(ScaleMode::FitAspect), Scale::FitScreen));
        assert!(matches!(minifb_scale(ScaleMode::Integer), Scale::FitScreen));
    }
}
