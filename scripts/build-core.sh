#!/bin/bash
# Builds the Rust core as a static library for the Swift package: target/cym/libcym_core.a
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TASK_REPO"
export MACOSX_DEPLOYMENT_TARGET=14.0
mkdir -p target/cym
if [ "${1:-}" = "--universal" ]; then
  for TASK_TARGET in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked -p cym-core --lib --target "$TASK_TARGET"
  done
  lipo -create target/aarch64-apple-darwin/release/libcym_core.a target/x86_64-apple-darwin/release/libcym_core.a -output target/cym/libcym_core.a
else
  cargo build --release --locked -p cym-core --lib
  cp target/release/libcym_core.a target/cym/libcym_core.a
fi
# SwiftPM does not track the external static library as a link input. Remove only
# generated executables when it changes, so SwiftPM relinks the app and tests.
TASK_CORE_SIGNATURE="$(shasum -a 256 target/cym/libcym_core.a | cut -d ' ' -f 1)"
TASK_CORE_STAMP=target/cym/swift-core-link.sha256
if [ ! -f "$TASK_CORE_STAMP" ] || [ "$(cat "$TASK_CORE_STAMP")" != "$TASK_CORE_SIGNATURE" ]; then
  for TASK_SWIFT_PRODUCTS in "$TASK_REPO"/.build/*-apple-macosx/debug "$TASK_REPO"/.build/*-apple-macosx/release; do
    rm -f "$TASK_SWIFT_PRODUCTS/CleanYourMac" "$TASK_SWIFT_PRODUCTS/CleanYourMacPackageTests.xctest/Contents/MacOS/CleanYourMacPackageTests"
  done
  printf '%s\n' "$TASK_CORE_SIGNATURE" > "$TASK_CORE_STAMP"
fi
printf 'Built %s\n' "$TASK_REPO/target/cym/libcym_core.a"
