//! Headless presentation: no host window.
//!
//! This is the default for the verification harness, for CI and for hosts without a
//! display. Frames are counted and hashed so tests can assert on rendered output, and
//! `OUNCE_DUMP_FRAMES=<dir>` writes each frame as a binary PPM for manual inspection.

#![allow(dead_code)]

use std::io::Write;
use std::path::PathBuf;

use super::{BackendConfig, Frame, PresentBackend, PresentStatus};

pub struct HeadlessBackend {
    frames: u64,
    last_hash: u64,
    dump_dir: Option<PathBuf>,
    dump_failed: bool,
}

impl HeadlessBackend {
    pub fn new(_config: &BackendConfig) -> Self {
        Self {
            frames: 0,
            last_hash: 0,
            dump_dir: std::env::var_os("OUNCE_DUMP_FRAMES").map(PathBuf::from),
            dump_failed: false,
        }
    }

    /// FNV-1a hash of the most recently presented frame, or 0 before the first frame.
    pub fn last_frame_hash(&self) -> u64 {
        self.last_hash
    }

    fn dump_frame(&mut self, frame: &Frame) {
        let Some(dir) = self.dump_dir.clone() else {
            return;
        };
        if self.dump_failed {
            return;
        }

        if let Err(e) = self.write_ppm(&dir, frame) {
            eprintln!("[Display] Frame dump to {} failed: {e}", dir.display());
            // Report once; a failing dump must not slow the run down frame after frame.
            self.dump_failed = true;
        }
    }

    fn write_ppm(&self, dir: &PathBuf, frame: &Frame) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("frame_{:06}.ppm", self.frames));
        let file = std::fs::File::create(path)?;
        let mut out = std::io::BufWriter::new(file);
        write!(out, "P6\n{} {}\n255\n", frame.width, frame.height)?;
        for pixel in frame.rgba.chunks_exact(4) {
            out.write_all(&pixel[..3])?;
        }
        out.flush()
    }
}

impl PresentBackend for HeadlessBackend {
    fn name(&self) -> &'static str {
        "headless"
    }

    fn present(&mut self, frame: &Frame) -> PresentStatus {
        self.dump_frame(frame);
        self.last_hash = frame.content_hash();
        self.frames += 1;
        PresentStatus::Continue
    }

    fn frames_presented(&self) -> u64 {
        self.frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::present::ScaleMode;

    fn config() -> BackendConfig {
        BackendConfig {
            width: 4,
            height: 2,
            title: "headless test".to_string(),
            target_fps: 60,
            scale: ScaleMode::Native,
        }
    }

    #[test]
    fn test_headless_counts_frames_and_tracks_hashes() {
        let mut backend = HeadlessBackend::new(&config());
        assert_eq!(backend.name(), "headless");
        assert_eq!(backend.last_frame_hash(), 0);

        let blank = Frame::new(4, 2);
        assert_eq!(backend.present(&blank), PresentStatus::Continue);
        let blank_hash = backend.last_frame_hash();
        assert_eq!(blank_hash, blank.content_hash());

        let mut drawn = Frame::new(4, 2);
        drawn.set_pixel(2, 1, 0xFF, 0x00, 0xA3);
        backend.present(&drawn);
        assert_eq!(backend.frames_presented(), 2);
        assert_ne!(backend.last_frame_hash(), blank_hash);

        // A headless backend never claims a window and never reports host input.
        assert!(!backend.is_open());
        assert!(backend.pressed_keys().is_empty());
    }
}
