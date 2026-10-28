# Dependency licence audit (Cargo)

Audit date: **2026-10-28**

Scope: all workspace crates and all resolved third-party dependencies in `Cargo.lock` (including
target-specific crates for Linux/macOS/Windows/Android/WASM).

Command used:

```bash
cargo metadata --format-version 1 --locked
```

Result summary:

- Third-party crates audited: **190**
- Crates with missing SPDX license expression: **0**
- Dominant licence families:
  - `MIT OR Apache-2.0`: 89
  - `MIT`: 31
  - `Apache-2.0 WITH LLVM-exception`: 16
  - `Apache-2.0 OR MIT`: 10
  - `MIT/Apache-2.0`: 9
  - Remaining crates are permissive variants (ISC, BSD, Zlib, Unlicense, or combinations).

Notes:

- `r-efi` declares `MIT OR Apache-2.0 OR LGPL-2.1-or-later`; this is an OR expression, so it can be
  consumed under a permissive option.
- Cranelift/Wasmtime-family crates are `Apache-2.0 WITH LLVM-exception`.
- This audit records declared crate metadata licences; it does not replace legal review for redistribution.
