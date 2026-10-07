#!/usr/bin/env python3
"""Render local, explicitly labelled design concepts from shared screen data."""
from pathlib import Path
from html import escape
import json
import math
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TOKENS = json.loads((ROOT / "docs/brand/tokens.json").read_text())
DATA = json.loads((ROOT / "design/figma/screens.json").read_text())
OUT = ROOT / "docs/design-concepts"
OUT.mkdir(parents=True, exist_ok=True)

class Board:
    def __init__(self, mode="light"):
        self.c = {name: value[mode] for name, value in TOKENS["colors"].items()}
        self.parts = []
        self.mode = mode
        self.raw('<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="860" viewBox="0 0 1200 860">')
        self.raw('<defs><clipPath id="window"><rect width="1200" height="820" rx="24"/></clipPath><clipPath id="results"><rect x="244" y="400" width="644" height="298" rx="20"/></clipPath></defs>')
        self.rect(0, 0, 1200, 860, "canvas")

    def raw(self, s): self.parts.append(s)
    def color(self, c): return self.c.get(c, c)
    def rect(self, x, y, w, h, color="surface", r=0, stroke=None):
        self.raw(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{self.color(color)}"' + (f' stroke="{self.color(stroke)}"' if stroke else "") + "/>")
    def text(self, x, y, s, size=13, color="ink", weight=400, rounded=False, anchor="start", mono=False):
        family = "SF Mono, Menlo, monospace" if mono else "SF Pro Rounded, SF Pro Display, Helvetica Neue, Arial, sans-serif" if rounded else "SF Pro, SF Pro Display, Helvetica Neue, Arial, sans-serif"
        self.raw(f'<text x="{x}" y="{y}" fill="{self.color(color)}" font-family="{family}" font-size="{size}" font-weight="{weight}" text-anchor="{anchor}">{escape(str(s))}</text>')
    def lines(self, x, y, s, width=48, size=12, color="muted", line=18):
        words = s.split()
        lines = []
        current = ""
        for word in words:
            if len(current + " " + word) > width and current:
                lines.append(current); current = word
            else: current = (current + " " + word).strip()
        if current: lines.append(current)
        for i, value in enumerate(lines): self.text(x, y + i * line, value, size, color)
        return len(lines) * line
    def mark(self, x, y, size):
        self.raw(f'<g transform="translate({x} {y}) scale({size/1024})">')
        self.raw('<rect width="1024" height="1024" rx="224" fill="#137969"/><path d="M265 740C202 642 209 491 307 382C405 272 589 226 776 246C784 422 741 603 632 713C526 819 371 830 265 740Z" fill="#5EBEA5"/><path d="M520 370L544 444L618 470L544 495L520 570L495 495L420 470L495 445Z" fill="#FFFDF5" stroke="#FFFDF5" stroke-width="14" stroke-linejoin="round"/>')
        self.raw("</g>")
    def icon(self, x, y, name, color="muted"):
        # Imported vector geometry in the eventual Figma builder uses SF Symbols.
        shapes = {
            "Overview": '<rect x="1" y="1" width="6" height="6" rx="2"/><rect x="11" y="1" width="6" height="6" rx="2"/><rect x="1" y="11" width="6" height="6" rx="2"/><rect x="11" y="11" width="6" height="6" rx="2"/>',
            "Git worktrees": '<path d="M4 3v11c0 3 9 3 9 0V7"/><circle cx="4" cy="3" r="2"/><circle cx="13" cy="5" r="2"/><circle cx="4" cy="15" r="2"/>',
            "iOS simulators": '<rect x="4" y="0" width="10" height="18" rx="3"/><path d="M8 14h2"/>',
            "Orphan processes": '<path d="M0 9h3l2-7 4 15 3-11 2 3h4"/>',
            "Node dependencies": '<path d="m1 5 8-4 8 4v9l-8 4-8-4V5Zm0 0 8 4 8-4M9 9v9"/>',
            "Storage explorer": '<rect x="1" y="2" width="16" height="14" rx="3"/><path d="M1 11h16M13 13h1"/>',
            "Exact duplicates": '<rect x="1" y="5" width="12" height="12" rx="2"/><path d="M5 3V1h12v12h-2"/>',
            "Large files": '<path d="M3 1h8l4 4v12H3V1Zm8 0v5h4"/>',
            "Downloads": '<circle cx="9" cy="9" r="8"/><path d="M9 4v9m-4-4 4 4 4-4"/>',
            "Activity": '<circle cx="9" cy="9" r="8"/><path d="M9 4v5l4 2"/>',
            "Trash": '<path d="M2 4h14M6 4V1h6v3M4 4l1 13h8l1-13M7 7v7m4-7v7"/>',
            "Settings": '<circle cx="9" cy="9" r="3"/><path d="m9 0 1 3 3 1 3-1 2 3-2 2v3l2 2-2 3-3-1-3 1-1 3-3-1-1-3-3-1-2-2 2-3V6L2 4l3-2 2 1 2-3Z"/>'
        }
        shape = shapes.get(name, '<rect x="1" y="3" width="16" height="13" rx="3"/><path d="M1 6h16M5 3V1h5l2 2"/>')
        self.raw(f'<g transform="translate({x} {y})" fill="none" stroke="{self.color(color)}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">{shape}</g>')
    def button(self, x, y, w, label, primary=False):
        self.rect(x, y, w, 40, "accent" if primary else "surface", 12, None if primary else "border")
        self.text(x + w/2, y + 25, label, 13, "onAccent" if primary else "ink", 550, anchor="middle")
    def badge(self, x, y, label, tone="mint"):
        w = max(70, min(160, len(label)*5.6 + 20))
        self.rect(x, y, w, 25, tone + "Soft", 12.5)
        self.text(x + 10, y + 17, label, 11, "destructive" if tone == "coral" else "warning" if tone == "amber" else "ink", 550)
        return w
    def metric(self, x, y, w, value, label, detail, tone=None):
        self.rect(x, y, w, 106, "surface", 20)
        self.text(x + 18, y + 24, label, 11, "muted", 550)
        self.text(x + 18, y + 62, value, 30, "ink", 600, rounded=True)
        self.text(x + 18, y + 86, detail, 10.5, "muted")
        if tone: self.rect(x + w - 30, y + 18, 10, 10, tone, 5)
    def shell(self, data):
        self.screen_name = data["name"]
        self.raw('<g clip-path="url(#window)">')
        self.rect(0, 0, 1200, 820, "canvas", 24)
        self.rect(0, 0, 220, 820, "sidebar")
        for i, color in enumerate(["coral", "amber", "mint"]): self.rect(20+i*21, 18, 12, 12, color, 6)
        self.text(710, 27, "CleanYourMac", 11, "muted", 500, anchor="middle")
        self.mark(20, 56, 46)
        self.text(78, 76, "CleanYourMac", 16, "ink", 650, rounded=True)
        self.text(78, 95, "Room to breathe.", 10.5, "muted")
        self.text(24, 133, "YOUR MAC", 9, "muted", 650)
        for i, item in enumerate(DATA["navigation"]):
            y = 148 + i*36
            active = item["name"] == data["name"]
            if active: self.rect(12, y, 196, 34, "mintSoft", 12)
            self.icon(24, y+8, item["name"], "accent" if active else "muted")
            self.text(54, y+22, item["name"], 12, "ink" if active else "muted", 600 if active else 400)
        self.rect(20, 735, 180, 1, "border")
        if data["name"] == "Settings": self.rect(12, 748, 196, 40, "mintSoft", 12)
        self.icon(24, 759, "Settings")
        self.text(54, 773, "Settings", 12, "ink", 500)
    def finder(self, data):
        self.rect(912, 48, 288, 772, "surface")
        self.rect(911, 48, 1, 772, "border")
        self.text(244, 90, data["title"], 28, "ink", 600, rounded=True)
        self.text(244, 117, data["subtitle"], 12, "muted")
        for index, k in enumerate(["included", "excluded"]):
            y = 143 + index*36
            self.text(244, y + 21, "Include" if k == "included" else "Exclude", 11, "muted", 500)
            x = 299
            for p in data[k][:2]:
                w = max(90, min(265, len(p)*6.1+26))
                self.rect(x, y, w, 30, "mintSoft" if k == "included" else "coralSoft", 15)
                display = p if len(p) < 39 else p[:36] + "…"
                self.text(x+12, y+20, display, 10.5, "ink", mono=True)
                x += w+8
            self.text(881, y+21, "+", 20, "accent", 500, anchor="end")
        self.rect(244, 224, 410, 42, "surface", 12, "border")
        self.raw(f'<circle cx="264" cy="244" r="5" fill="none" stroke="{self.color("muted")}" stroke-width="1.5"/><path d="m268 248 4 4" stroke="{self.color("muted")}" stroke-width="1.5"/>')
        self.text(284, 250, data["query"] or "Search by name", 13, "ink" if data["query"] else "muted")
        self.rect(666, 224, 222, 42, "surface", 12, "border")
        self.text(681, 250, data["sort"], 12)
        self.text(873, 250, "⌄", 14, "muted", anchor="end")
        for i, item in enumerate(data["summary"]):
            self.metric(244+i*218, 280, 208, item["value"], item["label"], item["detail"])
        self.rect(244, 400, 644, 298, "surface", 20)
        self.text(264, 424, "NAME", 9, "muted", 600)
        self.text(868, 424, "SIZE / STATUS", 9, "muted", 600, anchor="end")
        self.raw('<g clip-path="url(#results)">')
        display_rows = data["rows"][-4:] if data["name"] == "Git worktrees" else data["rows"]
        for i, r in enumerate(display_rows):
            y = 441 + i*64
            if i == 0 and data["name"] not in ["Git worktrees", "Exact duplicates"]: self.rect(252, y-1, 628, 66, "selection", 12)
            self.rect(262, y+15, 16, 16, "accent" if r.get("checked") else "surface", 4, None if r.get("checked") else "border")
            if r.get("checked"):
                self.raw(f'<path d="m266 {y+23} 3 3 6-7" stroke="{self.color("onAccent")}" fill="none" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>')
            self.text(291, y+22, r["title"][:44], 12, "ink", 550)
            self.text(291, y+43, r["path"][:49], 10, "muted", mono=True)
            self.text(867, y+21, r["value"], 13, "ink", 550, anchor="end")
            self.badge(867-max(70,min(160,len(r["badge"])*5.6+20)), y+30, r["badge"], r["tone"])
            if i < len(display_rows)-1: self.rect(291, y+60, 574, 1, "border")
        self.raw("</g>")
        if data["name"] == "Git worktrees":
            self.rect(881, 441, 3, 249, "canvas", 1.5)
            self.rect(881, 506, 3, 184, "border", 1.5)
        self.lines(244, 717, data["note"], width=103, size=10.5, line=16)
        self.rect(244, 755, 644, 49, "mintSoft", 16)
        self.text(260, 785, data["selection"], 12, "ink", 550)
        self.button(686, 760, 194, data["action"], True)
        ins = data["inspector"]
        self.text(936, 91, "INSPECTOR", 10, "muted", 600)
        self.text(936, 128, ins["title"][:25], 20, "ink", 600, rounded=True)
        self.badge(936, 143, ins["badge"], "coral" if ins["badge"] == "Orphaned" else "mint")
        for i, (label, value) in enumerate(ins["facts"]):
            y = 210+i*70
            self.text(936, y, label, 11, "muted", 550)
            self.lines(936, y+22, value, width=33, size=12, color="ink", line=17)
        self.rect(936, 497, 240, 148, "mintSoft", 16)
        self.text(952, 523, "What happens next", 12, "ink", 600, rounded=True)
        self.lines(952, 550, ins["consequence"], width=31, size=11.5, line=18)
        self.button(936, 673, 240, "Reveal in Finder")
        self.text(1056, 747, "Protect this path", 12, "accent", 550, anchor="middle")
    def overview(self, data):
        self.text(252, 99, data["title"], 30, "ink", 600, rounded=True)
        self.text(252, 128, data["subtitle"], 13, "muted")
        self.button(964, 78, 208, "Scan enabled modules", True)
        self.badge(252, 152, "Scanned just now", "mint")
        self.text(382, 169, "3 folders need access", 11, "warning", 500)
        for i, item in enumerate(data["metrics"]): self.metric(252+i*313, 201, 297, item["value"], item["label"], item["detail"], item["tone"])
        self.rect(252, 325, 349, 325, "surface", 20)
        self.text(276, 358, "Your Mac's breathing room", 17, "ink", 600, rounded=True)
        self.raw(f'<circle cx="426" cy="479" r="79" fill="none" stroke="{self.color("mintSoft")}" stroke-width="23"/><circle cx="426" cy="479" r="79" fill="none" stroke="{self.color("accent")}" stroke-width="23" stroke-linecap="round" stroke-dasharray="329 497" transform="rotate(-90 426 479)"/>')
        self.text(426, 478, "168 GB", 30, "ink", 600, rounded=True, anchor="middle")
        self.text(426, 501, "free to breathe", 12, "muted", anchor="middle")
        self.rect(281, 602, 8, 8, "accent", 4)
        self.text(298, 610, "332 GB used", 11, "muted")
        self.rect(455, 602, 8, 8, "mintSoft", 4)
        self.text(472, 610, "500 GB total", 11, "muted")
        self.rect(619, 325, 559, 325, "surface", 20)
        self.text(643, 358, "Where the space goes", 17, "ink", 600, rounded=True)
        for i, c in enumerate(data["categories"]):
            y=397+i*48
            self.text(643, y, c["name"], 12)
            self.text(1154, y, c["value"], 12, "ink", 550, anchor="end")
            self.rect(643, y+12, 511, 9, "canvas", 4.5)
            self.rect(643, y+12, 511*c["fraction"]/0.31, 9, c["tone"], 4.5)
        self.rect(252, 669, 926, 119, "mintSoft", 20)
        self.text(279, 703, "Start with things you can rebuild.", 20, "ink", 600, rounded=True)
        self.lines(279, 732, "Node dependencies and generated build data often make good first reviews. Every action shows its paths and consequences before it runs.", width=101, size=12, line=19)
        self.mark(1087, 690, 70)
    def review(self, data):
        self.rect(245, 75, 690, 685, "surface", 24)
        self.text(273, 125, data["title"], 26, "ink", 600, rounded=True)
        self.text(273, 155, data["subtitle"], 13, "muted")
        self.rect(273, 180, 634, 98, "mintSoft", 20)
        self.text(293, 210, "YOUR SELECTION", 10, "muted", 600)
        self.text(293, 253, "2 folders · " + data["total"], 32, "ink", 600, rounded=True)
        for i,r in enumerate(data["rows"]):
            y=312+i*100
            self.text(293,y,r["title"],14,"ink",550)
            self.text(293,y+27,r["path"],11,"muted",mono=True)
            self.text(883,y,r["value"],14,"ink",550,anchor="end")
            self.badge(293,y+43,r["badge"],r["tone"])
        self.rect(273, 505, 634, 128, "canvas", 16)
        self.text(293,532,"What happens next",14,"ink",600,rounded=True)
        self.lines(293,560,data["consequence"],width=88,size=12,line=19)
        self.text(273,660,"Paths and folder contents are verified again before the move.",11,"muted")
        self.button(514,690,150,"Cancel")
        self.button(680,690,227,data["action"],True)
        self.text(976, 127, "REVIEW FIRST", 10, "muted", 600)
        self.lines(976, 161, "A clear pause before anything changes. Exact paths stay visible, and actions describe their recovery limits.", width=25, size=13, line=21)
        self.mark(982, 340, 118)
    def settings(self, data):
        self.text(252,99,data["title"],30,"ink",600,rounded=True)
        self.text(252,130,data["subtitle"],13,"muted")
        for i,section in enumerate(data["sections"]):
            y=164+i*198
            self.rect(252,y,922,179,"surface",20)
            self.text(276,y+34,section["title"],17,"ink",600,rounded=True)
            for j,(label,value) in enumerate(section["rows"]):
                yy=y+70+j*35
                self.text(276,yy,label,13,"ink",500)
                self.text(590,yy,value,12,"muted")
        self.text(276, 788, "Privacy is local. Actions are always reviewed.", 12, "muted")
    def save(self, name):
        self.raw("</g>")
        self.text(1172, 845, "DESIGN CONCEPT · FIXTURE DATA", 10, "muted", 600, anchor="end")
        self.text(20, 845, "COMFY 1.0  /  " + self.screen_name.upper(), 10, "muted", 600)
        self.raw("</svg>")
        path = OUT/(name+".svg")
        path.write_text("".join(self.parts))
        subprocess.run(["rsvg-convert","-w","1800","-h","1290","-o",str(OUT/(name+".png")),str(path)],check=True)
        return path

for screen in DATA["screens"]:
    board = Board()
    board.shell(screen)
    getattr(board, screen["kind"])(screen)
    slug=screen["name"].lower().replace(" ","-")
    board.save(slug)
dark=Board("dark")
dark.shell(DATA["screens"][0]); dark.overview(DATA["screens"][0]); dark.save("overview-dark")
print(f"Rendered {len(DATA['screens'])+1} labelled design boards in {OUT}")
