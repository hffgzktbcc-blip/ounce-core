#![allow(dead_code)]

//! `IAudioRenderer` for libnx-shaped clients: `RequestUpdate` parses the update data libnx's audio
//! driver writes (layout and sizes from libnx `services/audren.h`) and answers with the output data;
//! each rendered frame mixes up to 64 voices into 48 kHz stereo.
//!
//! Voices may be PCM16, PCM float or DSP-ADPCM (`PcmFormat` from libnx `audio/audio.h`). Each voice
//! sample runs through the voice's two biquad filters (libnx `audrvVoiceSetBiquadFilter`), its volume,
//! and its channel resources' mix factors into the buffers of its destination mix. Submixes then route
//! into their destination mix through `volume * mix[src][dest]`, deepest first, until everything has
//! reached the final mix; the final mix volume applies and the device sink takes its input buffers as
//! left and right. Frames advance when the guest waits on the renderer's system event (see
//! `HorizonKernel::dispatch_svc`, 0x18), which stands in for the 5 ms hardware tick.
//!
//! Not modelled: effects and splitters (libnx's driver never writes those sections: "todo: effects,
//! splitters" in `audren.h`), voice drops, circular-buffer sinks.

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
/// `AudioRendererVoicePlayState_Started`; `PcmFormat_Int16`, `_Float`, `_Adpcm`.
const VOICE_STARTED: u8 = 0;
pub const PCM_INT16: u8 = 2;
pub const PCM_FLOAT: u8 = 5;
pub const PCM_ADPCM: u8 = 6;
const FINAL_MIX_ID: u32 = 0;
/// DSP-ADPCM: 8-byte frames of one header byte (coefficient pair index << 4 | scale shift) and 14
/// 4-bit samples. Coefficients are 8 pairs of Q11 values (`AudioRendererAdpcmParameters`).
pub const ADPCM_FRAME_BYTES: usize = 8;
pub const ADPCM_FRAME_SAMPLES: usize = 14;
/// Largest ADPCM wave buffer decoded at once (16 MiB of frames).
const ADPCM_MAX_BYTES: u64 = 16 << 20;

fn rd16(b: &[u8], off: usize) -> u16 {
    b.get(off..off + 2).map_or(0, |s| u16::from_le_bytes(s.try_into().unwrap()))
}
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

/// Decodes DSP-ADPCM samples `start..end` of `data`. `history` is (previous sample, the one before)
/// at `start`, from the wave buffer's `AudioRendererAdpcmContext` (`history0`, `history1`).
/// sample = clamp((((nibble * scale) << 11) + 1024 + c0 * h0 + c1 * h1) >> 11).
pub fn decode_dsp_adpcm(data: &[u8], coefs: &[i16; 16], start: usize, end: usize, history: (i16, i16)) -> Vec<i16> {
    let (mut h0, mut h1) = (history.0 as i32, history.1 as i32);
    let mut out = Vec::with_capacity(end.saturating_sub(start));
    for n in start..end {
        let frame = n / ADPCM_FRAME_SAMPLES * ADPCM_FRAME_BYTES;
        let pos = n % ADPCM_FRAME_SAMPLES;
        let (Some(&header), Some(&byte)) = (data.get(frame), data.get(frame + 1 + pos / 2)) else { break };
        let nibble = if pos % 2 == 0 { byte >> 4 } else { byte & 0xF };
        let nibble = ((nibble as i8) << 4 >> 4) as i32;
        let scale = 1i32 << (header & 0xF);
        let pair = ((header >> 4) & 7) as usize * 2;
        let (c0, c1) = (coefs[pair] as i32, coefs[pair + 1] as i32);
        let sample = ((((nibble * scale) << 11) + 1024 + c0 * h0 + c1 * h1) >> 11).clamp(i16::MIN as i32, i16::MAX as i32);
        out.push(sample as i16);
        h1 = h0;
        h0 = sample;
    }
    out
}

