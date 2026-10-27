#![allow(dead_code)]

//! `IAudioRenderer` for libnx-shaped clients: `RequestUpdate` parses the update data libnx's audio
//! driver writes (layout and sizes from libnx `services/audren.h`) and answers with the output data;
//! each rendered frame mixes up to 64 PCM16 voices into 48 kHz stereo.
//!
//! The mix follows the renderer's routing: a voice's channels go through their channel resources'
//! mix factors into the final mix's buffers, the final mix volume applies, and the device sink takes
//! its input buffers as left and right. Frames advance when the guest waits on the renderer's system
//! event (see `HorizonKernel::dispatch_svc`, 0x18), which stands in for the 5 ms hardware tick.

use crate::mmu::Memory;

/// Section sizes, from libnx `audren.h`.
pub const HEADER_SIZE: usize = 0x40;
pub const BEHAVIOR_IN_SIZE: usize = 0x10;
pub const BEHAVIOR_OUT_SIZE: usize = 0xB0;
pub const MEMPOOL_IN_SIZE: usize = 0x20;
pub const MEMPOOL_OUT_SIZE: usize = 0x10;
pub const CHANNEL_IN_SIZE: usize = 0x70;
pub const VOICE_IN_SIZE: usize = 0x170;
pub const VOICE_OUT_SIZE: usize = 0x10;
pub const WAVEBUF_SIZE: usize = 0x38;
pub const MIX_IN_SIZE: usize = 0x930;
pub const SINK_IN_SIZE: usize = 0x140;
pub const SINK_OUT_SIZE: usize = 0x20;
pub const PERF_IN_SIZE: usize = 0x10;
pub const PERF_OUT_SIZE: usize = 0x10;

pub const MAX_VOICES: usize = 64;
pub const MIX_BUFFERS: usize = 24;
pub const OUTPUT_RATE: u32 = 48_000;
pub const SAMPLES_PER_FRAME: usize = 240;

/// `AudioRendererMemPoolState`.
const MEMPOOL_REQUEST_DETACH: u32 = 2;
const MEMPOOL_DETACHED: u32 = 3;
const MEMPOOL_REQUEST_ATTACH: u32 = 4;
const MEMPOOL_ATTACHED: u32 = 5;
/// `AudioRendererVoicePlayState_Started`; `PcmFormat_Int16`.
const VOICE_STARTED: u8 = 0;
const PCM_INT16: u8 = 2;
const FINAL_MIX_ID: u32 = 0;

fn rd32(b: &[u8], off: usize) -> u32 {
    b.get(off..off + 4).map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()))
}
fn rd64(b: &[u8], off: usize) -> u64 {
    b.get(off..off + 8).map_or(0, |s| u64::from_le_bytes(s.try_into().unwrap()))
}
fn rdf32(b: &[u8], off: usize) -> f32 {
    f32::from_bits(rd32(b, off))
}
fn wr32(b: &mut [u8], off: usize, v: u32) {
    if let Some(s) = b.get_mut(off..off + 4) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}
fn wr64(b: &mut [u8], off: usize, v: u64) {
    if let Some(s) = b.get_mut(off..off + 8) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}

/// One `AudioRendererWaveBuf` as the client last described it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WaveBuf {
    pub addr: u64,
    pub size: u64,
    pub start: i32,
    pub end: i32,
    pub looping: bool,
}

/// Client-side voice parameters from the last update, plus the server's playback position.
#[derive(Debug, Clone, Default)]
pub struct Voice {
    pub used: bool,
    pub started: bool,
    pub pcm16: bool,
    pub sample_rate: u32,
    pub channel_count: u32,
    pub pitch: f32,
    pub volume: f32,
    pub dest_mix: u32,
    pub channel_ids: [u32; 6],
    pub wavebufs: [WaveBuf; 4],
    /// Ring slot being played, and the wave buffer last finished in each slot (not replayed unless the
    /// client puts a different one there, or it loops).
    pub slot: usize,
    pub finished: [Option<WaveBuf>; 4],
    /// Position in the current wave buffer, in source samples (fractional for resampling).
    pub position: f64,
    pub played: u64,
    pub consumed: u32,
}

/// Renderer parameters from `OpenAudioRenderer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererParams {
    pub sample_rate: u32,
    pub sample_count: u32,
    pub mix_buffer_count: u32,
    pub voice_count: u32,
    pub sink_count: u32,
}

