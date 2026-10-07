# Rust core, SwiftUI shell

CleanYourMac is a Rust core with a thin SwiftUI interface. The core owns discovery, scope and protection policy, file identities and folder fingerprints, duplicate hashing, Git and simulator adapters, orphan classification, action eligibility and execution, the action journal and aggregate analytics. SwiftUI owns presentation, observable view state, folder pickers, Finder reveals, keyboard commands and window lifecycle. The `cym` command-line tool links the same core.

~~~
SwiftUI views ── AppStore ── CoreEngine (Swift) ──C ABI (JSON)── Engine (Rust)
                                                                 ├─ Registry ── ScanModule × 14
                                                                 ├─ actions · analytics · policy
                                                                 └─ Services ── ports (traits) ── adapters
~~~

## Crates

The core is a Cargo workspace of layered crates, so a change rebuilds only its crate and the ones above it:

| Crate | Owns | Depends on |
| --- | --- | --- |
| `cym-model` | Wire models, path and protection policy | — |
| `cym-ports` | The replaceable traits | model |
| `cym-adapters` | Standard-library, macOS (`macos.m`) and journal implementations | model, ports |
| `cym-services` | The `Services` bundle, scope, traversal and identity rules | adapters |
| `cym-modules` | `ScanModule`, the registry, built-in modules, Git, simctl and process logic | services |
| `cym-store` | The result store, snapshots and totals | model |
| `cym-engine` | Scan coordination and reviewed actions | modules, store |
| `cym-core` | Re-exports everything as `cym_core::…`, the C ABI, the static library and the `cym` CLI | engine |

Editing a module recompiles `cym-modules`, `cym-engine` and `cym-core`; models, ports and adapters stay cached.

## Ports and adapters

Every platform dependency is a trait in `crates/cym-ports/src/ports.rs`, and scanners, safety checks and actions use only those traits. Default implementations live in `crates/cym-adapters` and are bundled by `Services` (`crates/cym-services`). Replacing one is a one-line change and needs no edits to modules or policy:

~~~rust
let services = Services { walker: Arc::new(JwalkWalker::default()), ..Services::native() };
let engine = Engine::new(services, modules::builtin());
~~~

| Port | Default adapter | Notes |
| --- | --- | --- |
| `FileSystem` | `StdFileSystem` | `lstat` metadata, children, resolve, remove, rename |
| `Walker` | `PrefetchWalker` | Depth-first, with the next folders listed ahead of time on the I/O pool; `StackWalker` is the sequential equivalent. Scope, exclusions, packages and limits are enforced by `Services::walk` |
| `Sizer` | `MetadataSizer` | Parallel (rayon), hard links counted once, order-independent folder fingerprint |
| `Hasher` | `Blake3Hasher` | `Sha256Hasher` is also provided; prefix hashing for staged duplicate detection |
| `CommandRunner` | `SystemRunner` | Allow-listed tools, argument arrays, clean environment, own process group, deadline, bounded output |
| `ProcessInspector` | `LibprocInspector` | libproc via `macos.m`; struct layout is checked at runtime |
| `Applications` | `NativeApplications` | NSWorkspace running apps, `Info.plist` bundle info |
| `Trash` | `NativeTrash` | FileManager's Trash operation |
| `Journal` | `JsonJournal` | Private, atomically written history; `MemoryJournal` for previews and tests |

Tests use the same seams. They run real adapters against generated fixtures, and inject fakes (`StubRunner`, `FakeProcesses`, `FixtureTrash`) where the real resource must not be touched.

## Modules

A module implements `ScanModule`. It declares a descriptor, streams findings to a `Sink`, lists the known locations its actions may touch (`action_roots`), and adds its own pre-action checks (`preflight`). The `Registry` holds modules by ID: built-ins can be replaced or removed and new ones registered without changing the engine, executor, CLI or app. Rules that are data (artifact evidence, lockfiles, cache locations) are tables on the module struct. See [module authoring](MODULES.md).

## Scans

`Engine::scan` runs modules concurrently and streams `ScanEvent`s (finding, warning, progress, module finished, then exactly one finished). Each module's output is deduplicated and capped at 30,000 findings. A failing module becomes a warning, so a broken integration never looks like an empty, successful scan. Progress events are throttled. Cancellation is cooperative through `ScanControl`, which every adapter checks.

Modules size their candidates in parallel and report them in order. Duplicate detection narrows candidates in stages: equal size, then an equal 64 KB prefix digest, then an equal full digest. Hard links to one inode are one copy.

## Actions

`actions::execute` normalizes overlapping selections, then for each item:

1. Checks eligibility and the request's current scope.
2. Runs the owning module's `preflight` (active tools, running apps, Git-tracked artifacts, duplicate rehash).
3. Revalidates protection, scope, symbolic-link ancestors and the item's fresh identity (folders by fingerprint).
4. Applies the action.
5. Journals the outcome before moving on.

Git removal re-reads the registration, HEAD and safety. Simulator actions require a fresh shut-down state for one specific UUID. Process signals require the exact instance (PID, UID, start time to the microsecond, executable), and Force Quit requires a graceful request at least two seconds old. Restore moves an unchanged Trash item back to its unoccupied original path.

## C ABI

`crates/cym-core/include/cym_core.h` is the versioned boundary (`CYM_ABI_VERSION`). Engines are explicit handles. `cym_call` takes `{"method", "params"}` and returns `{"ok", "result" | "error"}`. Every returned string is released with `cym_string_free`. `cym_scan_start` delivers events to a C callback and always ends with a single `finished` event, after which the context pointer is never used. `cym_scan_cancel` and `cym_scan_free` give per-scan cancellation. Panics never cross the boundary. Dates are Unix seconds.

In Swift, `CoreEngine` wraps this as typed calls, async actions and an `AsyncThrowingStream` of scan events, and decodes the internally tagged `Resource` with a small Codable adapter.

## Build

`scripts/build-core.sh [--universal]` builds `target/cym/libcym_core.a` (lipo for both architectures), and `Package.swift` links it through the `CCymCore` module map. `scripts/swift.sh` builds the core before invoking SwiftPM. CI runs rustfmt, clippy, cargo tests, the design lint, the Swift tests and a universal app build.

No privileged daemon, shell command interpolation, MacPaw dependency, background deletion, or automatic process force quit is used.
