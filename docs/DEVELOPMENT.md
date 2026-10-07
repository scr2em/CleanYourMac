# Development

CleanYourMac has a Rust core and a SwiftUI macOS app. Start with [architecture](ARCHITECTURE.md), [module authoring](MODULES.md), and [the design system](DESIGN_SYSTEM.md).

## Build

Use Xcode with Swift 6 and the official Rust toolchain installed through [rustup](https://rust-lang.org/tools/install/). For universal builds, install both targets:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bash scripts/build-app.sh
open build/CleanYourMac.app
```

Build for Apple silicon and Intel with `bash scripts/build-app.sh --universal`. The app bundle includes the SwiftPM brand-icon resources and is ad hoc signed. Developer ID signing and notarization are still needed for a notarized release.

## Checks

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
python3 scripts/lint-design.py
bash scripts/swift.sh test
```

`scripts/swift.sh` builds the Rust static library before invoking SwiftPM and keeps caches in the workspace. To work in Xcode, run `bash scripts/build-core.sh` first, then open Package.swift.

Optional native checks use generated disposable resources and require an installed iOS simulator runtime:

```sh
bash scripts/test-native.sh
```

## Preview and screenshots

```sh
bash scripts/preview-app.sh
bash scripts/preview-app.sh --dark
bash scripts/preview-app.sh --screen overview
```

The separate preview bundle has isolated preferences and always uses synthetic data with cleanup disabled. `--prepare-only` prepares it without launching. Capture the actual app window, including its inspector, and save PNG or JPEG captures to `docs/screenshots/`. Never publish screenshots of personal folders or processes.

For a large synthetic list: `bash scripts/preview-app.sh --demo-rows 1000000`.

## Read-only CLI

```sh
cargo build --release --locked --bin cym
./target/release/cym modules
./target/release/cym scan node ~/Projects --json
./target/release/cym scan worktrees ~/Projects
./target/release/cym scan simulators --json
```

CLI scans never apply cleanup actions. JSON abbreviates the home directory and omits process commands. Exit codes: 0 complete, 1 failed, 2 invalid invocation, 3 partial coverage.

## Distribution

`Casks/cleanyourmac.rb` points to the universal preview archive in GitHub Releases and pins its SHA-256 checksum. Build and verify the bundle, archive it with `ditto -c -k --sequesterRsrc --keepParent`, and update the cask checksum for every release. Do not disable Homebrew quarantine or macOS security checks.

Protection, Cloud Cleanup, Email Cleanup, and CleanMyMac/MacPaw Performance features are outside the project scope. See [verification](VERIFICATION.md) and [the original plan](PLAN.md).
