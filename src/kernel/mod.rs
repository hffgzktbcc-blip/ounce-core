#![allow(unused_imports)]

pub mod account;
pub mod apm;
pub mod applet;
pub mod audio_device;
pub mod audren;
pub mod cmif;
pub mod fsp;
pub mod hid;
pub mod hipc;
pub mod ipc;
#[cfg(test)]
mod ipc_fuzz_tests;
pub mod nvdrv;
pub mod pl;
pub mod scheduler;
pub mod sdk_hle;
pub mod set;
pub mod svc;
#[cfg(test)]
mod svc_fuzz_tests;
#[cfg(test)]
mod svc_tests;
pub mod sync;
pub mod time;
pub mod vi;

pub use ipc::{CMIF_IN_MAGIC, CMIF_OUT_MAGIC, DEFAULT_TLS_ADDR, IpcManager, ServiceType, TLS_SIZE};
pub use scheduler::{KThread, ThreadScheduler, ThreadState, ThreadTlsImage};
pub use sdk_hle::{SdkHleRuntime, StandardAllocator};
pub use svc::{HorizonKernel, SvcResult};
pub use sync::{KEvent, SyncManager};
