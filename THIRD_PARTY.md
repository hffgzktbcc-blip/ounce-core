# Third-party material

The bundled `assets/fonts/DejaVuSans.ttf` is DejaVu Sans 2.37, a Bitstream
Vera derivative. Its complete retained notice is
`assets/fonts/DejaVu-LICENSE.txt`. Distribution archives must include that
notice alongside this register and the project licence. See
`assets/fonts/README.md` for the recorded origin and SHA-256.

`THIRD_PARTY_NOTICES.txt` collects 169 available notice records from the
locked default release dependency graph and exact upstream revisions.
`docs/RELEASE_NOTICE_REVIEW.json` identifies the input hashes and remaining
questions. Release packaging includes these records and checks their
identity. Public release packaging requires a completed review record.
The current record is pending specialist review of objc2 attribution and
SDK-related terms, Rust runtime notices and final binary coverage.

This register records build inputs found in the fixture Makefiles and workspace metadata. It is incomplete: the checked-in NRO binaries have not been exhaustively inspected to identify every linked component.

- **libnx**: fixture Makefiles explicitly link `-lnx`. Upstream project and notices: [switchbrew/libnx](https://github.com/switchbrew/libnx).
- **newlib**: identified in the recorded devkitA64 toolchain metadata. Its notices are per-file; see [newlib](https://sourceware.org/newlib/). Binary-level inclusion has not been independently verified.
- **deko3d and `uam`**: the deko3d fixture Makefile links `-ldeko3d` and invokes `uam` to compile shaders. Exact versions, notices, and generated-output status are not recorded.
- **GCC runtime**: GCC is used by the fixture Makefiles. The exact runtime components in each NRO, including whether libgcc is linked, have not been verified. No libstdc++ linkage is established by the current C fixture Makefiles.
- **Rust crates**: see `Cargo.lock` and `docs/DEPENDENCY_LICENSE_AUDIT.md`. That inventory records declared Cargo licence metadata; it is not a redistribution review.

## Ported or cross-checked source code (MIT)

`src/gpu/sass` uses SASS field positions and small semantic formulas read from the projects below
(details and versions in `docs/PROVENANCE.md`). Each is MIT licensed; the notice applies to the portions taken.

- **Mesa NAK** (`src/nouveau/compiler/nak`): Copyright © 2022–2025 Collabora, Ltd., Valve Corporation and
  other Mesa contributors. SPDX-License-Identifier: MIT. <https://gitlab.freedesktop.org/mesa/mesa>
- **envytools** (`envydis/gm107.c`): Copyright © the envytools contributors. MIT.
  <https://github.com/envytools/envytools>
- **Ryujinx** (`Ryujinx.Graphics.Shader`): Copyright © Ryujinx Team and Contributors. MIT.
- **nxnative** (`dm/tools/patch_nso.py`, `dm/tools/nso_asm.py`, `dm/host/loader.c`): Copyright (c) 2026 seedds. MIT.
  <https://github.com/seedds/nxnative>. The native-execution rewriter in `crates/ounce-nce/src/patch.rs` is
  ported from it.

MIT licence text (as in each project):

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
> documentation files (the "Software"), to deal in the Software without restriction, including without limitation
> the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
> permit persons to whom the Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or substantial portions of
> the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
> THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
> TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

See `docs/PROVENANCE.md` for fixture-specific open checks.
