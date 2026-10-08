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

## A workspace that feels comfortable

Paper & Walnut is the default palette: light paper surfaces and warm brown accents. Prefer another mood? Choose Honey & Espresso, Ember, Terracotta & Navy, Cream & Mauve, or Linen & Rose in Settings. Light and dark appearances share the same design system.

## Free, independent, and open

CleanYourMac is [MIT licensed](LICENSE). No subscription is required. It is independently built and uses no MacPaw software or services. Orphan-process inspection draws on the owner’s MIT-licensed OrphanBar project; see [acknowledgments](THIRD_PARTY_NOTICES.md).

Have an idea or found something that needs fixing? [Open an issue](https://github.com/scr2em/CleanYourMac/issues).

Want to contribute? Start with the [development guide](docs/DEVELOPMENT.md), [architecture](docs/ARCHITECTURE.md), and [module guide](docs/MODULES.md).
