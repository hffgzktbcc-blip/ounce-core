//! Tier-4 native recompiler for ounce-core (M4, `docs/design/m4-native-jit.md`).
//!
//! Step 1 of the plan: executable-memory handling and a per-host self-test. No guest code is
//! translated yet. `unsafe` is denied crate-wide and allowed only in `exec`.

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

#[allow(unsafe_code)]
mod exec;

pub use exec::ExecError;
use exec::run_return_constant;

/// The host architecture this build generates code for, or `None` if the recompiler cannot run here.
pub fn host_arch() -> Option<&'static str> {
    if cfg!(target_arch = "aarch64") {
        Some("aarch64")
    } else if cfg!(target_arch = "x86_64") {
        Some("x86_64")
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfTestError {
    /// No code generator for this host architecture.
    UnsupportedHost,
    /// Executable memory could not be set up.
    Exec(ExecError),
    /// The generated function ran but returned the wrong value.
    WrongResult { expected: u64, got: u64 },
}

impl std::fmt::Display for SelfTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelfTestError::UnsupportedHost => write!(f, "no native code generator for this host"),
            SelfTestError::Exec(e) => write!(f, "executable memory: {e}"),
            SelfTestError::WrongResult { expected, got } => write!(f, "returned {got}, expected {expected}"),
        }
    }
}

impl std::error::Error for SelfTestError {}

/// Generates, seals and runs a tiny native function that returns 42. Proves that this host can
/// allocate W^X memory, synchronise the instruction cache and call generated code.
pub fn host_selftest() -> Result<u64, SelfTestError> {
    run_return_constant(42, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use exec::ExecBuffer;

    #[test]
    fn selftest_returns_42() {
        assert_eq!(host_selftest(), Ok(42));
    }

    #[test]
    fn independent_buffers_return_their_own_constants() {
        assert_eq!(run_return_constant(0x1234, 0), Ok(0x1234));
        assert_eq!(run_return_constant(7, 0), Ok(7));
    }

    #[test]
    fn code_spanning_several_pages_runs() {
        // 64 KiB of no-ops before the return: crosses 4 KiB and 16 KiB page boundaries.
        assert_eq!(run_return_constant(99, 16 * 1024), Ok(99));
    }

    #[test]
    fn empty_code_is_rejected() {
        assert!(matches!(ExecBuffer::seal(&[]), Err(ExecError::Empty)));
    }

    #[test]
    fn host_is_supported_on_ci_targets() {
        assert!(host_arch().is_some());
    }
}
