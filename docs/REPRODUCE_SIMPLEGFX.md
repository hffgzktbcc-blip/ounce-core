# Reproduce the simplegfx first-frame oracle

Needs: Linux x86_64 or arm64, Docker, Rust (stable), `git`, `libasound2-dev`, `libudev-dev`. About 10 minutes.

```bash
git clone https://github.com/hffgzktbcc-blip/ounce-core && cd ounce-core
mkdir -p /tmp/corpus
docker run --rm -v "$PWD:/repo:ro" -v /tmp/corpus:/corpus devkitpro/devkita64:latest \
  bash -lc 'python3 /repo/scripts/prepare_external_corpus.py --manifest /repo/tests/corpus/manifest.json --output /corpus/build --build'
sha256sum /tmp/corpus/build/source/graphics/simplegfx/simplegfx.nro
OUNCE_SIMPLEGFX_NRO=/tmp/corpus/build/source/graphics/simplegfx/simplegfx.nro \
  cargo test --release simplegfx_first_frame_oracle -- --ignored --nocapture
```

Expected:
- NRO SHA-256 `b59fc85f0079bcd93007d97150e2af8302ac0804d1120cee43cf32fd37b69bf1`. A different devkitA64 image can legitimately give a different hash; record yours and the image id from `docker image inspect devkitpro/devkita64:latest --format '{{.Id}}'`.
- Two lines `Interpreter: stop=FrameLimit ... queued=61` and `Tier3: ...`, then `test ... ok`.

Behind a proxy, add `--network host -e HTTPS_PROXY -e GIT_SSL_CAINFO=/ca.crt -v <ca bundle>:/ca.crt:ro` to `docker run`. If the container has no `python3`, install it first (`apt-get install -y python3`).

Post the result on PR #103: the command, NRO SHA-256, image id, and the last 3 output lines.
