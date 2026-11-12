#!/usr/bin/env python3
"""Write three minimal NRO images for the UX-flow walkthrough: halt.nro, svc.nro and loop.nro.

Each is a 0x1000-byte image with an NRO0 header and a single instruction word at offset 0x80
(0x00000000 undecodable, 0xD4000FE1 `svc #0x7f`, 0x14000000 `b .`). No third-party code.
Usage: make_failing_nros.py <output-dir>
"""
import struct
import sys
from pathlib import Path

CASES = {"halt": 0x00000000, "svc": 0xD4000FE1, "loop": 0x14000000}


def image(word):
    data = bytearray(0x1000)
    data[0:4] = struct.pack("<I", 0x14000020)  # b #0x80, over the header
    data[0x10:0x14] = b"NRO0"
    struct.pack_into("<I", data, 0x18, 0x1000)  # total size
    struct.pack_into("<II", data, 0x20, 0, 0x1000)  # text offset, size
    struct.pack_into("<II", data, 0x28, 0x1000, 0)  # rodata
    struct.pack_into("<II", data, 0x30, 0x1000, 0)  # data
    data[0x80:0x84] = struct.pack("<I", word)
    return bytes(data)


def main(argv):
    if len(argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    out = Path(argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for name, word in CASES.items():
        (out / f"{name}.nro").write_bytes(image(word))
        print(out / f"{name}.nro")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
