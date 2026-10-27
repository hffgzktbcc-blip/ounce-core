//! Host pushbuffer (GPFIFO segment) decoding into individual method writes.
//!
//! Header layout from NVIDIA's public `open-gpu-doc/manuals/turing/tu104/dev_ram.ref.txt`
//! (`NV_FIFO_DMA_*`; Ampere keeps the same format): METHOD_ADDRESS 11:0 (dword address),
//! METHOD_SUBCHANNEL 15:13, METHOD_COUNT 28:16, SEC_OP 31:29. Immediate headers carry their
//! 13-bit data in 28:16.

/// One method write as Host hands it to an engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodCall {
    pub subchannel: u8,
    /// Byte offset of the method, as in the class headers.
    pub method: u32,
    pub data: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushbufError {
    /// A header asked for more data words than the segment holds.
    Truncated { header_index: usize },
    /// SEC_OP 2 or 6 (reserved).
    ReservedOpcode { header_index: usize, sec_op: u32 },
}

const SEC_OP_GRP0_USE_TERT: u32 = 0;
const SEC_OP_INC_METHOD: u32 = 1;
const SEC_OP_NON_INC_METHOD: u32 = 3;
const SEC_OP_IMMD_DATA_METHOD: u32 = 4;
const SEC_OP_ONE_INC: u32 = 5;
const SEC_OP_END_PB_SEGMENT: u32 = 7;

/// Header builders, used by tests and synthetic command streams.
pub fn inc_header(subchannel: u8, method: u32, count: u32) -> u32 {
    header(SEC_OP_INC_METHOD, subchannel, method, count)
}

pub fn non_inc_header(subchannel: u8, method: u32, count: u32) -> u32 {
    header(SEC_OP_NON_INC_METHOD, subchannel, method, count)
}

pub fn one_inc_header(subchannel: u8, method: u32, count: u32) -> u32 {
    header(SEC_OP_ONE_INC, subchannel, method, count)
}

pub fn immd_header(subchannel: u8, method: u32, data: u32) -> u32 {
    header(SEC_OP_IMMD_DATA_METHOD, subchannel, method, data)
}

fn header(sec_op: u32, subchannel: u8, method: u32, count_or_data: u32) -> u32 {
    (sec_op << 29) | ((count_or_data & 0x1fff) << 16) | ((subchannel as u32 & 7) << 13) | ((method >> 2) & 0xfff)
}

/// Decodes one pushbuffer segment. Stops at END_PB_SEGMENT or the end of `words`.
pub fn decode(words: &[u32]) -> Result<Vec<MethodCall>, PushbufError> {
    let mut calls = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let header_index = i;
        let h = words[i];
        i += 1;
        let sec_op = h >> 29;
        let subchannel = ((h >> 13) & 7) as u8;
        let method = (h & 0xfff) << 2;
        let count = ((h >> 16) & 0x1fff) as usize;
        match sec_op {
            SEC_OP_GRP0_USE_TERT => {
                // Sub-device mask controls (TERT_OP 17:16); a single GPU ignores them.
            }
            SEC_OP_IMMD_DATA_METHOD => calls.push(MethodCall { subchannel, method, data: count as u32 }),
            SEC_OP_END_PB_SEGMENT => break,
            SEC_OP_INC_METHOD | SEC_OP_NON_INC_METHOD | SEC_OP_ONE_INC => {
                if i + count > words.len() {
                    return Err(PushbufError::Truncated { header_index });
                }
                for (n, &data) in words[i..i + count].iter().enumerate() {
                    let step = match sec_op {
                        SEC_OP_INC_METHOD => n as u32,
                        SEC_OP_ONE_INC => (n > 0) as u32,
                        _ => 0,
                    };
                    calls.push(MethodCall { subchannel, method: method + step * 4, data });
                }
                i += count;
            }
            _ => return Err(PushbufError::ReservedOpcode { header_index, sec_op }),
        }
    }
    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_each_sec_op() {
        let words = [
            inc_header(0, 0x0d80, 2), 1, 2,
            non_inc_header(1, 0x0d78, 2), 3, 4,
            one_inc_header(2, 0x1b00, 3), 5, 6, 7,
            immd_header(3, 0x1614, 0x1abc),
            0, // GRP0_USE_TERT with TERT_OP 0: ignored
            7 << 29,
            inc_header(0, 0x0100, 1), 9, // after END_PB_SEGMENT: not decoded
        ];
        let calls = decode(&words).unwrap();
        let got: Vec<(u8, u32, u32)> = calls.iter().map(|c| (c.subchannel, c.method, c.data)).collect();
        assert_eq!(
            got,
            vec![
                (0, 0x0d80, 1), (0, 0x0d84, 2),
                (1, 0x0d78, 3), (1, 0x0d78, 4),
                (2, 0x1b00, 5), (2, 0x1b04, 6), (2, 0x1b04, 7),
                (3, 0x1614, 0x1abc),
            ]
        );
    }

    #[test]
    fn rejects_truncated_and_reserved() {
        assert_eq!(decode(&[inc_header(0, 0x800, 3), 1]), Err(PushbufError::Truncated { header_index: 0 }));
        assert_eq!(decode(&[0, 2 << 29]), Err(PushbufError::ReservedOpcode { header_index: 1, sec_op: 2 }));
    }
}
