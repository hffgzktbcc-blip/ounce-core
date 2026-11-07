# RomFS linked-input review packet

Status: technical inventory for a specialist reviewer, **not legal clearance**.
The reproduction milestone and linked-component review remain open. This packet
ships text metadata only, with no executable, object, archive or source fixture.

The internal rebuild uses the exact source and image pins in [romfs.json](romfs.json).
Its NRO matches reference SHA-256
`d22f38b633df556c00ffecf1f435d96e63ccd69352d1f2f4e10cf316de590fb9`.
The [machine-readable inventory](romfs-linked-inputs.json) records archive hashes,
member names, startup-object hashes, package owners and ELF/map identities.
[Selected raw linker-map lines](romfs-link-map-selection.txt) and
[package ownership/metadata](romfs-linked-packages.txt) support those observations.

## Observed link inputs

| Input selected by GNU ld | Package in pinned image | Selected archive members |
| --- | --- | ---: |
| `libnx.a` | `libnx 4.12.0-1` | 51 |
| `libg.a` | `devkita64-newlib 4.6.0.20260123-4` | 81 |
| `libsysbase.a` | `devkita64-newlib 4.6.0.20260123-4` | 19 |
| `libgcc.a` | `devkita64-gcc 15.2.0-7` | 2 |

The map also loads GCC `crti.o`, `crtbegin.o`, `crtend.o` and `crtn.o`, plus
the sample's `main.o`. `libc.a` appears as a loaded archive without a member
selected under that pathname in the archive-selection table. The recorded
`libc.a` and `libg.a` hashes are identical. These are linker observations;
discarded sections and conversion from ELF to NRO still need consideration.
No `libstdc++`, deko3d, SDL2 or Mesa member is selected in this map. This finding
applies only to this RomFS rebuild, not to other corpus samples or old fixtures.

## Review still needed

1. Confirm the sample's upstream Public Domain Mark notice and the scope of its
   embedded RomFS data. The source pin is in the manifest; it is not a licence
   determination for all resulting bytes.
2. Review the selected libnx objects and embedded font/data against their source
   notices. The [v4.12.0 libnx notice](https://github.com/switchbrew/libnx/blob/v4.12.0/LICENSE.md)
   grants ISC-style permissions subject to notice retention; a project-level
   notice alone does not establish every asset's provenance.
3. Identify the exact devkitPro newlib/libsysbase source revision and patches for
   the installed package. Map the selected objects to their per-file notices.
   Newlib's [notice register](https://sourceware.org/newlib/COPYING.NEWLIB)
   contains multiple terms. Package metadata reports no licence value here;
   that is missing metadata, not permission to omit review.
4. Review the two selected libgcc objects and four GCC startup objects, including
   whether the [GCC Runtime Library Exception](https://www.gnu.org/licenses/gcc-exception-3.1.html)
   applies to each installed input and this compilation process. The GCC
   package's GPL metadata does not settle the terms for each runtime object.
5. Decide which notices, attributions and other conditions a distributed binary
   would require. Record reviewer identity, scope, source evidence and decision
   before describing the experiment as legally reviewed or distributing binaries.

## Recheck the inventory

Follow the published recipe to obtain a new build, preserving its actual hash
even if it differs. The generated map is
`$REPRO_OUT/builds/original/build/source/fs/romfs/build/romfs.map`;
the ELF sits alongside `romfs.nro`. Inspect the initial archive-member selection
table and every `LOAD` entry. Within the pinned container, use `sha256sum` and
`dkp-pacman -Qo` on each named archive/startup object, then `dkp-pacman -Qi`
on the owning packages. Compare these with the retained JSON and raw metadata.
No compiler options, source, registered pixels or corpus denominator were changed
to produce this packet.
