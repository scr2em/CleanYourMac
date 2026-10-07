#!/bin/bash
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TASK_REPO"
TASK_PREPARE_ONLY=0
if [ "${1:-}" = "--prepare-only" ]; then
  TASK_PREPARE_ONLY=1
  shift
fi
test -d build/CleanYourMac.app
ditto build/CleanYourMac.app build/CleanYourMacPreview.app
python3 - <<'PY'
import plistlib
from pathlib import Path
path = Path("build/CleanYourMacPreview.app/Contents/Info.plist")
with path.open("rb") as source:
    info = plistlib.load(source)
info["CFBundleIdentifier"] = "org.cleanyourmac.preview"
info["CFBundleName"] = "CleanYourMacPreview"
info["CFBundleDisplayName"] = "CleanYourMac Preview"
with path.open("wb") as target:
    plistlib.dump(info, target)
PY
codesign --force --sign - build/CleanYourMacPreview.app
if [ "$TASK_PREPARE_ONLY" = 1 ]; then exit 0; fi
open -n build/CleanYourMacPreview.app --args --demo "$@"
