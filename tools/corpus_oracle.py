#!/usr/bin/env python3
"""Pre-register first-frame oracles: compute expected frame hashes from each program's source.

Every expected value in `tests/corpus/oracles.json` is written by this tool from a spec that a
person derived by reading the program's source. No ounce-core run is consulted. The tool never
reads a frame produced by ounce-core.

Console programs: libnx 4.12.0's console (`nx/source/runtime/devices/console.c`, ISC) is
modelled here: escape parsing, cursor movement, clearing, and both renderers that the corpus
uses, the CPU renderer (`console_sw.c`, RGB565) and the deko3d GPU renderer of
switch-examples `graphics/deko3d/deko_console` (`gpu_console.c`, RGBA8 palette). Glyphs come
from libnx's `nx/data/default_font.bin`, read at generation time from a libnx checkout and
never committed. Pass it with `--font`; its SHA-256 is recorded next to the hashes.

  python3 tools/corpus_oracle.py --font <libnx>/nx/data/default_font.bin           # write
  python3 tools/corpus_oracle.py --font <libnx>/nx/data/default_font.bin --check   # verify
"""
import argparse
import hashlib
import json
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ORACLES = REPO / "tests/corpus/oracles.json"

WIDTH, HEIGHT = 80, 45  # libnx default console, 16x16 tiles -> 1280x720
BOLD, FAINT, ITALIC, UNDERLINE = 1 << 0, 1 << 1, 1 << 2, 1 << 3
BLINK_SLOW, BLINK_FAST, REVERSE, CONCEAL = 1 << 4, 1 << 5, 1 << 6, 1 << 7
CROSSED_OUT, FG_CUSTOM, BG_CUSTOM = 1 << 8, 1 << 9, 1 << 10
FG_BRIGHT, BG_BRIGHT = 1 << 11, 1 << 12


def rgb565(r, g, b):
    return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)


SW_PALETTE = [rgb565(*c) for c in (
    (0, 0, 0), (128, 0, 0), (0, 128, 0), (128, 128, 0), (0, 0, 128), (128, 0, 128), (0, 128, 128), (192, 192, 192),
    (128, 128, 128), (255, 0, 0), (0, 255, 0), (255, 255, 0), (0, 0, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255),
    (0, 0, 0), (64, 0, 0), (0, 64, 0), (64, 64, 0), (0, 0, 64), (64, 0, 64), (0, 64, 64), (96, 96, 96),
)]

# gpu_console.c g_paletteData, as unorm8 (round to nearest; 0.5 -> 128). Index 0 and 16 have alpha 0
# and fail the renderer's alpha test (Greater than 0), so those pixels keep the clear colour.
_DK = (
    (0, 0, 0, 0), (.5, 0, 0, 1), (0, .5, 0, 1), (.5, .5, 0, 1), (0, 0, .5, 1), (.5, 0, .5, 1), (0, .5, .5, 1), (.75, .75, .75, 1),
    (.5, .5, .5, 1), (1, 0, 0, 1), (0, 1, 0, 1), (1, 1, 0, 1), (0, 0, 1, 1), (1, 0, 1, 1), (0, 1, 1, 1), (1, 1, 1, 1),
    (0, 0, 0, 0), (.25, 0, 0, 1), (0, .25, 0, 1), (.25, .25, 0, 1), (0, 0, .25, 1), (.25, 0, .25, 1), (0, .25, .25, 1), (.375, .375, .375, 1),
)
DK_PALETTE = [tuple(int(v * 255 + 0.5) for v in c) for c in _DK]


