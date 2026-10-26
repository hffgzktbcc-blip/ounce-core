#![allow(dead_code)]

use crate::audio::audout::{generate_synth_pcm16, Waveform};
use crate::audio::host::HostAudioSink;
use crate::mmu::Memory;

/// Horizon OS `audren:u` Audio Renderer Sample Rates
pub const AUDREN_SAMPLE_RATE_48KHZ: u32 = 48_000;
pub const AUDREN_SAMPLE_RATE_32KHZ: u32 = 32_000;

pub type AudrenServer = AudioRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SampleFormat {
    Pcm16 = 0,
    Pcm8 = 1,
    Adpcm = 2,
    Float32 = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
}

/// Wave buffer descriptor queued to a voice
#[derive(Debug, Clone)]
pub struct WaveBuffer {
    pub buffer_addr: u64,
    pub buffer_size: usize,
    pub start_sample_offset: u32,
    pub end_sample_offset: u32,
    pub is_looping: bool,
    pub is_stream_ended: bool,
}

/// Represents an active sound voice in the Horizon `audren:u` DSP engine.
#[derive(Debug, Clone)]
pub struct AudioVoice {
    pub id: u32,
    pub format: SampleFormat,
    pub state: PlaybackState,
    pub sample_rate: u32,
    pub volume: f32,
    pub pitch: f32,
    /// 2-channel pan/mix volume [Left, Right]
    pub mix_volumes: [f32; 2],
    /// Queued wave buffers in guest memory
    pub wave_buffers: Vec<WaveBuffer>,
    pub current_sample_idx: usize,
    /// Fractional sample playback cursor for sample-rate conversion
    pub sample_pos: f32,
    /// In-memory synthetic PCM samples for direct testing/demos
    pub synth_samples: Vec<i16>,
    pub synth_looping: bool,
}

impl AudioVoice {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            format: SampleFormat::Pcm16,
            state: PlaybackState::Stopped,
            sample_rate: AUDREN_SAMPLE_RATE_48KHZ,
            volume: 1.0,
            pitch: 1.0,
            mix_volumes: [1.0, 1.0],
            wave_buffers: Vec::new(),
            current_sample_idx: 0,
            sample_pos: 0.0,
            synth_samples: Vec::new(),
            synth_looping: false,
        }
    }

    pub fn queue_buffer(&mut self, wb: WaveBuffer) {
        self.wave_buffers.push(wb);
        if self.state == PlaybackState::Stopped {
            self.state = PlaybackState::Playing;
        }
    }

    /// Sets up a synthetic oscillator tone directly on this voice.
    pub fn set_synth_tone(
        &mut self,
        waveform: Waveform,
        frequency_hz: f32,
        sample_rate: u32,
        duration_ms: f32,
        looping: bool,
    ) {
        self.sample_rate = sample_rate;
        self.synth_samples = generate_synth_pcm16(waveform, frequency_hz, sample_rate, duration_ms);
        self.synth_looping = looping;
        self.sample_pos = 0.0;
        self.current_sample_idx = 0;
        self.state = PlaybackState::Playing;
    }

    /// Helper for setting a sine wave oscillator tone on this voice.
    pub fn set_synth_sine(&mut self, frequency_hz: f32, sample_rate: u32, duration_ms: f32, looping: bool) {
        self.set_synth_tone(Waveform::Sine, frequency_hz, sample_rate, duration_ms, looping);
    }

    /// Helper for setting a square wave oscillator tone on this voice.
    pub fn set_synth_square(&mut self, frequency_hz: f32, sample_rate: u32, duration_ms: f32, looping: bool) {
        self.set_synth_tone(Waveform::Square, frequency_hz, sample_rate, duration_ms, looping);
    }

    /// Clears any synthetic samples loaded on this voice.
    pub fn clear_synth(&mut self) {
        self.synth_samples.clear();
        self.synth_looping = false;
    }
}

/// Horizon `audren:u` Renderer Configuration parameters.
#[derive(Debug, Clone)]
pub struct AudioRendererConfig {
    pub sample_rate: u32,
    pub sample_count: u32, // Samples per update tick (e.g., 240 = 5ms at 48kHz)
    pub mix_buffer_count: u32,
    pub submix_count: u32,
    pub voice_count: u32,
    pub sink_count: u32,
    pub effect_count: u32,
    pub performance_frame_count: u32,
}

