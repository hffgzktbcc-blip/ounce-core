# External homebrew candidates

`sources.json` records the pinned upstream revisions, observed notices, local build inputs and artifact hashes for three original-Switch samples. The source files were built unchanged. Generated NROs remain in external sibling work directories; this repository change adds metadata only.

The candidates have not passed the corpus gate. The input sample currently stops during guest startup with result `0x2559`; the C++ hello sample loads but stops at `svcBreak` before its expected console text is observed. The deko3d sample has fixed draw commands but no built-in pixel checksum or guest completion result, and this manifest records no Ounce-Core pass for it. The declared notice data is recorded as observed; this is not a legal assessment or a conclusion about binary redistribution.

A complete milestone still requires clean runs under Interpreter and Tier-3 on Linux, reproducible outputs, and reproduction by an outside developer from a clean checkout. Direct Switch 2 validation remains a later step pending authorized tooling and reference-hardware access. These original-Switch candidates do not establish Switch 2 compatibility.

## Rebuild from pinned source

With the recorded devkitPro tools installed, run from the repository root:

```sh
python3 tests/corpus/build.py /path/to/a/new/external-build-directory
```

The directory must be new. The script fetches the exact source commits, builds unchanged
upstream files, checks that tracked sources stayed unchanged, and writes build logs plus
`build-report.json` in that directory. It compares the resulting NRO SHA-256 values with
`sources.json` and fails if any differ. `--devkitpro` selects another toolchain location.

Compiler paths are normalized with `-ffile-prefix-map`: without it, checkout paths changed
the GNU build IDs even when execution bytes were identical. With normalization, all three
NRO hashes matched across two separate fresh build directories on macOS arm64. This is
build reproducibility evidence only. Linux execution, runtime outputs, and independent
reproduction remain outstanding. Generated binaries are kept outside the repository.
