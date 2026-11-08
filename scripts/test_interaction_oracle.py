"""Guard the registered Triangle probes against arithmetic transcription errors."""
from fractions import Fraction
import json
from pathlib import Path
import unittest


class TriangleRegistrationTests(unittest.TestCase):
    def test_registered_pixels_follow_pinned_triangle_geometry(self):
        manifest = Path(__file__).resolve().parents[1] / "tests/corpus/interactions.json"
        oracle = next(o for o in json.loads(manifest.read_text())["oracles"]
                      if o["id"] == "deko-menu-triangle")
        for check in oracle["checks"]:
            self.assertEqual(check["expect"], {"width": 1280, "height": 720})
            for probe in check["probes"]:
                # Pinned Example02_Triangle.cpp: red at (640,0), green at
                # (0,720), blue at (1280,720), sampled at pixel centers.
                vertical = Fraction(2 * probe["y"] + 1, 1440)
                red = 1 - vertical
                blue = (vertical + Fraction(2 * probe["x"] + 1 - 1280, 1280)) / 2
                green = 1 - red - blue
                rgb = [round(255 * c) for c in (red, green, blue)]
                if min(red, green, blue) < 0:
                    rgb = [0, 0, 0]  # black clear outside the primitive
                self.assertEqual(probe["rgb"], rgb, (probe["x"], probe["y"]))


if __name__ == "__main__":
    unittest.main()
