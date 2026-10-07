# Contributing

Start with an issue or a small focused change. Follow the contracts in [module authoring](docs/MODULES.md), and keep UI additions inside the [design system](docs/DESIGN_SYSTEM.md).

Core logic is Rust (stable, edition 2021) in `crates/cym-core`; the app is Swift 6 with macOS 14-compatible APIs. Platform access goes through the traits in `ports.rs`. Add a crate only when it solves a concrete problem and its license and platform support have been reviewed. Current Rust dependencies are serde, serde_json, sha2, blake3, rayon, plist, uuid and libc. Swift has no package dependencies.

Before submitting a change:

~~~sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 scripts/lint-design.py
bash scripts/swift.sh test
bash scripts/build-app.sh --universal
~~~

Describe the final behavior, why it changes, and how it was verified. New cleanup rules require evidence, fresh action validation, clear recovery limits and synthetic fixtures. Never make a test clean a real project, reset an existing simulator or terminate an unrelated process.

The Component Gallery and --demo mode support UI review with synthetic data. Check shared layouts in light/dark mode and at the minimum window size. Retain native keyboard and accessibility behavior.

New source contributions use the MIT license. Preserve attribution for adapted code and update THIRD_PARTY_NOTICES.md when appropriate.
