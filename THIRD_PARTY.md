# Third-party material

This register records build inputs found in the fixture Makefiles and workspace metadata. It is incomplete: the checked-in NRO binaries have not been exhaustively inspected to identify every linked component.

- **libnx**: fixture Makefiles explicitly link `-lnx`. Upstream project and notices: [switchbrew/libnx](https://github.com/switchbrew/libnx).
- **newlib**: identified in the recorded devkitA64 toolchain metadata. Its notices are per-file; see [newlib](https://sourceware.org/newlib/). Binary-level inclusion has not been independently verified.
- **deko3d and `uam`**: the deko3d fixture Makefile links `-ldeko3d` and invokes `uam` to compile shaders. Exact versions, notices, and generated-output status are not recorded.
- **GCC runtime**: GCC is used by the fixture Makefiles. The exact runtime components in each NRO, including whether libgcc is linked, have not been verified. No libstdc++ linkage is established by the current C fixture Makefiles.
- **Rust crates**: see `Cargo.lock` and `docs/DEPENDENCY_LICENSE_AUDIT.md`. That inventory records declared Cargo licence metadata; it is not a redistribution review.

See `docs/PROVENANCE.md` for fixture-specific open checks.
