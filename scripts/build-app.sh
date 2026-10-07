#!/bin/bash
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TASK_REPO"
TASK_APP="$TASK_REPO/build/CleanYourMac.app"
TASK_UNIVERSAL=""
if [ "$#" -gt 0 ]; then TASK_UNIVERSAL="$1"; fi
export CYM_SKIP_CORE=1
if [ "$TASK_UNIVERSAL" = "--universal" ]; then
  bash scripts/build-core.sh --universal
  bash scripts/swift.sh build -c release --triple arm64-apple-macosx14.0
  bash scripts/swift.sh build -c release --triple x86_64-apple-macosx14.0
else
  bash scripts/build-core.sh
  bash scripts/swift.sh build -c release
fi
mkdir -p "$TASK_APP/Contents/MacOS" "$TASK_APP/Contents/Resources"
if [ "$TASK_UNIVERSAL" = "--universal" ]; then
  lipo -create .build/arm64-apple-macosx/release/CleanYourMac .build/x86_64-apple-macosx/release/CleanYourMac -output "$TASK_APP/Contents/MacOS/CleanYourMac"
  TASK_PRODUCTS=.build/arm64-apple-macosx/release
else
  cp .build/release/CleanYourMac "$TASK_APP/Contents/MacOS/CleanYourMac"
  TASK_PRODUCTS=.build/release
fi
# SwiftPM resource bundles (brand logos) live in Resources, where the app looks for them.
for TASK_BUNDLE in "$TASK_PRODUCTS"/*.bundle; do
  rm -rf "$TASK_APP/Contents/Resources/$(basename "$TASK_BUNDLE")"
  cp -R "$TASK_BUNDLE" "$TASK_APP/Contents/Resources/"
done
cp Support/Info.plist "$TASK_APP/Contents/Info.plist"
cp Support/AppIcon.icns "$TASK_APP/Contents/Resources/AppIcon.icns"
codesign --force --sign - "$TASK_APP"
codesign --verify --strict "$TASK_APP"
printf 'Built %s\n' "$TASK_APP"
