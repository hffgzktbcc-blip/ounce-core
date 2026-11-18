#![allow(unused_imports)]
pub mod boot;
pub mod hbx;
mod hbx_frame;
pub mod homebrew;
pub mod init;
pub mod nro;
pub mod package;

pub use boot::{BootLoader, BootPackageInfo};
pub use homebrew::{ConfigEntry, ConfigError, HomebrewConfig};
pub use init::{GuestCallError, InitReport, call_guest_function, run_finalizers, run_initializers};
pub use nro::{HomebrewEnvironment, LoadError, NroHeader, NroLoader, TlsBlock, TlsLayout};
pub use package::{
    ACI0_MAGIC, ACID_MAGIC, LooseBootInfo, LoosePackage, LoosePackageLoader, NPDM_MAGIC, NpdmDescriptor, PackageError,
};

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
pub(crate) mod fuzz_tests;
