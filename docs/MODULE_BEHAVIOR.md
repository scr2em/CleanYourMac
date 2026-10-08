# How each tool behaves

What each of the 18 tools scans, what it lists, what it skips, and its default values. The last section lists the defaults that need a decision. Reviewed 8 October 2026 against the code at that date.

## Rules every tool follows

| Rule | What it means |
| --- | --- |
| Default scope | **Whole Mac** scans your home folder. **Chosen folders** limits every tool to those folders, including tools that read fixed locations. |
| Never entered or listed | System folders (`/System`, `/Library`, `/usr`, `/bin`, `/sbin`, `/private`, `/var`, `/etc`, `/tmp`, `/dev`, `/Network`). Credentials: `.ssh`, `.aws`, `.gnupg`, `.kube`, `.docker`, `.config`, `.netrc`, `.npmrc`, `.pypirc`, `.password-store`, `.azure`, `.terraform.d`, `.vault-token`, `.git-credentials`, `.env` and `.env.*`. Keychains, iCloud Drive and other cloud folders. Version-control data of 19 systems (`.git`, `.hg`, `.svn`, `.jj` and others) and Fossil or Monotone repository files. |
| Never followed | Symbolic links, iCloud files that are not downloaded, other drives mounted inside a scanned folder, and the inside of packages (`.app`, `.photoslibrary`, `.sparsebundle`, `.xcodeproj` and 25 more). A package is listed as one item. |
| Never acted on | `/`, your home folder, Library, Documents, Desktop, Downloads, Movies, Music, Pictures, the Trash folder, both Applications folders, and folders that hold one of them. Their *contents* can still be listed. Paths you protect, and folders that contain them. |
| Checked again before any action | The item is unchanged since the scan (same file, same folder contents), still in scope, has no linked parent folder, and the tool's own checks still pass. Items go to the Trash unless the badge says Permanent. |
| Selection | Nothing is selected after a scan. Recommended fixes select their items when you review them. A fix must free at least 1 MB. |
| Limits | 20 million entries per tool's walk, 2 million results per tool. A folder of more than 500,000 entries cannot be sized fully, so it is shown but blocked. |

Badge colors: green for **Rebuild required** (the tool makes it again), amber for **Review** (look before removing), red for **Permanent** (removed, not moved to the Trash), grey when blocked.

## The tools

