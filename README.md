<p align="center"><img src="docs/brand/logo.svg" width="120" alt="CleanYourMac leaf and sparkle logo"></p>

<h1 align="center">CleanYourMac</h1>

<p align="center">A little more room. A calmer Mac.</p>

<p align="center"><a href="LICENSE">MIT</a> · macOS 14+ · Rust core + SwiftUI · Independently built</p>

A comfy, modular, open-source macOS cleaner for people who build things. Find bulky dependencies, forgotten worktrees, simulator data, duplicate files, and processes left behind by their parents. Inspect exact items, then review each cleanup action.

**Development preview:** scanning, safety policy, actions, history and analytics run in a Rust core (`crates/cym-core`) behind replaceable interfaces. A thin SwiftUI app talks to it over a versioned C ABI. A trusted, notarized binary release is not available yet. See [the architecture](docs/ARCHITECTURE.md).

The [Comfy design system](docs/brand/README.md) has original branding and shared light and dark tokens. The default Honey & Espresso palette pairs cream surfaces with honey, burnt orange and espresso. Ember, Terracotta & Navy, Cream & Mauve and Linen & Rose are available in Settings. The app and the [Figma library](docs/FIGMA.md) both use it.

![Overview design concept with warm cream surfaces and mint analytics](docs/design-concepts/overview.png)

*Design concept for the upcoming interface; the current prototype has not been restyled yet.* [Design boards and Figma builder](design/figma) · [Migration handoff](docs/HANDOFF.md)

## Run the app

Requires Xcode (Swift 6) and a Rust toolchain (`rustup target add aarch64-apple-darwin x86_64-apple-darwin` for universal builds).

~~~sh
bash scripts/build-app.sh
open build/CleanYourMac.app
~~~

For a universal Apple silicon / Intel bundle:

~~~sh
bash scripts/build-app.sh --universal
~~~

The local bundle is ad hoc signed. Developer ID signing and notarization are required for a trusted downloadable release.

To explore synthetic data with cleanup disabled:

~~~sh
open -n build/CleanYourMac.app --args --demo
~~~

Choose root folders, open a tool, and scan. Findings start unselected. Clicking a row opens its inspector; its checkbox selects it for review. Every removal uses a review sheet with the selected resources and consequences.

## Included tools

| Tool | Behavior |
| --- | --- |
| Storage Explorer | Folder sizes, storage bars, folder navigation, Finder reveal |
| Large Files | Files at least 100 MB, size and modification-date filters |
| Exact Duplicates | SHA-256 verification, one preserved original, fresh verification before removal; skips system, build, and dependency folders across languages |
| Node Dependencies | Project-owned node_modules, package-manager evidence, nested dependency grouping, symlink-safe sizing |
| Build Artifacts | JavaScript, SwiftPM, Cargo, Python and Gradle rules with project evidence |
| Git Worktrees | Registered worktrees, orphaned-registration badges, branches, locks, changes, ignored files and upstream state; eligible linked-tree removal through Git |
| Simulators | Device app-data sizes, state and runtimes; reviewed reset/delete of a specific shutdown device |
| Xcode Data | DerivedData and device support; release archives remain inspectable and protected |
| Caches & Logs | Known package caches and user logs, with rebuild/offline-use consequences |
| Applications | Nested installed apps and separately selected, precisely named related data |
| App Leftovers | Possible leftovers in caches, preferences and saved state; uncertain ownership is explicit |
| Downloads | Manually reviewed downloaded files and installers |
| Trash | Permanently remove specifically selected local Trash items |
| Orphan Processes | Current-user suspected orphan processes, CPU/memory, reviewed SIGTERM and separately reviewed Force Quit |

Settings include roots, protected paths, ignored process names, module toggles, and an optional menu-bar monitor. Activity records local per-item outcomes and offers restore for unchanged items still in Trash.

Protection, Cloud Cleanup, Email Cleanup, and CleanMyMac/MacPaw Performance features are excluded. This project does not use MacPaw code, CLI tools, binaries, artwork or services. Orphan-process inspection adapts the owner's MIT-licensed OrphanBar project; see [notices](THIRD_PARTY_NOTICES.md).

Docker/VM integrations, AI models, similar-image detection, app updates, backup management, and runtime removal are future integrations.

## Read-only CLI

~~~sh
cargo build --release --bin cym
./target/release/cym modules
./target/release/cym scan node ~/Projects --json
./target/release/cym scan worktrees ~/Projects
./target/release/cym scan simulators --json
~~~

CLI scans never apply cleanup actions. JSON omits process commands and abbreviates the home directory. Exit codes: 0 complete, 1 failed, 2 invalid invocation, 3 partial coverage.

## Development

~~~sh
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings
cargo test
python3 scripts/lint-design.py
bash scripts/swift.sh test
~~~

`scripts/swift.sh` builds the Rust static library (`scripts/build-core.sh`) and then runs SwiftPM, keeping compiler and package caches inside the workspace. To work in Xcode, run `bash scripts/build-core.sh` once and then open Package.swift.

Optional live tests use only generated disposable resources and require an installed iOS simulator runtime:

~~~sh
bash scripts/test-native.sh
~~~

For UI review in a separate preview bundle with isolated preferences:

~~~sh
bash scripts/preview-app.sh --dark
~~~

The Rust core reaches the platform only through traits: filesystem, directory walker, sizer, hasher, command runner, process inspector, app inventory, Trash and journal. Any implementation can be swapped, for example for a library-backed walker, without touching scanners or safety rules. Features are registered modules, not feature-specific code in views. The SwiftUI shell keeps native folder pickers, Finder integration and presentation.

See [module authoring](docs/MODULES.md), [the design system](docs/DESIGN_SYSTEM.md), [the original product plan](docs/PLAN.md), and [verification](docs/VERIFICATION.md).

Scans stay local. Size and allocated-space values are estimates, and moving to Trash does not free disk space until Trash is emptied. Protected paths, stale identities, partial folder coverage, active tools and changed process instances can block an action. Filesystem checks reduce accidental removal; they do not provide atomic isolation from another program changing a path during an operation.
