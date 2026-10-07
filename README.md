<p align="center"><img src="docs/brand/logo.svg" width="120" alt="CleanYourMac leaf and sparkle logo"></p>

<h1 align="center">CleanYourMac</h1>

<p align="center">A little more room. A calmer Mac.</p>

<p align="center"><a href="LICENSE">MIT</a> · macOS 14+ · Rust core + SwiftUI · Independently built</p>

A comfy, modular, open-source macOS cleaner for people who build things. Find bulky dependencies, forgotten worktrees, simulator data, duplicate files, and processes left behind by their parents. Inspect exact items, then review each cleanup action.

**Development preview:** the functional Swift prototype is being migrated to a Rust core with a thin SwiftUI interface. The Rust crate and versioned bridge are under active development. This repository currently builds the Swift prototype; a trusted, notarized binary release is not available yet. See the [architecture and migration plan](docs/ARCHITECTURE.md).

The [Comfy design system](docs/brand/README.md) includes original branding and shared light/dark tokens. The app and Figma screens are being updated to use it.

![Overview design concept with warm cream surfaces and mint analytics](docs/design-concepts/overview.png)

*Design concept for the upcoming interface; the current prototype has not been restyled yet.* [Design boards and Figma builder](design/figma) · [Migration handoff](docs/HANDOFF.md)

## Run the app

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
bash scripts/swift.sh build
.build/debug/cym modules
.build/debug/cym scan node ~/Projects --json
.build/debug/cym scan worktrees ~/Projects
.build/debug/cym scan simulators --json
~~~

CLI scans never apply cleanup actions. JSON omits process commands and abbreviates the home directory. Exit codes: 0 complete, 1 failed, 2 invalid invocation, 3 partial coverage.

## Development

~~~sh
python3 scripts/lint-design.py
bash scripts/swift.sh test
~~~

Open Package.swift in Xcode, or use the SwiftPM scripts. scripts/swift.sh keeps compiler/package caches inside the workspace, which also supports restricted development environments.

Optional live tests use only generated disposable resources and require an installed iOS simulator runtime:

~~~sh
bash scripts/test-native.sh
~~~

For UI review in a separate preview bundle with isolated preferences:

~~~sh
bash scripts/preview-app.sh --dark
~~~

The prototype has separate Core, Platform, Modules, DesignSystem and UI targets. The migration moves scanning, policy, actions, persistence, and analytics into crates/cym-core. The SwiftUI shell will retain native folder pickers, Finder integration, and presentation. Features use registered modules rather than adding feature-specific cleanup code to views.

See [module authoring](docs/MODULES.md), [the design system](docs/DESIGN_SYSTEM.md), [the original product plan](docs/PLAN.md), and [verification](docs/VERIFICATION.md).

Scans stay local. Size and allocated-space values are estimates, and moving to Trash does not free disk space until Trash is emptied. Protected paths, stale identities, partial folder coverage, active tools and changed process instances can block an action. Filesystem checks reduce accidental removal; they do not provide atomic isolation from another program changing a path during an operation.
