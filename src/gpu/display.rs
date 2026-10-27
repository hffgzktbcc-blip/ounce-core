#![allow(dead_code)]

use crate::mmu::Memory;
use std::time::Instant;

use super::present::{
    open_backend, BackendConfig, BackendKind, Frame, Gpu3DDrawOp, HostKey, PresentBackend,
    PresentStatus, ScaleMode,
};

pub const SCREEN_WIDTH: usize = 1280;
pub const SCREEN_HEIGHT: usize = 720;
pub const FRAMEBUFFER_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT * 4; // 3,686,400 bytes (RGBA8)
pub const DEFAULT_FRAMEBUFFER_ADDR: u64 = 0x90000000;

/// Height in pixels of the telemetry bar drawn over the top of the guest frame.
const HUD_HEIGHT: usize = 20;
/// Width in pixels the memory-bucket bar is normalised to.
const HUD_BAR_WIDTH: usize = 260;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationMode {
    Handheld720p,
    Docked1080p,
    Dlss4kReconstruction,
}

impl PresentationMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Handheld720p => "720p Handheld",
            Self::Docked1080p => "1080p Docked",
            Self::Dlss4kReconstruction => "4K DLSS",
        }
    }
}

pub struct DisplayConfig {
    pub width: usize,
    pub height: usize,
    pub scale: ScaleMode,
    pub title: String,
    pub target_fps: u64,
    pub mode: PresentationMode,
    /// Which host presentation backend to open.
    pub backend: BackendKind,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            width: SCREEN_WIDTH,
            height: SCREEN_HEIGHT,
            scale: ScaleMode::FitAspect,
            title: "Ounce-Core | NVIDIA Tegra T239 Research Runtime".to_string(),
            target_fps: 60,
            mode: PresentationMode::Handheld720p,
            backend: BackendKind::from_env(),
        }
    }
}

impl DisplayConfig {
    fn backend_config(&self) -> BackendConfig {
        BackendConfig {
            width: self.width,
            height: self.height,
            title: self.title.clone(),
            target_fps: self.target_fps,
            scale: self.scale,
        }
    }
}

/// Double-buffered swapchain queue with VSync timing.
///
/// Buffers hold tightly packed RGBA8 frames, matching what the host backends upload.
pub struct Swapchain {
    pub front_buffer: Frame,
    pub back_buffer: Frame,
    pub vsync_count: u64,
    pub last_frame_time_us: u128,
}

impl Swapchain {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            front_buffer: Frame::new(width, height),
            back_buffer: Frame::new(width, height),
            vsync_count: 0,
            last_frame_time_us: 16_666,
        }
    }

    pub fn flip(&mut self, frame_time_us: u128) {
        std::mem::swap(&mut self.front_buffer, &mut self.back_buffer);
        self.vsync_count += 1;
        self.last_frame_time_us = frame_time_us;
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.front_buffer.resize(width, height);
        self.back_buffer.resize(width, height);
    }
}

/// The Display Server connects guest video memory (VRAM) to a host presentation
/// backend, composites the telemetry overlay and paces the swapchain.
pub struct DisplayServer {
    pub fb_addr: u64,
    pub width: usize,
    pub height: usize,
    pub config: DisplayConfig,
    pub swapchain: Swapchain,
    backend: Option<Box<dyn PresentBackend>>,
    frame_count: u64,
    fps_timer: Instant,
    last_fps: f64,
}

impl DisplayServer {
    pub fn new(fb_addr: u64, config: DisplayConfig) -> Self {
        let swapchain = Swapchain::new(config.width, config.height);
        Self {
            fb_addr,
            width: config.width,
            height: config.height,
            config,
            swapchain,
            backend: None,
            frame_count: 0,
            fps_timer: Instant::now(),
            last_fps: 60.0,
        }
    }

    /// Opens the host presentation backend (software window, or headless as a fallback).
    ///
    /// Kept under its original name so existing callers do not change; the backend
    /// decides whether that means a native window at all.
    pub fn init_window(&mut self) -> Result<(), String> {
        if self.backend.is_some() {
            return Ok(());
        }
        let backend = open_backend(self.config.backend, &self.config.backend_config());
        println!("[Display] Presentation backend: {}", backend.name());
        self.backend = Some(backend);
        Ok(())
    }

