//! CLI command implementations, split out of `main.rs` by what they do.
//! `main()` and the global statics stay in `main.rs`; each module here pulls the
//! crate root in with `use crate::*`.

#[cfg(feature = "nro")]
mod demos;
#[cfg(feature = "nro")]
mod run;
mod usage;
#[cfg(feature = "nro")]
mod vm;

#[cfg(feature = "nro")]
pub(crate) use demos::*;
#[cfg(feature = "nro")]
pub(crate) use run::*;
pub(crate) use usage::*;
#[cfg(feature = "nro")]
pub(crate) use vm::*;
