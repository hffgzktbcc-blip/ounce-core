//! Linux user-mode runner (vision step 1): static AArch64 Linux ELF executables on the CPU engines.
//!
//! `ounce-core run-elf <elf> [-- args]`. Programs that need anything outside the small, listed syscall
//! surface stop with a one-line diagnosis naming the syscall number or instruction word. See
//! `docs/verify/linux-elf/README.md` for the supported surface and honest limits.

pub mod elf;
pub mod runner;
pub mod syscalls;

#[cfg(test)]
mod tests;
