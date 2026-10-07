#!/bin/bash
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TASK_REPO"
TASK_APP="$TASK_REPO/build/CleanYourMac.app"
TASK_UNIVERSAL=""
if [ "$#" -gt 0 ]; then TASK_UNIVERSAL="$1"; fi
if [ "$TASK_UNIVERSAL" = "--universal" ]; then
  bash scripts/swift.sh build -c release --triple arm64-apple-macosx14.0
  bash scripts/swift.sh build -c release --triple x86_64-apple-macosx14.0
else
  bash scripts/swift.sh build -c release
fi
mkdir -p "$TASK_APP/Contents/MacOS" "$TASK_APP/Contents/Resources"
if [ "$TASK_UNIVERSAL" = "--universal" ]; then
  lipo -create .build/arm64-apple-macosx/release/CleanYourMac .build/x86_64-apple-macosx/release/CleanYourMac -output "$TASK_APP/Contents/MacOS/CleanYourMac"
else
  cp .build/release/CleanYourMac "$TASK_APP/Contents/MacOS/CleanYourMac"
fi
cp Support/Info.plist "$TASK_APP/Contents/Info.plist"
cp Support/AppIcon.icns "$TASK_APP/Contents/Resources/AppIcon.icns"
codesign --force --sign - "$TASK_APP"
codesign --verify --strict "$TASK_APP"
printf 'Built %s\n' "$TASK_APP"