/// One `AudioRendererWaveBuf` as the client last described it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WaveBuf {
    pub addr: u64,
    pub size: u64,
    pub start: i32,
    pub end: i32,
    pub looping: bool,
    /// `AudioRendererAdpcmContext` for ADPCM wave buffers (0 when absent).
    pub context_addr: u64,
    pub context_size: u64,
}

/// `AudioRendererBiquadFilter`: Q14 coefficients, already divided by a0 (libnx
/// `audrvVoiceSetBiquadFilter`). y = b0 x + b1 x[-1] + b2 x[-2] - a1 y[-1] - a2 y[-2].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Biquad {
    pub enable: bool,
    pub numerator: [i16; 3],
    pub denominator: [i16; 2],
}

/// Biquad history for one voice channel: x[-1], x[-2], y[-1], y[-2].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BiquadState(pub [f32; 4]);

impl Biquad {
    pub fn apply(&self, state: &mut BiquadState, x: f32) -> f32 {
        if !self.enable {
            return x;
        }
        let q = |c: i16| c as f32 / 16384.0;
        let [x1, x2, y1, y2] = state.0;
        let y = q(self.numerator[0]) * x + q(self.numerator[1]) * x1 + q(self.numerator[2]) * x2 - q(self.denominator[0]) * y1 - q(self.denominator[1]) * y2;
        state.0 = [x, x1, y, y1];
        y
    }
}

/// Client-side voice parameters from the last update, plus the server's playback position.
#[derive(Debug, Clone, Default)]
pub struct Voice {
    pub used: bool,
    pub started: bool,
    /// `PcmFormat` (`PCM_INT16`, `PCM_FLOAT`, `PCM_ADPCM`; others are silent).
    pub format: u8,
    pub sample_rate: u32,
    pub channel_count: u32,
    pub pitch: f32,
    pub volume: f32,
    pub biquads: [Biquad; 2],
    /// Biquad history per filter and channel.
    pub biquad_state: [[BiquadState; 6]; 2],
    /// `extra_params_ptr` / `extra_params_sz`: the ADPCM coefficients for ADPCM voices.
    pub extra_params: (u64, u64),
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
    /// Decoded samples `start..end` of the ADPCM wave buffer last played.
    pub adpcm_cache: Option<(WaveBuf, Vec<i16>)>,
}

impl Voice {
    /// Sample `frame` of channel `c` of `wb`, on the PCM16 scale.
    fn sample(&mut self, wb: &WaveBuf, frame: u64, c: usize, mmu: &Memory) -> f32 {
        let ch = self.channel_count as u64;
        match self.format {
            PCM_INT16 => mmu.read_u16(wb.addr + (frame * ch + c as u64) * 2).unwrap_or(0) as i16 as f32,
            PCM_FLOAT => {
                let x = f32::from_bits(mmu.read_u32(wb.addr + (frame * ch + c as u64) * 4).unwrap_or(0));
                if x.is_nan() { 0.0 } else { (x * 32768.0).clamp(i16::MIN as f32, i16::MAX as f32) }
            }
            PCM_ADPCM => {
                if self.adpcm_cache.as_ref().map(|(w, _)| w) != Some(wb) {
                    self.adpcm_cache = Some((*wb, self.decode_adpcm(wb, mmu)));
                }
                let decoded = &self.adpcm_cache.as_ref().unwrap().1;
                decoded.get((frame - wb.start as u64) as usize).copied().unwrap_or(0) as f32
            }
            _ => 0.0,
        }
    }

    fn decode_adpcm(&self, wb: &WaveBuf, mmu: &Memory) -> Vec<i16> {
        let mut coefs = [0i16; 16];
        let (params, params_size) = self.extra_params;
        if params != 0 && params_size >= 32 {
            for (i, c) in coefs.iter_mut().enumerate() {
                *c = mmu.read_u16(params + i as u64 * 2).unwrap_or(0) as i16;
            }
        }
        let mut history = (0, 0);
        if wb.context_addr != 0 && wb.context_size >= 6 {
            history = (mmu.read_u16(wb.context_addr + 2).unwrap_or(0) as i16, mmu.read_u16(wb.context_addr + 4).unwrap_or(0) as i16);
        }
        let mut data = vec![0u8; wb.size.min(ADPCM_MAX_BYTES) as usize];
        if mmu.read_bytes(wb.addr, &mut data).is_err() {
            data.fill(0);
        }
        decode_dsp_adpcm(&data, &coefs, wb.start.max(0) as usize, wb.end.max(0) as usize, history)
    }
}

