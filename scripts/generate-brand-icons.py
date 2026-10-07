#!/usr/bin/env python3
"""Generate Sources/CleanYourMacDesignSystem/BrandIcons.swift from the Simple Icons package.

Usage: scripts/generate-brand-icons.py PATH/TO/simple-icons/package

Simple Icons (https://simpleicons.org) is CC0. Each icon's SVG path is rewritten as absolute
move, line, cubic, quadratic and close commands in its 24x24 box, so the app needs only a tiny
parser and no SVG renderer. The slugs come from crates/cym-model/src/brand.rs.
"""
import json
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "Sources/CleanYourMacDesignSystem/BrandIcons.swift"
NUMBER = re.compile(r"[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?")
ARITY = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "S": 4, "Q": 4, "T": 2, "A": 7, "Z": 0}


def slugs():
    source = (ROOT / "crates/cym-model/src/brand.rs").read_text()
    found = set(re.findall(r'\(\s*"[^"]*",\s*"([a-z0-9]+)"\s*\)', source))
    found |= set(re.findall(r'Some\("([a-z0-9]+)"\)', source))
    return sorted(found)


def tokens(d):
    """Commands with their numeric arguments; arc flags may be written without separators."""
    i, n = 0, len(d)
    command = None
    while i < n:
        c = d[i]
        if c.isalpha():
            command = c
            i += 1
            if c in "Zz":
                yield c, []
            continue
        if c in " ,\t\r\n":
            i += 1
            continue
        args = []
        for k in range(ARITY[command.upper()]):
            while i < n and d[i] in " ,\t\r\n":
                i += 1
            if command in "Aa" and k in (3, 4):
                args.append(float(d[i]))
                i += 1
                continue
            match = NUMBER.match(d, i)
            if not match:
                raise ValueError(f"bad path near {d[i:i + 20]!r}")
            args.append(float(match.group()))
            i = match.end()
        yield command, args
        # Extra coordinate pairs after a move are lines.
        if command == "M":
            command = "L"
        elif command == "m":
            command = "l"


