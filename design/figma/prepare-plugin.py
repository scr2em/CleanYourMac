#!/usr/bin/env python3
"""Bundle the editable builder as an offline Figma development plugin."""
from pathlib import Path
import json

root = Path(__file__).resolve().parents[2]
design = root / "design" / "figma"
brand = root / "docs" / "brand"
out = root / "build" / "figma-plugin"
out.mkdir(parents=True, exist_ok=True)
inputs = {
    "tokens": json.loads((brand / "tokens.json").read_text()),
    "screens": json.loads((design / "screens.json").read_text()),
    "logo": (brand / "logo.svg").read_text(),
}
code = (design / "build.js").read_text()
code += "\nconst CYM_INPUT = " + json.dumps(inputs, ensure_ascii=False) + ";\n"
code += """
async function runCleanYourMac() {
  const ledger = { fileKey: figma.fileKey || null, pages: {}, components: {}, screens: {}, results: [] };
  const base = await buildCleanYourMac(figma, { ...CYM_INPUT, phase: "foundations" });
  ledger.pages.foundations = base.pageID;
  ledger.results.push(base);
  const library = await buildCleanYourMac(figma, { ...CYM_INPUT, phase: "components" });
  ledger.pages.components = library.pageID;
  ledger.components = library.componentIDs;
  ledger.results.push(library);
  for (const screen of CYM_INPUT.screens.screens) {
    const result = await buildCleanYourMac(figma, {
      ...CYM_INPUT, phase: "screens", screenName: screen.name, componentIDs: ledger.components
    });
    ledger.pages.screens = result.pageID;
    ledger.screens[screen.name] = result.screenID;
    ledger.results.push(result);
  }
  for (const screen of CYM_INPUT.screens.screens) {
    const audit = await buildCleanYourMac(figma, {
      ...CYM_INPUT, phase: "audit", screenName: screen.name, componentIDs: ledger.components
    });
    if (audit.imageNodes.length || audit.unexpectedFonts.length) throw new Error("Editable screen audit failed: " + screen.name);
    ledger.results.push(audit);
  }
  console.log(JSON.stringify(ledger, null, 2));
  figma.closePlugin("CleanYourMac: foundations, 9 component families, and 10 editable screens created. Inspect each screen before publishing.");
}
runCleanYourMac().catch((error) => {
  console.error(error);
  figma.closePlugin("CleanYourMac build stopped: " + error.message);
});
"""
(out / "code.js").write_text(code)
(out / "manifest.json").write_text(json.dumps({
    "name": "CleanYourMac Comfy",
    "api": "1.0.0",
    "main": "code.js",
    "editorType": ["figma"],
    "documentAccess": "dynamic-page",
    "networkAccess": {"allowedDomains": ["none"]},
}, indent=2) + "\n")
print(out / "manifest.json")