/// One `AudioRendererMixInfoIn`: the final mix (`mix_id` 0) or a submix.
#[derive(Debug, Clone)]
pub struct Mix {
    pub used: bool,
    pub volume: f32,
    pub buffer_count: u32,
    pub mix_id: u32,
    pub dest_mix: u32,
    /// `mix[src][dest]`, flattened (`src * MIX_BUFFERS + dest`).
    pub matrix: Vec<f32>,
}

impl Mix {
    fn final_mix() -> Self {
        Self { used: true, volume: 1.0, buffer_count: MIX_BUFFERS as u32, mix_id: FINAL_MIX_ID, dest_mix: 0x7FFF_FFFF, matrix: vec![0.0; MIX_BUFFERS * MIX_BUFFERS] }
    }
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
    /// Mix objects in update-data order; until the client sends any, a final mix at volume 1.
    pub mixes: Vec<Mix>,
    /// Device sink inputs (final mix buffer indices), left then right.
    pub sink_inputs: Vec<u8>,
    /// Mixed interleaved stereo frames waiting to go to the host.
    pub output: Vec<i16>,
    pub frames_rendered: u64,
    pub updates: u64,
}

type MixBuffers = Vec<[f32; SAMPLES_PER_FRAME]>;

impl AudioRendererModel {
    pub fn new(params: RendererParams) -> Self {
        let voices = (params.voice_count as usize).clamp(1, MAX_VOICES);
        Self {
            params,
            started: false,
            voices: vec![Voice::default(); voices],
            channels: vec![[0.0; MIX_BUFFERS]; voices],
            mixes: vec![Mix::final_mix()],
            sink_inputs: vec![0, 1],
            output: Vec::new(),
            frames_rendered: 0,
            updates: 0,
        }
    }

    /// The final mix's volume.
    pub fn final_mix_volume(&self) -> f32 {
        self.final_mix_index().map_or(0.0, |i| self.mixes[i].volume)
    }

    fn mix_index(&self, mix_id: u32) -> Option<usize> {
        self.mixes.iter().position(|m| m.used && m.mix_id == mix_id)
    }

    fn final_mix_index(&self) -> Option<usize> {
        self.mix_index(FINAL_MIX_ID)
    }

