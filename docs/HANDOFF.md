# Handoff

The user asked to push the current work and stop so another agent can continue. The active objective is **a Rust core with a thin SwiftUI interface**. The public repository is https://github.com/scr2em/CleanYourMac; publish further work there using the owner's personal `scr2em` account.

## Current state

The migration's core work is done. The app runs on the Rust core:

- **Rust core** (`crates/cym-core`): the ports-and-adapters layer, 14 registered modules, a parallel scan coordinator, the reviewed-action executor with per-item journaling and restore, analytics, a versioned C ABI and the read-only `cym` CLI. Parallel sizing and hashing; staged duplicate detection.
- **Tests:** 37 Rust tests ported from the Swift safety and integration suites, using fixtures and injected fakes, plus opt-in native tests in `tests/native.rs`. Swift tests cover the bridge.
- **SwiftUI:** a thin client through `CoreEngine`. The Swift scanners, platform services, executor and CLI have been removed.
- **Design:** the Comfy linen-and-rose palette is in `tokens.json` and `DesignSystem.swift`. The Figma library is published (see [FIGMA.md](FIGMA.md)).

## Continue in this order

1. Run `scripts/test-native.sh` on a Mac with a simulator runtime and record the results in VERIFICATION.md.
   Also check AI Tools with real data: a Claude Code session still resumes after its `tool-results` folder is in the Trash; the bundle identifiers of Kiro, Trae, Void and the Codex app (`crates/cym-modules/src/modules/ai/catalog.rs`) match the installed apps; Ollama frees a model's files when it starts after its manifest is in the Trash.
2. Give each finder its own scope in AppStore: included and excluded paths, query, filters, sort order and scan task. The current global scan task cancels other finders' scans. Known-location tools should filter their inventory by the finder's scope.
3. Finish the visual pass to match the Figma screens: result summaries directly below search, Overview analytics charts from `Analytics.modules`, orphaned-worktree badges in rows, and size and name sorting. Capture real app screenshots using synthetic data.
4. Add Code Connect mappings and more Figma screens (Overview, Activity, Settings).

## Requirements to preserve

- No MacPaw CLI, code, assets, binaries, or service dependency.
- No Protection, Cloud Cleanup, Email Cleanup, or CleanMyMac/MacPaw Performance feature port.
- Composable, registered feature modules and a governed joyful/comfy design system in SwiftUI and Figma.
- Dependency discovery (the `node` module, titled Dependencies) stops at each node_modules, virtual environment, Pods, vendor or deps folder; it never descends to find nested ones. Build Artifacts skips those folders too, so nothing is listed twice.
- Duplicate scanning skips system directories and language build/dependency/package stores. Rust policy lists the ignored directory names; test coverage is still needed.
- Orphaned worktrees need a visible badge. Missing Git registrations are inspection-only; do not blindly prune them.
- Every finder retains independent included/excluded paths, name query, sorting and scans. Result summaries sit immediately below the search box.
- Overview should present aggregate analytics visually without double-counting overlapping paths or mixing process memory with disk storage.
- Public repository, polished README, original logo, real screenshots, description and topics. Initial public publication is done; screenshots and the final migration presentation remain pending.

## Verification and environment

~~~sh
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
bash scripts/swift.sh test
bash scripts/build-app.sh --universal
~~~

The notes below come from the earlier handoff.

Rust 1.89.0 and Cargo 1.89.0 are installed. Dependency downloads and build artifacts are kept under `.build/`:

~~~sh
CARGO_HOME="$PWD/.build/cargo-home" CARGO_TARGET_DIR="$PWD/.build/rust" cargo check --offline
CARGO_HOME="$PWD/.build/cargo-home" CARGO_TARGET_DIR="$PWD/.build/rust" cargo build --offline
cargo fmt --all
bash scripts/swift.sh test
~~~

Rust cargo check and cargo build pass. The current Swift suite passes 27 discovered tests with three opt-in native tests skipped; an earlier explicit native-fixture run passed all 27 before the Rust port. Compile success does not establish behavioral parity for the Rust implementation.

The normal app may be in use by the owner. Use `scripts/preview-app.sh` and its separate `org.cleanyourmac.preview` demo bundle for UI inspection; do not reset or quit the owner's normal app. Demo cleanup actions are disabled.