impl Default for AudioRendererConfig {
    fn default() -> Self {
        Self {
            sample_rate: AUDREN_SAMPLE_RATE_48KHZ,
            sample_count: 240, // 5ms DSP update slice
            mix_buffer_count: 8,
            submix_count: 4,
            voice_count: 64,
            sink_count: 2,
            effect_count: 4,
            performance_frame_count: 0,
        }
    }
}

/// Horizon OS `audren:u` Audio Renderer Hardware-Level DSP Mixer.
#[derive(Debug)]
pub struct AudioRenderer {
    pub config: AudioRendererConfig,
    pub is_active: bool,
    pub voices: Vec<AudioVoice>,
    pub output_ring_buffer: Vec<i16>,
    pub frames_rendered: u64,
    pub total_samples_mixed: u64,
    pub peak_amplitude: i16,
}

impl AudioRenderer {
    pub fn new(config: AudioRendererConfig) -> Self {
        let voice_count = config.voice_count as usize;
        let mut voices = Vec::with_capacity(voice_count);
        for id in 0..voice_count {
            voices.push(AudioVoice::new(id as u32));
        }

        Self {
            config,
            is_active: true,
            voices,
            output_ring_buffer: Vec::with_capacity(192_000),
            frames_rendered: 0,
            total_samples_mixed: 0,
            peak_amplitude: 0,
        }
    }

    /// Allocates or retrieves a voice by index.
    pub fn get_voice_mut(&mut self, voice_id: u32) -> Option<&mut AudioVoice> {
        self.voices.get_mut(voice_id as usize)
    }

