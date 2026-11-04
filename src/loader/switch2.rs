#![allow(dead_code)]

//! Oracle sets and the program-intake check for the first-frame corpus, including the Switch 2
//! homebrew set (PLAN step 8, `docs/switch2-corpus.md`).
//!
//! An oracle set is one `oracles.json` file. Its header names the platform its programs were built
//! for and their origin: `outside` (a real program we did not write, the only kind that counts) or
//! `synthetic` (our own harness programs, which never count). Every program is checked against its
//! entry before it runs: the platform must match the set, and the container named by `format` must
//! be the one the file holds. Only `nro` is accepted today. No public Switch 2 homebrew toolchain
//! exists yet, so its output format is unknown; when one appears, its container gets a variant here
//! and nothing else in the oracle flow changes. Retail containers (NCA/NSP/XCI) are never accepted.

use serde_json::Value;

/// The platform a corpus program was built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Original Switch homebrew (devkitA64 + libnx), the step 2 corpus.
    Switch,
    /// Switch 2 homebrew built with public, open-source tools (PLAN step 8).
    Switch2,
}

impl Platform {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "switch" => Ok(Self::Switch),
            "switch2" => Ok(Self::Switch2),
            other => Err(format!("unknown platform {other:?} (expected \"switch\" or \"switch2\")")),
        }
    }

    pub fn name(self) -> &'static str {
        match self { Self::Switch => "switch", Self::Switch2 => "switch2" }
    }

    /// The Ounce-Verify `target` its records carry (`tools/ounce_verify.py`).
    pub fn verify_target(self) -> &'static str {
        match self { Self::Switch => "original-switch-libnx", Self::Switch2 => "switch2-homebrew" }
    }
}

/// Where a set's programs come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Real programs we did not write. Only these move the headline metric.
    Outside,
    /// Our own harness programs. They test the oracle path and never count as progress.
    Synthetic,
}

impl Origin {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "outside" => Ok(Self::Outside),
            "synthetic" => Ok(Self::Synthetic),
            other => Err(format!("unknown origin {other:?} (expected \"outside\" or \"synthetic\")")),
        }
    }
}

/// A program container the corpus runner can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramFormat {
    /// Homebrew `NRO0` executable, started through libnx's `crt0` with a homebrew `ConfigEntry` list.
    Nro,
}

impl ProgramFormat {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "nro" => Ok(Self::Nro),
            other => Err(format!("program format {other:?} is not supported yet (only \"nro\")")),
        }
    }

    /// The container `bytes` hold, if the runner knows it.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        (bytes.get(0x10..0x14) == Some(b"NRO0")).then_some(Self::Nro)
    }
}

/// The header of one oracle set and its entries.
#[derive(Debug, Clone)]
pub struct OracleSet {
    pub platform: Platform,
    pub origin: Origin,
    pub oracles: Vec<Value>,
}

impl OracleSet {
    /// Parses a set. The step 2 file predates the header fields, so it defaults to outside Switch programs.
    pub fn parse(json: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if value["schema_version"] != 1 {
            return Err("oracle set must have schema_version 1".into());
        }
        let platform = Platform::parse(value["platform"].as_str().unwrap_or("switch"))?;
        let origin = Origin::parse(value["origin"].as_str().unwrap_or("outside"))?;
        let oracles = value["oracles"].as_array().ok_or("oracle set has no oracles array")?.clone();
        Ok(Self { platform, origin, oracles })
    }

    pub fn find(&self, id: &str) -> Option<&Value> {
        self.oracles.iter().find(|o| o["id"] == id)
    }

    /// Checks one entry and its program bytes before a run; returns the container to start.
    pub fn check_program(&self, oracle: &Value, bytes: &[u8]) -> Result<ProgramFormat, String> {
        let id = oracle["id"].as_str().unwrap_or("?");
        if let Some(platform) = oracle["platform"].as_str() {
            if Platform::parse(platform)? != self.platform {
                return Err(format!("{id}: platform {platform:?} differs from its set ({})", self.platform.name()));
            }
        }
        let declared = ProgramFormat::parse(oracle["format"].as_str().unwrap_or("nro"))?;
        match ProgramFormat::detect(bytes) {
            Some(found) if found == declared => Ok(found),
            Some(found) => Err(format!("{id}: declared {declared:?}, file holds {found:?}")),
            None => Err(format!("{id}: file is not a {declared:?} container")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nro_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; 0x80];
        bytes[0x10..0x14].copy_from_slice(b"NRO0");
        bytes
    }

    #[test]
    fn step2_set_defaults_to_outside_switch_programs() {
        let set = OracleSet::parse(include_str!("../../tests/corpus/oracles.json")).unwrap();
        assert_eq!((set.platform, set.origin), (Platform::Switch, Origin::Outside));
        assert_eq!(set.oracles.len(), 20);
    }

    #[test]
    fn switch2_sets_are_labelled_honestly() {
        let outside = OracleSet::parse(include_str!("../../tests/corpus/switch2/oracles.json")).unwrap();
        assert_eq!((outside.platform, outside.origin), (Platform::Switch2, Origin::Outside));
        let synthetic = OracleSet::parse(include_str!("../../tests/corpus/switch2/synthetic-oracles.json")).unwrap();
        assert_eq!((synthetic.platform, synthetic.origin), (Platform::Switch2, Origin::Synthetic));
        assert!(!synthetic.oracles.is_empty());
        for oracle in &synthetic.oracles {
            assert!(outside.find(oracle["id"].as_str().unwrap()).is_none(), "synthetic id in the outside set");
        }
        assert_eq!(outside.platform.verify_target(), "switch2-homebrew");
    }

    #[test]
    fn program_check_rejects_wrong_platform_and_container() {
        let set = OracleSet { platform: Platform::Switch2, origin: Origin::Outside, oracles: vec![] };
        let entry = serde_json::json!({"id": "a", "platform": "switch2", "format": "nro"});
        assert_eq!(set.check_program(&entry, &nro_bytes()), Ok(ProgramFormat::Nro));
        let wrong_platform = serde_json::json!({"id": "a", "platform": "switch", "format": "nro"});
        assert!(set.check_program(&wrong_platform, &nro_bytes()).unwrap_err().contains("differs from its set"));
        let mut nso = vec![0u8; 0x80];
        nso[..4].copy_from_slice(b"NSO0");
        assert!(set.check_program(&entry, &nso).unwrap_err().contains("not a Nro container"));
        let unknown = serde_json::json!({"id": "a", "format": "nca"});
        assert!(set.check_program(&unknown, &nro_bytes()).unwrap_err().contains("not supported"));
        assert!(OracleSet::parse(r#"{"schema_version":1,"platform":"switch3","oracles":[]}"#).is_err());
        assert!(OracleSet::parse(r#"{"schema_version":1,"origin":"maybe","oracles":[]}"#).is_err());
    }
}
