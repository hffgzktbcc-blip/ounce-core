#![allow(dead_code)]

use crate::mmu::Memory;

/// Horizon OS `audren:u` Audio Renderer Sample Rates
pub const AUDREN_SAMPLE_RATE_48KHZ: u32 = 48_000;
pub const AUDREN_SAMPLE_RATE_32KHZ: u32 = 32_000;

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
    /// Queued wave buffers
    pub wave_buffers: Vec<WaveBuffer>,
    pub current_sample_idx: usize,
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
        }
    }

    pub fn queue_buffer(&mut self, wb: WaveBuffer) {
        self.wave_buffers.push(wb);
        if self.state == PlaybackState::Stopped {
            self.state = PlaybackState::Playing;
        }
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
            output_ring_buffer: Vec::with_capacity(48_000),
            frames_rendered: 0,
            total_samples_mixed: 0,
            peak_amplitude: 0,
        }
    }

    /// Allocates or retrieves a voice by index.
    pub fn get_voice_mut(&mut self, voice_id: u32) -> Option<&mut AudioVoice> {
        self.voices.get_mut(voice_id as usize)
    }

    /// Mixes one DSP tick frame (typically 240 stereo samples) from all active voices into the output sink.
    pub fn render_dsp_tick(&mut self, mmu: &Memory) -> usize {
        if !self.is_active {
            return 0;
        }

        let samples_per_frame = self.config.sample_count as usize;
        // Temporary float accumulation buffers for stereo output
        let mut mix_l = vec![0.0f32; samples_per_frame];
        let mut mix_r = vec![0.0f32; samples_per_frame];

        for voice in &mut self.voices {
            if voice.state != PlaybackState::Playing || voice.wave_buffers.is_empty() {
                continue;
            }

            let vol_l = voice.volume * voice.mix_volumes[0];
            let vol_r = voice.volume * voice.mix_volumes[1];

            let wb = &voice.wave_buffers[0];
            let raw_samples = wb.buffer_size / 2; // 16-bit samples

            for i in 0..samples_per_frame {
                if voice.current_sample_idx >= raw_samples {
                    if wb.is_looping {
                        voice.current_sample_idx = 0;
                    } else {
                        voice.state = PlaybackState::Stopped;
                        break;
                    }
                }

                let sample_addr = wb.buffer_addr + (voice.current_sample_idx * 2) as u64;
                let sample_val = mmu.read_u16(sample_addr).unwrap_or(0) as i16 as f32;
                voice.current_sample_idx += 1;

                mix_l[i] += sample_val * vol_l;
                mix_r[i] += sample_val * vol_r;
            }

            if voice.state == PlaybackState::Stopped {
                voice.wave_buffers.remove(0);
                voice.current_sample_idx = 0;
                if !voice.wave_buffers.is_empty() {
                    voice.state = PlaybackState::Playing;
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

            if self.output_ring_buffer.len() < 96_000 {
                self.output_ring_buffer.push(sample_l);
                self.output_ring_buffer.push(sample_r);
            }
        }

        self.peak_amplitude = peak;
        self.frames_rendered += 1;
        self.total_samples_mixed += (samples_per_frame * 2) as u64;
        samples_per_frame * 2
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
}
