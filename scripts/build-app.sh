#!/bin/bash
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TASK_REPO"
TASK_APP="$TASK_REPO/build/CleanYourMac.app"
TASK_UNIVERSAL=""
if [ "$#" -gt 0 ]; then TASK_UNIVERSAL="$1"; fi
export CYM_SKIP_CORE=1
# Ask SwiftPM where products go: Swift Build (Swift 6.4+) uses .build/out/Products/Release for every
# triple, so each slice is kept aside before the next triple's build overwrites it.
TASK_SLICES="$TASK_REPO/.build/universal-slices"
if [ "$TASK_UNIVERSAL" = "--universal" ]; then
  bash scripts/build-core.sh --universal
  rm -rf "$TASK_SLICES"
  mkdir -p "$TASK_SLICES"
  for TASK_TRIPLE in arm64-apple-macosx14.0 x86_64-apple-macosx14.0; do
    bash scripts/swift.sh build -c release --triple "$TASK_TRIPLE"
    TASK_PRODUCTS="$(bash scripts/swift.sh build -c release --triple "$TASK_TRIPLE" --show-bin-path)"
    cp "$TASK_PRODUCTS/CleanYourMac" "$TASK_SLICES/CleanYourMac-$TASK_TRIPLE"
  done
else
  bash scripts/build-core.sh
  bash scripts/swift.sh build -c release
  TASK_PRODUCTS="$(bash scripts/swift.sh build -c release --show-bin-path)"
fi
mkdir -p "$TASK_APP/Contents/MacOS" "$TASK_APP/Contents/Resources"
if [ "$TASK_UNIVERSAL" = "--universal" ]; then
  lipo -create "$TASK_SLICES"/CleanYourMac-* -output "$TASK_APP/Contents/MacOS/CleanYourMac"
else
  cp "$TASK_PRODUCTS/CleanYourMac" "$TASK_APP/Contents/MacOS/CleanYourMac"
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
