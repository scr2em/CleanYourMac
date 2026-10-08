# Reporting a safety issue

Report a cleanup rule that can target unintended data, bypass fresh validation, or signal the wrong process privately to the repository owner. When a public repository is configured, use its private vulnerability-reporting channel. Do not publish personal paths, process command arguments or credentials.

Include the app version, macOS/Xcode/Git versions, affected module, expected and observed behavior, and a synthetic reproduction if possible. Avoid repeating an unsafe cleanup on valuable data to reproduce it.

The initial app runs as the current user, has no privileged helper, uses local scans/history, and loads only compiled trusted modules. Full Disk Access can expand filesystem visibility but does not make every cleanup rule eligible.

## Security model

The app treats every file it scans, and every program's output, as untrusted. In short:

- **Actions act only on what a scan found.** The app picks findings by ID from its own results. Example data can never be acted on. Restores come from the local history. Each item is checked again before it changes: same identity, unchanged, in scope, no symbolic links on its path, and its path in plain form.
- **Commands come from a fixed list.** The app starts only `git`, `xcrun`, `launchctl`, and `docker` or `podman` from their usual install locations. It passes arguments as a list, with a cleared environment, a time limit and limited output. A cleanup command is chosen again from the tool's fixed list right before it runs. `docker` and `podman` must point at an engine on this Mac. A tool linked from the home folder must resolve into an app in `/Applications`. A tool's resolved file and every folder above it must belong to root or to you and must not be writable by other users.
- **Git never runs a repository's own programs.** A repository whose own settings name a program (filters, text converters, signing programs, included files, a pager) or that contains a nested repository is left for you to inspect.
- **Other version-control systems are never run.** Their data (`policy::VERSION_CONTROL`) is never listed or entered, and an item inside one of their checkouts is refused, since whether it is tracked cannot be checked without running the system's own tool.
- **Untrusted files are read with limits.** The app does not follow symbolic links or wait on pipes and devices. Sizes are capped. Property lists are read with a nesting limit, and binary ones only at the top level. A crash in one tool ends only that tool's scan.

Known limits:

- A program that already runs as you can change what the app reads and runs, just as it could change your shell profile. If you gave the app Full Disk Access, keep only trusted software installed.
- A helper process of an approved tool that moves itself out of the tool's process group is not stopped when the tool times out.