class Console:
    """libnx 4.12.0 console state machine. Records each drawChar into a cell grid."""

    def __init__(self):
        self.cells = {}
        self.cx = self.cy = 1
        self.px = self.py = 1
        self.fg, self.bg, self.flags = 7, 0, 0
        self.state = "none"
        self.cls(2)  # consoleInit

    # console.c consoleDrawChar / renderer->drawChar
    def draw(self, c):
        if c < 0 or c > 256:
            return
        self.cells[(self.cx - 1, self.cy - 1)] = (c, self.fg, self.bg, self.flags)

    def new_row(self):
        self.cy += 1
        if self.cy > HEIGHT:
            # scrollWindow then clear the last line
            moved = {}
            for (x, y), cell in self.cells.items():
                if y >= 1:
                    moved[(x, y - 1)] = cell
            self.cells = moved
            self.cy = HEIGHT
            self.clear_line(2)

    def put(self, c):
        if c == 0:
            return
        if c == 8:  # \b
            self.cx -= 1
            if self.cx < 1:
                if self.cy > 1:
                    self.cx, self.cy = WIDTH, self.cy - 1
                else:
                    self.cx = 1
            self.draw(32)
        elif c == 9:  # \t, tab size 3
            spaces = 3 - ((self.cx - 1) % 3)
            if self.cx + spaces > WIDTH:
                spaces = WIDTH - self.cx
            for _ in range(spaces):
                self.put(32)
        elif c == 10:
            self.new_row()
            self.cx = 1
        elif c == 13:
            self.cx = 1
        else:
            if self.cx > WIDTH:
                self.cx = 1
                self.new_row()
            self.draw(c)
            self.cx += 1

    def cls(self, mode):
        if mode == 0:
            col, row = self.cx, self.cy
            for _ in range(WIDTH * HEIGHT - (row * WIDTH + col)):
                self.put(32)
            self.cx, self.cy = col, row
        elif mode == 1:
            col, row = self.cx, self.cy
            self.cx = self.cy = 1
            for _ in range(row * WIDTH + col):
                self.put(32)
            self.cx, self.cy = col, row
        elif mode == 2:
            self.cx = self.cy = 1
            for _ in range(WIDTH * HEIGHT):
                self.put(32)
            self.cx = self.cy = 1

    def clear_line(self, mode):
        col = self.cx
        if mode == 0:
            for _ in range(WIDTH - col + 1):
                self.put(32)
        elif mode == 1:
            self.cx = 1
            for _ in range(col - 1):
                self.put(32)
        elif mode == 2:
            self.cx = 1
            for _ in range(WIDTH):
                self.put(32)
        self.cx = col

    def position(self, x, y):
        if x < 0 or y < 0:
            return
        self.cx = min(max(x, 1), WIDTH)
        self.cy = min(max(y, 1), HEIGHT)

    def color(self, code):
        f = self.eflags
        if code == 0:
            f, self.ebg, self.efg = 0, 0, 7
        elif code == 1:
            f = (f & ~FAINT) | BOLD
        elif code == 2:
            f = (f & ~BOLD) | FAINT
        elif code == 3:
            f |= ITALIC
        elif code == 4:
            f |= UNDERLINE
        elif code == 5:
            f = (f & ~BLINK_FAST) | BLINK_SLOW
        elif code == 6:
            f = (f & ~BLINK_SLOW) | BLINK_FAST
        elif code == 7:
            f |= REVERSE
        elif code == 8:
            f |= CONCEAL
        elif code == 9:
            f |= CROSSED_OUT
        elif code == 21:
            f &= ~BOLD
        elif code == 22:
            f &= ~(BOLD | FAINT)
        elif code == 23:
            f &= ~ITALIC
        elif code == 24:
            f &= ~UNDERLINE
        elif code == 25:
            f &= ~(BLINK_SLOW | BLINK_FAST)
        elif code == 27:
            f &= ~REVERSE
        elif code == 29:
            f &= ~CROSSED_OUT
        elif 30 <= code <= 37:
            f &= ~FG_CUSTOM
            self.efg = code - 30
        elif code == 39:
            f &= ~FG_CUSTOM
            self.efg = 7
        elif 40 <= code <= 47:
            f &= ~BG_CUSTOM
            self.ebg = code - 40
        elif code == 49:
            f &= ~BG_CUSTOM
            self.ebg = 0
        elif 90 <= code <= 97:
            f = (f & ~(FAINT | BG_CUSTOM)) | FG_BRIGHT
            self.efg = code - 90
        elif 100 <= code <= 107:
            f = (f & ~(FAINT | BG_CUSTOM)) | BG_BRIGHT
            self.ebg = code - 100
        elif code in (38, 48):
            raise ValueError("custom 38/48 colours are not modelled")
        self.eflags = f

    # console.c con_write
    def write(self, data):
        for chr_ in data:
            if self.state == "none":
                if chr_ == 0x1b:
                    self.state = "start"
                else:
                    self.put(chr_)
            elif self.state == "start":
                if chr_ == ord("["):
                    self.state = "build"
                    self.has_arg = False
                    self.args = [0] * 16
                    self.efg, self.ebg, self.eflags = self.fg, self.bg, self.flags
                    self.idx = 0
                else:
                    self.put(0x1b)
                    self.put(chr_)
                    self.state = "none"
            else:
                c = chr(chr_)
                a = self.args
                if c.isdigit():
                    self.has_arg = True
                    a[self.idx] = a[self.idx] * 10 + int(c)
                    continue
                if c == ";":
                    if self.has_arg and self.idx < 16:
                        self.idx += 1
                    self.has_arg = False
                    continue
                default_one = not self.has_arg and not self.idx
                if c == "A":
                    self.cy = max(self.cy - (1 if default_one else a[0]), 1)
                elif c == "B":
                    self.cy = min(self.cy + (1 if default_one else a[0]), HEIGHT)
                elif c == "C":
                    self.cx = min(self.cx + (1 if default_one else a[0]), WIDTH)
                elif c == "D":
                    self.cx = max(self.cx - (1 if default_one else a[0]), 1)
                elif c in "Hf":
                    self.position(a[1], a[0])
                elif c == "J":
                    self.cls(0 if default_one else a[0])
                elif c == "K":
                    self.clear_line(0 if default_one else a[0])
                elif c == "s":
                    self.px, self.py = self.cx, self.cy
                elif c == "u":
                    self.cx, self.cy = self.px, self.py
                elif c == "m":
                    if self.idx == 0 and not self.has_arg:
                        a[0] = 0
                        self.idx = 1
                    elif self.has_arg:
                        self.idx += 1
                    self.efg, self.ebg, self.eflags = self.fg, self.bg, self.flags
                    for arg in range(self.idx):
                        self.color(a[arg])
                    self.fg, self.bg, self.flags = self.efg, self.ebg, self.eflags
                self.state = "none"


