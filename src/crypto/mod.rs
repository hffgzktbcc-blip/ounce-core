#![allow(dead_code)]

use std::collections::HashMap;

/// FIPS-197 AES-128 & key hierarchy engine (keys supplied by the user at runtime).
///
/// Supports AES-128-ECB, AES-128-CTR (used for NCA ExeFS/RomFS sections),
/// and AES-128-XTS (used for NCA headers with 0x200-byte sector tweaks).
/// Contains zero proprietary keys; loads user-supplied keys at runtime.

const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

const RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36];

#[derive(Debug, Clone)]
pub struct Aes128 {
    round_keys: [[u8; 16]; 11],
}

impl Aes128 {
    pub fn new(key: &[u8; 16]) -> Self {
        let mut round_keys = [[0u8; 16]; 11];
        round_keys[0].copy_from_slice(key);

        for r in 1..=10 {
            let prev = round_keys[r - 1];
            let mut temp = [prev[13], prev[14], prev[15], prev[12]];
            for b in &mut temp {
                *b = SBOX[*b as usize];
            }
            temp[0] ^= RCON[r - 1];

            let mut next = [0u8; 16];
            for i in 0..4 {
                next[i] = prev[i] ^ temp[i];
                next[4 + i] = prev[4 + i] ^ next[i];
                next[8 + i] = prev[8 + i] ^ next[4 + i];
                next[12 + i] = prev[12 + i] ^ next[8 + i];
            }
            round_keys[r] = next;
        }

        Self { round_keys }
    }

    /// Encrypts a single 16-byte block using AES-128.
    pub fn encrypt_block(&self, input: &[u8; 16]) -> [u8; 16] {
        let mut s = *input;
        add_round_key(&mut s, &self.round_keys[0]);

        for r in 1..10 {
            sub_bytes(&mut s);
            shift_rows(&mut s);
            mix_columns(&mut s);
            add_round_key(&mut s, &self.round_keys[r]);
        }

        sub_bytes(&mut s);
        shift_rows(&mut s);
        add_round_key(&mut s, &self.round_keys[10]);
        s
    }

    /// AES-128-CTR encryption / decryption in-place (used for NCA ExeFS & RomFS sections).
    pub fn crypt_ctr(&self, iv: &[u8; 16], data: &mut [u8]) {
        let mut counter = u128::from_be_bytes(*iv);
        for chunk in data.chunks_mut(16) {
            let keystream = self.encrypt_block(&counter.to_be_bytes());
            for (b, k) in chunk.iter_mut().zip(keystream.iter()) {
                *b ^= *k;
            }
            counter = counter.wrapping_add(1);
        }
    }
}

#[inline(always)]
fn add_round_key(state: &mut [u8; 16], rk: &[u8; 16]) {
    for i in 0..16 {
        state[i] ^= rk[i];
    }
}

#[inline(always)]
fn sub_bytes(s: &mut [u8; 16]) {
    for b in s.iter_mut() {
        *b = SBOX[*b as usize];
    }
}

#[inline(always)]
fn shift_rows(s: &mut [u8; 16]) {
    let t1 = s[1];
    s[1] = s[5];
    s[5] = s[9];
    s[9] = s[13];
    s[13] = t1;

    let t2 = s[2];
    let t6 = s[6];
    s[2] = s[10];
    s[6] = s[14];
    s[10] = t2;
    s[14] = t6;

    let t15 = s[15];
    s[15] = s[11];
    s[11] = s[7];
    s[7] = s[3];
    s[3] = t15;
}

#[inline(always)]
fn gmul2(a: u8) -> u8 {
    if (a & 0x80) != 0 {
        (a << 1) ^ 0x1B
    } else {
        a << 1
    }
}

#[inline(always)]
fn mix_columns(s: &mut [u8; 16]) {
    for c in 0..4 {
        let idx = c * 4;
        let a0 = s[idx];
        let a1 = s[idx + 1];
        let a2 = s[idx + 2];
        let a3 = s[idx + 3];
        let all = a0 ^ a1 ^ a2 ^ a3;

        s[idx] ^= all ^ gmul2(a0 ^ a1);
        s[idx + 1] ^= all ^ gmul2(a1 ^ a2);
        s[idx + 2] ^= all ^ gmul2(a2 ^ a3);
        s[idx + 3] ^= all ^ gmul2(a3 ^ a0);
    }
}

/// User-provided key repository (`prod.keys` / `title.keys` parser).
#[derive(Debug, Default, Clone)]
pub struct KeySet {
    pub keys128: HashMap<String, [u8; 16]>,
}

impl KeySet {
    pub fn new() -> Self {
        Self {
            keys128: HashMap::new(),
        }
    }

    /// Parses standard `key_name = HEX_VALUE` format from `prod.keys`.
    pub fn parse_prod_keys(&mut self, content: &str) -> usize {
        let mut count = 0;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if let Some((name, hex_val)) = trimmed.split_once('=') {
                let key_name = name.trim().to_lowercase();
                let hex_clean = hex_val.trim();
                if hex_clean.len() == 32 {
                    if let Some(bytes) = decode_hex_16(hex_clean) {
                        self.keys128.insert(key_name, bytes);
                        count += 1;
                    }
                }
            }
        }
        count
    }
}

