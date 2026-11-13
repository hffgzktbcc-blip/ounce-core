# Report This Problem: prefilled form

Built with `scripts/build_macos_app.sh`, opened a static AArch64 Linux program whose first instruction word was
overwritten with `0xD4A00000` (a copy of `tests/linux-elf/bin/args.elf`, 1,808 bytes), clicked **Report This Problem** on the
"Could not run this program" result with Orca computer use on an Apple M3 Mac, and screenshotted the form. Nothing was submitted.

`form-from-app-before-merge.png` shows the form **before this branch's template is on `main`**: GitHub renders the template
from the default branch, so it still has the old one-line *Program and version* input (newlines are dropped) and the old
Engine labels (Engine shows "None"). The address itself carried every field: `engine=Tier-3 and Tier-4 (both)`, the program
name, size and SHA-256, the expected-result text, `Unimplemented instruction 0xD4A00000 at PC: 0x210138`, both
`run-elf ... --cpu=tier3|tier4 --max-insns=2000000000` lines, and `Ounce Core 0.7.0 (commit 5fd0416aca), macOS 26.6.2,
Apple M3, via the Mac app`. After the template merges, Engine preselects and *Program and version* keeps its lines.
The template's wording was not seen rendered on GitHub for the same reason.
