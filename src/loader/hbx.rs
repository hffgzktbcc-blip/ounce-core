//! Split AArch64 images from the public HBLauncher native app SDK, ABI 3.
#![allow(dead_code)]

use crate::mmu::{Memory, MemoryError, Permission};
use serde_json::Value;
use sha2::{Digest, Sha256};

const PAGE: usize = 4096;
const MAX_SEGMENT: usize = 64 * 1024 * 1024;
const ADDRESS_LIMIT: u64 = 1 << 48;

#[derive(Debug, thiserror::Error)]
pub enum HbxLoadError {
    #[error("invalid HBLauncher report: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid HBLauncher image field: {0}")]
    Invalid(&'static str),
    #[error("HBLauncher image overlaps existing guest memory")]
    Mapped,
    #[error("failed to write HBLauncher image: {0}")]
    Memory(#[from] MemoryError),
}

pub struct HbxImage<'a> {
    pub code_base: u64,
    pub data_base: u64,
    pub entry: u64,
    code: &'a [u8],
    data: &'a [u8],
    data_region: usize,
}

impl<'a> HbxImage<'a> {
    pub fn parse(report: &[u8], code: &'a [u8], data: &'a [u8]) -> Result<Self, HbxLoadError> {
        use HbxLoadError::Invalid;
        if report.len() > PAGE
            || code.is_empty()
            || code.len() > MAX_SEGMENT
            || data.len() > MAX_SEGMENT
        {
            return Err(Invalid("image size"));
        }
        let report: Value = serde_json::from_slice(report)?;
        if report["format"].as_str() != Some("hblauncher-native-app/1")
            || report["kind"].as_str() != Some("native")
            || report["abi"].as_u64() != Some(3)
        {
            return Err(Invalid("format/kind/ABI"));
        }
        let number = |key: &'static str| report[key].as_u64().ok_or(Invalid(key));
        let address = |key: &'static str| {
            report[key]
                .as_str()
                .and_then(|value| value.strip_prefix("0x"))
                .and_then(|hex| u64::from_str_radix(hex, 16).ok())
                .ok_or(Invalid(key))
        };
        let code_base = address("codeBase")?;
        let data_base = address("dataBase")?;
        let entry = address("entry")?;
        let data_region = number("dataRegionBytes")?;
        if data_region > MAX_SEGMENT as u64
            || data_region < data.len() as u64
            || number("bssBytes")? > data_region - data.len() as u64
        {
            return Err(Invalid("data region"));
        }
        for (size, digest, bytes) in [
            ("codeBytes", "codeSha256", code),
            ("dataBytes", "dataSha256", data),
        ] {
            if number(size)? != bytes.len() as u64 {
                return Err(Invalid(size));
            }
            if report[digest].as_str() != Some(format!("{:x}", Sha256::digest(bytes)).as_str()) {
                return Err(Invalid(digest));
            }
        }
        let code_size = code.len().next_multiple_of(PAGE);
        let data_region = data_region as usize;
        let data_size = data_region.next_multiple_of(PAGE);
        let end = |base: u64, size: usize| {
            if base == 0 || !base.is_multiple_of(PAGE as u64) {
                return Err(Invalid("segment alignment"));
            }
            base.checked_add(size as u64)
                .filter(|&end| end <= ADDRESS_LIMIT)
                .ok_or(Invalid("segment address"))
        };
        let code_end = end(code_base, code_size)?;
        let data_end = end(data_base, data_size)?;
        if data_size != 0 && code_base < data_end && data_base < code_end {
            return Err(Invalid("overlapping segments"));
        }
        if entry % 4 != 0 || entry < code_base || entry >= code_base + code.len() as u64 {
            return Err(Invalid("entry"));
        }
        Ok(Self {
            code_base,
            data_base,
            entry,
            code,
            data,
            data_region,
        })
    }

    pub fn map(&self, memory: &mut Memory) -> Result<(), HbxLoadError> {
        let segments = [
            (
                self.code_base,
                self.code.len(),
                self.code,
                Permission::READ_WRITE_EXECUTE,
            ),
            (
                self.data_base,
                self.data_region,
                self.data,
                Permission::READ_WRITE,
            ),
        ];
        for (base, size, _, _) in segments {
            if (0..size)
                .step_by(PAGE)
                .any(|offset| memory.is_mapped(base + offset as u64))
            {
                return Err(HbxLoadError::Mapped);
            }
        }
        for (base, size, bytes, permission) in segments {
            memory.map_range(base, size, permission);
            memory.write_bytes(base, bytes)?;
        }
        Ok(())
    }
}
