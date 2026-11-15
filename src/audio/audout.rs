#![allow(dead_code)]

use crate::audio::host::HostAudioSink;
use crate::mmu::Memory;

pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const AUDIO_CHANNELS: u32 = 2; // Stereo

pub type AudoutServer = AudioOutServer;

/// Supported synthetic waveform shapes for diagnostic testing and audio verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Square,
    Sawtooth,
    Triangle,
}

/// Generates mono 16-bit signed PCM samples for synthetic waveforms.
pub fn generate_synth_pcm16(
    waveform: Waveform,
    frequency_hz: f32,
    sample_rate: u32,
    duration_ms: f32,
) -> Vec<i16> {
    if sample_rate == 0 || duration_ms <= 0.0 || frequency_hz <= 0.0 {
        return Vec::new();
    }
    let total_samples = ((sample_rate as f32) * (duration_ms / 1000.0)).round() as usize;
    let mut out = Vec::with_capacity(total_samples);
    let dt = 1.0 / (sample_rate as f32);
    for i in 0..total_samples {
        let t = (i as f32) * dt;
        let phase = (t * frequency_hz).fract();
        let val = match waveform {
            Waveform::Sine => (phase * 2.0 * std::f32::consts::PI).sin(),
            Waveform::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Sawtooth => 2.0 * phase - 1.0,
            Waveform::Triangle => 2.0 * (2.0 * (phase - 0.5).abs() - 0.5),
        };
        let sample = (val * 32767.0).clamp(-32768.0, 32767.0) as i16;
        out.push(sample);
    }
    out
}

/// Horizon OS `audout:u` / `audren` 48kHz Stereo 16-bit PCM Audio Sink Implementation.
#[derive(Debug)]
pub struct AudioOutServer {
    pub sample_rate: u32,
    pub channels: u32,
    pub is_playing: bool,
    pub buffers_queued: u64,
    pub total_samples_processed: u64,
    pub peak_amplitude: i16,
    ring_buffer: Vec<i16>,
}

impl Default for AudioOutServer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioOutServer {
    pub fn new() -> Self {
        Self {
            sample_rate: AUDIO_SAMPLE_RATE,
            channels: AUDIO_CHANNELS,
            is_playing: true,
            buffers_queued: 0,
            total_samples_processed: 0,
            peak_amplitude: 0,
            ring_buffer: Vec::with_capacity(48_000),
        }
    }

    /// Reads 16-bit signed PCM samples from guest memory and queues them to the audio mixer.
    pub fn append_pcm_buffer(&mut self, mmu: &Memory, sample_addr: u64, sample_count: usize) {
        if sample_addr == 0 || sample_count == 0 {
            return;
        }

        let mut peak = self.peak_amplitude;
        // A guest-supplied count can be 4 billion; only the first 1M samples are read.
        for i in 0..sample_count.min(1 << 20) {
            let addr = sample_addr.wrapping_add((i * 2) as u64);
            if let Ok(raw) = mmu.read_u16(addr) {
                let sample = raw as i16;
                let abs_val = sample.saturating_abs();
                if abs_val > peak {
                    peak = abs_val;
                }
                if self.ring_buffer.len() < 192_000 {
                    self.ring_buffer.push(sample);
                }
            }
        }

        self.peak_amplitude = peak;
        self.buffers_queued += 1;
        self.total_samples_processed += sample_count as u64;
    }

    /// Synthesizes a waveform tone and queues interleaved stereo 48 kHz PCM samples.
    pub fn generate_waveform(
        &mut self,
        waveform: Waveform,
        frequency_hz: f32,
        duration_ms: f32,
        volume: f32,
    ) -> usize {
        let mono = generate_synth_pcm16(waveform, frequency_hz, self.sample_rate, duration_ms);
        let vol = volume.clamp(0.0, 1.0);
        let mut peak = self.peak_amplitude;
        let sample_count = mono.len() * 2;
        for s in mono {
            let scaled = ((s as f32) * vol).clamp(-32768.0, 32767.0) as i16;
            let abs_val = scaled.saturating_abs();
            if abs_val > peak {
                peak = abs_val;
            }
            if self.ring_buffer.len() < 192_000 {
                self.ring_buffer.push(scaled); // Left
                self.ring_buffer.push(scaled); // Right
            }
        }
        self.peak_amplitude = peak;
        self.buffers_queued += 1;
        self.total_samples_processed += sample_count as u64;
        sample_count
    }

