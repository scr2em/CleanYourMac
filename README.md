<p align="center"><img src="docs/brand/logo.svg" width="120" alt="CleanYourMac leaf and sparkle logo"></p>

<h1 align="center">CleanYourMac</h1>

<p align="center"><strong>A little more room. A calmer Mac.</strong></p>

<p align="center">Free & open source · Private by default · Made for macOS</p>

Your Mac collects things you no longer need: oversized downloads, old project dependencies, forgotten worktrees, and simulator data. CleanYourMac helps you find them, understand what they are, and choose what goes.

**You’re in control.** Nothing is selected automatically. Inspect an item, select it, and review the consequences before cleaning up. Your scans and files stay on your Mac.

![CleanYourMac overview with storage totals and cleanup tools](docs/screenshots/overview.jpg)

*Real app screenshots using fictional demo data. Cleanup actions are disabled in the demo.*

## Make room for what’s next

| Find the clutter | Decide what to keep |
| --- | --- |
| **Big files & downloads** | Sort by size, search by name or path, and narrow results by size or age. |
| **Exact duplicates** | Find verified identical files while keeping an original. System, build, and dependency folders are skipped. |
| **Project dependencies & build outputs** | See which projects hold bulky dependencies (node_modules, Python environments, Pods, vendor folders and more) and generated files. Searches stop at each dependency folder. |
| **Git worktrees** | Inspect branches, local changes, locks, and sizes. Badges highlight orphaned registrations. |
| **Simulators & Xcode data** | Review simulator app data, DerivedData, and device support. Xcode archives stay protected. |
| **Toolchains, SDKs & caches** | Find installed language versions, SDK components, virtual devices, package caches, and logs. |
| **Apps & possible leftovers** | Review applications and their related files separately, with uncertain ownership clearly marked. |
| **Orphan processes** | Inspect suspected abandoned processes, their CPU use and memory, before choosing to quit them. |

Choose folders to search and exclude paths you want to protect. Each tool keeps its own results, and the totals below the search box follow your current filters. The overview brings your findings together without counting overlapping storage twice.

## Install with Homebrew

Requires macOS 14 or newer. The preview supports Apple silicon and Intel Macs.

```sh
brew tap scr2em/cleanyourmac https://github.com/scr2em/CleanYourMac
brew install --cask scr2em/cleanyourmac/cleanyourmac
```

Open **CleanYourMac** from Applications. To update later:

```sh
brew update
brew upgrade --cask scr2em/cleanyourmac/cleanyourmac
```

