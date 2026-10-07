# Verification

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

Not yet re-run after the migration:

- the native tests (`scripts/test-native.sh`), which cover a real Trash round trip, generated orphan processes and a disposable simulator;
- a manual review of the packaged app with the new palettes.

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
