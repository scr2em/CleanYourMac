# Handoff

The user asked to push the current work and stop so another agent can continue. The active objective is **a Rust core with a thin SwiftUI interface**. The public repository is https://github.com/scr2em/CleanYourMac; publish further work there using the owner's personal `scr2em` account.

## Current state

The Swift prototype still builds and runs independently. It remains the application's backend today. The Rust migration is real source work but **is not connected to SwiftUI yet**.

`crates/cym-core` now contains:

- Serializable wire models, scope/path policy, module registration, cancellation tokens.
- Filesystem walking, physical root deduplication, symlink-aware sizing, SHA-256 content hashing, folder metadata fingerprints, and fresh identity validation.
- Rust scanners for storage, large files, exact duplicates, Node dependencies, build artifacts, Xcode data, known caches/logs, downloads, Trash, applications/leftovers, Git worktrees, simulators, and orphan processes.
- Git porcelain parsing and worktree eligibility/removal adapters; simulator inventory and specific-device actions; orphan classification, CPU samples and separately gated process signals.
- A small Objective-C adapter for libproc snapshots, NSWorkspace running-app inventory and FileManager's native Trash operation. Ownership and eligibility decisions live in Rust.

The Rust CLI currently prints a placeholder banner. Rust has no action executor, journal, analytics API, C bridge, or regression tests yet. The existing Swift safety and integration tests have not been ported.

## Continue in this order

1. Add meaningful Rust tests for the ported behavior. Compare native and fixture results against the existing Swift implementation before removing it. Validate C/Rust process struct layout and native APIs with generated disposable resources.
2. Harden the Rust command runner's timeout and cancellation behavior when descendant processes inherit output pipes. Reader-thread joins can currently outlive the owned child's timeout. Do this before wiring mutation APIs.
3. Implement the Rust typed action executor, current-scope revalidation, per-item journal, restore checks, and aggregate analytics. Preserve the existing active-tool, stale-identity, protected-descendant, duplicate rehash, Git safety, fresh simulator state, and fresh process-instance checks.
4. Introduce a versioned C ABI with explicit allocation/free and per-scan cancellation. The Rust Resource wire enum uses `kind` tags; Swift's current synthesized Resource encoding differs and needs a small Codable bridge adapter. Dates should use Unix seconds across the boundary.
5. Make SwiftUI presentation-only: decode Rust descriptors/results, retain native UI integration, and remove the duplicated Swift scanners/executor. Keep a functional CLI linked to the same Rust crate. Update builds, CI and packaging for the Rust static library and both Mac architectures.
6. Refactor AppStore to independent finder scopes, queries, filters, sort choices and scan tasks. Its current global scan task still cancels other searches. Each finder needs included and excluded paths; known-location tools should filter their known inventory using that finder’s scope.
7. Apply the shared Comfy tokens and logo to the app, replace the utility styling with custom components, add summaries directly below search, overview analytics, orphaned-worktree badges, and size/name sorting. Capture actual app screenshots using synthetic data.
8. Publish the Figma screens after connector reauthentication, then update the README and verification record and push the finished migration.

## Requirements to preserve

- No MacPaw CLI, code, assets, binaries, or service dependency.
- No Protection, Cloud Cleanup, Email Cleanup, or CleanMyMac/MacPaw Performance feature port.
- Composable, registered feature modules and a governed joyful/comfy design system in SwiftUI and Figma.
- Node discovery stops at each node_modules directory; never descends to find nested node_modules.
- Duplicate scanning skips system directories and language build/dependency/package stores. Rust policy lists the ignored directory names; test coverage is still needed.
- Orphaned worktrees need a visible badge. Missing Git registrations are inspection-only; do not blindly prune them.
- Every finder retains independent included/excluded paths, name query, sorting and scans. Result summaries sit immediately below the search box.
- Overview should present aggregate analytics visually without double-counting overlapping paths or mixing process memory with disk storage.
- Public repository, polished README, original logo, real screenshots, description and topics. Initial public publication is done; screenshots and the final migration presentation remain pending.

## Verification and environment

Rust 1.89.0 and Cargo 1.89.0 are installed. Dependency downloads and build artifacts are kept under `.build/`:

~~~sh
CARGO_HOME="$PWD/.build/cargo-home" CARGO_TARGET_DIR="$PWD/.build/rust" cargo check --offline
CARGO_HOME="$PWD/.build/cargo-home" CARGO_TARGET_DIR="$PWD/.build/rust" cargo build --offline
cargo fmt --all
bash scripts/swift.sh test
~~~

Rust cargo check and cargo build pass. The current Swift suite passes 27 discovered tests with three opt-in native tests skipped; an earlier explicit native-fixture run passed all 27 before the Rust port. Compile success does not establish behavioral parity for the Rust implementation.

The normal app may be in use by the owner. Use `scripts/preview-app.sh` and its separate `org.cleanyourmac.preview` demo bundle for UI inspection; do not reset or quit the owner's normal app. Demo cleanup actions are disabled.
