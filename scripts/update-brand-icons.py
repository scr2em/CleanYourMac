#!/usr/bin/env python3
"""Copy the brand logos the core can name into Sources/CleanYourMacDesignSystem/BrandIcons.

Usage: scripts/update-brand-icons.py PATH/TO/simple-icons/package

Logos come from Simple Icons (CC0, https://simpleicons.org); brand names and logos are
trademarks of their owners and identify the ecosystem a finding belongs to. Each file is
copied unchanged except that the brand colour becomes the root element's `fill`, so the
folder previews correctly and the app reads the colour from the file. To add or replace a
logo by hand, drop a single-colour 24x24 SVG named `<slug>.svg` with a `fill` into the folder.
The slugs come from crates/cym-model/src/brand.rs.
"""
import json
import re
import sys
from pathlib import Path

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
    package = Path(sys.argv[1])
    colours = {icon["slug"]: icon["hex"] for icon in json.loads((package / "data/simple-icons.json").read_text())}
    FOLDER.mkdir(parents=True, exist_ok=True)
    for slug in slugs():
        if slug not in colours:
            sys.exit(f"Simple Icons has no {slug}")
        svg = (package / "icons" / f"{slug}.svg").read_text().strip()
        svg = re.sub(r"\sfill=\"[^\"]*\"", "", svg, count=1)
        svg = svg.replace("<svg ", f'<svg fill="#{colours[slug]}" ', 1)
        (FOLDER / f"{slug}.svg").write_text(svg + "\n")
    print(f"Wrote {len(slugs())} logos to {FOLDER.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
