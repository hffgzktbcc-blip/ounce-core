#![allow(dead_code)]

use crate::mmu::Memory;
use minifb::{Key, Window, WindowOptions};
use std::time::Instant;

pub const SCREEN_WIDTH: usize = 1280;
pub const SCREEN_HEIGHT: usize = 720;
pub const FRAMEBUFFER_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT * 4; // 3,686,400 bytes (RGBA8)
pub const DEFAULT_FRAMEBUFFER_ADDR: u64 = 0x90000000;

pub struct DisplayConfig {
    pub width: usize,
    pub height: usize,
    pub scale: minifb::Scale,
    pub title: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            width: SCREEN_WIDTH,
            height: SCREEN_HEIGHT,
            scale: minifb::Scale::FitScreen,
            title: "Ounce-Core | Nintendo Switch 2 (T239) Display".to_string(),
        }
    }
}

/// The Display Server connects guest video memory (VRAM) to the host window.
pub struct DisplayServer {
    pub fb_addr: u64,
    pub width: usize,
    pub height: usize,
    window: Option<Window>,
    host_buffer: Vec<u32>,
    frame_count: u64,
    fps_timer: Instant,
    last_fps: f64,
}

impl DisplayServer {
    pub fn new(fb_addr: u64, config: DisplayConfig) -> Self {
        let host_buffer = vec![0u32; config.width * config.height];
        Self {
            fb_addr,
            width: config.width,
            height: config.height,
            window: None,
            host_buffer,
            frame_count: 0,
            fps_timer: Instant::now(),
            last_fps: 0.0,
        }
    }

    /// Initializes the native host window (macOS Cocoa / Linux X11/Wayland / Windows).
    pub fn init_window(&mut self) -> Result<(), String> {
        let mut opts = WindowOptions::default();
        opts.scale = minifb::Scale::FitScreen;
        opts.resize = true;

        let mut window = Window::new(
            "Ounce-Core | Nintendo Switch 2 (T239) Display [60 FPS Target]",
            self.width,
            self.height,
            opts,
        )
        .map_err(|e| format!("Failed to create host window: {e}"))?;

        // Target 60 FPS presentation
        window.set_target_fps(60);
        self.window = Some(window);
        Ok(())
    }

    /// Pulls the guest framebuffer from virtual memory and blits it to the native window.
    /// Returns `false` if the window was closed or user pressed Escape.
    pub fn present_frame(&mut self, mmu: &Memory) -> bool {
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

        // Read RGBA8888 pixels from guest memory into host 0x00RRGGBB buffer
        let total_pixels = self.width * self.height;
        let mut raw_bytes = vec![0u8; total_pixels * 4];

        if mmu.read_bytes(self.fb_addr, &mut raw_bytes).is_ok() {
            for i in 0..total_pixels {
                let r = raw_bytes[i * 4] as u32;
                let g = raw_bytes[i * 4 + 1] as u32;
                let b = raw_bytes[i * 4 + 2] as u32;
                // Convert RGBA -> 0x00RRGGBB format expected by minifb
                self.host_buffer[i] = (r << 16) | (g << 8) | b;
            }
        }

        // Draw On-Screen Telemetry HUD Bar (Top 16px overlay showing Memory Buckets & Frame Activity)
        self.draw_hud_overlay(mmu);

        let window = self.window.as_mut().unwrap();

        // Blit to screen
        if let Err(e) = window.update_with_buffer(&self.host_buffer, self.width, self.height) {
            eprintln!("[Display Error] Failed to update frame: {e}");
            return false;
        }

        // FPS tracking
        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed();
        if elapsed.as_secs_f32() >= 1.0 {
            self.last_fps = self.frame_count as f64 / elapsed.as_secs_f64();
            window.set_title(&format!(
                "Ounce-Core | Switch 2 (T239) | {:.1} FPS | Frame {}",
                self.last_fps, self.frame_count
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
        let hud_h = 16usize.min(self.height);
        let stats = mmu.bucket_stats();
        let total = stats.total_bytes.max(1);

        let code_w = ((stats.code_bytes * 240) / total).clamp(4, 240);
        let vram_w = ((stats.vram_bytes * 240) / total).clamp(4, 240);
        let stack_w = ((stats.stack_tls_bytes * 240) / total).clamp(4, 240);

        for y in 0..hud_h {
            for x in 0..self.width {
                let idx = y * self.width + x;
                // Dark translucent backdrop
                let bg = (self.host_buffer[idx] >> 1) & 0x007F_7F7F;
                self.host_buffer[idx] = bg;

                // Memory Bucket Visualizer Bar at y = 4..12
                if (4..12).contains(&y) {
                    if (16..16 + code_w).contains(&x) {
                        self.host_buffer[idx] = 0x0038_EF7D; // Emerald: Code pages
                    } else if (16 + code_w..16 + code_w + vram_w).contains(&x) {
                        self.host_buffer[idx] = 0x0000_D2FF; // Cyan: VRAM pages
                    } else if (16 + code_w + vram_w..16 + code_w + vram_w + stack_w).contains(&x) {
                        self.host_buffer[idx] = 0x00FF_8008; // Amber: Stack/TLS/Heap pages
                    }
                }
            }
        }
    }
}
