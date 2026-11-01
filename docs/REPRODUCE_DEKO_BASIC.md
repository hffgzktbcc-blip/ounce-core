# Reproduce the deko_basic first-frame oracle

Needs: Linux x86_64 or arm64, Docker, Rust (stable), `git`, `libasound2-dev`, `libudev-dev`. About 10 minutes.

```bash
git clone https://github.com/hffgzktbcc-blip/ounce-core && cd ounce-core
mkdir -p /tmp/corpus
docker run --rm -v "$PWD:/repo:ro" -v /tmp/corpus:/corpus devkitpro/devkita64:latest \
  bash -lc 'python3 /repo/scripts/prepare_external_corpus.py --manifest /repo/tests/corpus/manifest.json --output /corpus/build --build'
sha256sum /tmp/corpus/build/source/graphics/deko3d/deko_basic/deko_basic.nro
OUNCE_DEKO_BASIC_NRO=/tmp/corpus/build/source/graphics/deko3d/deko_basic/deko_basic.nro \
  cargo test --release deko_basic_first_frame_oracle -- --ignored --nocapture
```

Expected:
- Canonical NRO SHA-256 `4f2ea875311acc46e5cef0632b2031091c72787aca64e676d91eaadf6cee5dfa` or `5cec608c6358d5aa5d110c5f5d98674e9ba46685752c497d567e2d286f8766e9` (built with `devkitpro/devkita64:latest` image `sha256:1fc388c3a0d34bd2045a6dadcb1020e069d5f876a187fd705de14b4440c00282`, devkitA64 GCC 15.2.0, binutils 2.45.1; RomFS timestamp variation affects RomFS metadata hash). An alternative local toolchain (e.g. GCC 16.1.0) produces `5d108784619f6830952f7a7342ab5079c678d3d09521659204a83ac45a52b696`. Record yours and the image id from `docker image inspect devkitpro/devkita64:latest --format '{{.Id}}'`.
- Two lines `Interpreter: stop=FrameLimit ... queued=5` and `Tier3: ...`, then `test ... ok`.

Behind a proxy, add `--network host -e HTTPS_PROXY -e GIT_SSL_CAINFO=/ca.crt -v <ca bundle>:/ca.crt:ro` to `docker run`. If the container has no `python3`, install it first (`apt-get install -y python3`).

Post the result on PR #121: the command, NRO SHA-256, image id, and the output lines.
