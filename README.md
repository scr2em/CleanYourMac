<p align="center"><img src="docs/brand/logo.svg" width="120" alt="CleanYourMac leaf and sparkle logo"></p>

<h1 align="center">CleanYourMac</h1>

<p align="center"><strong>Find what uses space on your Mac. Remove only what you select.</strong></p>

<p align="center">Free and open source · Private · For macOS</p>

CleanYourMac finds items that use disk space on your Mac. Examples are large downloads, old project dependencies, Git worktrees and simulator data. The app shows what each item is. You select the items to remove.

**You control all changes.** The app does not select items for you. Before an action starts, you review the items and the result of the action. Your scan results and your files stay on your Mac.

![CleanYourMac overview: storage totals and the recommended fixes](docs/screenshots/overview.png)

- **Made for developers.** It knows `node_modules`, Python environments, Cargo `target`, DerivedData, simulators, toolchains, Git worktrees and Docker. It shows the project that owns each item.
- **Cleans up after AI tools.** It sorts the caches, transcripts and models of Claude Code, Codex, Gemini CLI, Cursor, Ollama and 20 more tools into Safe, Review and Caution tiers.
- **Safe by design.** Items go to the Trash. The app examines each item again before it acts, and it does not touch credentials, settings or files that Git tracks.
- **Fast.** A Rust core reads the disk in parallel and keeps a million results searchable.

*The screenshots show the app design with example data.*

## What the app finds

| Item type | What you can do |
| --- | --- |
| **Large files and downloads** | Sort by size. Search by name or path. Filter by size and age. |
| **Exact duplicates** | Find identical files. The app keeps one original of each file. The app does not search system, build or dependency folders. |
| **Project dependencies and build output** | See the projects that have large dependency folders (`node_modules`, Python environments, Pods, `vendor` and more) and generated files. The search stops at each dependency folder. |
| **Git worktrees** | See the branch, local changes, locks and size of each worktree. A badge shows registrations that have no folder. |
| **Idle projects and Git data** | Hibernate a project that you have not changed for 180 days into a checked `.zip` file. Pack the loose objects of a repository with `git gc`. |
| **System Data** | Remove Time Machine local snapshots. See how much space Apple Intelligence models, macOS update downloads, the GarageBand and Logic sound library and aerial videos use, and where macOS lets you remove them. |
| **Simulators and Xcode data** | See simulator app data, runtimes, DerivedData, device support, preview simulators and extra Xcode installs. Xcode archives are protected. |
| **Containers and virtual machines** | See the real size of the disks of Docker, OrbStack, Colima, Lima and Podman, and of UTM, Parallels, VMware Fusion, VirtualBox and Tart virtual machines. Free space inside the engines with their own cleanup commands. |
| **Installers** | Find macOS installers, Xcode archives, disk images and packages that you already installed from. |
| **Toolchains, SDKs and caches** | Find installed language versions, SDK components, virtual devices, package caches and logs. Remove old Homebrew versions and unused Nix store paths with their own commands. |
| **AI tools** | Find the caches, session transcripts and downloaded models of Claude Code, Codex, Gemini CLI, Cursor, Ollama and other AI tools. Each item has a risk tier. |
| **Applications and possible leftovers** | See each application and its related files as separate items. The app shows when the owner of a file is not certain. |
| **Orphan processes** | See processes that their parent left running, with CPU and memory use. Then you can quit them. |

Select the folders to scan, or scan the whole Mac. Add exclusions for paths that you want to keep. Each tool keeps its own results. The totals below the search field show only the items that agree with your filters. The overview adds the results of all tools. It counts storage that two items share one time only.

## Install with Homebrew

The app needs macOS 14 or later. The preview operates on Mac computers with Apple silicon and with Intel processors.

```sh
brew tap scr2em/cleanyourmac https://github.com/scr2em/CleanYourMac
brew install --cask scr2em/cleanyourmac/cleanyourmac
```

Open **CleanYourMac** from the Applications folder. To update the app, do these commands:

```sh
brew update
brew upgrade --cask scr2em/cleanyourmac/cleanyourmac
```

