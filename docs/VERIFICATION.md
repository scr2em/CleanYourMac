# Verification

## Latest Mac test run — October 8, 2026

Tested `d4831e2` with the local fixes below on Apple silicon, macOS 26.5.1, Swift 6.3.3 and Rust 1.89.0:

- All 149 default Rust tests pass. The three opt-in native Rust tests also pass: a real Trash/restore round trip, graceful and explicit forced termination of generated orphan processes, and reset/delete of a newly created simulator.
- All 17 Swift tests pass with `CYM_NATIVE_TESTS=1`, including the native Trash/restore round trip through the Swift bridge.
- Rust formatting, Clippy with warnings denied, design-token lint and shell syntax checks pass.
- The arm64 release app builds and passes strict ad hoc signature verification. The Projects and System Data pages rendered correctly during the preceding `fcd0110` launch check; the latest commits change the Rust core and its tests, not those SwiftUI pages.

The native checks exposed a restore failure because Swift encodes UUIDs in uppercase while the journal lookup compared them case-sensitively. The fix accepts UUID case variations while still using only the journal's recorded paths; a fixture regression test verifies both. They also exposed stale Swift executables after Rust-only changes. The core build now removes only generated app/test executables when the static library contents change, forcing SwiftPM to relink them.

The native test script now selects the Mac fixture suite instead of also running the unrelated release-only million-row benchmark in debug mode. Its process test checks that Force Quit is refused before a graceful request; the graceful call itself already observes a survivor for the two-second grace period. Generated files, processes and simulator devices were cleaned up. These fixes were committed on October 12, 2026, after the default Rust and Swift suites passed again on macOS 27.0.1.

This run does not cover Intel execution, older macOS versions, the release-only million-row benchmark, or cleanup against live container engines and AI tools. The checks below describe that remaining coverage.

## macOS preview verification — October 8, 2026

Verified locally after the SVG resource update:

- Rust formatting, Clippy with warnings denied, and the full Rust test suite pass.
- The design-token lint and full Swift test suite pass (native mutation tests remain opt-in).
- The packaged app builds for both arm64 and x86_64, contains its brand-icon resources, and passes strict ad hoc signature verification.
- The universal app opens on Apple silicon. Intel and older macOS systems were cross-built, not run locally.
- The README screenshots are rendered from the Figma design file with example data. All displayed paths and processes are fictional.

Build checks fix the Swift module-count expectation, the SVG raw-string delimiter, and machine-dependent random exclusion coverage. Demo inspection waits for the result store to finish loading, and the simulator fixture includes its data path so aggregate totals match normal scan results.

The Homebrew cask pins the universal preview archive by SHA-256. This preview is not Developer ID signed or notarized.

## Checks that need a Mac

Fixture tests cover these tools on Linux with stubbed command output. Confirm them on a Mac with real data:

- **Containers & VMs:** the output of `docker system df --format '{{json .}}'` and `docker context show` on Docker Desktop, OrbStack and Colima; the folder of Podman's machine disk and the output of `podman system df`.
- **Simulators:** the fields of `xcrun simctl runtime list -j` (`deletable`, `sizeBytes`, `lastUsedAt`), and that a deleted runtime frees its space.
- **Caches & Logs:** the Electron web caches list no app whose `Cache` folder holds more than a cache.
- **AI Tools:** a Claude Code session still resumes after its `tool-results` folder is in the Trash; the bundle identifiers of Kiro, Trae, Void and the Codex app in `crates/cym-modules/src/modules/ai/catalog.rs` match the installed apps; Ollama frees a model's files when it starts after its manifest is in the Trash.
- **Native tests:** run `scripts/test-native.sh` on a Mac with a simulator runtime and record the result here.

## Current: Rust core with a SwiftUI client

GitHub Actions on `macos-latest` passes every step for the migration commits on `main`:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --locked -- -D warnings`
- `cargo test --locked`
- the design lint
- `scripts/swift.sh test`
- a universal (arm64 and x86_64) app build with ad hoc signing
- a release build of the `cym` CLI

The Rust tests also pass on Linux (37 tests, plus a doc example that does not run). They cover:

- path policy;
- identity, symbolic-link and hard-link handling;
- folder fingerprints;
- replaceable walkers;
- the Node, artifact, duplicate (staged), storage and large-file modules;
- module registration and replacement;
- fail-closed actions, Trash and restore, and journaling;
- analytics de-duplication;
- the Git worktree parser, real fixture worktrees and orphaned registrations;
- command-runner allow-listing, cancellation, and its deadline when a descendant holds the pipe open;
- simulator checks;
- orphan classification and exact-instance signals;
- the C ABI round trip.

Swift tests cover the bridge: descriptor decoding, streamed findings, `Resource` round trips through the core, fail-closed actions and history, analytics, cancellation and the navigation state.

Remaining manual appearance coverage:

- a review of every palette in both light and dark appearance.

Written on Linux and only type-checked for `aarch64-apple-darwin`, so first verified on a Mac:

- the `getattrlistbulk` listing (`cargo test --test files bulk_listing`, which compares it with `lstat` for files, folders, hard links, symbolic links, a FIFO, Unicode names and a 3,000-entry folder);
- the Spotlight last-used lookup and the CoreServices link in `Package.swift`;
- the Swift result table, last-used column and Toolchains scope text.

## Before the migration

The Swift prototype passed 27 tests on macOS 26.5.1 with Swift 6.3.3. Native fixture checks covered a disposable Trash round trip, generated orphan processes and a newly created simulator. Existing user resources were not used for mutation tests. The packaged app was built for Apple silicon and Intel, ad hoc signed, and inspected in an isolated demo bundle.

## Repeatable checks

~~~sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 scripts/lint-design.py
bash scripts/swift.sh test
bash scripts/test-native.sh   # opt-in; needs an iOS simulator runtime
~~~

Native tests create and clean up their own resources.
