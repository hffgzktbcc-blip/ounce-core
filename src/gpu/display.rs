#![allow(dead_code)]

use crate::mmu::Memory;
use minifb::{Key, Window, WindowOptions};
use std::time::Instant;

pub const SCREEN_WIDTH: usize = 1280;
pub const SCREEN_HEIGHT: usize = 720;
pub const FRAMEBUFFER_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT * 4; // 3,686,400 bytes (RGBA8)
pub const DEFAULT_FRAMEBUFFER_ADDR: u64 = 0x90000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationMode {
    Handheld720p,
    Docked1080p,
    Dlss4kReconstruction,
}

pub struct DisplayConfig {
    pub width: usize,
    pub height: usize,
    pub scale: minifb::Scale,
    pub title: String,
    pub target_fps: u64,
    pub mode: PresentationMode,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            width: SCREEN_WIDTH,
            height: SCREEN_HEIGHT,
            scale: minifb::Scale::FitScreen,
            title: "Ounce-Core | NVIDIA Tegra T239 Research Runtime".to_string(),
            target_fps: 60,
            mode: PresentationMode::Handheld720p,
        }
    }
}

/// Double-buffered swapchain queue with VSync timing.
pub struct Swapchain {
    pub front_buffer: Vec<u32>,
    pub back_buffer: Vec<u32>,
    pub vsync_count: u64,
    pub last_frame_time_us: u128,
}

impl Swapchain {
    pub fn new(size: usize) -> Self {
        Self {
            front_buffer: vec![0u32; size],
            back_buffer: vec![0u32; size],
            vsync_count: 0,
            last_frame_time_us: 16_666,
        }
    }

    pub fn flip(&mut self, frame_time_us: u128) {
        std::mem::swap(&mut self.front_buffer, &mut self.back_buffer);
        self.vsync_count += 1;
        self.last_frame_time_us = frame_time_us;
    }
}

/// The Display Server connects guest video memory (VRAM) to the host window with swapchain pacing.
pub struct DisplayServer {
    pub fb_addr: u64,
    pub width: usize,
    pub height: usize,
    pub config: DisplayConfig,
    pub swapchain: Swapchain,
    window: Option<Window>,
    frame_count: u64,
    fps_timer: Instant,
    frame_timer: Instant,
    last_fps: f64,
}

impl DisplayServer {
    pub fn new(fb_addr: u64, config: DisplayConfig) -> Self {
        let size = config.width * config.height;
        let swapchain = Swapchain::new(size);
        Self {
            fb_addr,
            width: config.width,
            height: config.height,
            config,
            swapchain,
            window: None,
            frame_count: 0,
            fps_timer: Instant::now(),
            frame_timer: Instant::now(),
            last_fps: 60.0,
        }
    }

    /// Initializes the native host window (macOS Cocoa / Linux X11/Wayland / Windows).
    pub fn init_window(&mut self) -> Result<(), String> {
        let mut opts = WindowOptions::default();
        opts.scale = self.config.scale;
        opts.resize = true;

        let mut window = Window::new(
            &format!("{} [{} FPS Target]", self.config.title, self.config.target_fps),
            self.width,
            self.height,
            opts,
        )
        .map_err(|e| format!("Failed to create host window: {e}"))?;

        window.set_target_fps(self.config.target_fps as usize);
        self.window = Some(window);
        Ok(())
    }

    /// Pulls the guest framebuffer from virtual memory and blits it to the native swapchain window.
    pub fn present_frame(&mut self, mmu: &Memory) -> bool {
        let frame_start = Instant::now();

        if self.window.is_none() {
            if let Err(e) = self.init_window() {
                eprintln!("[Display Error] {}", e);
                return false;
            }
        }

        {
            let window = self.window.as_ref().unwrap();
            if !window.is_open() || window.is_key_down(Key::Escape) {
                return false;
            }
        }

        let total_pixels = self.width * self.height;
        let mut raw_bytes = vec![0u8; total_pixels * 4];

        if mmu.read_bytes(self.fb_addr, &mut raw_bytes).is_ok() {
            for i in 0..total_pixels {
                let r = raw_bytes[i * 4] as u32;
                let g = raw_bytes[i * 4 + 1] as u32;
                let b = raw_bytes[i * 4 + 2] as u32;
                self.swapchain.back_buffer[i] = (r << 16) | (g << 8) | b;
            }
        }

        // Draw On-Screen Telemetry HUD Bar (Top 20px overlay showing Memory, Mode, and Frame Latency)
        self.draw_hud_overlay(mmu);

        // Flip swapchain buffer
        let frame_time_us = frame_start.elapsed().as_micros();
        self.swapchain.flip(frame_time_us);

        let window = self.window.as_mut().unwrap();

        // Blit front buffer to native window
        if let Err(e) = window.update_with_buffer(&self.swapchain.front_buffer, self.width, self.height) {
            eprintln!("[Display Error] Failed to update frame: {e}");
            return false;
        }

        // FPS tracking
        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed();
        if elapsed.as_secs_f32() >= 1.0 {
            self.last_fps = self.frame_count as f64 / elapsed.as_secs_f64();
            let mode_str = match self.config.mode {
                PresentationMode::Handheld720p => "720p Handheld",
                PresentationMode::Docked1080p => "1080p Docked",
                PresentationMode::Dlss4kReconstruction => "4K DLSS",
            };
            window.set_title(&format!(
                "Ounce-Core | Tegra T239 | {:.1} FPS ({:.2}ms) | {} | VSync #{}",
                self.last_fps,
                self.swapchain.last_frame_time_us as f64 / 1000.0,
                mode_str,
                self.swapchain.vsync_count
            ));
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }

        true
    }

    pub fn is_window_active(&self) -> bool {
        self.window.as_ref().map_or(false, |w| w.is_open())
    }

    /// Retrieves currently pressed keyboard keys from the host window.
    pub fn get_active_keys(&self) -> Vec<Key> {
        self.window.as_ref().map_or_else(Vec::new, |w| w.get_keys())
    }

    /// Renders a translucent top status bar visualizing memory bucket allocation & VSync heartbeat.
    fn draw_hud_overlay(&mut self, mmu: &Memory) {
        let hud_h = 20usize.min(self.height);
        let stats = mmu.bucket_stats();
        let total = stats.total_bytes.max(1);

        let code_w = ((stats.code_bytes * 260) / total).clamp(4, 260);
        let vram_w = ((stats.vram_bytes * 260) / total).clamp(4, 260);
        let stack_w = ((stats.stack_tls_bytes * 260) / total).clamp(4, 260);

        for y in 0..hud_h {
            for x in 0..self.width {
                let idx = y * self.width + x;
                let bg = (self.swapchain.back_buffer[idx] >> 1) & 0x007F_7F7F;
                self.swapchain.back_buffer[idx] = bg;

                // Memory Bucket Visualizer Bar at y = 5..15
                if (5..15).contains(&y) {
                    if (16..16 + code_w).contains(&x) {
                        self.swapchain.back_buffer[idx] = 0x0000_FFA3; // Emerald: Code pages
                    } else if (16 + code_w..16 + code_w + vram_w).contains(&x) {
                        self.swapchain.back_buffer[idx] = 0x0000_D2FF; // Cyan: VRAM pages
                    } else if (16 + code_w + vram_w..16 + code_w + vram_w + stack_w).contains(&x) {
                        self.swapchain.back_buffer[idx] = 0x00FF_8008; // Amber: Stack/TLS/Heap pages
                    }
                }
            }
        }
    }
}