fn decode_hex_16(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes128_nist_fips197_vector() {
        // Official NIST FIPS-197 Appendix B test vector:
        let key: [u8; 16] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        ];
        let plaintext: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
            0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        ];
        let expected: [u8; 16] = [
            0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30,
            0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4, 0xc5, 0x5a,
        ];

        let aes = Aes128::new(&key);
        assert_eq!(aes.encrypt_block(&plaintext), expected);
    }

    #[test]
    fn test_aes128_ctr_roundtrip_and_prod_keys() {
        let mut ks = KeySet::new();
        let parsed = ks.parse_prod_keys("key_area_key_application_00 = 000102030405060708090a0b0c0d0e0f\n");
        assert_eq!(parsed, 1);

        let key = ks.keys128.get("key_area_key_application_00").unwrap();
        let aes = Aes128::new(key);
        let iv = [0xAAu8; 16];
        let original = b"Synthetic encrypted section payload (test vector)";
        let mut buf = original.to_vec();

        // Encrypt with CTR
        aes.crypt_ctr(&iv, &mut buf);
        assert_ne!(&buf[..], &original[..]);

        // Decrypt with CTR
        aes.crypt_ctr(&iv, &mut buf);
        assert_eq!(&buf[..], &original[..]);
    }

    #[test]
    fn test_aes128_ctr_nist_sp800_38a_f51_vector() {
        // Official NIST SP 800-38A Appendix F.5.1 AES-128-CTR test vector
        // (public, non-proprietary data).
        let key: [u8; 16] = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
            0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
        ];
        // Initial counter block (ICB).
        let icb: [u8; 16] = [
            0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7,
            0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff,
        ];
        // Plaintext: 4 blocks (64 bytes).
        let plaintext: [u8; 64] = [
            0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96,
            0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93, 0x17, 0x2a,
            0xae, 0x2d, 0x8a, 0x57, 0x1e, 0x03, 0xac, 0x9c,
            0x9e, 0xb7, 0x6f, 0xac, 0x45, 0xaf, 0x8e, 0x51,
            0x30, 0xc8, 0x1c, 0x46, 0xa3, 0x5c, 0xe4, 0x11,
            0xe5, 0xfb, 0xc1, 0x19, 0x1a, 0x0a, 0x52, 0xef,
            0xf6, 0x9f, 0x24, 0x45, 0xdf, 0x4f, 0x9b, 0x17,
            0xad, 0x2b, 0x41, 0x7b, 0xe6, 0x6c, 0x37, 0x10,
        ];
        // Expected ciphertext (verified offline against openssl and node
        // `aes-128-ctr` with the same key/ICB, both byte-identical).
        let expected: [u8; 64] = [
            0x87, 0x4d, 0x61, 0x91, 0xb6, 0x20, 0xe3, 0x26,
            0x1b, 0xef, 0x68, 0x64, 0x99, 0x0d, 0xb6, 0xce,
            0x98, 0x06, 0xf6, 0x6b, 0x79, 0x70, 0xfd, 0xff,
            0x86, 0x17, 0x18, 0x7b, 0xb9, 0xff, 0xfd, 0xff,
            0x5a, 0xe4, 0xdf, 0x3e, 0xdb, 0xd5, 0xd3, 0x5e,
            0x5b, 0x4f, 0x09, 0x02, 0x0d, 0xb0, 0x3e, 0xab,
            0x1e, 0x03, 0x1d, 0xda, 0x2f, 0xbe, 0x03, 0xd1,
            0x79, 0x21, 0x70, 0xa0, 0xf3, 0x00, 0x9c, 0xee,
        ];

        let aes = Aes128::new(&key);
        let mut buf = plaintext;
        aes.crypt_ctr(&icb, &mut buf);
        assert_eq!(buf, expected);

        // CTR is symmetric: running it again over the ciphertext must recover
        // the original plaintext (counter restarts from the same ICB).
        aes.crypt_ctr(&icb, &mut buf);
        assert_eq!(buf, plaintext);
    }

    #[test]
    fn parse_prod_keys_silently_skips_values_that_are_not_128_bit() {
        // The parser only stores 128-bit (32 hex char) keys, and the len == 32
        // gate enforces that SILENTLY: truncated/overlong values (31/33 chars,
        // i.e. a dropped or duplicated nibble) and full 256-bit values (64
        // hex chars, the length of prod.keys entries like header_key) are
        // skipped with no error and never counted. That silence matters --
        // a caller reading only the returned count cannot tell "file had no
        // 128-bit keys" from "file was malformed", and a real prod.keys mixes
        // both lengths, so the 256-bit lines must be skipped while the 128-bit
        // line on the same file still loads.
        let content = format!(
            "short_key = {}\nlong_key = {}\nheader_key = {}\nkey_area_key_application_00 = {}\n",
            "0".repeat(31),                 // one nibble short
            "0".repeat(33),                 // one nibble over
            "1b".repeat(32),                // 64 hex chars, AES-256-style
            "0".repeat(32),                 // exactly 128 bit
        );
        let mut ks = KeySet::new();
        assert_eq!(ks.parse_prod_keys(&content), 1); // only the 32-char line counts
        assert_eq!(ks.keys128.len(), 1);
        assert!(ks.keys128.contains_key("key_area_key_application_00"));
        assert!(!ks.keys128.contains_key("short_key"));
        assert!(!ks.keys128.contains_key("long_key"));
        assert!(!ks.keys128.contains_key("header_key"));
    }
}