    /// Hops from mix `i` to the final mix, or `None` when its destination chain is broken or loops.
    fn depth(&self, mut i: usize) -> Option<usize> {
        for hops in 0..=self.mixes.len() {
            if self.mixes[i].mix_id == FINAL_MIX_ID {
                return Some(hops);
            }
            i = self.mix_index(self.mixes[i].dest_mix)?;
        }
        None
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
            voice.format = v[0xB];
            voice.sample_rate = rd32(v, 0xC);
            voice.channel_count = rd32(v, 0x18).min(6);
            voice.pitch = rdf32(v, 0x1C);
            voice.volume = rdf32(v, 0x20);
            for (i, bq) in voice.biquads.iter_mut().enumerate() {
                let f = 0x24 + i * 0xC;
                let new = Biquad {
                    enable: v[f] != 0,
                    numerator: [rd16(v, f + 2) as i16, rd16(v, f + 4) as i16, rd16(v, f + 6) as i16],
                    denominator: [rd16(v, f + 8) as i16, rd16(v, f + 0xA) as i16],
                };
                if new != *bq {
                    // A changed (or newly enabled) filter starts from silence.
                    voice.biquad_state[i] = Default::default();
                }
                *bq = new;
            }
            voice.extra_params = (rd64(v, 0x48), rd64(v, 0x50));
            voice.dest_mix = rd32(v, 0x58);
            for (i, wb) in voice.wavebufs.iter_mut().enumerate() {
                let w = &v[0x60 + i * WAVEBUF_SIZE..];
                *wb = WaveBuf {
                    addr: rd64(w, 0),
                    size: rd64(w, 8),
                    start: rd32(w, 0x10) as i32,
                    end: rd32(w, 0x14) as i32,
                    looping: w[0x18] != 0,
                    context_addr: rd64(w, 0x20),
                    context_size: rd64(w, 0x28),
                };
            }
            for (i, c) in voice.channel_ids.iter_mut().enumerate() {
                *c = rd32(v, 0x140 + i * 4);
            }
        }
        if mixes.len() >= MIX_IN_SIZE {
            self.mixes = mixes
                .chunks_exact(MIX_IN_SIZE)
                .map(|m| Mix {
                    used: m[0xC] != 0,
                    volume: rdf32(m, 0),
                    buffer_count: rd32(m, 8).min(MIX_BUFFERS as u32),
                    mix_id: rd32(m, 0x10),
                    dest_mix: rd32(m, 0x924),
                    matrix: (0..MIX_BUFFERS * MIX_BUFFERS).map(|i| rdf32(m, 0x24 + i * 4)).collect(),
                })
                .collect();
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
        let mut buffers: Vec<MixBuffers> = vec![vec![[0f32; SAMPLES_PER_FRAME]; MIX_BUFFERS]; self.mixes.len()];
        if self.started {
            for v in 0..self.voices.len() {
                if let Some(m) = self.mix_index(self.voices[v].dest_mix) {
                    self.mix_voice(v, mmu, &mut buffers[m]);
                }
            }
            self.route_submixes(&mut buffers);
        }
        let final_mix = self.final_mix_index();
        let volume = self.final_mix_volume();
        let left = self.sink_inputs.first().copied().unwrap_or(0) as usize % MIX_BUFFERS;
        let right = self.sink_inputs.get(1).copied().unwrap_or(left as u8) as usize % MIX_BUFFERS;
        let clamp = |x: f32| (x * volume).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        for i in 0..SAMPLES_PER_FRAME {
            let (l, r) = final_mix.map_or((0.0, 0.0), |m| (buffers[m][left][i], buffers[m][right][i]));
            self.output.push(clamp(l));
            self.output.push(clamp(r));
        }
        self.frames_rendered += 1;
    }

    /// Adds every submix into its destination mix (`volume * mix[src][dest]`), deepest first, so a
    /// chain of submixes has fully accumulated before it moves on. Mixes whose chain never reaches the
    /// final mix are dropped.
    fn route_submixes(&self, buffers: &mut [MixBuffers]) {
        let mut order: Vec<(usize, usize)> = (0..self.mixes.len())
            .filter(|&i| self.mixes[i].used && self.mixes[i].mix_id != FINAL_MIX_ID)
            .filter_map(|i| self.depth(i).map(|d| (d, i)))
            .collect();
        order.sort_by(|a, b| b.cmp(a));
        for (_, i) in order {
            let mix = &self.mixes[i];
            let Some(dest) = self.mix_index(mix.dest_mix) else { continue };
            let dest_count = self.mixes[dest].buffer_count as usize;
            let src = std::mem::take(&mut buffers[i]);
            for (s, src_buf) in src.iter().enumerate().take(mix.buffer_count as usize) {
                for d in 0..dest_count {
                    let gain = mix.volume * mix.matrix[s * MIX_BUFFERS + d];
                    if gain != 0.0 {
                        for (o, x) in buffers[dest][d].iter_mut().zip(src_buf) {
                            *o += x * gain;
                        }
                    }
                }
            }
        }
    }

