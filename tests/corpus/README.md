# External homebrew candidates

`oracles.json` pre-registers one expected-frame oracle per outside program (20 today); `oracle-results.json` is the latest canonical pass/fail per program, and the root `README.md` headline is checked against it in CI. The oracles run on Linux in CI (the corpus is built in the devkitA64 Docker image). Current counts, failing programs and their causes are tracked in `PLAN.md`, not here.

`sources.json` is the older record of three original-Switch samples (input, C++ hello, deko3d): pinned upstream revisions, observed notices, local build inputs and artifact hashes. The source files were built unchanged. Generated NROs remain in external sibling work directories; this repository holds metadata only. The notice data is recorded as observed; this is not a legal assessment or a conclusion about binary redistribution.

Still outstanding: reproduction of the corpus by an outside developer from a clean checkout, and Switch 2 validation, which waits for authorized tooling and reference-hardware access. These original-Switch programs do not establish Switch 2 compatibility.

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
build reproducibility evidence only; independent reproduction by an outside developer remains outstanding. Generated binaries are kept outside the repository.
