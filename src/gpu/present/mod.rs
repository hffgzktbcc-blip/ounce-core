//! Host presentation backends.
//!
//! The guest framebuffer is composited on the CPU into a [`Frame`] and handed to a
//! [`PresentBackend`]. Backends differ only in how that frame reaches the host:
//! not at all ([`headless`]), through a software window ([`minifb_window`]), or
//! through a GPU swapchain (added in a later step, behind the `gpu-present` feature).
//!
//! This module owns no guest-side GPU modelling; that stays in [`crate::gpu::vulkan`].

#![allow(dead_code)]

pub mod headless;
pub mod minifb_window;

/// Host keyboard keys the HID service maps to Horizon controller buttons.
///
/// Declaring our own key type keeps `hid` and the rest of the runtime independent of
/// whichever windowing crate a backend happens to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostKey {
    Left,
    Right,
    Up,
    Down,
    W,
    A,
    S,
    D,
    Space,
    J,
    K,
    U,
    I,
    Q,
    E,
    Z,
    C,
    Enter,
    Backspace,
    Escape,
}

/// How a backend scales the guest frame into the host window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    /// One guest pixel per host pixel.
    Native,
    /// Largest integer multiple that fits, letterboxed.
    Integer,
    /// Fit the window, preserving aspect ratio and letterboxing.
    FitAspect,
}

/// A composited host-ready frame: tightly packed RGBA8, `width * height * 4` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Frame {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            rgba: vec![0u8; width * height * 4],
        }
    }

    pub fn pixel_count(&self) -> usize {
        self.width * self.height
    }

    /// Writes one opaque pixel. Out-of-range coordinates are ignored.
    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = (y * self.width + x) * 4;
        self.rgba[i] = r;
        self.rgba[i + 1] = g;
        self.rgba[i + 2] = b;
        self.rgba[i + 3] = 0xFF;
    }

    pub fn pixel(&self, x: usize, y: usize) -> Option<(u8, u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y * self.width + x) * 4;
        Some((self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]))
    }

    /// Resizes in place, dropping previous contents, when the presentation mode changes.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.rgba.clear();
        self.rgba.resize(width * height * 4, 0);
    }

    /// FNV-1a hash of the pixel bytes, used by the headless backend for golden tests.
    pub fn content_hash(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in &self.rgba {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash
    }
}

/// Result of handing a frame to a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentStatus {
    /// The frame reached the host; keep running.
    Continue,
    /// The host window is gone (closed, or Escape pressed); the runtime should stop.
    Closed,
}

/// Parameters a backend needs when it opens its surface.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub width: usize,
    pub height: usize,
    pub title: String,
    pub target_fps: u64,
    pub scale: ScaleMode,
}

/// A host presentation target.
pub trait PresentBackend {
    /// Short identifier used in logs (`headless`, `minifb`, `wgpu`).
    fn name(&self) -> &'static str;

    /// Shows a composited frame. Also pumps host events, so it must be called every frame.
    fn present(&mut self, frame: &Frame) -> PresentStatus;

    /// Host keys held down since the last [`PresentBackend::present`].
    fn pressed_keys(&mut self) -> Vec<HostKey> {
        Vec::new()
    }

    /// Whether a host surface is currently open. Always false for headless.
    fn is_open(&self) -> bool {
        false
    }

    /// Updates the window title; a no-op where there is no window.
    fn set_title(&mut self, _title: &str) {}

    /// Number of frames this backend has accepted.
    fn frames_presented(&self) -> u64;
}

/// Which backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Best available: GPU if compiled in and usable, then minifb, then headless.
    Auto,
    Headless,
    Minifb,
    /// Reserved for the `gpu-present` feature; currently falls back.
    Gpu,
}

impl BackendKind {
    /// Parses a `--present=` / `OUNCE_PRESENT=` value.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "headless" | "none" => Some(Self::Headless),
            "minifb" | "software" | "cpu" => Some(Self::Minifb),
            "gpu" | "wgpu" | "vulkan" | "metal" => Some(Self::Gpu),
            _ => None,
        }
    }

    /// Reads `OUNCE_PRESENT`, defaulting to [`BackendKind::Auto`].
    pub fn from_env() -> Self {
        std::env::var("OUNCE_PRESENT")
            .ok()
            .and_then(|v| Self::parse(&v))
            .unwrap_or(Self::Auto)
    }
}

/// Opens a backend, falling back in order until one initialises.
///
/// `Auto` and `Gpu` currently resolve to minifb because the GPU backend lands in the
/// next step; `Gpu` logs that it fell back so the choice is never silently ignored.
pub fn open_backend(kind: BackendKind, config: &BackendConfig) -> Box<dyn PresentBackend> {
    let requested_gpu = matches!(kind, BackendKind::Gpu);
    if requested_gpu {
        eprintln!("[Display] GPU presentation is not compiled in; falling back to the software window.");
    }

    match kind {
        BackendKind::Headless => Box::new(headless::HeadlessBackend::new(config)),
        BackendKind::Auto | BackendKind::Minifb | BackendKind::Gpu => {
            match minifb_window::MinifbBackend::new(config) {
                Ok(backend) => Box::new(backend),
                Err(e) => {
                    eprintln!("[Display] Software window unavailable ({e}); presenting headless.");
                    Box::new(headless::HeadlessBackend::new(config))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_pixels_resize_and_hash() {
        let mut frame = Frame::new(4, 2);
        assert_eq!(frame.rgba.len(), 32);
        assert_eq!(frame.pixel_count(), 8);

        let empty_hash = frame.content_hash();
        frame.set_pixel(1, 1, 0x10, 0x20, 0x30);
        assert_eq!(frame.pixel(1, 1), Some((0x10, 0x20, 0x30, 0xFF)));
        assert_ne!(frame.content_hash(), empty_hash);

        // Out-of-range writes and reads are ignored rather than panicking.
        frame.set_pixel(4, 0, 1, 2, 3);
        frame.set_pixel(0, 2, 1, 2, 3);
        assert_eq!(frame.pixel(4, 0), None);

        frame.resize(2, 2);
        assert_eq!(frame.rgba.len(), 16);
        assert_eq!(frame.pixel(1, 1), Some((0, 0, 0, 0)));
    }

    #[test]
    fn test_backend_kind_parsing() {
        assert_eq!(BackendKind::parse("auto"), Some(BackendKind::Auto));
        assert_eq!(BackendKind::parse(" Headless "), Some(BackendKind::Headless));
        assert_eq!(BackendKind::parse("minifb"), Some(BackendKind::Minifb));
        assert_eq!(BackendKind::parse("GPU"), Some(BackendKind::Gpu));
        assert_eq!(BackendKind::parse("potato"), None);
    }

    #[test]
    fn test_open_backend_headless_is_always_available() {
        let config = BackendConfig {
            width: 8,
            height: 4,
            title: "test".to_string(),
            target_fps: 60,
            scale: ScaleMode::Native,
        };
        let mut backend = open_backend(BackendKind::Headless, &config);
        assert_eq!(backend.name(), "headless");
        assert!(!backend.is_open());
        assert_eq!(backend.present(&Frame::new(8, 4)), PresentStatus::Continue);
        assert_eq!(backend.frames_presented(), 1);
        assert!(backend.pressed_keys().is_empty());
    }
}
