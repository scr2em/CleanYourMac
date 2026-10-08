# Adding a module

CleanYourMac uses compiled, trusted Rust modules. It does not load arbitrary binary plugins or execute rule scripts. A finder can be added without modifying the engine, the executor, the CLI or the shared SwiftUI screens: the app and CLI read descriptors from the registry.

## Layout

| Path | Owns |
| --- | --- |
| `crates/cym-ports/src/ports.rs` | Replaceable boundaries: filesystem, walker, sizer, hasher, commands, processes, apps, usage, Trash, journal, sink |
| `crates/cym-adapters` | Default implementations of the ports |
| `crates/cym-services` | The adapter bundle plus scope, traversal and identity rules |
| `crates/cym-modules/src/modules/` | `ScanModule`, `Registry`, built-in modules |
| `crates/cym-engine`, `crates/cym-store`, `crates/cym-model` | Executor, result store and totals, models and path policy |
| `crates/cym-core/src/ffi.rs`, `crates/cym-core/include/cym_core.h` | Versioned C ABI |
| `Sources/CleanYourMacCore` | Swift wire models and `CoreEngine` bridge |
| `Sources/CleanYourMacDesignSystem`, `Sources/CleanYourMacUI` | Tokens, components and presentation only |

## A discovery-only module

~~~rust
use cym_core::{model::*, modules::ScanModule, ports::*, Services};

pub struct ExampleModule;
impl ScanModule for ExampleModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor::new("example", "Example", "Developer", "folder",
            "Inspect a documented kind of generated data.", true)
    }
    fn scan(&self, s: &Services, c: &ScanContext, k: &ScanControl, sink: &mut dyn Sink) -> Result<()> {
        let mut warnings = vec![];
        s.walk(c, k, &mut warnings, &mut |e| {
            if e.directory && e.name() == ".example-cache" {
                sink.finding(s.file_finding(e, "example", None, "Regenerated on demand.", vec![], Risk::Rebuild, k)?);
                return Ok(false);
            }
            Ok(true)
        })?;
        for w in warnings { sink.warning(w); }
        Ok(())
    }
}

let registry = cym_core::modules::builtin().register(std::sync::Arc::new(ExampleModule));
~~~

`register` replaces a module with the same ID in place, and `remove` drops one. Category is one of Storage, Developer, Applications or Tools. `symbol` is an SF Symbol name.

Use `Services::walk` for chosen-root traversal: it enforces exclusions, system locations, package boundaries and the entry limit around whichever `Walker` is configured. Use `Services::file_finding`, or the parallel `add_files` helper inside the crate, to capture size, coverage, a fresh identity and a folder fingerprint. Never follow symbolic links. Preserve the owning project and show evidence rather than inferring that a folder name is disposable. Express rule data as tables on the module struct so it can be extended without code changes.

Build-artifact rules (`ArtifactRule`) match a folder name and require at least one evidence file in the owning project folder. A name containing `/` describes a nested layout, such as `ios/Pods` or `android/app/build`, and its evidence is read from the folder above the first component. Names and evidence may use one `*` per component (`cmake-build-*`, `*.csproj`, `svelte.config.*`), and evidence ending in `/` must be a folder (`*.xcodeproj/`). `requires` lists files that must also exist in the project folder, `beside` files in the folder that directly contains the artifact (`Podfile.lock` next to `ios/Pods`), and `inside` files in the artifact itself (`pyvenv.cfg`, or `CMakeCache.txt` for a CMake build tree). A folder name is never evidence on its own. Installed dependencies use the same rules (`dependency_rules`): the Dependencies module lists them next to `node_modules`, and Build Artifacts neither lists nor searches inside them. The Dependencies module keeps the ID `node` so saved settings and history still match. It also lists the shared package stores that the Caches & Logs table marks with `package_store()`, and never searches inside them. Rules marked `hide_tracked()` (Go, Cargo and Composer `vendor`) leave out folders Git tracks, since those are part of the project's source. Tests build modules with `common::builtin(&fixture)`, which points every home-relative table at the fixture, so a test can never act on the real home folder. Rules, caches and toolchains can name the tool's official clean or uninstall command, shown as an "Official command" detail, and the bundle-identifier prefixes of apps that must be closed before acting. Cache and toolchain tables take an optional `home` so tests can use a fixture home folder instead of changing `$HOME`.

Report warnings for unavailable tools, permission denial, coverage limits and skipped items. Return an error when the integration fails; the engine turns it into a warning. Check `ScanControl` inside loops.

## Actions

A finding names a typed `Resource` and declares `ActionKind` values. An empty action list makes it discovery-only. `blocked_reason` explains why an item is ineligible. Findings start unselected in the app.

File actions use the shared Trash path. A module that acts outside the user's chosen roots declares those locations in `action_roots`, and adds its own pre-action checks in `preflight`, for example active tools, running apps, Git-tracked files or rehashing. The executor still normalizes overlapping selections, rechecks current exclusions and scope, revalidates identities and records an outcome for each item. A new destructive operation needs:

- a typed action;
- a resource-specific adapter behind a port;
- a consequence and recovery description in the Swift `ActionKind`;
- fresh validation;
- meaningful tests.

Do not add arbitrary command strings or route mutations through `scan`.

An unchanged item in Trash can be restored to its original unoccupied location. Git worktree removal, simulator actions, permanent Trash removal and process termination have no app-level undo.

## Replacing an implementation

Implement the port and swap it into `Services`:

~~~rust
struct JwalkWalker;
impl Walker for JwalkWalker { /* honor Visit::Skip/Stop; never follow links */ }
let services = Services { walker: Arc::new(JwalkWalker), ..Services::native() };
~~~

The sizer's folder signature format belongs to the implementation. Use the same `Services` for scanning and for actions.

## Visual additions

A finding's row icon is its ecosystem's logo when `cym_model::brand` names one (from the module, the "Ecosystem" detail or the package manager), its Finder icon for plain files, folders and apps, and otherwise the module's SF Symbol. Logos are single-colour 24×24 SVG files in `Sources/CleanYourMacDesignSystem/BrandIcons`, named by slug, with the brand colour as the root `fill`. To add one, map the ecosystem in `brand.rs` and drop `<slug>.svg` into the folder, or run `scripts/update-brand-icons.py` against a Simple Icons package. A test fails if a slug has no file.

Use the shared Finder, Inspector, Action Review, Activity and Settings patterns. Feature backends cannot return custom views. Reuse Space, Layout, TypeStyle, Palette and approved components. A new component belongs in CleanYourMacDesignSystem, with gallery examples, semantic variants, keyboard and accessibility behavior, and a light and dark review. `scripts/lint-design.py` rejects raw feature-local colors, font sizes, corner radii, padding and frame dimensions.

## Required verification

Add Rust tests under `crates/cym-core/tests` with fixtures from `tests/common`. They should cover positive identification, ambiguous ownership, exclusions, changed resources, incomplete coverage and cancellation where they apply. Inject `StubRunner` for tool output, `FakeProcesses` for process tables and `FixtureTrash` so tests never write to the user's Trash. Real Git tests create and remove only fixture repositories. Native tests (`tests/native.rs`) are opt-in through `scripts/test-native.sh` and touch only resources they create. Keep process commands and personal paths out of committed fixtures and screenshots.
