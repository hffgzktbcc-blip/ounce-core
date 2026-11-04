# Synthetic Switch 2 harness programs (self-made, never progress)

These two programs were written for ounce-core to exercise the Switch 2 oracle path
(`tests/corpus/switch2/synthetic-oracles.json`, `docs/switch2-corpus.md`): the set header, the intake
check, both engines and the frame hashes. They are **synthetic**. They are not outside homebrew, they
are not Switch 2 programs, and they never count toward the headline metric or PLAN step 8 (Rule 0).

No public Switch 2 homebrew toolchain exists yet, so they are built with the original-Switch
toolchain: devkitA64 r30-1 and libnx 4.12.0-1 (ISC), like `../nro/*`. They use nothing but libnx.

| program | what it draws | NRO build ID (0x40..0x53) | SHA-256 |
|---|---|---|---|
| `rects/rects.nro` | 3 framebuffer frames: a fill, a fixed rectangle, one moving 16 px per frame | `8035251e3172bf80a7e8cce0d4b032b09a0feae2` | `6239ac659e8acb166909026a61538966001efa52d7afb5b264cf9696b40981cf` |
| `console/console.nro` | 3 CPU-console frames: a title line, then one `frame N` line per frame | `6b0ea80d247c69c0ff2781660b2ef18d8a87dc61` | `e949970a9375421e6eb7dfc4110478638df572e45d796482e15e109fdbe19149` |

Rebuild: `make -C tests/fixtures/switch2-synthetic/<program> clean all` (byte-reproducible: two
clean builds gave the hashes above). Test: `cargo test --release switch2_synthetic`.
