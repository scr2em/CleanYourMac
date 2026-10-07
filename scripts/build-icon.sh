#!/bin/bash
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TASK_REPO"
mkdir -p .build/ModuleCache
xcrun swift -module-cache-path "$TASK_REPO/.build/ModuleCache" scripts/make-icon.swift "$TASK_REPO/build/AppIcon.iconset"
iconutil -c icns "$TASK_REPO/build/AppIcon.iconset" -o "$TASK_REPO/Support/AppIcon.icns"
