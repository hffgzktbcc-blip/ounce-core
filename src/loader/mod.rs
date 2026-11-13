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
pub use init::{call_guest_function, run_finalizers, run_initializers, GuestCallError, InitReport};
pub use nro::{HomebrewEnvironment, LoadError, NroHeader, NroLoader, TlsBlock, TlsLayout};
pub use package::{
    LooseBootInfo, LoosePackage, LoosePackageLoader, NpdmDescriptor, PackageError,
    ACI0_MAGIC, ACID_MAGIC, NPDM_MAGIC,
};

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
pub(crate) mod fuzz_tests;
