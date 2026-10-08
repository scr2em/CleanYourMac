#!/usr/bin/env python3
"""Copy the brand logos the core can name into Sources/CleanYourMacDesignSystem/BrandIcons.

Usage: scripts/update-brand-icons.py PATH/TO/simple-icons/package
       scripts/update-brand-icons.py --themes   (recompute colours of the bundled files)

Logos come from Simple Icons (CC0, https://simpleicons.org); brand names and logos are
trademarks of their owners and identify the ecosystem a finding belongs to. Each file is
copied unchanged except for its colours, set on the root element:
- `fill`: the colour in light appearance, the brand colour itself when it keeps at least
  3:1 contrast on white, otherwise the same hue darkened until it does;
- `data-fill-dark`: the colour in dark appearance, the brand colour lightened until it keeps
  3:1 contrast on the lightest dark surface of every theme;
- black, white and grey marks switch to near-black or near-white instead;
- `data-brand`: the original brand colour, when either of the above differs from it.
Edit `fill` or `data-fill-dark` by hand to override either appearance. To add or replace a
logo by hand, drop a single-colour 24x24 SVG named `<slug>.svg` with a `fill` into the folder.
The slugs come from crates/cym-model/src/brand.rs.
"""
import colorsys
import json
import re
import sys
from pathlib import Path

# The worst cases among the app's themes: the lightest light surface and the lightest dark one.
LIGHT_BACKGROUND = 0xFFFFFF
DARK_BACKGROUND = 0x403533
MINIMUM_CONTRAST = 3.0  # WCAG 2 for graphics.


def luminance(rgb):
    def channel(c):
        c /= 255
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)


def contrast(a, b):
    la, lb = sorted((luminance(a), luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def shifted(rgb, background, step):
    """The colour with its lightness moved by `step` until it contrasts with `background`."""
    r, g, b = ((rgb >> 16) & 255) / 255, ((rgb >> 8) & 255) / 255, (rgb & 255) / 255
    h, l, s = colorsys.rgb_to_hls(r, g, b)
    while contrast(rgb, background) < MINIMUM_CONTRAST and 0 <= l <= 1:
        l = min(1, max(0, l + step))
        r, g, b = colorsys.hls_to_rgb(h, l, s)
        rgb = (round(r * 255) << 16) | (round(g * 255) << 8) | round(b * 255)
        if l in (0, 1):
            break
    return rgb


def themed(svg):
    """Sets the light and dark colours on an SVG whose root `fill` is the brand colour."""
    brand = re.search(r'data-brand="#([0-9A-Fa-f]{6})"', svg) or re.search(r'\sfill="#([0-9A-Fa-f]{6})"', svg)
    colour = int(brand.group(1), 16)
    r, g, b = ((colour >> 16) & 255) / 255, ((colour >> 8) & 255) / 255, (colour & 255) / 255
    if colorsys.rgb_to_hls(r, g, b)[2] < 0.12:
        # Black, white and grey marks swap to the far end instead of a muddy grey.
        light = colour if contrast(colour, LIGHT_BACKGROUND) >= MINIMUM_CONTRAST else 0x1F1F1F
        dark = colour if contrast(colour, DARK_BACKGROUND) >= MINIMUM_CONTRAST else 0xEDEDED
    else:
        light = shifted(colour, LIGHT_BACKGROUND, -0.02)
        dark = shifted(colour, DARK_BACKGROUND, +0.02)
    svg = re.sub(r'\s(fill|data-fill-dark|data-brand)="[^"]*"', "", svg.split(">", 1)[0]) + ">" + svg.split(">", 1)[1]
    extra = f' data-brand="#{colour:06X}"' if colour != light or colour != dark else ""
    return svg.replace("<svg ", f'<svg fill="#{light:06X}" data-fill-dark="#{dark:06X}"{extra} ', 1)

ROOT = Path(__file__).resolve().parent.parent
FOLDER = ROOT / "Sources/CleanYourMacDesignSystem/BrandIcons"


def slugs():
    source = (ROOT / "crates/cym-model/src/brand.rs").read_text()
    found = set(re.findall(r'\(\s*"[^"]*",\s*"([a-z0-9]+)"\s*\)', source))
    found |= set(re.findall(r'Some\("([a-z0-9]+)"\)', source))
    return sorted(found)


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    if sys.argv[1] == "--themes":
        for file in sorted(FOLDER.glob("*.svg")):
            file.write_text(themed(file.read_text().strip()) + "\n")
        print(f"Set light and dark colours on {len(list(FOLDER.glob('*.svg')))} logos")
        return
    package = Path(sys.argv[1])
    colours = {icon["slug"]: icon["hex"] for icon in json.loads((package / "data/simple-icons.json").read_text())}
    FOLDER.mkdir(parents=True, exist_ok=True)
    for slug in slugs():
        if slug not in colours:
            sys.exit(f"Simple Icons has no {slug}")
        svg = (package / "icons" / f"{slug}.svg").read_text().strip()
        svg = re.sub(r"\sfill=\"[^\"]*\"", "", svg, count=1)
        svg = svg.replace("<svg ", f'<svg fill="#{colours[slug]}" ', 1)
        (FOLDER / f"{slug}.svg").write_text(themed(svg) + "\n")
    print(f"Wrote {len(slugs())} logos to {FOLDER.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
