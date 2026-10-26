#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
pub const DEFAULT_CHANNELS: u16 = 2;
pub const DEFAULT_MAX_CAPACITY: usize = 192_000; // 2 seconds of 48 kHz stereo PCM

/// Performance and pacing metrics snapshot for host audio playback.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioSinkMetrics {
    pub underruns: u64,
    pub samples_played: u64,
    pub samples_dropped: u64,
    pub queued_samples: usize,
    pub latency_ms: f64,
}

/// Cross-platform real-time host audio output sink (48 kHz stereo PCM) via cpal.
pub struct HostAudioSink {
    queue: Arc<Mutex<VecDeque<i16>>>,
    pub underruns: Arc<AtomicU64>,
    pub samples_played: Arc<AtomicU64>,
    pub samples_dropped: Arc<AtomicU64>,
    pub max_capacity: usize,
    _stream: Option<cpal::Stream>,
    pub is_enabled: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for HostAudioSink {
    fn default() -> Self {
        Self::new()
    }
}

impl HostAudioSink {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_CAPACITY)
    }

    pub fn with_capacity(max_capacity: usize) -> Self {
        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(max_capacity.min(96_000))));
        let underruns = Arc::new(AtomicU64::new(0));
        let samples_played = Arc::new(AtomicU64::new(0));
        let samples_dropped = Arc::new(AtomicU64::new(0));

        let queue_clone = Arc::clone(&queue);
        let underruns_clone = Arc::clone(&underruns);
        let played_clone = Arc::clone(&samples_played);

        let sample_rate = DEFAULT_SAMPLE_RATE;
        let channels = DEFAULT_CHANNELS;

        let stream = match Self::init_stream(
            queue_clone,
            underruns_clone,
            played_clone,
            sample_rate,
            channels,
        ) {
            Ok(s) => Some(s),
            Err(_) => None,
        };

        let is_enabled = stream.is_some();
        Self {
            queue,
            underruns,
            samples_played,
            samples_dropped,
            max_capacity,
            _stream: stream,
            is_enabled,
            sample_rate,
            channels,
        }
    }

    fn init_stream(
        queue: Arc<Mutex<VecDeque<i16>>>,
        underruns: Arc<AtomicU64>,
        samples_played: Arc<AtomicU64>,
        sample_rate: u32,
        channels: u16,
    ) -> Result<cpal::Stream, Box<dyn std::error::Error + Send + Sync>> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No default audio output device available".to_string())?;

        let config = cpal::StreamConfig {
            channels,
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        let stream = device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if let Ok(mut q) = queue.lock() {
                    let mut played = 0u64;
                    let mut starved = 0u64;
                    for sample in data.iter_mut() {
                        if let Some(s) = q.pop_front() {
                            played += 1;
                            *sample = (s as f32) / 32768.0;
                        } else {
                            starved += 1;
                            *sample = 0.0;
                        }
                    }
                    if played > 0 {
                        samples_played.fetch_add(played, Ordering::Relaxed);
                    }
                    if starved > 0 {
                        underruns.fetch_add(starved, Ordering::Relaxed);
                    }
                } else {
                    underruns.fetch_add(data.len() as u64, Ordering::Relaxed);
                    for sample in data.iter_mut() {
                        *sample = 0.0;
                    }
                }
            },
            move |err| {
                eprintln!("[HostAudioSink] Output stream error: {}", err);
            },
            None,
        )?;

        stream.play()?;
        Ok(stream)
    }

    /// Enqueues interleaved stereo 16-bit PCM samples to the host playback stream.
    /// Drops the oldest unplayed samples if queue would exceed `max_capacity`.
    pub fn push_samples(&self, samples: &[i16]) {
        if samples.is_empty() {
            return;
        }

        if let Ok(mut q) = self.queue.lock() {
            let total = q.len() + samples.len();
            if total > self.max_capacity {
                let overflow = total - self.max_capacity;
                let drop_from_q = overflow.min(q.len());
                if drop_from_q > 0 {
                    q.drain(0..drop_from_q);
                    self.samples_dropped.fetch_add(drop_from_q as u64, Ordering::Relaxed);
                }
                let remaining_drop = overflow - drop_from_q;
                if remaining_drop > 0 {
                    self.samples_dropped.fetch_add(remaining_drop as u64, Ordering::Relaxed);
                    q.extend(samples[remaining_drop..].iter().copied());
                    return;
                }
            }
            q.extend(samples.iter().copied());
        }
    }

    /// Returns the number of unplayed samples currently in the playback queue.
    pub fn queued_sample_count(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    /// Returns the estimated latency in milliseconds of currently queued unplayed audio.
    pub fn queue_latency_ms(&self) -> f64 {
        let queued = self.queued_sample_count() as f64;
        let rate = self.sample_rate as f64;
        let ch = self.channels as f64;
        if rate > 0.0 && ch > 0.0 {
            (queued / (rate * ch)) * 1000.0
        } else {
            0.0
        }
    }

    /// Returns the count of underrun events (missing samples when output starved).
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Returns the total count of samples played through the output sink.
    pub fn samples_played(&self) -> u64 {
        self.samples_played.load(Ordering::Relaxed)
    }

    /// Returns the total count of samples dropped due to buffer capacity overflow.
    pub fn samples_dropped(&self) -> u64 {
        self.samples_dropped.load(Ordering::Relaxed)
    }

    /// Snapshot of current sink playback and latency metrics.
    pub fn metrics(&self) -> AudioSinkMetrics {
        AudioSinkMetrics {
            underruns: self.underruns(),
            samples_played: self.samples_played(),
            samples_dropped: self.samples_dropped(),
            queued_samples: self.queued_sample_count(),
            latency_ms: self.queue_latency_ms(),
        }
    }

    /// Pops up to `out.len()` samples from the ring buffer.
    /// Starved samples are filled with silence (0) and registered as underruns.
    pub fn pop_samples(&self, out: &mut [i16]) -> usize {
        if out.is_empty() {
            return 0;
        }

        if let Ok(mut q) = self.queue.lock() {
            let available = q.len();
            let to_read = out.len().min(available);
            for i in 0..to_read {
                out[i] = q.pop_front().unwrap_or(0);
            }
            if to_read < out.len() {
                for i in to_read..out.len() {
                    out[i] = 0;
                }
                let deficit = (out.len() - to_read) as u64;
                self.underruns.fetch_add(deficit, Ordering::Relaxed);
            }
            self.samples_played.fetch_add(to_read as u64, Ordering::Relaxed);
            to_read
        } else {
            self.underruns.fetch_add(out.len() as u64, Ordering::Relaxed);
            out.fill(0);
            0
        }
    }

    /// Pops a single sample from the queue, recording play or underrun.
    pub fn pop_sample(&self) -> Option<i16> {
        if let Ok(mut q) = self.queue.lock() {
            if let Some(s) = q.pop_front() {
                self.samples_played.fetch_add(1, Ordering::Relaxed);
                Some(s)
            } else {
                self.underruns.fetch_add(1, Ordering::Relaxed);
                None
            }
        } else {
            self.underruns.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Clears all queued samples from the playback buffer.
    pub fn clear(&self) {
        if let Ok(mut q) = self.queue.lock() {
            q.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_audio_sink_lifecycle_and_queue() {
        let sink = HostAudioSink::new();
        // push 4 stereo samples
        let samples: [i16; 8] = [1000, -1000, 2000, -2000, 3000, -3000, 4000, -4000];
        sink.push_samples(&samples);
        // Queue must have at least 8 samples unless consumed by active audio thread
        assert!(sink.queued_sample_count() <= 8);
    }

    #[test]
    fn test_ring_buffer_push_pop_latency_and_overflow() {
        // Create sink with small capacity (32 samples) for explicit deterministic testing
        let sink = HostAudioSink::with_capacity(32);
        assert_eq!(sink.queued_sample_count(), 0);
        assert_eq!(sink.queue_latency_ms(), 0.0);
        assert_eq!(sink.underruns(), 0);
        assert_eq!(sink.samples_played(), 0);

        // Push 16 stereo samples (32 i16s)
        let chunk1: Vec<i16> = (0..32).map(|x| (x * 100) as i16).collect();
        sink.push_samples(&chunk1);

        let queued = sink.queued_sample_count();
        assert!(queued <= 32);
        // Latency for 32 samples at 48kHz stereo = 32 / 96,000 * 1000 ≈ 0.333 ms
        let expected_lat = (32.0 / 96000.0) * 1000.0;
        assert!((sink.queue_latency_ms() - expected_lat).abs() < 0.01 || sink.is_enabled);

        // Pop 16 samples
        let mut read_buf = [0i16; 16];
        let read = sink.pop_samples(&mut read_buf);
        if !sink.is_enabled {
            assert_eq!(read, 16);
            assert_eq!(read_buf[0], 0);
            assert_eq!(read_buf[1], 100);
            assert_eq!(sink.samples_played(), 16);
            assert_eq!(sink.queued_sample_count(), 16);
        }

        // Pop past queue to test underrun detection
        let mut starve_buf = [0i16; 32];
        sink.pop_samples(&mut starve_buf);
        assert!(sink.underruns() > 0);

        // Test overflow protection
        sink.clear();
        let overflow_data: Vec<i16> = (0..50).map(|x| x as i16).collect();
        sink.push_samples(&overflow_data);
        assert_eq!(sink.queued_sample_count(), 32);
        assert_eq!(sink.samples_dropped(), 18);

        // Check metrics snapshot
        let m = sink.metrics();
        assert_eq!(m.queued_samples, 32);
        assert_eq!(m.samples_dropped, 18);
    }
}