    /// Returns the count of currently playing voices.
    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.state == PlaybackState::Playing).count()
    }

    /// Mixes one DSP tick frame (typically 240 stereo samples) from all active voices into the output sink,
    /// performing multi-voice polyphonic mixing and sample rate conversion to 48 kHz.
    pub fn render_dsp_tick(&mut self, mmu: &Memory) -> usize {
        if !self.is_active {
            return 0;
        }

        let samples_per_frame = self.config.sample_count as usize;
        // Temporary float accumulation buffers for stereo output
        let mut mix_l = vec![0.0f32; samples_per_frame];
        let mut mix_r = vec![0.0f32; samples_per_frame];

        for voice in &mut self.voices {
            if voice.state != PlaybackState::Playing {
                continue;
            }

            let has_synth = !voice.synth_samples.is_empty();
            let has_wb = !voice.wave_buffers.is_empty();

            if !has_synth && !has_wb {
                continue;
            }

            let vol_l = voice.volume * voice.mix_volumes[0];
            let vol_r = voice.volume * voice.mix_volumes[1];

            // Resampling factor: advances voice position relative to the renderer output rate
            let step = if self.config.sample_rate > 0 {
                (voice.sample_rate as f32 / self.config.sample_rate as f32) * voice.pitch
            } else {
                1.0
            };

            // Sync sample_pos if current_sample_idx was set independently
            if voice.sample_pos == 0.0 && voice.current_sample_idx != 0 {
                voice.sample_pos = voice.current_sample_idx as f32;
            }

            if has_synth {
                let raw_samples = voice.synth_samples.len();
                let is_looping = voice.synth_looping;

                for i in 0..samples_per_frame {
                    if raw_samples == 0 {
                        voice.state = PlaybackState::Stopped;
                        break;
                    }

                    if voice.sample_pos >= raw_samples as f32 {
                        if is_looping {
                            voice.sample_pos %= raw_samples as f32;
                        } else {
                            voice.state = PlaybackState::Stopped;
                            break;
                        }
                    }

                    let idx0 = voice.sample_pos.floor() as usize;
                    if idx0 >= raw_samples {
                        if is_looping {
                            voice.sample_pos %= raw_samples as f32;
                        } else {
                            voice.state = PlaybackState::Stopped;
                            break;
                        }
                    }

                    let idx0 = voice.sample_pos.floor() as usize;
                    let frac = voice.sample_pos - (idx0 as f32);
                    let s0 = voice.synth_samples[idx0] as f32;
                    let s1 = if idx0 + 1 < raw_samples {
                        voice.synth_samples[idx0 + 1] as f32
                    } else if is_looping {
                        voice.synth_samples[0] as f32
                    } else {
                        s0
                    };

                    let sample_val = s0 * (1.0 - frac) + s1 * frac;
                    voice.sample_pos += step;
                    voice.current_sample_idx = voice.sample_pos.floor() as usize;

                    mix_l[i] += sample_val * vol_l;
                    mix_r[i] += sample_val * vol_r;
                }

                if voice.state == PlaybackState::Stopped {
                    voice.sample_pos = 0.0;
                    voice.current_sample_idx = 0;
                }
            } else {
                let wb = &voice.wave_buffers[0];
                let raw_samples = wb.buffer_size / 2; // 16-bit samples
                let is_looping = wb.is_looping;
                let buf_addr = wb.buffer_addr;

                for i in 0..samples_per_frame {
                    if raw_samples == 0 {
                        voice.state = PlaybackState::Stopped;
                        break;
                    }

                    if voice.sample_pos >= raw_samples as f32 {
                        if is_looping {
                            voice.sample_pos %= raw_samples as f32;
                        } else {
                            voice.state = PlaybackState::Stopped;
                            break;
                        }
                    }

                    let idx0 = voice.sample_pos.floor() as usize;
                    if idx0 >= raw_samples {
                        if is_looping {
                            voice.sample_pos %= raw_samples as f32;
                        } else {
                            voice.state = PlaybackState::Stopped;
                            break;
                        }
                    }

                    let idx0 = voice.sample_pos.floor() as usize;
                    let frac = voice.sample_pos - (idx0 as f32);
                    let addr0 = buf_addr + (idx0 * 2) as u64;
                    let s0 = mmu.read_u16(addr0).unwrap_or(0) as i16 as f32;
                    let s1 = if idx0 + 1 < raw_samples {
                        let addr1 = buf_addr + ((idx0 + 1) * 2) as u64;
                        mmu.read_u16(addr1).unwrap_or(0) as i16 as f32
                    } else if is_looping {
                        mmu.read_u16(buf_addr).unwrap_or(0) as i16 as f32
                    } else {
                        s0
                    };

                    let sample_val = s0 * (1.0 - frac) + s1 * frac;
                    voice.sample_pos += step;
                    voice.current_sample_idx = voice.sample_pos.floor() as usize;

                    mix_l[i] += sample_val * vol_l;
                    mix_r[i] += sample_val * vol_r;
                }

                if voice.state == PlaybackState::Stopped {
                    voice.wave_buffers.remove(0);
                    voice.sample_pos = 0.0;
                    voice.current_sample_idx = 0;
                    if !voice.wave_buffers.is_empty() {
                        voice.state = PlaybackState::Playing;
                    }
                }
            }
        }

        // Clip and write interleaved 16-bit PCM (L, R) into output sink
        let mut peak = self.peak_amplitude;
        for i in 0..samples_per_frame {
            let sample_l = (mix_l[i].clamp(-32768.0, 32767.0)) as i16;
            let sample_r = (mix_r[i].clamp(-32768.0, 32767.0)) as i16;

            let abs_l = sample_l.saturating_abs();
            let abs_r = sample_r.saturating_abs();
            if abs_l > peak {
                peak = abs_l;
            }
            if abs_r > peak {
                peak = abs_r;
            }

            if self.output_ring_buffer.len() < 192_000 {
                self.output_ring_buffer.push(sample_l);
                self.output_ring_buffer.push(sample_r);
            }
        }

        self.peak_amplitude = peak;
        self.frames_rendered += 1;
        self.total_samples_mixed += (samples_per_frame * 2) as u64;
        samples_per_frame * 2
    }

    /// Configures a synthetic oscillator tone on the specified voice.
    pub fn set_voice_synth(
        &mut self,
        voice_id: u32,
        waveform: Waveform,
        frequency_hz: f32,
        sample_rate: u32,
        duration_ms: f32,
        looping: bool,
    ) -> bool {
        if let Some(voice) = self.get_voice_mut(voice_id) {
            voice.set_synth_tone(waveform, frequency_hz, sample_rate, duration_ms, looping);
            true
        } else {
            false
        }
    }

    /// Configures a sine oscillator on the specified voice.
    pub fn set_voice_sine(
        &mut self,
        voice_id: u32,
        freq_hz: f32,
        sample_rate: u32,
        duration_ms: f32,
        looping: bool,
    ) -> bool {
        self.set_voice_synth(voice_id, Waveform::Sine, freq_hz, sample_rate, duration_ms, looping)
    }

    /// Configures a square wave oscillator on the specified voice.
    pub fn set_voice_square(
        &mut self,
        voice_id: u32,
        freq_hz: f32,
        sample_rate: u32,
        duration_ms: f32,
        looping: bool,
    ) -> bool {
        self.set_voice_synth(voice_id, Waveform::Square, freq_hz, sample_rate, duration_ms, looping)
    }

    /// Mixes one DSP frame and pushes the generated 48 kHz stereo samples to the host audio sink.
    pub fn pace_frame(&mut self, mmu: &Memory, host: Option<&HostAudioSink>) -> usize {
        let mixed = self.render_dsp_tick(mmu);
        if let Some(sink) = host {
            let samples = self.drain_samples();
            if !samples.is_empty() {
                sink.push_samples(&samples);
            }
        }
        mixed
    }

    /// Renders multiple DSP frames in sequence, pacing each to the host audio sink if provided.
    pub fn render_and_pace_frames(
        &mut self,
        mmu: &Memory,
        frame_count: usize,
        host: Option<&HostAudioSink>,
    ) -> usize {
        let mut total = 0;
        for _ in 0..frame_count {
            total += self.pace_frame(mmu, host);
        }
        total
    }

    /// Flushes all mixed samples in `output_ring_buffer` directly to the host audio sink.
    pub fn sync_to_host(&mut self, host: &HostAudioSink) -> usize {
        let samples = self.drain_samples();
        let count = samples.len();
        if count > 0 {
            host.push_samples(&samples);
        }
        count
    }

    /// Returns the number of unplayed mixed samples in the output ring buffer.
    pub fn buffered_sample_count(&self) -> usize {
        self.output_ring_buffer.len()
    }

    /// Latency of the current renderer output ring buffer in milliseconds.
    pub fn latency_ms(&self) -> f64 {
        let queued = self.output_ring_buffer.len() as f64;
        let rate = self.config.sample_rate as f64;
        if rate > 0.0 {
            (queued / (rate * 2.0)) * 1000.0
        } else {
            0.0
        }
    }

    /// Drains all mixed output samples from the ring buffer for host audio playback.
    pub fn drain_samples(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.output_ring_buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    #[test]
    fn test_audren_voice_mixing_and_dsp_ticks() {
        let mut mmu = Memory::new();
        let mut renderer = AudioRenderer::new(AudioRendererConfig {
            sample_rate: AUDREN_SAMPLE_RATE_48KHZ,
            sample_count: 4, // 4 stereo samples = 8 values
            ..Default::default()
        });

        let buf_addr = 0x8200_0000u64;
        mmu.map_range(buf_addr, 4096, Permission::READ_WRITE);

        // Write a sine/pulse wave into memory
        let samples: [i16; 4] = [5000, 10000, -5000, -10000];
        for (i, &s) in samples.iter().enumerate() {
            mmu.write_u16(buf_addr + (i * 2) as u64, s as u16).unwrap();
        }

        // Configure voice 0
        let voice = renderer.get_voice_mut(0).unwrap();
        voice.volume = 1.0;
        voice.mix_volumes = [0.8, 0.5]; // 80% left, 50% right
        voice.queue_buffer(WaveBuffer {
            buffer_addr: buf_addr,
            buffer_size: samples.len() * 2,
            start_sample_offset: 0,
            end_sample_offset: 4,
            is_looping: false,
            is_stream_ended: true,
        });

        let mixed = renderer.render_dsp_tick(&mmu);
        assert_eq!(mixed, 8); // 4 stereo pairs = 8 samples
        assert_eq!(renderer.frames_rendered, 1);
        assert!(renderer.peak_amplitude >= 8000);
        assert_eq!(renderer.output_ring_buffer.len(), 8);
    }

    #[test]
    fn test_audren_multi_voice_polyphony_and_pan() {
        let mut mmu = Memory::new();
        let mut renderer = AudioRenderer::new(AudioRendererConfig {
            sample_rate: AUDREN_SAMPLE_RATE_48KHZ,
            sample_count: 2, // 2 stereo samples
            ..Default::default()
        });

        let buf_addr_a = 0x8200_0000u64;
        let buf_addr_b = 0x8200_1000u64;
        mmu.map_range(buf_addr_a, 4096, Permission::READ_WRITE);
        mmu.map_range(buf_addr_b, 4096, Permission::READ_WRITE);

        // Voice A: 10000 PCM
        mmu.write_u16(buf_addr_a, 10000u16).unwrap();
        mmu.write_u16(buf_addr_a + 2, 10000u16).unwrap();

        // Voice B: 5000 PCM
        mmu.write_u16(buf_addr_b, 5000u16).unwrap();
        mmu.write_u16(buf_addr_b + 2, 5000u16).unwrap();

        // Configure Voice 0 (Panned 100% Left)
        let v0 = renderer.get_voice_mut(0).unwrap();
        v0.mix_volumes = [1.0, 0.0];
        v0.queue_buffer(WaveBuffer {
            buffer_addr: buf_addr_a,
            buffer_size: 4,
            start_sample_offset: 0,
            end_sample_offset: 2,
            is_looping: false,
            is_stream_ended: true,
        });

        // Configure Voice 1 (Panned 100% Right)
        let v1 = renderer.get_voice_mut(1).unwrap();
        v1.mix_volumes = [0.0, 1.0];
        v1.queue_buffer(WaveBuffer {
            buffer_addr: buf_addr_b,
            buffer_size: 4,
            start_sample_offset: 0,
            end_sample_offset: 2,
            is_looping: false,
            is_stream_ended: true,
        });

        let mixed = renderer.render_dsp_tick(&mmu);
        assert_eq!(mixed, 4); // 2 samples * 2 channels = 4 samples
        assert_eq!(renderer.active_voice_count(), 2);

        // Left channel should be ~10000, Right channel should be ~5000
        let left_out = renderer.output_ring_buffer[0];
        let right_out = renderer.output_ring_buffer[1];
        assert_eq!(left_out, 10000);
        assert_eq!(right_out, 5000);
    }

    #[test]
    fn test_audren_multivoice_48khz_conversion_and_pacing() {
        let mmu = Memory::new();
        let mut renderer = AudioRenderer::new(AudioRendererConfig {
            sample_rate: AUDREN_SAMPLE_RATE_48KHZ,
            sample_count: 240, // 5ms frame = 480 stereo samples
            ..Default::default()
        });
        let host = HostAudioSink::new();

        // Voice 0: 32 kHz tone (will be resampled to 48 kHz by 2/3 ratio)
        renderer.set_voice_sine(0, 440.0, AUDREN_SAMPLE_RATE_32KHZ, 20.0, true);
        // Voice 1: 48 kHz tone (1:1 ratio)
        renderer.set_voice_square(1, 880.0, AUDREN_SAMPLE_RATE_48KHZ, 20.0, true);

        assert_eq!(renderer.active_voice_count(), 2);

        // Pace 1 frame (5ms at 48kHz = 240 frames = 480 stereo samples)
        let mixed = renderer.pace_frame(&mmu, Some(&host));
        assert_eq!(mixed, 480);
        assert_eq!(renderer.frames_rendered, 1);
        assert!(renderer.total_samples_mixed == 480);
        assert!(renderer.peak_amplitude > 10_000);

        // The samples were drained and paced directly to the host sink
        assert_eq!(renderer.buffered_sample_count(), 0);
        assert!(host.queued_sample_count() <= 480);
    }

    #[test]
    fn test_audren_voice_pitch_scaling() {
        let mmu = Memory::new();
        let mut renderer = AudioRenderer::new(AudioRendererConfig {
            sample_rate: AUDREN_SAMPLE_RATE_48KHZ,
            sample_count: 100,
            ..Default::default()
        });

        // Set voice 0 with pitch 2.0 (octave up)
        renderer.set_voice_sine(0, 440.0, AUDREN_SAMPLE_RATE_48KHZ, 10.0, true);
        let v = renderer.get_voice_mut(0).unwrap();
        v.pitch = 2.0;

        let mixed = renderer.render_dsp_tick(&mmu);
        assert_eq!(mixed, 200);

        // Since pitch is 2.0 and sample_rate is 48k, sample_pos should have advanced by 200 samples
        let v = renderer.get_voice_mut(0).unwrap();
        assert!((v.sample_pos - 200.0).abs() < 1.0);
    }
}
