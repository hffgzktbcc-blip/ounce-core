# Independent homebrew corpus preparation

Status: preparation only, reviewed 2026-10-29 against `fb80e573fc9888da456c1b102e5dc4b829c1ece4`. No external sample has been built or executed by this change. No compatibility milestone is complete.

## Selected trials

All candidates are unmodified programs from independently maintained switchbrew/switch-examples at the exact revision in `tests/corpus/manifest.json`. They are independent of Ounce-Core but share an upstream repository and libnx ecosystem; they are not three independent toolchain implementations.

| Trial | Upstream directory | Observable target | Investigation |
|---|---|---|---|
| Startup baseline | templates/application | Initial console frame, scripted Plus exit | [CPU](independent-corpus-cpu.md) |
| CPU/JIT | jit | Generated function returns 7, normal exit | [CPU](independent-corpus-cpu.md) |
| Filesystem | fs/romfs | Both bundled files read, including Unicode path | [Services](independent-corpus-services.md) |
| Graphics | graphics/simplegfx | Raw framebuffer follows fixed fill sequence | [Graphics](independent-corpus-graphics.md) |

## Preparation and build

Requires Python 3.9+ and Git. Use `python3 scripts/prepare_external_corpus.py --help` for the preparation tool. Supply `--manifest tests/corpus/manifest.json --output /absolute/path/outside/ounce-core`. Default operation fetches the pinned checkout and writes an inventory; `--build` explicitly opts into running the upstream Makefiles and requires a configured devkitPro installation. Work outside the Ounce-Core checkout. No downloaded source or generated NRO is imported into this repository.

The inventory records build inputs and output hashes. It is not an execution result, a complete binary-component audit, or proof that a toolchain is reproducible. Preserve installed package versions and notices alongside it. The upstream README declares Public Domain Mark status; linked libnx/newlib/runtime and default icon components require separate review before binary redistribution.

## Integration order

1. Prepare source, validate the pin and record build inputs/toolchain versions.
2. Build the startup baseline unchanged and record the first actual failure. Source inspection suggests libnx VI/Binder window/console presentation is the first shared blocker; this is an inference, not an observed execution result.
3. Implement the required display/service path with focused regressions. Capture raw guest buffers before telemetry overlays, alpha normalization or host scaling.
4. Add an explicitly engine-selectable, bounded NRO test adapter with scripted input and structured outcomes. The current `--cpu=tier3` CLI option selects Stage 20, not the NRO path.
5. Run the same binaries under interpreter and Tier-3 on Linux. Resolve differences from evidence, preserving upstream sources unchanged.
6. Obtain an outside developer's clean-checkout reproduction before marking the roadmap corpus milestone complete.

See [runner audit](independent-corpus-runner.md) for the adapter design and stop/result requirements. Host process exit zero, a budget stop, a black frame or a successful preparation/build must never be counted as guest compatibility success.

## Scope

These are original Switch homebrew foundation trials. They establish no Switch 2 ABI, service, GPU, commercial-software or performance compatibility. This change follows the clean-room protocol: it adds source references, preparation tooling and plans only; no proprietary material or external implementation source is imported.