#[derive(Debug)]
pub struct AudioRendererModel {
    pub params: RendererParams,
    pub started: bool,
    pub voices: Vec<Voice>,
    /// `AudioRendererChannelInfoIn::mix` per channel resource.
    pub channels: Vec<[f32; MIX_BUFFERS]>,
    pub final_mix_volume: f32,
    /// Device sink inputs (mix buffer indices), left then right.
    pub sink_inputs: Vec<u8>,
    /// Mixed interleaved stereo frames waiting to go to the host.
    pub output: Vec<i16>,
    pub frames_rendered: u64,
    pub updates: u64,
}

impl AudioRendererModel {
    pub fn new(params: RendererParams) -> Self {
        let voices = (params.voice_count as usize).clamp(1, MAX_VOICES);
        Self {
            params,
            started: false,
            voices: vec![Voice::default(); voices],
            channels: vec![[0.0; MIX_BUFFERS]; voices],
            final_mix_volume: 1.0,
            sink_inputs: vec![0, 1],
            output: Vec::new(),
            frames_rendered: 0,
            updates: 0,
        }
    }

    /// `RequestUpdate`: applies `input` and writes the output data into `out`. Returns a Horizon result.
    pub fn update(&mut self, input: &[u8], out: &mut [u8]) -> u32 {
        if input.len() < HEADER_SIZE {
            return 0x2A1; // audio: invalid update data
        }
        self.updates += 1;
        let revision = rd32(input, 0);
        let sz = |i: usize| rd32(input, 4 + i * 4) as usize;
        let (behavior_sz, mempools_sz, voices_sz, channels_sz, effects_sz, mixes_sz, sinks_sz, perf_sz) =
            (sz(0), sz(1), sz(2), sz(3), sz(4), sz(5), sz(6), sz(7));
        let mut off = HEADER_SIZE + behavior_sz;
        let mempools = &input[off.min(input.len())..(off + mempools_sz).min(input.len())];
        off += mempools_sz;
        let channels = &input[off.min(input.len())..(off + channels_sz).min(input.len())];
        off += channels_sz;
        let voices = &input[off.min(input.len())..(off + voices_sz).min(input.len())];
        off += voices_sz + effects_sz;
        let mixes = &input[off.min(input.len())..(off + mixes_sz).min(input.len())];
        off += mixes_sz;
        let sinks = &input[off.min(input.len())..(off + sinks_sz).min(input.len())];
        let _ = perf_sz;

        for (i, c) in channels.chunks_exact(CHANNEL_IN_SIZE).enumerate() {
            if let Some(dst) = self.channels.get_mut(i) {
                if c[0x64] != 0 {
                    for (b, m) in dst.iter_mut().enumerate() {
                        *m = rdf32(c, 4 + b * 4);
                    }
                }
            }
        }
        // One entry per voice slot, in order; unused slots are zero (their `id` field too).
        for (id, v) in voices.chunks_exact(VOICE_IN_SIZE).enumerate() {
            let Some(voice) = self.voices.get_mut(id) else { continue };
            if v[0x8] != 0 {
                // is_new: a fresh voice starts at the wave buffer ring's head.
                *voice = Voice { slot: (rd32(v, 0x40) as i16).max(0) as usize % 4, ..Default::default() };
            }
            voice.used = v[0x9] != 0;
            voice.started = v[0xA] == VOICE_STARTED;
            voice.pcm16 = v[0xB] == PCM_INT16;
            voice.sample_rate = rd32(v, 0xC);
            voice.channel_count = rd32(v, 0x18).min(6);
            voice.pitch = rdf32(v, 0x1C);
            voice.volume = rdf32(v, 0x20);
            voice.dest_mix = rd32(v, 0x58);
            for (i, wb) in voice.wavebufs.iter_mut().enumerate() {
                let w = &v[0x60 + i * WAVEBUF_SIZE..];
                *wb = WaveBuf { addr: rd64(w, 0), size: rd64(w, 8), start: rd32(w, 0x10) as i32, end: rd32(w, 0x14) as i32, looping: w[0x18] != 0 };
            }
            for (i, c) in voice.channel_ids.iter_mut().enumerate() {
                *c = rd32(v, 0x140 + i * 4);
            }
        }
        for m in mixes.chunks_exact(MIX_IN_SIZE) {
            if m[0xC] != 0 && rd32(m, 0x10) == FINAL_MIX_ID {
                self.final_mix_volume = rdf32(m, 0);
            }
        }
        for s in sinks.chunks_exact(SINK_IN_SIZE) {
            // Device sink (type 1), in use: its input buffers become left and right.
            if s[0] == 1 && s[1] != 0 {
                let count = rd32(s, 0x20 + 0x100).min(6) as usize;
                self.sink_inputs = s[0x20 + 0x104..0x20 + 0x104 + count].to_vec();
            }
        }

        // Output: header, mempool states, voice status, sink status, performance, behavior.
        let n_mempools = mempools_sz / MEMPOOL_IN_SIZE;
        let n_voices = voices_sz / VOICE_IN_SIZE;
        let n_sinks = sinks_sz / SINK_IN_SIZE;
        let total = HEADER_SIZE + n_mempools * MEMPOOL_OUT_SIZE + n_voices * VOICE_OUT_SIZE + n_sinks * SINK_OUT_SIZE + PERF_OUT_SIZE + BEHAVIOR_OUT_SIZE;
        if out.len() < total {
            return 0x2A1;
        }
        out[..total].fill(0);
        wr32(out, 0, revision);
        wr32(out, 4, BEHAVIOR_OUT_SIZE as u32);
        wr32(out, 8, (n_mempools * MEMPOOL_OUT_SIZE) as u32);
        wr32(out, 12, (n_voices * VOICE_OUT_SIZE) as u32);
        wr32(out, 28, (n_sinks * SINK_OUT_SIZE) as u32);
        wr32(out, 32, PERF_OUT_SIZE as u32);
        wr32(out, 60, total as u32);
        let mut o = HEADER_SIZE;
        for p in mempools.chunks_exact(MEMPOOL_IN_SIZE) {
            let state = match rd32(p, 0x10) {
                MEMPOOL_REQUEST_ATTACH => MEMPOOL_ATTACHED,
                MEMPOOL_REQUEST_DETACH => MEMPOOL_DETACHED,
                s => s,
            };
            wr32(out, o, state);
            o += MEMPOOL_OUT_SIZE;
        }
        for (id, _) in voices.chunks_exact(VOICE_IN_SIZE).enumerate() {
            if let Some(voice) = self.voices.get(id) {
                wr64(out, o, voice.played);
                wr32(out, o + 8, voice.consumed);
            }
            o += VOICE_OUT_SIZE;
        }
        0
    }

