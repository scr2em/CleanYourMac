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
printf 'Built %s\n' "$TASK_REPO/target/cym/libcym_core.a"
