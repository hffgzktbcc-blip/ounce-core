"""Appends a homebrew asset section holding a RomFS image to an NRO.

elf2nro --romfsdir does the same, but on macOS its option parsing stops at the first positional
argument, so the option is silently ignored. Layout (homebrew NRO asset section): "ASET", version 0,
then (offset, size) pairs for icon, NACP and RomFS, offsets relative to the asset header.
"""
import struct
import sys

nro_path, romfs_path = sys.argv[1], sys.argv[2]
nro = open(nro_path, "rb").read()
romfs = open(romfs_path, "rb").read()
assert nro[0x10:0x14] == b"NRO0"
assert len(nro) == struct.unpack_from("<I", nro, 0x18)[0], "NRO already has an asset section"
HEADER = 0x38
asset = b"ASET" + struct.pack("<I", 0) + struct.pack("<QQQQQQ", 0, 0, 0, 0, HEADER, len(romfs))
open(nro_path, "wb").write(nro + asset + romfs)