    /// Name of the active backend, or `None` before it is opened.
    pub fn backend_name(&self) -> Option<&'static str> {
        self.backend.as_ref().map(|b| b.name())
    }

    /// Reads the guest framebuffer into the back buffer and draws the telemetry overlay.
    ///
    /// Split out from presentation so it can be tested without a host surface.
    pub fn composite(&mut self, mmu: &Memory) {
        if self.swapchain.back_buffer.width != self.width || self.swapchain.back_buffer.height != self.height {
            self.swapchain.resize(self.width, self.height);
        }

        let frame = &mut self.swapchain.back_buffer;
        // The guest framebuffer is already RGBA8 in the host's layout, so it is read
        // straight into the frame instead of being converted pixel by pixel.
        if mmu.read_bytes(self.fb_addr, &mut frame.rgba).is_ok() {
            for pixel in frame.rgba.chunks_exact_mut(4) {
                pixel[3] = 0xFF;
            }
        }

        self.draw_hud_overlay(mmu);
    }

    /// Returns true if the active presentation backend supports host GPU 3D acceleration.
    pub fn is_gpu_accelerated(&self) -> bool {
        self.backend.as_ref().map_or_else(
            || {
                #[cfg(feature = "gpu-present")]
                {
                    matches!(self.config.backend, BackendKind::Gpu | BackendKind::Auto)
                }
                #[cfg(not(feature = "gpu-present"))]
                {
                    false
                }
            },
            |b| b.is_gpu_accelerated(),
        )
    }

    /// Presents a frame to the host, executing hardware 3D draw calls when GPU accelerated,
    /// or falling back to CPU software rasterization composite.
    pub fn present_frame_3d(&mut self, mmu: &Memory, draw_ops: &[Gpu3DDrawOp]) -> bool {
        let frame_start = Instant::now();

        if self.backend.is_none() {
            if let Err(e) = self.init_window() {
                eprintln!("[Display Error] {}", e);
                return false;
            }
        }

        let is_gpu = self.is_gpu_accelerated();
        if is_gpu && !draw_ops.is_empty() {
            let backend = self.backend.as_mut().expect("backend opened above");
            if let Err(e) = backend.render_3d(draw_ops) {
                eprintln!("[Display Error] 3D render failed: {}", e);
            }
        } else {
            self.composite(mmu);
        }

        let frame_time_us = frame_start.elapsed().as_micros();
        self.swapchain.flip(frame_time_us);

        let backend = self.backend.as_mut().expect("backend opened above");
        if backend.present(&self.swapchain.front_buffer) == PresentStatus::Closed {
            return false;
        }

        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed();
        if elapsed.as_secs_f32() >= 1.0 {
            self.last_fps = self.frame_count as f64 / elapsed.as_secs_f64();
            let title = format!(
                "Ounce-Core | Tegra T239 | {:.1} FPS ({:.2}ms) | {} | VSync #{}",
                self.last_fps,
                self.swapchain.last_frame_time_us as f64 / 1000.0,
                self.config.mode.label(),
                self.swapchain.vsync_count
            );
            backend.set_title(&title);
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }

        true
    }

    /// Pulls the guest framebuffer from virtual memory and presents it to the host.
    ///
    /// Returns `false` once the host surface is gone, which stops the runtime loop.
    #[inline]
    pub fn present_frame(&mut self, mmu: &Memory) -> bool {
        self.present_frame_3d(mmu, &[])
    }

    pub fn is_window_active(&self) -> bool {
        self.backend.as_ref().map_or(false, |b| b.is_open())
    }

    /// Pumps backend window events (keyboard presses, window close, focus) without
    /// waiting for a guest frame presentation.
    pub fn pump_events(&mut self) -> bool {
        if self.backend.is_none() {
            if let Err(e) = self.init_window() {
                eprintln!("[Display Error] {}", e);
                return false;
            }
        }
        let backend = self.backend.as_mut().expect("backend opened above");
        backend.pump_events() != PresentStatus::Closed
    }

    /// Retrieves currently pressed host keyboard keys from the presentation backend.
    pub fn get_active_keys(&mut self) -> Vec<HostKey> {
        self.backend.as_mut().map_or_else(Vec::new, |b| b.pressed_keys())
    }

    /// Renders a translucent top status bar visualizing memory bucket allocation & VSync heartbeat.
    fn draw_hud_overlay(&mut self, mmu: &Memory) {
        let frame = &mut self.swapchain.back_buffer;
        let hud_h = HUD_HEIGHT.min(frame.height);
        let stats = mmu.bucket_stats();
        let total = stats.total_bytes.max(1);

        let code_w = ((stats.code_bytes * HUD_BAR_WIDTH) / total).clamp(4, HUD_BAR_WIDTH);
        let vram_w = ((stats.vram_bytes * HUD_BAR_WIDTH) / total).clamp(4, HUD_BAR_WIDTH);
        let stack_w = ((stats.stack_tls_bytes * HUD_BAR_WIDTH) / total).clamp(4, HUD_BAR_WIDTH);

        for y in 0..hud_h {
            for x in 0..frame.width {
                let i = (y * frame.width + x) * 4;
                // Dim the guest pixels the bar sits on so the overlay stays readable.
                frame.rgba[i] /= 2;
                frame.rgba[i + 1] /= 2;
                frame.rgba[i + 2] /= 2;
                frame.rgba[i + 3] = 0xFF;

                // Memory Bucket Visualizer Bar at y = 5..15
                if !(5..15).contains(&y) {
                    continue;
                }
                let bucket_end = 16 + code_w + vram_w + stack_w;
                let (r, g, b) = if (16..16 + code_w).contains(&x) {
                    (0x00, 0xFF, 0xA3) // Emerald: Code pages
                } else if (16 + code_w..16 + code_w + vram_w).contains(&x) {
                    (0x00, 0xD2, 0xFF) // Cyan: VRAM pages
                } else if (16 + code_w + vram_w..bucket_end).contains(&x) {
                    (0xFF, 0x80, 0x08) // Amber: Stack/TLS/Heap pages
                } else {
                    continue;
                };
                frame.rgba[i] = r;
                frame.rgba[i + 1] = g;
                frame.rgba[i + 2] = b;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    fn headless_server(width: usize, height: usize) -> DisplayServer {
        DisplayServer::new(
            DEFAULT_FRAMEBUFFER_ADDR,
            DisplayConfig {
                width,
                height,
                backend: BackendKind::Headless,
                ..Default::default()
            },
        )
    }

    #[test]
    fn test_composite_reads_guest_framebuffer_and_draws_hud() {
        let width = 320;
        let height = 64;
        let mut mmu = Memory::new();
        mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, width * height * 4, Permission::READ_WRITE);

        // Paint the guest framebuffer a known colour.
        let guest_pixels = vec![0x40u8; width * height * 4];
        mmu.write_bytes(DEFAULT_FRAMEBUFFER_ADDR, &guest_pixels).unwrap();

        let mut display = headless_server(width, height);
        display.composite(&mmu);
        let frame = &display.swapchain.back_buffer;

        // Below the overlay the guest pixels survive unchanged, with alpha forced opaque.
        assert_eq!(frame.pixel(0, HUD_HEIGHT), Some((0x40, 0x40, 0x40, 0xFF)));
        // Inside the overlay rows the backdrop is dimmed.
        assert_eq!(frame.pixel(0, 0), Some((0x20, 0x20, 0x20, 0xFF)));
        // The first bucket segment is emerald.
        assert_eq!(frame.pixel(16, 7), Some((0x00, 0xFF, 0xA3, 0xFF)));
    }

    #[test]
    fn test_present_frame_runs_headless_and_paces_the_swapchain() {
        let width = 64;
        let height = 32;
        let mut mmu = Memory::new();
        mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, width * height * 4, Permission::READ_WRITE);

        let mut display = headless_server(width, height);
        assert!(display.present_frame(&mmu));
        assert!(display.present_frame(&mmu));

        assert_eq!(display.backend_name(), Some("headless"));
        assert_eq!(display.swapchain.vsync_count, 2);
        // Headless presentation never opens a window and reports no host input.
        assert!(!display.is_window_active());
        assert!(display.get_active_keys().is_empty());
    }

    #[test]
    fn test_hud_overlay_height_clamps_on_short_frames() {
        let width = 32;
        let height = 8; // shorter than the 20px overlay
        let mut mmu = Memory::new();
        mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, width * height * 4, Permission::READ_WRITE);

        let mut display = headless_server(width, height);
        display.composite(&mmu);
        assert_eq!(display.swapchain.back_buffer.rgba.len(), width * height * 4);
    }
}
