#!/bin/bash
set -euo pipefail
TASK_REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TASK_REPO"
mkdir -p .build/NativeFixtures
xcrun clang Tests/NativeFixtures/OrphanFixture.c -o .build/NativeFixtures/OrphanFixture
CYM_NATIVE_TESTS=1 CYM_NATIVE_PROCESS_TESTS=1 CYM_NATIVE_SIMULATOR_TESTS=1 bash scripts/swift.sh test
