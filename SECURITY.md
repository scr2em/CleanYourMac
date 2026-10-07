# Reporting a safety issue

Report a cleanup rule that can target unintended data, bypass fresh validation, or signal the wrong process privately to the repository owner. When a public repository is configured, use its private vulnerability-reporting channel. Do not publish personal paths, process command arguments or credentials.

Include the app version, macOS/Xcode/Git versions, affected module, expected and observed behavior, and a synthetic reproduction if possible. Avoid repeating an unsafe cleanup on valuable data to reproduce it.

The initial app runs as the current user, has no privileged helper, uses local scans/history, and loads only compiled trusted modules. Full Disk Access can expand filesystem visibility but does not make every cleanup rule eligible.