    /// Helper for generating a sine wave tone for testing.
    pub fn generate_sine_wave(
        &mut self,
        frequency_hz: f32,
        duration_ms: f32,
        volume: f32,
    ) -> usize {
        self.generate_waveform(Waveform::Sine, frequency_hz, duration_ms, volume)
    }

    /// Helper for generating a square wave tone for testing.
    pub fn generate_square_wave(
        &mut self,
        frequency_hz: f32,
        duration_ms: f32,
        volume: f32,
    ) -> usize {
        self.generate_waveform(Waveform::Square, frequency_hz, duration_ms, volume)
    }

    /// Automated audio frame pacing helper that transfers up to `frame_samples`
    /// from the guest ring buffer into the host audio sink.
    pub fn pace_frame(&mut self, frame_samples: usize, host: Option<&HostAudioSink>) -> usize {
        if self.ring_buffer.is_empty() || frame_samples == 0 {
            return 0;
        }
        let count = frame_samples.min(self.ring_buffer.len());
        let chunk: Vec<i16> = self.ring_buffer.drain(0..count).collect();
        if let Some(sink) = host {
            sink.push_samples(&chunk);
        }
        count
    }

    /// Synchronizes all buffered samples directly into the host audio sink.
    pub fn sync_to_host(&mut self, host: &HostAudioSink) -> usize {
        let samples = self.drain_samples();
        let count = samples.len();
        if count > 0 {
            host.push_samples(&samples);
        }
        count
    }

    /// Returns the number of buffered unplayed samples in the ring buffer.
    pub fn buffered_sample_count(&self) -> usize {
        self.ring_buffer.len()
    }

    /// Returns the estimated latency in milliseconds of the buffered audio.
    pub fn latency_ms(&self) -> f64 {
        let queued = self.ring_buffer.len() as f64;
        let rate = self.sample_rate as f64;
        let ch = self.channels as f64;
        if rate > 0.0 && ch > 0.0 {
            (queued / (rate * ch)) * 1000.0
        } else {
            0.0
        }
    }

    /// Drains all buffered samples for playback through the host audio sink.
    pub fn drain_samples(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.ring_buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    #[test]
    fn test_audio_append_pcm_buffer() {
        let mut mmu = Memory::new();
        let mut audio = AudioOutServer::new();
        let pcm_addr = 0x80060000u64;
        mmu.map_range(pcm_addr, 4096, Permission::READ_WRITE);

        // Write 4 stereo samples (8 i16s) with peak amplitude 12000
        let samples: [i16; 8] = [2000, -2000, 6000, -6000, 12000, -12000, 4000, -4000];
        for (i, &s) in samples.iter().enumerate() {
            mmu.write_u16(pcm_addr + (i * 2) as u64, s as u16).unwrap();
        }

        audio.append_pcm_buffer(&mmu, pcm_addr, samples.len());
        assert_eq!(audio.buffers_queued, 1);
        assert_eq!(audio.total_samples_processed, 8);
        assert_eq!(audio.peak_amplitude, 12000);
    }

    #[test]
    fn test_audout_synth_waveform_and_frame_pacing() {
        let mut audio = AudioOutServer::new();
        let host = HostAudioSink::new();

        // Generate 10ms of 440 Hz sine tone at 48kHz (480 stereo frames = 960 raw samples)
        let generated = audio.generate_sine_wave(440.0, 10.0, 0.8);
        assert_eq!(generated, 960);
        assert_eq!(audio.buffered_sample_count(), 960);
        assert!(audio.peak_amplitude > 20_000);
        assert!(audio.latency_ms() > 9.0 && audio.latency_ms() < 11.0);

        // Pace a 5ms frame (480 samples) to host
        let paced = audio.pace_frame(480, Some(&host));
        assert_eq!(paced, 480);
        assert_eq!(audio.buffered_sample_count(), 480);

        // Sync remaining to host
        let synced = audio.sync_to_host(&host);
        assert_eq!(synced, 480);
        assert_eq!(audio.buffered_sample_count(), 0);
    }
}