def glyph_rows(font, c):
    return [int.from_bytes(font[32 * c + 2 * r: 32 * c + 2 * r + 2], "little") for r in range(16)]


def render_console(cells, font, renderer):
    """Return a 1280x720 RGBA8 image as ounce-core's `frame_from_surface` delivers it."""
    w, h = WIDTH * 16, HEIGHT * 16
    image = bytearray(b"\x00\x00\x00\xff" * (w * h))
    for (x, y), (c, fg, bg, flags) in cells.items():
        rows = glyph_rows(font, c)
        if renderer == "sw":
            fgc = SW_PALETTE[fg + 8] if flags & BOLD else SW_PALETTE[fg + 16] if flags & FAINT else SW_PALETTE[fg]
            bgc = SW_PALETTE[bg]
            if flags & REVERSE:
                fgc, bgc = bgc, fgc
            if flags & UNDERLINE:
                rows[15] |= 0xffff
            if flags & CROSSED_OUT:
                rows[7] |= 0xffff
            fgp, bgp = expand565(fgc), expand565(bgc)
        else:  # deko_console GpuRenderer_drawChar: no underline/strike, palette indices
            wi = fg + 8 if flags & BOLD else fg + 16 if flags & FAINT else fg
            si = bg
            if flags & REVERSE:
                wi, si = si, wi
            fgp, bgp = deko_pixel(wi), deko_pixel(si)
        for r in range(16):
            bits = rows[r]
            base = ((y * 16 + r) * w + x * 16) * 4
            for i in range(16):
                image[base + 4 * i: base + 4 * i + 4] = fgp if bits & (0x8000 >> i) else bgp
    return w, h, image