def arc_to_cubics(x1, y1, rx, ry, angle, large, sweep, x2, y2):
    """Endpoint arc to cubic Bézier segments (SVG implementation notes, F.6)."""
    if (x1, y1) == (x2, y2):
        return []
    if rx == 0 or ry == 0:
        return [(x1, y1, x2, y2, x2, y2)]
    rx, ry = abs(rx), abs(ry)
    phi = math.radians(angle % 360)
    cos, sin = math.cos(phi), math.sin(phi)
    dx, dy = (x1 - x2) / 2, (y1 - y2) / 2
    x1p, y1p = cos * dx + sin * dy, -sin * dx + cos * dy
    scale = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry)
    if scale > 1:
        rx, ry = rx * math.sqrt(scale), ry * math.sqrt(scale)
    num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p
    den = rx * rx * y1p * y1p + ry * ry * x1p * x1p
    factor = math.sqrt(max(0, num / den)) if den else 0
    if large == sweep:
        factor = -factor
    cxp, cyp = factor * rx * y1p / ry, -factor * ry * x1p / rx
    cx = cos * cxp - sin * cyp + (x1 + x2) / 2
    cy = sin * cxp + cos * cyp + (y1 + y2) / 2

    def angle_of(ux, uy, vx, vy):
        a = math.atan2(ux * vy - uy * vx, ux * vx + uy * vy)
        return a

    theta = angle_of(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry)
    delta = angle_of((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry)
    if not sweep and delta > 0:
        delta -= 2 * math.pi
    elif sweep and delta < 0:
        delta += 2 * math.pi
    segments = max(1, math.ceil(abs(delta) / (math.pi / 2) - 1e-9))
    step = delta / segments
    k = 4 / 3 * math.tan(step / 4)
    out = []

    def point(t):
        return (cx + rx * math.cos(t) * cos - ry * math.sin(t) * sin,
                cy + rx * math.cos(t) * sin + ry * math.sin(t) * cos)

    def derivative(t):
        return (-rx * math.sin(t) * cos - ry * math.cos(t) * sin,
                -rx * math.sin(t) * sin + ry * math.cos(t) * cos)

    t = theta
    for _ in range(segments):
        p0, p3 = point(t), point(t + step)
        d0, d3 = derivative(t), derivative(t + step)
        out.append((p0[0] + k * d0[0], p0[1] + k * d0[1],
                    p3[0] - k * d3[0], p3[1] - k * d3[1], p3[0], p3[1]))
        t += step
    # Land exactly on the endpoint.
    last = out[-1]
    out[-1] = (last[0], last[1], last[2], last[3], x2, y2)
    return out


def normalize(d):
    out = []
    x = y = sx = sy = 0.0
    last_control = None  # (kind, x, y) for S/T reflection
    for command, a in tokens(d):
        relative = command.islower()
        c = command.upper()
        ox, oy = (x, y) if relative else (0.0, 0.0)
        control = None
        if c == "M":
            x, y = a[0] + ox, a[1] + oy
            sx, sy = x, y
            out.append(("M", x, y))
        elif c == "L":
            x, y = a[0] + ox, a[1] + oy
            out.append(("L", x, y))
        elif c == "H":
            x = a[0] + (x if relative else 0)
            out.append(("L", x, y))
        elif c == "V":
            y = a[0] + (y if relative else 0)
            out.append(("L", x, y))
        elif c == "C":
            x1, y1, x2, y2 = a[0] + ox, a[1] + oy, a[2] + ox, a[3] + oy
            x, y = a[4] + ox, a[5] + oy
            out.append(("C", x1, y1, x2, y2, x, y))
            control = ("C", x2, y2)
        elif c == "S":
            if last_control and last_control[0] == "C":
                x1, y1 = 2 * x - last_control[1], 2 * y - last_control[2]
            else:
                x1, y1 = x, y
            x2, y2 = a[0] + ox, a[1] + oy
            x, y = a[2] + ox, a[3] + oy
            out.append(("C", x1, y1, x2, y2, x, y))
            control = ("C", x2, y2)
        elif c == "Q":
            x1, y1 = a[0] + ox, a[1] + oy
            x, y = a[2] + ox, a[3] + oy
            out.append(("Q", x1, y1, x, y))
            control = ("Q", x1, y1)
        elif c == "T":
            if last_control and last_control[0] == "Q":
                x1, y1 = 2 * x - last_control[1], 2 * y - last_control[2]
            else:
                x1, y1 = x, y
            x, y = a[0] + ox, a[1] + oy
            out.append(("Q", x1, y1, x, y))
            control = ("Q", x1, y1)
        elif c == "A":
            nx, ny = a[5] + ox, a[6] + oy
            for segment in arc_to_cubics(x, y, a[0], a[1], a[2], int(a[3]), int(a[4]), nx, ny):
                out.append(("C",) + segment)
            x, y = nx, ny
        elif c == "Z":
            out.append(("Z",))
            x, y = sx, sy
        last_control = control
    return out


def number(value):
    text = f"{value:.3f}".rstrip("0").rstrip(".")
    return "0" if text in ("-0", "") else text


def encode(commands):
    return " ".join(c[0] + " ".join(number(v) for v in c[1:]) for c in commands)


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    package = Path(sys.argv[1])
    data = json.loads((package / "data/simple-icons.json").read_text())
    icons = {icon["slug"]: icon for icon in data}
    entries = []
    for slug in slugs():
        icon = icons.get(slug)
        if not icon:
            sys.exit(f"Simple Icons has no {slug}")
        svg = (package / "icons" / f"{slug}.svg").read_text()
        d = re.search(r'<path d="([^"]+)"', svg).group(1)
        entries.append((slug, icon["title"], icon["hex"], encode(normalize(d))))
    lines = [
        "// Generated by scripts/generate-brand-icons.py from Simple Icons (CC0, https://simpleicons.org).",
        "// Brand names and logos are trademarks of their owners; they identify the ecosystem a finding belongs to.",
        "// Do not edit by hand.",
        "",
        "extension BrandCatalog {",
        "    /// Slug, brand name, brand colour and path in a 24×24 box (absolute M, L, C, Q, Z).",
        "    static let icons: [String: (name: String, hex: UInt32, path: String)] = [",
    ]
    for slug, title, hex, path in entries:
        lines.append(f'        "{slug}": ({json.dumps(title)}, 0x{hex}, "{path}"),')
    lines += ["    ]", "}", ""]
    OUTPUT.write_text("\n".join(lines))
    print(f"Wrote {len(entries)} icons to {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
