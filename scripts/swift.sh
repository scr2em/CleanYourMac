#!/bin/bash
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$TASK_REPO/.build/ModuleCache" "$TASK_REPO/.build/PackageCache"
export CLANG_MODULE_CACHE_PATH="$TASK_REPO/.build/ModuleCache"
export SWIFT_MODULECACHE_PATH="$TASK_REPO/.build/ModuleCache"
cd "$TASK_REPO"
exec xcrun swift "$@" --cache-path "$TASK_REPO/.build/PackageCache" --disable-sandbox
