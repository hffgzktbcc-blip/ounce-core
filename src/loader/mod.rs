#![allow(unused_imports)]
pub mod homebrew;
pub mod init;
pub mod nca;
pub mod nro;
pub mod package;

pub use nca::{
    BootLoader, BootPackageInfo, Hfs0Archive, NcaImage, Pfs0Archive, Pfs0FileEntry, XciCartridge,
    HFS0_MAGIC, NCA3_MAGIC, PFS0_MAGIC, XCI_HEAD_MAGIC,
};
pub use homebrew::{ConfigEntry, HomebrewConfig};
pub use init::{call_guest_function, run_finalizers, run_initializers, GuestCallError, InitReport};
pub use nro::{HomebrewEnvironment, LoadError, NroHeader, NroLoader, TlsBlock, TlsLayout};
pub use package::{
    LooseBootInfo, LoosePackage, LoosePackageLoader, NpdmDescriptor, PackageError,
    ACI0_MAGIC, ACID_MAGIC, NPDM_MAGIC,
};

#[cfg(test)]
mod fixture_tests;