Prefer a direct download? Get the app from [GitHub Releases](https://github.com/scr2em/CleanYourMac/releases/tag/v0.1.0-preview.1), unzip it, and move it to Applications.

**Early preview:** the app is still evolving and isn’t Apple notarized yet. macOS may require approval on the first launch.

## Find it. Understand it. Review it.

1. **Choose a tool and scan.** Pick your folders and exclusions, then look for the kind of clutter you want to review.
2. **Explore the results.** Search by name or path, sort by size, and open an item to see its details.
3. **Review before cleanup.** Select exact items and check what the action will do. Permanent actions are identified explicitly.

Moving files to Trash leaves a recovery option; space is freed when Trash is emptied. Activity keeps a local record of cleanup outcomes and can restore unchanged items still in Trash.

### Search that keeps up with you

Dependency search, matching totals, size sorting, and project details in one place.

![Node dependency name search with aggregate totals and item details](docs/screenshots/node-dependencies.jpg)

### Know what’s inside a simulator

See the device, runtime, state, and app-data size before reviewing a reset or removal.

![Simulator app-data results and device details](docs/screenshots/simulators.jpg)

### Spot processes left behind

CPU, memory, process identity, and working folder help you make an informed choice.

![Orphan-process results with CPU and memory totals](docs/screenshots/orphan-processes.jpg)

## How each tool works

**Scan** on the overview runs every tool except Exact Duplicates, which reads every file and runs only when you open it. Results appear while the scan is still running. The overview then suggests a few one-click fixes:
- dependencies and build output of projects untouched for 30 days;
- caches that rebuild themselves;
- Xcode build data;
- duplicate copies, once you have scanned for them;
- emptying the Trash.

### Rules every tool follows

- **Nothing is selected for you.** Each item is checked again right before it is changed: still the same item, unchanged since the scan, still inside your chosen folders and outside your exclusions.
- **Some places are never offered:**
  - system folders;
  - your home, Library, Documents and Desktop folders themselves, and the Applications folders;
  - credentials such as `~/.ssh`, `~/.aws`, `~/.gnupg` and Keychains;
  - iCloud Drive and other cloud folders;
  - anything inside `.git` or named `.env`.

  An item whose size could not be fully read is listed but cannot be selected. Hover over a disabled item to see why.
- **Items in use are named.** When an app or tool is using an item, the app tells you which process holds it (name, PID, folder) and lets you go ahead anyway. Checks that protect your data cannot be skipped, such as files Git tracks or content that changed since the scan.
- **A failed item doesn't stop the batch.** The rest continue, and you see how many were done and why the others were not.
- **Most removals are reversible.** Moving to Trash can be undone from Activity while the item is unchanged in the Trash. Removing a worktree, resetting or deleting a simulator, emptying the Trash and quitting a process are permanent, and are marked that way.
- **Badges explain the cost.** *Rebuild* means the item comes back by itself or with one command. *Review* means look first. *Permanent* cannot be undone.

<details>
<summary><strong>Storage Explorer</strong>: what fills a folder</summary>

Lists each item directly inside the folders you choose, with its size on disk and when you last used it. Anything that isn't protected can be moved to the Trash after review. This is a map, not a recommendation.
</details>

<details>
<summary><strong>Large Files</strong>: files of 100 MB or more</summary>

Searches your chosen folders for files of at least 100 MB. Filter by size, age and last use, then move what you no longer need to the Trash.
</details>

<details>
<summary><strong>Exact Duplicates</strong>: verified identical files</summary>

Compares files of at least 4 KB:
1. by size;
2. then by the first 64 KB;
3. then by the full contents.

Hard links to the same file count once. Each group keeps one original, the first path in alphabetical order, which cannot be selected. Before a copy is moved, both it and the original are read again and must still match.

Version control, build output, dependency and package-cache folders are skipped. Not part of the overview scan.
</details>

<details>
<summary><strong>Dependencies</strong>: installed packages, by project</summary>

Finds each project's installed dependencies and stops at each one, never searching inside. A folder only counts when the project files that install it are present, never by its name alone. Each row shows its project and the files that prove it, plus the official command where one exists. Dependencies of a project whose tools are running (a dev server, a build) are reported as in use.

Many ecosystems keep downloaded packages in one shared store that every project uses, instead of inside the project. Those stores are listed here too, marked *Shared by all projects*. They also appear in Caches & Logs, and totals never count them twice.

| Ecosystem | In the project | Shared store |
| --- | --- | --- |
| Node.js (npm, pnpm, Yarn, Bun) | `node_modules` next to `package.json`, labelled with the package manager its lockfile names | npm cache, Yarn caches, pnpm store, Bun cache |
| Deno | `node_modules` next to `deno.json` | Deno cache |
| Python | `.venv`, `venv` or `env` containing `pyvenv.cfg`, in a project with `pyproject.toml`, `requirements.txt` or similar | pip, Poetry and conda package caches |
| Flutter / Dart | `ios/Pods`, `macos/Pods` | Pub hosted and Git packages |
| iOS & macOS | CocoaPods `Pods`, `Carthage/Build`, SwiftPM `.build/checkouts`, `repositories` and `artifacts` | CocoaPods, SwiftPM and Carthage caches |
| React Native, Capacitor | `ios/Pods`, `ios/App/Pods` | CocoaPods cache |
| Rust | `vendor` made by `cargo vendor` | Cargo registry and Git checkouts |
| Go | `vendor` containing `modules.txt` | Go module cache (shown only: Go makes it read-only, so use `go clean -modcache`) |
| Ruby | `vendor/bundle` | Bundler cache |
| PHP | Composer `vendor` | Composer cache |
| Java, Kotlin, Scala | none: Maven and Gradle keep nothing per project | Maven repository, Gradle caches, Ivy, Coursier |
| .NET | none | NuGet packages |
| Elixir | `deps` | Hex packages |
| Others | Terraform `.terraform`, Yarn `.yarn/unplugged`, Bower `bower_components`, renv `renv/library` | |

Go, Cargo and Composer `vendor` folders that Git tracks are left out, because they are part of the project's source. Folders that package managers and version managers keep for themselves are never searched. That covers pnpm's store, npm's `_npx`, and global installs from nvm, fnm, Volta, asdf and mise.
</details>

<details>
<summary><strong>Build Artifacts</strong>: generated output, by project</summary>

Finds folders a build creates, each proven by the project file that produces it:
- **JavaScript:** Next.js `.next`; Nuxt `.nuxt` and `.output`; SvelteKit, Angular, Docusaurus, Nx, Turborepo, Storybook, Parcel and coverage reports.
- **Mobile:** React Native, Expo, Capacitor and Flutter build folders.
- **Apple:** SwiftPM `.build` (without its fetched packages) and project-local `DerivedData`.
- **Rust, JVM and native:** Cargo `target`; Maven, sbt and Gradle; Zig; CMake build trees, which must contain `CMakeCache.txt`.
- **.NET:** `bin` and `obj`.
- **Game engines:** Unity and Unreal.
- **Python:** test and tool caches, `__pycache__` and `*.egg-info`.
- **Other languages:** Elixir `_build`, Haskell, Elm.

Everything rebuilds with the next build, except Unreal's `Saved` folder (autosaves and crash logs), which is marked *Review*. The owning IDE must be closed, and folders containing files Git tracks are refused. Each row shows the tool's own clean command when there is one.
</details>

<details>
<summary><strong>Git Worktrees</strong>: linked worktrees and their state</summary>

Lists the linked worktrees of repositories in your chosen folders, with branch, size, last commit and local changes. Removing one uses `git worktree remove` and keeps the branch.

Removal is blocked when the worktree:
- is locked or prunable;
- has submodules;
- has uncommitted or untracked changes;
- is on a detached commit that no branch or tag contains.

Ignored files, which are deleted with it, are listed first. A registration whose folder is gone is badged *Orphaned* and is only shown, never pruned.
</details>

<details>
<summary><strong>Simulators</strong>: devices and runtimes</summary>

Reads Xcode's simulator list: each device's runtime, state, app-data size and when it was last booted. Reset and Delete are offered only for a shut-down device. Runtimes are shown for information; manage them in Xcode.
</details>

<details>
<summary><strong>Xcode Data</strong>: DerivedData, device support, archives</summary>

Lists each folder in DerivedData and iOS DeviceSupport. Both rebuild by themselves, and DerivedData shows when Xcode last opened it.

Archives are protected. An archive may be the only copy of a build you shipped, with the debug symbols needed to read its crash reports. You can still move one to the Trash from its details, after confirming that you accept that loss. Nothing is moved while Xcode, `xcodebuild` or the Swift compiler is running.
</details>

<details>
<summary><strong>Caches & Logs</strong>: caches, logs and Mac leftovers</summary>

- **Developer caches:** JavaScript, Python, Apple, Flutter, JVM and Android, .NET, Rust, Go, PHP, Ruby, Elixir, Homebrew, JetBrains, VS Code and Unity, each with what removing it costs and the tool's own clean command.
- **`~/.cache`:** one item, at *Review*, listing what's inside. It warns when it holds a Hugging Face sign-in token.
- **`~/Library/Logs`:** each app's logs, at *Review*.
- **Mac leftovers:**
  - app caches, including sandboxed apps';
  - saved window state;
  - Mail downloads;
  - iPhone and iPad software updates;
  - device backups, at *Review*.

  Apple's own caches are marked *Review*, and each app must be closed before its cache is moved.
</details>

<details>
<summary><strong>Toolchains & SDKs</strong>: installed language versions</summary>

Lists each installed version, with its size and the manager's own uninstall command:
- **Language version managers:** rustup, nvm, fnm, Volta, pyenv, rbenv and FVM.
- **Swift toolchains.**
- **Multi-tool managers:** SDKMAN, asdf and mise.
- **Android:** system images, NDK, build tools and virtual devices.

A version currently in use cannot be selected: set as the default or global version, pinned in `.tool-versions` or a mise config, or used by an Android virtual device. A virtual device is blocked only while its emulator is running.
</details>

<details>
<summary><strong>Applications</strong> and <strong>App Leftovers</strong></summary>

**Applications** lists apps in `/Applications` and `~/Applications`. Each app's caches, preferences, saved state and Application Support folder are separate rows, so you choose exactly what goes. Apple's apps are protected, and a running app or its files cannot be moved.

**App Leftovers** lists caches, preferences and saved state in your Library named after apps that are no longer installed. Ownership is a guess from the name, so every leftover is marked *Review*.
</details>

<details>
<summary><strong>Downloads</strong> and <strong>Trash</strong></summary>

**Downloads** lists each item in your Downloads folder with when you last opened it. **Trash** lists what's already in your Trash; emptying it is permanent.
</details>

<details>
<summary><strong>Orphan Processes</strong>: programs left running</summary>

Lists your processes whose parent has exited. It leaves out:
- processes macOS manages (launchd jobs);
- apps and their helpers;
- system processes;
- processes outside your folders, when you limit it to them.

Rows show CPU, memory, working folder and executable. Quit asks the process to exit; Force Quit is offered only after that, and the process's identity is checked again each time.
</details>

## A workspace that feels comfortable

Paper & Walnut is the default palette: light paper surfaces and warm brown accents. Prefer another mood? Choose Honey & Espresso, Ember, Terracotta & Navy, Cream & Mauve, or Linen & Rose in Settings. Light and dark appearances share the same design system.

## Free, independent, and open

CleanYourMac is [MIT licensed](LICENSE). No subscription is required. It is independently built and uses no MacPaw software or services. Orphan-process inspection draws on the owner’s MIT-licensed OrphanBar project; see [acknowledgments](THIRD_PARTY_NOTICES.md).

Have an idea or found something that needs fixing? [Open an issue](https://github.com/scr2em/CleanYourMac/issues).

Want to contribute? Start with the [development guide](docs/DEVELOPMENT.md), [architecture](docs/ARCHITECTURE.md), and [module guide](docs/MODULES.md).