    fn mix_voice(&mut self, index: usize, mmu: &Memory, buffers: &mut [[f32; SAMPLES_PER_FRAME]]) {
        let channels = self.channels.clone();
        let voice = &mut self.voices[index];
        let format_ok = matches!(voice.format, PCM_INT16 | PCM_FLOAT | PCM_ADPCM);
        if !(voice.used && voice.started && format_ok) || voice.channel_count == 0 {
            return;
        }
        // DSP-ADPCM voices are mono.
        let ch = if voice.format == PCM_ADPCM { 1 } else { voice.channel_count as usize };
        let step = voice.sample_rate as f64 * voice.pitch.max(0.0) as f64 / OUTPUT_RATE as f64;
        for i in 0..SAMPLES_PER_FRAME {
            // The current slot plays unless it is empty or the client has not replaced the wave buffer
            // finished there (then nothing more is queued).
            let wb = voice.wavebufs[voice.slot];
            if wb.addr == 0 || wb.end <= wb.start || voice.finished[voice.slot] == Some(wb) {
                return;
            }
            let frame = wb.start as u64 + voice.position as u64;
            for c in 0..ch {
                let mut sample = voice.sample(&wb, frame, c, mmu);
                for f in 0..2 {
                    sample = voice.biquads[f].apply(&mut voice.biquad_state[f][c], sample);
                }
                let sample = sample * voice.volume;
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

    /// Update data with `channels` channel resources, `voices` voice slots, `mixes` mix objects (entry 0 is
    /// the final mix: volume 1, 2 buffers) and a device sink on final mix buffers 0 and 1. Returns the
    /// buffer and the offsets of the channel, voice and mix sections.
    fn layout(channels: usize, voices: usize, mixes: usize) -> (Vec<u8>, usize, usize, usize) {
        let sizes = [BEHAVIOR_IN_SIZE, 0, voices * VOICE_IN_SIZE, channels * CHANNEL_IN_SIZE, 0, mixes * MIX_IN_SIZE, SINK_IN_SIZE, PERF_IN_SIZE];
        let total = HEADER_SIZE + sizes.iter().sum::<usize>();
        let mut b = vec![0u8; total];
        wr32(&mut b, 0, 0x3656_4552);
        for (i, s) in sizes.iter().enumerate() {
            wr32(&mut b, 4 + i * 4, *s as u32);
        }
        wr32(&mut b, 60, total as u32);
        let ch = HEADER_SIZE + BEHAVIOR_IN_SIZE;
        let vo = ch + channels * CHANNEL_IN_SIZE;
        let mx = vo + voices * VOICE_IN_SIZE;
        let sk = mx + mixes * MIX_IN_SIZE;
        set_mix(&mut b, mx, FINAL_MIX_ID, 1.0, 2, 0x7FFF_FFFF);
        b[sk] = 1;
        b[sk + 1] = 1;
        wr32(&mut b, sk + 0x20 + 0x100, 2);
        b[sk + 0x20 + 0x104] = 0;
        b[sk + 0x20 + 0x105] = 1;
        (b, ch, vo, mx)
    }

    fn set_channel(b: &mut [u8], at: usize, factors: &[(usize, f32)]) {
        for &(buf, f) in factors {
            wr32(b, at + 4 + buf * 4, f.to_bits());
        }
        b[at + 0x64] = 1;
    }

    fn set_mix(b: &mut [u8], at: usize, id: u32, volume: f32, buffers: u32, dest: u32) {
        wr32(b, at, volume.to_bits());
        wr32(b, at + 8, buffers);
        b[at + 0xC] = 1;
        wr32(b, at + 0x10, id);
        wr32(b, at + 0x924, dest);
    }

    fn set_mix_factor(b: &mut [u8], at: usize, src: usize, dest: usize, f: f32) {
        wr32(b, at + 0x24 + (src * MIX_BUFFERS + dest) * 4, f.to_bits());
    }

    /// A new, started voice at 48 kHz, pitch 1, volume 1, playing one wave buffer of `samples` frames.
    fn set_voice(b: &mut [u8], at: usize, format: u8, channel_ids: &[u32], dest: u32, addr: u64, size: u64, samples: i32) {
        b[at + 0x8] = 1;
        b[at + 0x9] = 1;
        b[at + 0xA] = VOICE_STARTED;
        b[at + 0xB] = format;
        wr32(b, at + 0xC, 48_000);
        wr32(b, at + 0x18, channel_ids.len() as u32);
        wr32(b, at + 0x1C, 1.0f32.to_bits());
        wr32(b, at + 0x20, 1.0f32.to_bits());
        wr32(b, at + 0x58, dest);
        wr64(b, at + 0x60, addr);
        wr64(b, at + 0x68, size);
        wr32(b, at + 0x74, samples as u32);
        for (i, c) in channel_ids.iter().enumerate() {
            wr32(b, at + 0x140 + i * 4, *c);
        }
    }

    fn renderer() -> AudioRendererModel {
        let mut r = AudioRendererModel::new(RendererParams { sample_rate: 48_000, sample_count: 240, mix_buffer_count: 24, voice_count: 4, sink_count: 1 });
        r.started = true;
        r
    }

    fn mapped() -> Memory {
        let mut mmu = Memory::new();
        mmu.map_range(0x5000_0000, 0x4000, Permission::READ_WRITE);
        mmu
    }

    /// Runs `update` then one frame and returns the frame's (left, right) pairs.
    fn render(r: &mut AudioRendererModel, mmu: &Memory, input: &[u8]) -> Vec<(i16, i16)> {
        let mut out = vec![0u8; 0x1000];
        assert_eq!(r.update(input, &mut out), 0);
        r.render_frame(mmu);
        r.drain().chunks_exact(2).map(|p| (p[0], p[1])).collect()
    }

    #[test]
    fn dsp_adpcm_decodes_known_frames() {
        let mut coefs = [0i16; 16];
        coefs[2] = 2048; // pair 1: x + h0
        coefs[4] = 4096; // pair 2: x + 2 h0 - h1
        coefs[5] = -2048;
        // Pair 0, scale 4: nibbles 1, -1, 7, -8 -> 4, -4, 28, -32.
        let data = [0x02, 0x1F, 0x78, 0, 0, 0, 0, 0];
        assert_eq!(decode_dsp_adpcm(&data, &coefs, 0, 4, (0, 0)), [4, -4, 28, -32]);
        // Pair 1, scale 1, nibble 1 from history 100.
        let data = [0x10, 0x11, 0x10, 0, 0, 0, 0, 0];
        assert_eq!(decode_dsp_adpcm(&data, &coefs, 0, 4, (100, 0)), [101, 102, 103, 103]);
        // Pair 2, nibble 0 from history (10, 4): 16, 22, 28, ...
        let data = [0x20, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(decode_dsp_adpcm(&data, &coefs, 0, 3, (10, 4)), [16, 22, 28]);
        // Clamps: pair 1, scale 2^11, nibble 7 from 32000.
        let data = [0x1B, 0x77, 0, 0, 0, 0, 0, 0];
        assert_eq!(decode_dsp_adpcm(&data, &coefs, 0, 2, (32_000, 0)), [32767, 32767]);
        // Sample 14 reads the second frame's header (pair 0, scale 1, nibble 3).
        let mut data = [0u8; 16];
        data[0] = 0x00;
        data[8] = 0x00;
        data[9] = 0x30;
        assert_eq!(decode_dsp_adpcm(&data, &coefs, 13, 15, (0, 0)), [0, 3]);
        // Starting mid-frame with the context history matches decoding from the frame start.
        let data = [0x21, 0x12, 0x3F, 0xE1, 0, 0, 0, 0];
        let full = decode_dsp_adpcm(&data, &coefs, 0, 8, (7, -3));
        assert_eq!(decode_dsp_adpcm(&data, &coefs, 3, 8, (full[2], full[1])), full[3..]);
        // Short data ends the decode rather than reading past it.
        assert_eq!(decode_dsp_adpcm(&data, &coefs, 0, 20, (0, 0)).len(), 14);
    }

    #[test]
    fn adpcm_voice_uses_coefficients_and_context() {
        let mut mmu = mapped();
        let (pcm, params, ctx) = (0x5000_0000u64, 0x5000_1000u64, 0x5000_2000u64);
        // Frame: pair 1 (x + h0), scale 1, nibbles 1, 1, 1, 1, then zeros.
        mmu.write_bytes(pcm, &[0x10, 0x11, 0x11, 0, 0, 0, 0, 0]).unwrap();
        mmu.write_u16(params + 2 * 2, 2048).unwrap();
        // AudioRendererAdpcmContext { index, history0 = 100, history1 = 0 }.
        mmu.write_u16(ctx + 2, 100).unwrap();
        let (mut b, ch, vo, _) = layout(1, 1, 1);
        set_channel(&mut b, ch, &[(0, 1.0), (1, 0.5)]);
        set_voice(&mut b, vo, PCM_ADPCM, &[0], FINAL_MIX_ID, pcm, 8, 6);
        wr64(&mut b, vo + 0x48, params);
        wr64(&mut b, vo + 0x50, 32);
        wr64(&mut b, vo + 0x60 + 0x20, ctx);
        wr64(&mut b, vo + 0x60 + 0x28, 6);
        let mut r = renderer();
        let frame = render(&mut r, &mmu, &b);
        assert_eq!(frame[..7], [(101, 51), (102, 51), (103, 52), (104, 52), (104, 52), (104, 52), (0, 0)]);
        assert!(frame[7..].iter().all(|&p| p == (0, 0)));
        assert_eq!((r.voices[0].played, r.voices[0].consumed), (6, 1));
    }

    #[test]
    fn float_voice_converts_to_pcm16_scale_per_channel() {
        let mut mmu = mapped();
        let pcm = 0x5000_0000u64;
        // Stereo frames: (0.5, -0.25), (1.5, -2.0) (clamped), (-1.0, 0.125).
        for (i, x) in [0.5f32, -0.25, 1.5, -2.0, -1.0, 0.125].iter().enumerate() {
            mmu.write_u32(pcm + i as u64 * 4, x.to_bits()).unwrap();
        }
        let (mut b, ch, vo, _) = layout(2, 1, 1);
        set_channel(&mut b, ch, &[(0, 1.0)]);
        set_channel(&mut b, ch + CHANNEL_IN_SIZE, &[(1, 1.0)]);
        set_voice(&mut b, vo, PCM_FLOAT, &[0, 1], FINAL_MIX_ID, pcm, 24, 3);
        let frame = render(&mut renderer(), &mmu, &b);
        assert_eq!(frame[..4], [(16384, -8192), (32767, -32768), (-32768, 4096), (0, 0)]);
    }

    #[test]
    fn submixes_route_deepest_first_into_the_final_mix() {
        let mut mmu = mapped();
        let (a, c) = (0x5000_0000u64, 0x5000_1000u64);
        for i in 0..4u64 {
            mmu.write_u16(a + i * 2, 1000).unwrap();
            mmu.write_u16(c + i * 2, 2000).unwrap();
        }
        // Mixes: final (entry 0), submix 1 -> final, submix 2 -> submix 1, and 3 <-> 4 (a loop).
        let (mut b, ch, vo, mx) = layout(3, 3, 5);
        let sub1 = mx + MIX_IN_SIZE;
        set_mix(&mut b, sub1, 1, 0.5, 2, FINAL_MIX_ID);
        set_mix_factor(&mut b, sub1, 0, 0, 1.0);
        set_mix_factor(&mut b, sub1, 0, 1, 0.5);
        set_mix_factor(&mut b, sub1, 1, 1, 1.0);
        let sub2 = mx + 2 * MIX_IN_SIZE;
        set_mix(&mut b, sub2, 2, 1.0, 2, 1);
        set_mix_factor(&mut b, sub2, 1, 0, 1.0);
        set_mix(&mut b, mx + 3 * MIX_IN_SIZE, 3, 1.0, 2, 4);
        set_mix(&mut b, mx + 4 * MIX_IN_SIZE, 4, 1.0, 2, 3);
        set_mix_factor(&mut b, mx + 3 * MIX_IN_SIZE, 0, 0, 1.0);
        set_mix_factor(&mut b, mx + 4 * MIX_IN_SIZE, 0, 0, 1.0);
        // Voice 0 (1000) into submix 1 buffer 0; voice 1 (2000) into submix 2 buffer 1; voice 2 into the loop.
        set_channel(&mut b, ch, &[(0, 1.0)]);
        set_channel(&mut b, ch + CHANNEL_IN_SIZE, &[(1, 1.0)]);
        set_channel(&mut b, ch + 2 * CHANNEL_IN_SIZE, &[(0, 1.0)]);
        set_voice(&mut b, vo, PCM_INT16, &[0], 1, a, 8, 4);
        set_voice(&mut b, vo + VOICE_IN_SIZE, PCM_INT16, &[1], 2, c, 8, 4);
        set_voice(&mut b, vo + 2 * VOICE_IN_SIZE, PCM_INT16, &[2], 3, a, 8, 4);
        let mut r = renderer();
        let frame = render(&mut r, &mmu, &b);
        // Submix 1 buffer 0 = 1000 + 2000; left = 3000 * 0.5, right = 3000 * 0.5 * 0.5.
        assert_eq!(frame[..5], [(1500, 750), (1500, 750), (1500, 750), (1500, 750), (0, 0)]);
        assert_eq!(r.mixes.len(), 5);
        assert!(r.voices[2].played == 4, "voices into a looping mix still advance");

        // Without the submix 2 -> 1 hop, voice 1 never reaches the final mix.
        set_mix(&mut b, sub2, 2, 1.0, 2, 9);
        b[vo + 0x8] = 1;
        b[vo + VOICE_IN_SIZE + 0x8] = 1;
        let frame = render(&mut r, &mmu, &b);
        assert_eq!(frame[0], (500, 250));
    }

    #[test]
    fn biquads_filter_each_voice_channel_in_series() {
        let mut mmu = mapped();
        let pcm = 0x5000_0000u64;
        // Impulse, then a step.
        let input = [1024i16, 0, 0, 0, 1000, 1000, 1000];
        for (i, x) in input.iter().enumerate() {
            mmu.write_u16(pcm + i as u64 * 2, *x as u16).unwrap();
        }
        let (mut b, ch, vo, _) = layout(1, 1, 1);
        set_channel(&mut b, ch, &[(0, 1.0), (1, 1.0)]);
        set_voice(&mut b, vo, PCM_INT16, &[0], FINAL_MIX_ID, pcm, 14, 7);
        // Filter 0: y = x + 0.5 y[-1] (b0 = 1, a1 = -0.5). Filter 1: y = 0.5 x + 0.5 x[-1].
        let f0 = vo + 0x24;
        b[f0] = 1;
        wr32(&mut b, f0 + 2, 16384u16 as u32);
        b[f0 + 8..f0 + 10].copy_from_slice(&(-8192i16).to_le_bytes());
        let f1 = vo + 0x30;
        b[f1] = 1;
        b[f1 + 2..f1 + 4].copy_from_slice(&8192i16.to_le_bytes());
        b[f1 + 4..f1 + 6].copy_from_slice(&8192i16.to_le_bytes());
        let mut r = renderer();
        let frame = render(&mut r, &mmu, &b);
        // Filter 0: 1024, 512, 256, 128, 1064, 1532, 1766. Filter 1 averages neighbours. Then the
        // wave buffer has ended.
        let left: Vec<i16> = frame[..8].iter().map(|p| p.0).collect();
        assert_eq!(left, [512, 768, 384, 192, 596, 1298, 1649, 0]);
        assert!(frame.iter().all(|p| p.0 == p.1));
    }
}
