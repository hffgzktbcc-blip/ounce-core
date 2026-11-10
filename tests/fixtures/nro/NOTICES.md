# Notices for the legacy NRO fixtures

Applies to the eleven `*.nro` test inputs under `tests/fixtures/nro/`. They are test inputs only and are not
in the runtime release archives. This file retains notices; it is not a licence review and does not mark any
fixture `VERIFIED` in `tests/fixtures/provenance.json`.

## Sources

Each folder's `main.c` (with its Makefile and, for `deko3d`, two GLSL shaders) is a short program written for
this project. All fixture files were committed by the project's own account, and none of the sources carries a
third-party copyright or licence notice. Authorship of the text cannot be proven from the repository alone, so
the register still says `UNVERIFIED`. The build inputs and toolchain are in `TOOLCHAIN_VERSIONS.md` and
`tests/fixtures/rebuild-evidence.json`.

## Components linked into the binaries

- **libnx** (ISC), https://github.com/switchbrew/libnx. Full notice below.
- **newlib** (per-file BSD-style and similar notices), https://sourceware.org/newlib/. Distributed with devkitA64.
- **libgcc and libstdc++ runtime pieces** from devkitA64's GCC (GPL-3.0 with the GCC Runtime Library Exception).
- `deko3d` shaders are compiled to DKSH with devkitPro's `uam`. The compiler's licence is not carried into its output.

## libnx notice

Copyright 2017-2018 libnx Authors

Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
