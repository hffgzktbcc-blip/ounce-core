//! Native code execution (NCE, PLAN.md step 4).
//!
//! A homebrew image runs directly on an AArch64 host CPU. Its `svc`, `TPIDR_EL0` / `TPIDRRO_EL0`,
//! timer and `x18` instructions are rewritten into trampolines first ([`patch`]); everything else is
//! the guest's own code, executed unchanged. The technique and the rewrite rules are ported from
//! nxnative (MIT, seedds, `dm/tools/patch_nso.py`); see `THIRD_PARTY.md` and `docs/PROVENANCE.md`.
//!
//! - [`patch`] (safe): finds the instructions to rewrite and builds the patch area.
//! - `arena` (unsafe): the host address range guest memory lives in, at guest address == host address.
//! - `exec` (unsafe): entering guest code, the trampoline exit, and fault signals.
//!
//! `unsafe` is denied crate-wide and allowed only in `arena` and `exec`. On hosts other than
//! AArch64 macOS / Linux both modules are stubs whose constructors return [`NceError::Unsupported`].

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod context;
pub mod patch;

#[cfg_attr(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")), allow(unsafe_code))]
mod arena;
#[cfg_attr(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")), allow(unsafe_code))]
mod exec;

pub use arena::Arena;
pub use context::Context;
pub use exec::{enter, set_preemption, Exit, MAX_NO_PREEMPT};

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NceError {
    /// Native execution needs an AArch64 macOS or Linux host.
    Unsupported,
    /// An OS call failed. Holds the call name and the OS error code.
    Os(&'static str, i32),
    /// An address range outside the arena or not committed.
    Range { va: u64, len: u64 },
    /// The image holds an instruction the rewriter cannot handle.
    Patch(String),
}

impl fmt::Display for NceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NceError::Unsupported => write!(f, "native execution needs an AArch64 macOS or Linux host"),
            NceError::Os(call, code) => write!(f, "{call} failed (OS error {code})"),
            NceError::Range { va, len } => write!(f, "range {va:#x}+{len:#x} is outside the committed arena"),
            NceError::Patch(why) => write!(f, "rewrite failed: {why}"),
        }
    }
}

impl std::error::Error for NceError {}

/// True on hosts where [`Arena::reserve`] and [`enter`] can work.
pub const SUPPORTED: bool = cfg!(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")));