def expand565(v):
    r, g, b = v >> 11 & 0x1f, v >> 5 & 0x3f, v & 0x1f
    return bytes(((r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2), 0xff))


def deko_pixel(index):
    r, g, b, a = DK_PALETTE[index]
    return bytes((0, 0, 0, 0xff)) if a == 0 else bytes((r, g, b, 0xff))


def render_rects(spec):
    w, h = spec["width"], spec["height"]
    image = bytearray(bytes(spec["fill"] + [0xff]) * (w * h))
    for x, y, rw, rh, *rgb in spec["rects"]:
        px = bytes(rgb + [0xff])
        for row in range(max(y, 0), min(y + rh, h)):
            start, end = max(x, 0), min(x + rw, w)
            image[(row * w + start) * 4:(row * w + end) * 4] = px * (end - start)
    return w, h, image


def fnv1a64(data):
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    return value


def expected(spec, font):
    kind = spec["kind"]
    if kind in ("console_sw", "console_deko"):
        console = Console()
        console.write(spec["stream"].encode("latin-1"))
        w, h, image = render_console(console.cells, font, "sw" if kind == "console_sw" else "deko")
    elif kind == "rects":
        w, h, image = render_rects(spec)
    else:
        raise ValueError(f"unknown spec kind {kind}")
    return {"width": w, "height": h, "rgba_fnv1a64": f"{fnv1a64(image):#018x}"}


def console_probes(spec, font):
    """Sample foreground and background pixels in selected stable console cells."""
    console = Console()
    console.write(spec["stream"].encode("latin-1"))
    width, _, image = render_console(console.cells, font, "sw")
    probes = []
    for col, row in spec["probe_cells"]:
        char, *_ = console.cells[col, row]
        rows = glyph_rows(font, char)
        for foreground in (True, False):
            points = [(x, y) for y, bits in enumerate(rows) for x in range(16)
                      if bool(bits & (0x8000 >> x)) == foreground]
            if not points:
                raise ValueError(f"probe cell {(col, row)} has no requested glyph pixels")
            x, y = points[len(points) // 2]
            x, y = 16 * col + x, 16 * row + y
            offset = (y * width + x) * 4
            probes.append({"x": x, "y": y, "rgb": list(image[offset:offset + 3])})
    return probes


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--font", type=Path, required=True, help="libnx 4.12.0 nx/data/default_font.bin")
    parser.add_argument("--check", action="store_true", help="fail if any committed value differs")
    args = parser.parse_args()
    font = args.font.read_bytes()
    if len(font) != 256 * 32:
        raise SystemExit("font must be 256 16x16 1bpp glyphs (8192 bytes)")
    oracles = json.loads(ORACLES.read_text())
    font_sha = hashlib.sha256(font).hexdigest()
    changed = []
    for oracle in oracles["oracles"]:
        for check in oracle["checks"]:
            if "spec" not in check:
                continue
            spec = check["spec"]
            if "probe_cells" in spec:
                if spec["kind"] != "console_sw":
                    raise ValueError("probe cells require a software console spec")
                probes = console_probes(spec, font)
                if check.get("probes") != probes:
                    changed.append(oracle["id"])
                    check["probes"] = probes
                value = {"width": WIDTH * 16, "height": HEIGHT * 16}
            else:
                value = expected(spec, font)
            if check["spec"]["kind"].startswith("console"):
                value["font_sha256"] = font_sha
            if check.get("expect") != value:
                changed.append(oracle["id"])
                check["expect"] = value
    if args.check:
        if changed:
            raise SystemExit(f"oracle values differ from the specs: {sorted(set(changed))}")
        print("all oracle values match their specs")
        return
    ORACLES.write_text(json.dumps(oracles, indent=2, ensure_ascii=False) + "\n")
    print(f"updated: {sorted(set(changed)) or 'nothing'}")


if __name__ == "__main__":
    main()