    /// Mixes one 5 ms frame (240 stereo samples at 48 kHz) into `output`.
    pub fn render_frame(&mut self, mmu: &Memory) {
        let mut buffers = vec![[0f32; SAMPLES_PER_FRAME]; MIX_BUFFERS];
        if self.started {
            for v in 0..self.voices.len() {
                self.mix_voice(v, mmu, &mut buffers);
            }
        }
        let left = self.sink_inputs.first().copied().unwrap_or(0) as usize % MIX_BUFFERS;
        let right = self.sink_inputs.get(1).copied().unwrap_or(left as u8) as usize % MIX_BUFFERS;
        let clamp = |x: f32| (x * self.final_mix_volume).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        for i in 0..SAMPLES_PER_FRAME {
            self.output.push(clamp(buffers[left][i]));
            self.output.push(clamp(buffers[right][i]));
        }
        self.frames_rendered += 1;
    }

    fn mix_voice(&mut self, index: usize, mmu: &Memory, buffers: &mut [[f32; SAMPLES_PER_FRAME]]) {
        let channels = self.channels.clone();
        let voice = &mut self.voices[index];
        if !(voice.used && voice.started && voice.pcm16) || voice.channel_count == 0 || voice.dest_mix != FINAL_MIX_ID {
            return;
        }
        let step = voice.sample_rate as f64 * voice.pitch.max(0.0) as f64 / OUTPUT_RATE as f64;
        let ch = voice.channel_count as usize;
        for i in 0..SAMPLES_PER_FRAME {
            // The current slot plays unless it is empty or the client has not replaced the wave buffer
            // finished there (then nothing more is queued).
            let wb = voice.wavebufs[voice.slot];
            if wb.addr == 0 || wb.end <= wb.start || voice.finished[voice.slot] == Some(wb) {
                return;
            }
            let frame = wb.start as u64 + voice.position as u64;
            for c in 0..ch {
                let addr = wb.addr + (frame * ch as u64 + c as u64) * 2;
                let sample = mmu.read_u16(addr).unwrap_or(0) as i16 as f32 * voice.volume;
                if let Some(mix) = channels.get(voice.channel_ids[c] as usize) {
                    for (b, gain) in mix.iter().enumerate() {
                        if *gain != 0.0 {
                            buffers[b][i] += sample * gain;
                        }
                    }
                }
            }
            voice.played += 1;
            voice.position += step;
            if wb.start as f64 + voice.position >= wb.end as f64 {
                voice.position = 0.0;
                voice.consumed += 1;
                if !wb.looping {
                    voice.finished[voice.slot] = Some(wb);
                    voice.slot = (voice.slot + 1) % 4;
                }
            }
        }
    }

