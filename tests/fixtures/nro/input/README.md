# input.nro: libnx controller input fixture

Homebrew-only test program built with devkitA64 and libnx, like `../hello`.

1. `smInitialize`, `hidInitialize` (maps the HID shared memory).
2. `padConfigureInput(1, HidNpadStyleSet_NpadStandard)`, `padInitializeDefault` (player 1 + handheld).
3. Four `padUpdate` calls with a short `svcSleepThread` between them, like a main loop.
4. Prints `ounce-input buttons=0x........` (`padGetButtons`) and `ounce-input stick_l=0xXXXXYYYY`
   (left stick x in the high half, y in the low half), then `ounce-input result=0x00000EEE` if the pad
   read as connected, else `0xBAD0000 | bits` after an `rc=` line.

The kernel writes host input into the npad LIFOs of the shared memory the guest mapped
(`src/kernel/hid.rs`). `libnx_input_fixture_reads_host_controller` holds A + D-pad right and a stick
position and checks the fixture reads them back. Under `ounce-core run` with no `--gui` there is no host
input, so it prints `buttons=0x00000000`; with `--gui` the window's keyboard and any gamepad feed it.

Rebuild: `make -C tests/fixtures/nro/input clean all` (byte-reproducible).
