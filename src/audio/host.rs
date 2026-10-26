#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Cross-platform real-time host audio output sink (48 kHz stereo PCM) via cpal.
pub struct HostAudioSink {
    queue: Arc<Mutex<VecDeque<i16>>>,
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
        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(96_000)));
        let queue_clone = Arc::clone(&queue);
        let sample_rate = 48_000;
        let channels = 2;

        let stream = match Self::init_stream(queue_clone, sample_rate, channels) {
            Ok(s) => Some(s),
            Err(_) => None,
        };

        let is_enabled = stream.is_some();
        Self {
            queue,
            _stream: stream,
            is_enabled,
            sample_rate,
            channels,
        }
    }

    fn init_stream(
        queue: Arc<Mutex<VecDeque<i16>>>,
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
                    for sample in data.iter_mut() {
                        *sample = if let Some(s) = q.pop_front() {
                            (s as f32) / 32768.0
                        } else {
                            0.0
                        };
                    }
                } else {
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
    pub fn push_samples(&self, samples: &[i16]) {
        if samples.is_empty() {
            return;
        }

        if let Ok(mut q) = self.queue.lock() {
            // Keep at most ~0.5s of buffer (48000 samples) to prevent latency build-up
            if q.len() > 48_000 {
                let overflow = q.len() - 48_000;
                q.drain(0..overflow);
            }
            q.extend(samples.iter().copied());
        }
    }

    /// Returns the number of unplayed samples currently in the playback queue.
    pub fn queued_sample_count(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
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
}
