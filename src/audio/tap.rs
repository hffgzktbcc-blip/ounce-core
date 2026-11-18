//! A running count and hash of the PCM a guest mixed, for the `audio_frames_mixed` / `audio_hash` fields of
//! `ounce.run.v1` (#478).
//!
//! The tap sees every sample as it enters an output ring buffer, so the result does not depend on when or whether the
//! host drained the buffer: a headless run (no sound device) and a run that plays the audio report the same values.

/// FNV-1a 64 offset basis and prime, the same hash `framebuffer_hash` uses.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Interleaved 16-bit samples per frame. Both audio paths (audout and audren) output 48 kHz stereo.
const CHANNELS: u64 = 2;

/// Counts and hashes the samples of one output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioTap {
    samples: u64,
    state: u64,
}

impl Default for AudioTap {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioTap {
    pub const fn new() -> Self {
        Self { samples: 0, state: FNV_OFFSET }
    }

    /// Records one interleaved sample: its two bytes, little-endian, in stream order.
    pub fn push(&mut self, sample: i16) {
        for byte in sample.to_le_bytes() {
            self.state = (self.state ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
        }
        self.samples += 1;
    }

    /// Stereo frames recorded so far (a trailing unpaired sample does not count).
    pub fn frames(&self) -> u64 {
        self.samples / CHANNELS
    }

    /// FNV-1a 64 over every recorded sample, or `None` when nothing was recorded.
    fn digest(&self) -> Option<u64> {
        (self.samples > 0).then_some(self.state)
    }
}

/// What a run reports about its audio.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioSummary {
    pub frames_mixed: u64,
    /// `fnv1a64:<16 hex digits>`; `None` when the guest mixed no audio.
    pub hash: Option<String>,
}

/// One summary for the two audio paths. One path alone reports its own digest. When both mixed audio, the digest is
/// FNV-1a 64 over the audren digest then the audout digest (8 little-endian bytes each), so the value does not depend
/// on how the two streams interleaved in time.
pub fn combined_summary(audren: &AudioTap, audout: &AudioTap) -> AudioSummary {
    let digest = match (audren.digest(), audout.digest()) {
        (None, None) => None,
        (Some(only), None) | (None, Some(only)) => Some(only),
        (Some(a), Some(b)) => {
            let mut state = FNV_OFFSET;
            for byte in a.to_le_bytes().into_iter().chain(b.to_le_bytes()) {
                state = (state ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
            }
            Some(state)
        }
    };
    AudioSummary {
        frames_mixed: audren.frames() + audout.frames(),
        hash: digest.map(|d| format!("fnv1a64:{d:016x}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference FNV-1a 64 over bytes (the published algorithm), independent of `AudioTap`.
    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
    }

    fn tap_of(samples: &[i16]) -> AudioTap {
        let mut tap = AudioTap::new();
        samples.iter().for_each(|&s| tap.push(s));
        tap
    }

    #[test]
    fn the_hash_is_fnv1a_over_the_little_endian_sample_bytes() {
        let samples = [0i16, 1, -1, 8000, -8000, i16::MIN, i16::MAX];
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(tap_of(&samples).digest(), Some(fnv1a64(&bytes)));
        // Published FNV-1a 64 test vectors, so the reference above is the real algorithm.
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn frames_are_stereo_pairs_and_an_empty_tap_has_no_hash() {
        assert_eq!(tap_of(&[]).frames(), 0);
        assert_eq!(combined_summary(&tap_of(&[]), &tap_of(&[])), AudioSummary::default());
        assert_eq!(tap_of(&[1, 2, 3, 4, 5]).frames(), 2);
    }

    #[test]
    fn one_path_reports_its_own_digest_whichever_path_it_is() {
        let tap = tap_of(&[10, -10, 20, -20]);
        let expected = format!("fnv1a64:{:016x}", tap.digest().unwrap());
        for summary in [combined_summary(&tap, &AudioTap::new()), combined_summary(&AudioTap::new(), &tap)] {
            assert_eq!((summary.frames_mixed, summary.hash.as_deref()), (2, Some(expected.as_str())));
        }
    }

    #[test]
    fn both_paths_combine_in_a_fixed_order_that_ignores_interleaving() {
        let (renderer, out) = (tap_of(&[1, 1, 2, 2]), tap_of(&[3, 3]));
        let both = combined_summary(&renderer, &out);
        assert_eq!(both.frames_mixed, 3);
        let mut bytes = renderer.digest().unwrap().to_le_bytes().to_vec();
        bytes.extend(out.digest().unwrap().to_le_bytes());
        assert_eq!(both.hash, Some(format!("fnv1a64:{:016x}", fnv1a64(&bytes))));
        assert_ne!(both.hash, combined_summary(&out, &renderer).hash, "which path is which matters");
    }

    #[test]
    fn different_audio_gives_a_different_hash() {
        assert_ne!(tap_of(&[0, 0, 100, 100]).digest(), tap_of(&[0, 0, 100, 101]).digest());
        assert_ne!(tap_of(&[0, 0]).digest(), tap_of(&[0, 0, 0, 0]).digest(), "length is part of the stream");
    }
}
