//! CLI command implementations, split out of `main.rs` by what they do.
//! `main()` and the global statics stay in `main.rs`; each module here pulls the
//! crate root in with `use crate::*`.

mod demos;
mod run;
mod usage;
mod vm;

pub(crate) use demos::*;
pub(crate) use run::*;
pub(crate) use usage::*;
pub(crate) use vm::*;
