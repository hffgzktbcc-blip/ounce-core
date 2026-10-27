# audren.nro: libnx audio renderer fixture

Homebrew-only test program built with devkitA64 and libnx, like `../hello`. It plays a 2400-sample
(50 ms) tone through libnx's audio driver:

1. `audrenInitialize` (48 kHz, 4 voices, 1 sink, 1 mix, 2 mix buffers) and `audrvCreate`.
2. A memory pool holding the tone (mono PCM16, a square wave of +/-8000 with a 48-sample period),
   attached; a stereo device sink on mix buffers 0 and 1; `audrvUpdate`, `audrenStartAudioRenderer`.
3. Voice 0 (1 channel, PCM16, 48 kHz) to the final mix with mix factor 1.0 on both channels, one wave
   buffer, started.
4. `audrvUpdate` + `audrenWaitFrame` until the wave buffer is done (each wait renders one 5 ms frame).

It prints `ounce-audren played=0x00000960` (2400 samples) and `ounce-audren result=0x00000EEE`, else
`0xBAD0000 | bits`. `libnx_audren_fixture_plays_a_tone` also checks the mixed 48 kHz stereo output holds
exactly the tone on both channels, followed by silence.

To listen: `cargo run --release -- run tests/fixtures/nro/audren/audren.nro long` loops the tone for
2 seconds at 20% volume; the runner plays libnx audio renderer output on the host as it is mixed, paced by the
device's playback clock.

Rebuild: `make -C tests/fixtures/nro/audren clean all` (byte-reproducible).