| Tool | Looks in | Lists | Skips or never touches | Defaults |
| --- | --- | --- | --- | --- |
| **Storage Explorer** | Your scan folders, one level at a time | Every file and folder, with its size | Symbolic links | Review. No size limit. Top folders such as Library are listed but blocked. |
| **Large Files** | Your scan folders, every level | Files of at least 100 MB | The inside of packages. Files in `~/Library` and hidden home folders are listed but blocked as app data. | Review. 100 MB, fixed. |
| **Exact Duplicates** | Your scan folders, every level. Runs only when you open it. | Files of at least 4 KB with identical content, checked by size, then the first 64 KB, then the full file | Hidden folders, any folder named `Library`, about 100 build and dependency names at any depth (`node_modules`, `build`, `dist`, `out`, `bin`, `target`, `vendor` and others), hard links of one file | Review. One copy per group is kept: Documents, Desktop and media folders first, Downloads late, caches and hidden folders last. Copies Git tracks are refused. |
| **Downloads** | `~/Downloads`, one level | Every item | Nothing extra | Review. |
| **Installers** | Both Applications folders (top level), `~/Downloads` and `~/Desktop` plus one level of subfolders | `.dmg`, `.pkg`, `.mpkg`, `.xip`, `.iso` files and macOS installer apps | Hidden names, folders, subfolders with a dot in the name | Installer apps Rebuild, files Review. No age limit. |
| **Trash** | `~/.Trash`, one level | Every item | Other drives' trash, iCloud Drive's trash | Permanent: deleted, not moved. |
| **Dependencies** | Your scan folders, plus 30 shared package stores (npm, Yarn, pnpm, pip, Gradle, Maven, Cargo, Go, CocoaPods and others) | `node_modules` next to `package.json` or `deno.json`. 16 more rules, each needing a project file: Pods, Carthage, Python environments, `vendor`, SwiftPM checkouts, `.terraform`, Elixir `deps`, `bower_components`, `renv` | Package-manager homes (`.npm`, `.nvm`, `.volta`, pnpm store and others). Never looks inside a folder it found. Composer, Go and Cargo `vendor` folders that Git tracks. Items in a non-Git checkout are refused. | Rebuild. Maven repository Review. Go module cache blocked (Go makes it read-only). Recommended when the project is idle 30 days. |
| **Build Artifacts** | Your scan folders | 35 rules, each needing a project file, such as `target` with `Cargo.toml`, `.next` with `next.config.*`, `DerivedData` with an `.xcodeproj`, `build` with `CMakeCache.txt` | Inside `node_modules`, `site-packages` and dependency folders. Items Git tracks, or in a non-Git checkout, are refused. | Rebuild. Unreal `Saved` Review. A folder holding a shipped build (`.xcarchive`, `.dSYM`, `.ipa`, `.aab`) needs your confirmation. Recommended when the project is idle 30 days. |
| **Git Worktrees** | Repositories in your scan folders | Linked worktrees, not the main one | Does not enter `node_modules`, `.build`, `target` | Permanent (`git worktree remove`). Blocked with uncommitted or untracked work, locks, submodules, risky Git settings, or a detached commit no branch holds. |
| **Simulators** | `xcrun simctl` | Simulator devices and downloaded runtimes | Running devices, runtimes Xcode installed, runtimes a running device uses | Devices Permanent (erase or delete), runtimes Review. |
| **Xcode Data** | `~/Library/Developer` and both Applications folders | DerivedData, Device Support for 5 platforms, archives, device logs, documentation caches, simulator caches, previews, extra Xcode installs | The Xcode that `xcode-select` selects. Archives need confirmation. Nothing while Xcode or Simulator runs. | Mostly Rebuild. Archives, device logs and extra Xcodes Review. Recommended with no idle age. |
| **Caches & Logs** | About 120 fixed places: developer caches (npm, pip, Gradle, Cargo, Homebrew, VS Code and others), `~/Library/Caches`, sandboxed app caches, saved window state, Electron app caches, `~/.cache` as one item, `~/Library/Logs`, iPhone and iPad updates, device backups | One item per cache or per app | Places another tool owns. Browser profile caches. Apple caches are downgraded to Review. | Rebuild. Maven, JetBrains, Android Studio, Mail downloads, device backups and logs Review. Go module cache blocked. Recommended with no idle age. |
| **Toolchains & SDKs** | 16 version-manager folders: rustup, nvm, fnm, Volta, pyenv, rbenv, FVM, swift.org, SDKMAN, asdf, mise, Android SDK images, NDK and build tools, Android emulators | One item per installed version | The default or current version, versions a running process uses, `current` links | Rebuild. Rust toolchains and Android emulators Review. |
| **Containers & VMs** | Docker Desktop, OrbStack, Colima, Lima, Rancher Desktop, Podman and Apple container disks, Lima and Colima download caches, and the engine's own usage report | Each virtual disk (for information), build cache, unused images, stopped containers | Volumes, always. Remote Docker or Podman engines. The disks themselves. | Build cache Rebuild, images and containers Review. Cleanup runs the engine's own prune command after re-checking the engine. |
| **AI Tools** | Data folders of 26 tools (Claude Code, Codex, Gemini CLI, Cursor, Ollama, Hugging Face, LM Studio and others) | Caches and logs (Safe), plans, worktrees and models (Review), transcripts and history (Caution) | Settings, sign-in files, memory, MCP and skills folders. Running sessions. The two newest Claude Code versions. Worktrees with work in them. | Safe is Rebuild; Review and Caution are Review. The tier is the badge. |
| **Applications** | Both Applications folders | Each app, plus its Caches, Preferences, Saved State and Application Support folders by bundle ID | Apple apps, running apps | Review. |
| **App Leftovers** | `~/Library/Caches`, `Preferences`, `Saved Application State` | Items named like a bundle ID whose app is not installed | `com.apple.*`, `group.*`, items whose app is running | Review. |
| **Orphan Processes** | Running processes | Your processes left behind by their parent (parent is launchd), not managed by launchd, not part of an app | System paths. 37 long-running servers (tmux, Postgres, Redis, Ollama and others) are shown but blocked. Your ignored names (`ssh-agent`, `gpg-agent`, `keyboxd`, `dirmngr` by default). | Permanent. Force Quit only after Quit. |

