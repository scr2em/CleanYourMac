#!/bin/bash
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$TASK_REPO/.build/ModuleCache" "$TASK_REPO/.build/PackageCache"
export CLANG_MODULE_CACHE_PATH="$TASK_REPO/.build/ModuleCache"
export SWIFT_MODULECACHE_PATH="$TASK_REPO/.build/ModuleCache"
# The Swift package links the Rust core; build it first unless the caller already did.
if [ "${CYM_SKIP_CORE:-}" != "1" ]; then bash "$TASK_REPO/scripts/build-core.sh"; fi
cd "$TASK_REPO"
exec xcrun swift "$@" --cache-path "$TASK_REPO/.build/PackageCache" --disable-sandbox