    /// Takes the mixed samples for the host.
    pub fn drain(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    /// Update data with one mempool, two channels, one voice, one final mix and one device sink.
    fn update_data(voice_new: bool, started: bool, pcm_addr: u64, samples: i32) -> Vec<u8> {
        let sizes = [BEHAVIOR_IN_SIZE, MEMPOOL_IN_SIZE, VOICE_IN_SIZE, 2 * CHANNEL_IN_SIZE, 0, MIX_IN_SIZE, SINK_IN_SIZE, PERF_IN_SIZE];
        let total = HEADER_SIZE + sizes.iter().sum::<usize>();
        let mut b = vec![0u8; total];
        wr32(&mut b, 0, 0x3156_4552);
        for (i, s) in sizes.iter().enumerate() {
            wr32(&mut b, 4 + i * 4, *s as u32);
        }
        wr32(&mut b, 60, total as u32);
        let mut off = HEADER_SIZE + BEHAVIOR_IN_SIZE;
        wr32(&mut b, off + 0x10, MEMPOOL_REQUEST_ATTACH);
        off += MEMPOOL_IN_SIZE;
        // Channel 0 -> mix buffer 0 (left), channel 1 -> buffer 1 (right); voice uses channel 0 only, so
        // route channel 0 to both.
        for c in 0..2 {
            let ch = off + c * CHANNEL_IN_SIZE;
            wr32(&mut b, ch + 4, 1.0f32.to_bits());
            wr32(&mut b, ch + 8, 1.0f32.to_bits());
            b[ch + 0x64] = 1;
        }
        off += 2 * CHANNEL_IN_SIZE;
        let v = off;
        b[v + 0x8] = voice_new as u8;
        b[v + 0x9] = 1;
        b[v + 0xA] = if started { VOICE_STARTED } else { 1 };
        b[v + 0xB] = PCM_INT16;
        wr32(&mut b, v + 0xC, 48_000);
        wr32(&mut b, v + 0x18, 1);
        wr32(&mut b, v + 0x1C, 1.0f32.to_bits());
        wr32(&mut b, v + 0x20, 0.5f32.to_bits());
        wr64(&mut b, v + 0x60, pcm_addr);
        wr64(&mut b, v + 0x68, samples as u64 * 2);
        wr32(&mut b, v + 0x74, samples as u32);
        off += VOICE_IN_SIZE;
        let m = off;
        wr32(&mut b, m, 1.0f32.to_bits());
        b[m + 0xC] = 1;
        off += MIX_IN_SIZE;
        let s = off;
        b[s] = 1;
        b[s + 1] = 1;
        wr32(&mut b, s + 0x20 + 0x100, 2);
        b[s + 0x20 + 0x104] = 0;
        b[s + 0x20 + 0x105] = 1;
        b
    }

    #[test]
    fn one_voice_mixes_to_both_channels_and_finishes() {
        let mut mmu = Memory::new();
        mmu.map_range(0x5000_0000, 0x2000, Permission::READ_WRITE);
        for i in 0..480u64 {
            mmu.write_u16(0x5000_0000 + i * 2, 1000).unwrap();
        }
        let mut r = AudioRendererModel::new(RendererParams { sample_rate: 48_000, sample_count: 240, mix_buffer_count: 2, voice_count: 4, sink_count: 1 });
        r.started = true;
        let mut out = vec![0u8; 0x400];
        assert_eq!(r.update(&update_data(true, true, 0x5000_0000, 480), &mut out), 0);
        assert_eq!(rd32(&out, HEADER_SIZE), MEMPOOL_ATTACHED);
        r.render_frame(&mmu);
        r.render_frame(&mmu);
        r.render_frame(&mmu);
        let pcm = r.drain();
        assert_eq!(pcm.len(), 3 * 2 * SAMPLES_PER_FRAME);
        // 1000 * volume 0.5 on both sides for 480 samples, then silence.
        assert!(pcm[..960].iter().all(|&s| s == 500), "{:?}", &pcm[..8]);
        assert!(pcm[960..].iter().all(|&s| s == 0));
        assert_eq!(r.update(&update_data(false, true, 0x5000_0000, 480), &mut out), 0);
        let voice_out = HEADER_SIZE + MEMPOOL_OUT_SIZE;
        assert_eq!((rd64(&out, voice_out), rd32(&out, voice_out + 8)), (480, 1));
    }
}
