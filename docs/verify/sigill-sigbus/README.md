# SIGILL, SIGBUS and the exclusive monitor

Evidence for [`fault_signals`](../../../tests/linux-elf/src/fault_signals.c) and the exclusive-monitor rules in
[LIMITS](../../LIMITS.md#known-gaps-in-what-the-linux-elf-runner-and-cpu-models-execute).

**Signal ABI (ground truth: Linux arm64, not macOS).** The guest sees what the kernel delivers
([`arch/arm64/kernel/traps.c`](https://github.com/torvalds/linux/blob/v6.12/arch/arm64/kernel/traps.c), v6.12):
`do_el0_undef` (and `do_el0_sys`, which falls back to it) raises `SIGILL` with `ILL_ILLOPC` and `si_addr` = the faulting
PC; an alignment fault raises `SIGBUS` with `BUS_ADRALN` and `si_addr` = the address
([`arch/arm64/mm/fault.c`](https://github.com/torvalds/linux/blob/v6.12/arch/arm64/mm/fault.c), `do_alignment_fault`).
`ILL_ILLOPN` is only used for pointer-authentication failures (`do_el0_fpac`), so it is not used here. The signal is
forced: with no handler, or with the signal ignored or blocked, the action resets to default and the process dies
(status 128 + signal).

**Apple M3 (`m3probe.c`, macOS 26.6.2).** Instruction-level behaviour only. Output: `m3probe.expected`.
macOS reports `ILL_ILLTRP` (2) where Linux reports `ILL_ILLOPC` (1) and numbers `SIGBUS` 10 instead of 7, so the M3
is not evidence for those two values. What it does show:

- UDF, a reserved word (`0x00010000`), `MSR SCTLR_EL1` and `MRS ID_AA64ISAR0_EL1` from EL0 all raise SIGILL at the
  instruction's own PC.
- Exclusives follow the rule of FEAT_LSE2 (`AArch64.CheckAlignment`): an exclusive that crosses a 16-byte boundary
  raises an alignment fault (`LDXR x` at +12, `LDXR w` at +14, `LDXRH` at +15, `STXR x` at +12, `LDXP x` at +8,
  `LDAXP x` at +1); one that stays inside a 16-byte window does not (`LDXR x` at +1, `LDXP w` at +4). Pre-LSE2 cores and
  ounce-core fault on any misalignment, because ounce-core does not advertise `HWCAP_USCAT`. `fault_signals` therefore
  uses only crossing addresses and also passes on LSE2 hardware.
- Monitor: `LDXR;STXR` succeeds; `CLREX`, an unpaired `STXR` and a second `STXR` fail. An ordinary store to the same
  16-byte granule between them also fails the `STXR`, a store to another cache line does not, and the M3 accepts a
  differing size or address. Those last three are IMPLEMENTATION DEFINED or CONSTRAINED UNPREDICTABLE; ounce-core's
  choices differ from the M3 and are listed in LIMITS.

No CI job runs the committed `tests/linux-elf` fixtures on the `ubuntu-24.04-arm` runner (it runs `cargo test`, whose
Linux fixtures execute on the emulator), so `fault_signals` has not been run on Linux AArch64 hardware. Build it with
`tests/linux-elf/src/build.sh`; run it on hardware to compare.

**Hardware validity of `fault_signals`.** It uses `MRS SCTLR_EL1` for the unexposed-register case because real Linux
(4.11 and later) sets `HWCAP_CPUID` and emulates EL0 reads of `MIDR_EL1`, `MPIDR_EL1`, `REVIDR_EL1` and the ID
registers (`cpufeature.c`, `do_emulate_mrs`), so an ID-register read would not fault there. ounce-core still faults
those reads; a follow-up will emulate them. The `SIGBUS` frame carries `esr_context` (`signal.c`, `setup_sigframe`),
checked for load (`0x92000021`) and store-exclusive (`0x92000061`) faults.