You can also download the app from [GitHub Releases](https://github.com/scr2em/CleanYourMac/releases/tag/v0.1.0-preview.1). Unzip the file. Then move the app to the Applications folder.

**Preview release:** The app is not notarized by Apple at this time. macOS can ask for your approval when you open the app for the first time.

## How to use the app

1. **Select a tool and scan.** Select the folders to scan and the paths to exclude. Then start the scan.
2. **Examine the results.** Search by name or path. Sort by size. Click an item to see its details.
3. **Review before you remove.** Select the items. Read what the action will do. The app identifies permanent actions.

When you move an item to the Trash, you can restore it. The disk space becomes free when you empty the Trash. The Activity page keeps a local record of each action. From Activity, you can restore an item that is in the Trash and that has not changed.

## A tour of the app

<table>
<tr>
<td width="50%"><img src="docs/screenshots/ai-tools.png" alt="AI Tools page with Claude Code, Ollama, Cursor and Gemini items in Safe, Review and Caution tiers"><br><strong>AI Tools.</strong> Transcripts, caches and models of AI tools, each with a risk tier and the project of each session.</td>
<td width="50%"><img src="docs/screenshots/ai-tools-dark.png" alt="AI Tools page in dark appearance"><br><strong>Light and dark.</strong> Every page follows the appearance of your Mac, in seven palettes.</td>
</tr>
<tr>
<td><img src="docs/screenshots/dependencies.png" alt="Dependencies page with node_modules, a Python environment, Pods and the Cargo registry"><br><strong>Dependencies.</strong> Installed packages of each project, and the shared stores that all projects use.</td>
<td><img src="docs/screenshots/build-artifacts.png" alt="Build Artifacts page with Cargo, Xcode, Flutter, Gradle and Next.js output"><br><strong>Build Artifacts.</strong> Output that the next build makes again, identified by the files of its project.</td>
</tr>
<tr>
<td><img src="docs/screenshots/git-worktrees.png" alt="Git Worktrees page with linked, locked and orphaned worktrees"><br><strong>Git Worktrees.</strong> Branch, state and size of each worktree. Worktrees with local changes stay.</td>
<td><img src="docs/screenshots/xcode-data.png" alt="Xcode Data page with DerivedData, device support and a protected archive"><br><strong>Xcode Data.</strong> DerivedData and device support. Archives are protected.</td>
</tr>
<tr>
<td><img src="docs/screenshots/simulators.png" alt="Simulators page with devices and an installed runtime"><br><strong>Simulators.</strong> App data of each device, with its runtime and the date of the last boot.</td>
<td><img src="docs/screenshots/toolchains.png" alt="Toolchains page with Swift, Android, Rust, Python and Node.js versions"><br><strong>Toolchains &amp; SDKs.</strong> Installed language versions. The version in use stays.</td>
</tr>
<tr>
<td><img src="docs/screenshots/containers.png" alt="Containers and VMs page with Docker disks, build cache, unused images and volumes"><br><strong>Containers &amp; VMs.</strong> The real size of Docker, OrbStack, Colima and Podman disks, and their own cleanup commands.</td>
<td><img src="docs/screenshots/installers.png" alt="Installers page with a macOS installer, an Xcode archive and disk images"><br><strong>Installers.</strong> macOS installers, Xcode archives, disk images and packages that you already used.</td>
</tr>
<tr>
<td><img src="docs/screenshots/caches-logs.png" alt="Caches and Logs page with Gradle, Homebrew, Cargo and pip caches"><br><strong>Caches &amp; Logs.</strong> Caches that tools download or make again, with the cost of each removal.</td>
<td><img src="docs/screenshots/large-files.png" alt="Large Files page with installers, disk images and videos"><br><strong>Large Files.</strong> Files of 100 MB or more, with size, age and last-use filters.</td>
</tr>
<tr>
<td><img src="docs/screenshots/exact-duplicates.png" alt="Exact Duplicates page with originals and duplicate copies"><br><strong>Exact Duplicates.</strong> Verified identical files. The original of each group stays.</td>
<td><img src="docs/screenshots/storage-explorer.png" alt="Storage Explorer page with the folders of the home folder by size"><br><strong>Storage Explorer.</strong> What uses space in a folder, with the date of last use.</td>
</tr>
<tr>
<td><img src="docs/screenshots/downloads.png" alt="Downloads page with installers and documents"><br><strong>Downloads.</strong> Installers and files that you downloaded, with the date that you last opened them.</td>
<td><img src="docs/screenshots/trash.png" alt="Trash page with items selected to empty"><br><strong>Trash.</strong> The exact items in your Trash, before you empty it.</td>
</tr>
<tr>
<td><img src="docs/screenshots/applications.png" alt="Applications page with apps and their related files as separate items"><br><strong>Applications.</strong> Each app and its related files, as separate items. Running and Apple apps are protected.</td>
<td><img src="docs/screenshots/app-leftovers.png" alt="App Leftovers page with files of apps that are not installed"><br><strong>App Leftovers.</strong> Files that have the name of an app that is not installed.</td>
</tr>
<tr>
<td><img src="docs/screenshots/orphan-processes.png" alt="Orphan Processes page with node, esbuild, python and ruby processes"><br><strong>Orphan Processes.</strong> Processes that their parent left running, with CPU and memory use.</td>
<td><img src="docs/screenshots/overview.png" alt="Overview page with recommended fixes"><br><strong>Overview.</strong> One scan for all tools, and the fixes that free the most space.</td>
</tr>
</table>

## How each tool operates

**Scan** on the overview starts all tools except Exact Duplicates. Exact Duplicates reads the contents of all files, so it starts only when you open it. Results show while the scan continues. After the scan, the overview recommends these actions:
- Remove the dependencies and build output of projects that you did not use for 30 days or more.
- Remove caches that the tools make again when necessary.
- Remove Xcode build data.
- Empty the Trash.

### Rules for all tools

- **The app does not select items for you.** Immediately before an action, the app examines each item again. The item must be the same item, it must not have changed since the scan, it must be in the folders to scan, and it must not be excluded.
- **The app never shows these locations as items:**
  - system folders;
  - your home, Library, Documents and Desktop folders, and the Applications folders;
  - credentials, for example `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.kube`, `~/.docker`, `~/.config`, `~/.netrc`, `~/.npmrc` and Keychains;
  - iCloud Drive and other cloud folders;
  - version-control data of every system: Git, Mercurial, Sapling, Subversion, Jujutsu, Bazaar, Darcs, Pijul, Fossil, CVS, RCS, SCCS, BitKeeper, Monotone, GNU Arch, Plastic SCM, Team Foundation, Google `repo` and DVC folders, and Fossil and Monotone repository files;
  - all files with the name `.env`.

  The app shows an item but does not let you select it when it cannot read the full size. Put the pointer on a disabled item to see the reason.
- **The app shows what uses an item.** When an app or a tool uses an item, the app shows the process (name, PID and folder). You can then continue the action. You cannot skip checks that protect your data. For example, the app does not remove files that Git tracks, items inside another version-control system's checkout, or items that changed after the scan.
- **One failure does not stop the action.** When an item fails, the app continues with the other items. Then it shows how many items it changed and why the other items failed.
- **The owning app must be closed.** The app does not remove an item while the app it belongs to runs, such as an editor's cache while the editor is open. When only developer tools are running, it warns you and you can continue.
- **You can undo most actions.** You can restore an item that you moved to the Trash from Activity, if the item did not change. These actions are permanent: remove a worktree, reset or delete a simulator, run the cleanup command of a tool, empty the Trash and quit a process. The app identifies them.
- **A tool's own command does the work where the Trash cannot.** For Docker, Podman, simulator runtimes and SwiftUI preview simulators, the app runs the cleanup command of the tool (for example `docker builder prune`). The app chooses the command from a fixed list and shows it on the item. Nothing goes to the Trash, so you cannot restore it.
- **Badges show the cost of an action.** *Rebuild* means that the item comes back automatically or with one command. *Review* means that you must examine the item first. *Permanent* means that you cannot undo the action.

<details>
<summary><strong>Storage Explorer</strong>: what uses space in a folder</summary>

Storage Explorer shows each item directly in the folders that you select. For each item, it shows the size on disk and the date of last use. You can move an item that is not protected to the Trash. The folders directly in your Library folder (Application Support, Containers, Mail and others) hold macOS and app data as a whole, so you can open them but not remove them. Storage Explorer shows where space goes. It does not recommend items.
</details>

<details>
<summary><strong>Large Files</strong>: files of 100 MB or more</summary>

Large Files finds files of 100 MB or more in the folders that you select. Filter by size, age and last use. Then move the files that you do not need to the Trash.

Files in your Library folder and in hidden folders of your home folder belong to an app or a tool, for example a virtual machine disk, a model, a device backup or a database. Large Files shows them, but you cannot select them. Remove them with their app or tool. Version-control data (`.git`, `.hg`, `.svn`, `.jj` and the other systems above) is never searched, on any drive: its files belong to the repository. A large file that Git tracks, for example a model or a test asset, stays. Large Files does not search build output or installed packages (for example `target` next to a `Cargo.toml`, or `node_modules` next to a `package.json`): removing one file from them breaks the build, so Build Artifacts and Dependencies offer the whole folder instead. A folder that only has such a name, without the project files, is searched.
</details>

<details>
<summary><strong>Exact Duplicates</strong>: identical files</summary>

Exact Duplicates compares files of 4 KB or more in three steps:
1. It compares the size.
2. It compares the first 64 KB.
3. It compares the full contents.

Hard links to one file count as one file. Each group keeps one original, and you cannot select it. The original is the copy in the most deliberate place: Documents, Desktop, Pictures, Movies or Music first, then other folders, then Downloads. A copy in the Trash, a cache or a hidden folder is never the original. When every copy is in such a place, the app keeps them all. Before the app moves a copy, it reads the copy and the original again. They must still be identical, and Git must not track the copy.

Exact Duplicates does not search version control, build output, dependency or package-cache folders, hidden folders or Library folders. It recognises build output and dependencies the way Build Artifacts and Dependencies do, from the project's files: a folder named `build` or `dist` is skipped next to a project file, but your own `Documents/Build` folder is compared. It is not part of the overview scan, and duplicates are never part of the one-click fixes on the overview.
</details>

<details>
<summary><strong>Dependencies</strong>: installed packages, by project</summary>

Dependencies finds the installed packages of each project. The search stops at each dependency folder and does not search in it. The name of a folder is not sufficient. The project files that install the folder must also be present. Each item shows its project, the files that identify it and, if available, the official command of the tool. When a tool of the project runs (for example, a development server or a build), the app shows the item as in use. Dependencies does not search your Library folder or the hidden folders in your home folder, where apps and editors keep their own packages: the extensions of VS Code, Cursor, Windsurf and other VS Code editors are never offered for removal. It does not search build output either: packages inside it, such as `.next/standalone/node_modules`, are part of the build, and Build Artifacts offers it.

Many ecosystems keep downloaded packages in one shared store that all projects use. Dependencies also shows these stores, with the label *Shared by all projects*. Caches & Logs shows the same stores. The totals count each store one time only.

| Ecosystem | In the project | Shared store |
| --- | --- | --- |
| Node.js (npm, pnpm, Yarn, Bun) | `node_modules` next to `package.json`. The lockfile identifies the package manager. | npm cache, Yarn caches, pnpm store, Bun cache |
| Deno | `node_modules` next to `deno.json` | Deno cache |
| Python | `.venv`, `venv` or `env` that contains `pyvenv.cfg`, in a project with `pyproject.toml`, `requirements.txt` or a similar file | pip, Poetry and conda package caches |
| Flutter / Dart | `ios/Pods`, `macos/Pods` | Pub hosted packages and Git packages |
| iOS and macOS | CocoaPods `Pods`, `Carthage/Build`, SwiftPM `.build/checkouts`, `repositories` and `artifacts` | CocoaPods, SwiftPM and Carthage caches |
| React Native, Capacitor | `ios/Pods`, `ios/App/Pods` | CocoaPods cache |
| Rust | `vendor` from `cargo vendor` | Cargo registry and Git checkouts |
| Go | `vendor` that contains `modules.txt` | Go module cache. The app only shows it, because Go makes it read-only. Use `go clean -modcache`. |
| Ruby | `vendor/bundle` | Bundler cache |
| PHP | Composer `vendor` | Composer cache |
| Java, Kotlin, Scala | None. Maven and Gradle keep no packages in the project. | Maven repository, Gradle caches, Ivy, Coursier |
| .NET | None | NuGet packages |
| Elixir | `deps` | Hex packages |
| Other | Terraform `.terraform`, Yarn `.yarn/unplugged`, Bower `bower_components`, renv `renv/library` | |

The app does not show Go, Cargo and Composer `vendor` folders that Git tracks, because they are part of the source of the project. The app does not search the folders that package managers and version managers keep for their own use. Examples are the pnpm store, the npm `_npx` folder and the global installations of nvm, fnm, Volta, asdf and mise.
</details>

<details>
<summary><strong>Build Artifacts</strong>: generated output, by project</summary>

Build Artifacts finds folders that a build makes. A project file must identify each folder:
- **JavaScript:** Next.js `.next`; Nuxt `.nuxt` and `.output`; SvelteKit, Angular, Docusaurus, Nx, Turborepo, Storybook, Parcel and coverage reports.
- **Mobile:** React Native, Expo, Capacitor and Flutter build folders.
- **Apple:** SwiftPM `.build` (without the downloaded packages) and `DerivedData` in a project.
- **Rust, JVM and native code:** Cargo `target`; Maven, sbt and Gradle; Zig; CMake build trees that contain `CMakeCache.txt`.
- **.NET:** `bin` and `obj`.
- **Game engines:** Unity and Unreal.
- **Python:** test and tool caches, `__pycache__` and `*.egg-info`.
- **Other languages:** Elixir `_build`, Haskell, Elm.

The next build makes all of these folders again. The exception is the Unreal `Saved` folder (autosaves and crash logs), which has the *Review* badge. A build folder that holds a shipped build or its crash symbols (an `.xcarchive`, `.dSYM`, `.ipa`, `.aab` or an Android `outputs/mapping` folder) is protected like an Xcode archive: you can remove it only from its details, after you confirm. You must close the IDE of the project before an action. The app does not remove folders that contain files that Git tracks. Inside a checkout of another version-control system (Mercurial, Subversion, Jujutsu without Git, and others) it removes nothing, because it asks only Git whether an item is tracked: running another system's tool could run commands that the repository configures. If the tool has its own clean command, each item shows it.
</details>

<details>
<summary><strong>Git Worktrees</strong>: linked worktrees and their state</summary>

Git Worktrees shows the linked worktrees of the repositories in the folders that you select. For each worktree, it shows the branch, size, last commit and local changes. To remove a worktree, the app uses `git worktree remove`. The branch stays.

The app does not remove a worktree when:
- the worktree is locked or prunable;
- the worktree has submodules;
- the worktree has uncommitted or untracked changes;
- the worktree is on a detached commit that no branch or tag contains.

The app shows the ignored files first, because the removal deletes them. A registration that has no folder has the *Orphaned* badge. The app only shows it and does not prune it.
</details>

<details>
<summary><strong>Projects</strong>: idle projects and unpacked Git data</summary>

Projects shows the Git projects in the folders that you select. It does not look in `~/Library` or in hidden folders of your home folder, where apps and tools keep their own repositories.

- **Idle projects:** a project without a commit or a change to its files for 180 days. *Hibernate Project* compresses the whole folder, with its Git history and uncommitted work, into `<name>.zip` next to it (`ditto`). The app checks the archive (`unzip -t`), and then moves the folder to the Trash. To get the project back, open the `.zip` file in Finder, or restore the folder from Activity while it is in the Trash. The app does not hibernate a project when a `.zip` file with that name exists, or while Git or a developer tool works in it. Remove the dependencies and build output first to make the archive smaller.
- **Unpacked Git objects:** a repository with 100 MB or more of loose or temporary objects. *Run Cleanup* runs `git gc`, which packs them. Git keeps unreachable objects for two weeks, and branches, stashes and reflogs stay. The app does not run Git in a repository whose settings run programs.
</details>

<details>
<summary><strong>Simulators</strong>: devices and runtimes</summary>

Simulators reads the simulator list of Xcode. For each device, it shows the runtime, state, app-data size and the date of the last boot. You can reset or delete a device only when it is shut down. A device whose runtime is not installed has the *Unavailable* badge.

Each downloaded runtime shows its size and the date of last use. The app deletes a runtime with `xcrun simctl runtime delete`, because macOS protects the runtime files. The app does not delete a runtime while a simulator of it runs. Runtimes that come with Xcode are for information only.
</details>

<details>
<summary><strong>Xcode Data</strong>: DerivedData, device support and archives</summary>

Xcode Data shows:
- each folder in DerivedData, with the date that Xcode last opened it;
- device support for iOS, watchOS, tvOS, visionOS and macOS;
- the documentation cache and index, and the shared simulator caches;
- device logs, with the *Review* badge;
- the data of apps removed from a simulator that the simulator did not clean up (`Dead` folders), with the *Review* badge;
- SwiftUI preview simulators. The app removes them with `xcrun simctl --set previews delete all`;
- Xcode installations other than the one that `xcode-select` selects. The selected Xcode stays. When `xcode-select` selects the Command Line Tools, the newest Xcode stays.

Xcode makes all of these again when necessary, except device logs and other Xcode versions.

Archives are protected. An archive can be the only copy of a build that you shipped. It also contains the debug symbols that you need to read its crash reports. You can move an archive to the Trash from its details, after you confirm that you accept this loss. The app does not move items while Xcode, Simulator, `xcodebuild` or the Swift compiler runs.
</details>

<details>
<summary><strong>Caches & Logs</strong>: caches, logs and Mac leftovers</summary>

- **Developer caches:** JavaScript (also Puppeteer browsers), Python (also uv and pre-commit), Apple (also CocoaPods spec repos), Flutter, JVM and Android (also Gradle JDKs), .NET, Rust, C and C++ (ccache, Bazel), Go, PHP, Ruby, Elixir, Homebrew, JetBrains, VS Code, Unity, and the image caches of Tart, Vagrant and minikube. Each item shows the cost of its removal and the clean command of the tool.
- **Browser caches:** the script, GPU and offline caches of each Chrome, Edge and Brave profile, and the on-device AI model of Chrome. History, cookies, passwords and local storage stay.
- **App caches:** the Steam shader and web caches and unfinished downloads, and the Adobe media cache.
- **For information only:** the iCloud Drive sync cache, the Google Drive offline files and cache, and Messages attachments. The app shows their size and tells you how to free space in their app. It does not remove them, because they can hold changes that are not uploaded, or leave broken messages.
- **`~/.cache`:** one item with the *Review* badge. It shows what the folder contains. It shows a warning when the folder contains a Hugging Face sign-in token.
- **`~/Library/Logs`:** the logs of each app, with the *Review* badge.
- **Mac leftovers:**
  - app caches, also of sandboxed apps;
  - saved window state;
  - Mail downloads;
  - iPhone and iPad software updates;
  - device backups, with the *Review* badge;
  - the Chromium caches of Electron apps such as Slack, Discord, Figma and Notion (web, script, GPU and offline caches). The app identifies an Electron app from its cache folders. It never touches their login data and local storage.

  The caches of Apple apps have the *Review* badge. You must close an app before the app moves its cache.
</details>

<details>
<summary><strong>Toolchains & SDKs</strong>: installed language versions</summary>

Toolchains & SDKs shows each installed version, with its size and the uninstall command of its manager:
- **Language version managers:** rustup, nvm, fnm, Volta, pyenv, rbenv and FVM.
- **Python:** versions that uv installed, and conda environments (Miniconda, Anaconda, Miniforge).
- **Java:** JDKs that an IDE downloaded to `~/Library/Java/JavaVirtualMachines`.
- **Swift toolchains.**
- **Managers for many tools:** SDKMAN, asdf and mise.
- **Android:** system images, NDK, build tools and virtual devices.
- **Package managers:** old Homebrew versions and downloads (`brew cleanup --dry-run` measures them, `brew cleanup` removes them), and Nix store paths that nothing uses (`nix store gc --dry-run`, then `nix store gc`). Old Nix generations stay, so rollbacks still work.

You cannot select a version that is in use. A version is in use when it is the default or global version, when `.tool-versions` or a mise configuration pins it, or when an Android virtual device uses it. You cannot select a virtual device while its emulator runs.
</details>

<details>
<summary><strong>Containers & VMs</strong>: Docker, OrbStack, Colima, Lima, Podman and virtual machines</summary>

Container engines keep all images, containers and volumes in one virtual disk. The app shows the real size of each disk. The file reports a larger size, because it is sparse. A disk never goes to the Trash, because that deletes all its contents.

To free space inside a disk, the app uses the command of the engine. It asks the engine how much each kind of data can free (`docker system df`):

| Item | Command | Badge |
| --- | --- | --- |
| Build cache | `docker builder prune --force` | *Rebuild* |
| Unused images | `docker image prune --all --force` | *Review* |
| Stopped containers | `docker container prune --force` | *Review* |
| Unused volumes | None. Volumes can hold databases. Remove them in Docker. | For information only |

Podman items use the same `podman` commands. Docker commands reach the engine that the current Docker context names (Docker Desktop, OrbStack, Colima or Rancher Desktop). The app works only with an engine on this Mac: a context that points at a remote host is not listed. Before a command runs, the app checks that the context and its endpoint did not change, and measures again: when the command would now free clearly more than you reviewed, it does not run. The engine must run to measure and clean its data. The download caches of Lima and Colima go to the Trash.

The app also shows each virtual machine of UTM, Parallels Desktop, VMware Fusion, VirtualBox and Tart, with its size and the date of its last change. A virtual machine never goes to the Trash. Remove it in its app, which also forgets it. The item tells you how.
</details>

<details>
<summary><strong>Installers</strong>: installers that you already used</summary>

Installers shows macOS installer apps in the Applications folders, and `.dmg`, `.pkg`, `.xip` and `.iso` files in Downloads and on the Desktop, with the date that you last opened them. The app does not move an installer that runs. You can download each installer again from its publisher.
</details>

<details>
<summary><strong>AI Tools</strong>: caches, sessions and models of AI tools</summary>

AI Tools reads the data folders of AI coding tools and local model stores. Each item has one of three tiers:

| Tier | Badge | Examples | What you lose |
| --- | --- | --- | --- |
| **Safe** | *Rebuild* | Caches, logs, telemetry, plugin downloads, shell snapshots, old Claude Code versions | Nothing. The tool makes the item again. |
| **Review** | *Review* | Subagent transcripts, saved tool output, job scratch files, worktrees of agents, downloaded models | Examine the item first. You download a model again to use it. |
| **Caution** | *Review* | Session transcripts, chats, prompt history, edit checkpoints | You cannot resume the session again. |

The app never shows settings, credentials, instructions (`CLAUDE.md`, `AGENTS.md`), skills, agents, commands, plugins, marketplaces or memory.

**Sessions.** For each session, the app shows the project folder, the first prompt, the branch and the start date. When the project folder does not exist, the item has the label *Orphan*. Worktrees that agents made and removed are identified. When a project folder cannot be read (no permission) or is on a drive that is not connected, its sessions are not labeled *Orphan*. Worktrees that AI tools made are checked with Git: a worktree with uncommitted or untracked work stays. For Claude Code, you can remove the `subagents` and `tool-results` folders of a session and keep its main transcript. The session continues to resume. When a background job is blocked and has open tasks, its scratch files show a warning.

**Safety.** All actions move items to the Trash. The app does not move items while the tool runs. For Claude Code, the app reads `sessions/` and does not move the files of a running session, also when you continue the action. Each item shows the cleanup command or setting of the tool, if the tool has one (for example, `cleanupPeriodDays` of Claude Code). Each item shows its tier as its badge. Select a tier with the **Tier** filter: the totals then show how much space that tier frees, before you act.

| Tool | Data folders | What the app reads |
| --- | --- | --- |
| Claude Code | `~/.claude`, other accounts in `~/.claude-*`, `CLAUDE_CONFIG_DIR`, `~/.local/share/claude/versions` | Transcripts by project, background jobs, running sessions, empty account folders. It keeps the version of the launcher and the two newest versions. |
| Claude Desktop | `~/Library/Application Support/Claude` | Caches, the virtual machine image, Claude Code sessions |
| Codex | `~/.codex`, `CODEX_HOME` | Session rollouts by date, archived sessions, worktrees, logs |
| Gemini CLI, Qwen Code | `~/.gemini`, `~/.qwen` | Chats of each project, saved chats, restore snapshots |
| GitHub Copilot CLI, Cline CLI, opencode, Hermes, herdr, Goose, Amp, Kiro CLI, Crush | The data folder of each tool | Sessions, logs, caches and worktrees |
| Cursor, VS Code, Windsurf, Kiro, Trae, Void | `~/Library/Application Support/<editor>`, `~/.cursor` | Editor caches (VS Code caches are in Caches & Logs), workspace data of folders that do not exist, chat sessions, Cline, Roo Code and Kilo Code tasks |
| Zed, Continue | `~/Library/Application Support/Zed`, `~/.continue` | Agent threads, chat sessions, code index |
| Ollama | `~/.ollama/models`, `OLLAMA_MODELS` | Each model, with the size of the files that only it uses. The app moves the manifest to the Trash, as `ollama rm` does. Ollama removes the model files when it starts again. Unfinished downloads that are older than one hour. |
| Hugging Face | `~/.cache/huggingface/hub`, `HF_HOME`, `HF_HUB_CACHE` | Each model, dataset and Space |
| LM Studio, Whisper, PyTorch Hub, GPT4All, DiffusionBee | The model folder of each tool | Each downloaded model |
</details>

<details>
<summary><strong>Applications</strong> and <strong>App Leftovers</strong></summary>

**Applications** shows the apps in `/Applications` and `~/Applications`. The caches, preferences, saved state and Application Support folder of each app are separate items. Thus you select exactly what to remove. Apple apps are protected. You cannot move an app or its files while the app runs.

**App Leftovers** shows caches, preferences and saved state in your Library that have the name of an app that is not installed. The app identifies the owner from the name only. Thus each leftover has the *Review* badge.
</details>

<details>
<summary><strong>Downloads</strong> and <strong>Trash</strong></summary>

**Downloads** shows each item in your Downloads folder, with the date that you last opened it. **Trash** shows the items in your Trash. When you empty the Trash, the removal is permanent. Neither shows Finder's own `.DS_Store` and `.localized` files.
</details>

<details>
<summary><strong>System Data</strong>: snapshots and data that macOS manages</summary>

Settings shows these items as System Data:
- **Time Machine local snapshots:** backups that Time Machine keeps on the startup disk while the backup disk is not connected. The app shows how many there are and their dates. macOS does not give the size of each snapshot. *Run Cleanup* runs `tmutil thinlocalsnapshots`, which removes the snapshots that macOS lets go. The backups on your Time Machine disk stay. This action is permanent.
- **For information only:** Apple Intelligence and other on-device models, macOS update downloads (`/Library/Updates`), the GarageBand and Logic sound library, and aerial screen saver and wallpaper videos. The app shows their size and where macOS lets you remove them. The app does not change them, because the system protects these folders.

System Data does not show items when you scan only the folders that you select.
</details>

<details>
<summary><strong>Orphan Processes</strong>: programs that continue to run</summary>

Orphan Processes shows your processes whose parent stopped. It does not show:
- processes that macOS manages (launchd jobs);
- apps and their helper processes;
- system processes;
- processes outside your folders, when you scan only the folders that you select.

Programs that run detached by design are shown but you cannot select them: terminal sessions (tmux, screen, mosh), editor daemons, databases (PostgreSQL, MySQL, MongoDB, Redis) and local servers. Stop them with their own commands.

Each item shows the CPU use, memory use, working folder and executable. **Quit** asks the process to stop. **Force Quit** becomes available only after Quit. Before each action, the app examines the identity of the process again.
</details>

## Appearance

The app has its own design, *Soft Studio*: rounded type, soft cards and pill-shaped controls. The default palette is White & Gray, with white surfaces on light gray and a graphite accent. In Settings, you can select a different palette: Paper & Walnut, Honey & Espresso, Ember, Terracotta & Navy, Cream & Mauve or Linen & Rose. Each palette has a light and a dark version. In Settings, select System, Light or Dark. System follows the setting of your Mac.

## License and contributions

CleanYourMac has the [MIT license](LICENSE). It does not need a subscription. It is an independent project and does not use MacPaw software or services. The orphan-process inspection uses parts of OrphanBar, an MIT-licensed project of the same owner. See [acknowledgments](THIRD_PARTY_NOTICES.md).

To report a problem or to suggest an idea, [open an issue](https://github.com/scr2em/CleanYourMac/issues).

To contribute, read the [development guide](docs/DEVELOPMENT.md), the [architecture](docs/ARCHITECTURE.md) and the [module guide](docs/MODULES.md). [How each tool behaves](docs/MODULE_BEHAVIOR.md) lists, in one table, what every tool scans, skips and defaults to.
