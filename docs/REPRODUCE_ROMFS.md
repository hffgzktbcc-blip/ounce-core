# Reproduce the romfs first-frame oracle

Needs: Linux x86_64 or arm64, Docker, Rust (stable), `git`, `libasound2-dev`, `libudev-dev`. About 10 minutes.

```bash
git clone https://github.com/hffgzktbcc-blip/ounce-core && cd ounce-core
mkdir -p /tmp/corpus
docker run --rm -v "$PWD:/repo:ro" -v /tmp/corpus:/corpus devkitpro/devkita64:latest \
  bash -lc 'python3 /repo/scripts/prepare_external_corpus.py --manifest /repo/tests/corpus/manifest.json --output /corpus/build --build'
sha256sum /tmp/corpus/build/source/fs/romfs/romfs.nro
OUNCE_ROMFS_NRO=/tmp/corpus/build/source/fs/romfs/romfs.nro \
  cargo test --release romfs_first_frame_oracle -- --ignored --nocapture
```

Expected:
- Canonical NRO SHA-256 (built with `devkitpro/devkita64:latest` image, devkitA64 GCC 15.2.0, binutils 2.45.1; RomFS timestamp variation affects RomFS metadata hash). Record yours and the image id from `docker image inspect devkitpro/devkita64:latest --format '{{.Id}}'`.
- Two lines `Interpreter: stop=FrameLimit ... queued=5` and `Tier3: ...`, then `test ... ok`.

Behind a proxy, add `--network host -e HTTPS_PROXY -e GIT_SSL_CAINFO=/ca.crt -v <ca bundle>:/ca.crt:ro` to `docker run`. If the container has no `python3`, install it first (`apt-get install -y python3`).
