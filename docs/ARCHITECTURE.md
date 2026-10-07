# Rust core, SwiftUI shell

The Rust crate owns discovery, file identities and fingerprints, scope policy, duplicate hashing, Git and simulator command adapters, orphan classification, action eligibility, journal persistence, and aggregate analytics. The command-line tool links the same crate.

SwiftUI owns presentation and observable view state. A versioned JSON interface crosses a small C ABI with explicit allocation and cancellation. The Swift host supplies only native application inventory and FileManager's Trash operation; Rust validates every mutation before calling it. Finder reveals, folder pickers, keyboard commands, and window lifecycle stay in Swift.

Each feature implements the Rust `ScanModule` interface and declares a descriptor. Registering a module makes it available to both the app and CLI. Each finder retains its own included paths, excluded paths, query, sorting, filters, and independent cancellation handle.

Migration sequence:

1. Introduce the crate, wire models, filesystem policy, scanners, and tests.
2. Port Git, simctl, process inspection and typed actions, including fresh identity checks.
3. Replace Swift scanner/executor implementations with the bridge and remove duplicate business logic.
4. Verify Rust tests, Swift integration, native fixture actions and packaged application.
5. Finish the shared Comfy design system, app screenshots, Figma screens, and public repository.

No privileged daemon, shell command interpolation, MacPaw dependency, background deletion, or automatic process force quit is used.