## Defaults that need a decision

Ordered by risk. "Verified" means the behavior was reproduced by running the code.

### Could remove something important

| # | Tool | What happens | Recommendation |
| --- | --- | --- | --- |
| S1 | Dependencies | `node_modules` inside VS Code or Cursor extensions (`~/.vscode/extensions/*`) is listed as project dependencies and, after 30 idle days, recommended. Removing it breaks the extension. **Verified.** | Never enter editor extension folders. |
| S2 | Storage Explorer | Browsing into `~/Library` offers Trash on `Application Support`, `Containers`, `Mail` and similar folders as whole items. **Verified.** | Block the immediate children of `~/Library`. |
| S3 | Large Files | No Git check: a large tracked file in a repository is offered for Trash. Duplicates already refuses these. | Refuse tracked files, as Duplicates does. |
| S4 | Xcode Data | When `xcode-select` points to the Command Line Tools, no Xcode counts as selected, so every installed Xcode is offered. | Keep the newest Xcode when none is selected. |
| S5 | Caches & Logs, Xcode Data | Device Support and DerivedData are recommended with no idle age, including today's. Saved window state is Rebuild, so it joins the one-click cache fix. | Require 30 idle days for Device Support and DerivedData; make saved window state Review. |
| S6 | Containers & VMs | The "Lima or Colima is running" check for their caches never fires, because those process names are not in the list it reads. | Look for `limactl` and `colima` directly. |
| S7 | AI Tools | `~/.claude/local` is Safe, but it can be the npm install that a shell alias runs. | Make it Review, or check where the `claude` command points. |
| S8 | Downloads, Trash | `.DS_Store` and `.localized` are listed; removing `.localized` changes the folder's name in Finder. **Verified.** | Skip both names. |

### Things it misses

| # | Tool | What happens | Recommendation |
| --- | --- | --- | --- |
| G1 | Caches & Logs | Chrome's cache (`~/Library/Caches/Google/Chrome`, often several GB) is never listed: the Android Studio entry claims all of `~/Library/Caches/Google`. **Verified.** | Claim only the Android Studio folders. |
| G2 | Large Files | Packages are never listed, so a Mac's largest items (`.photoslibrary`, `.sparsebundle`, `.utm`, `.pvm` virtual machines) never appear. | List packages of at least 100 MB as one item, blocked as app data where they belong to an app. |
| G3 | Chosen folders | `~/Developer` (Apple's convention), `~/src`, `~/repos` are not suggested as project folders. | Add them. |
| G4 | Several | Not covered: `~/Library/Developer/XCTestDevices`, Tart, UTM, Parallels and Vagrant images, IntelliJ JDKs (`~/Library/Java/JavaVirtualMachines`), uv-managed Pythons. | Add in a later pass. |

### Inconsistent rules

| # | What happens | Recommendation |
| --- | --- | --- |
| C1 | Duplicates skips generic folder names at any depth, so a `Documents/Build` or `Music/Packages` folder is never compared. | Skip those names only next to a project file, as Build Artifacts does. |
| C2 | "Quit the app first" can be overridden in Caches & Logs but not in Applications or App Leftovers. Xcode Data offers an override that always fails. | Make app-running checks hard everywhere, and stop offering the override where it cannot work. |
| C3 | The 30-day idle rule applies only to Dependencies and Build Artifacts. | Apply one idle rule to every recommended fix that removes project or Xcode data. |
| C4 | Shared package stores appear in both Dependencies and Caches & Logs. | Keep them in Dependencies only. |
| C5 | `~/.config` counts as a credential folder, so nothing under it is ever scanned, including plain caches. | Keep for safety; list the known safe caches under it explicitly if needed. |

### Sensible as they are

- **Large Files 100 MB, Duplicates 4 KB:** common thresholds; smaller duplicates waste little space.
- **Nothing pre-selected:** every removal starts with your review.
- **Volumes never pruned, virtual disks never removed:** they hold data that cannot be rebuilt.
- **Permanent only where the action cannot use the Trash:** Trash, worktrees, simulators, processes.
- **Never entering version control, credentials or cloud folders:** these hold data that cannot be rebuilt.
