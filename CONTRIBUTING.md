# Contributing

Start with an issue or a small focused change. Follow the contracts in [module authoring](docs/MODULES.md), and keep UI additions inside the [design system](docs/DESIGN_SYSTEM.md).

Use Swift 6 and macOS 14-compatible APIs. This project has no external package dependencies. Add a dependency only when it solves a concrete problem and its license and platform support have been reviewed.

Before submitting a change:

~~~sh
python3 scripts/lint-design.py
bash scripts/swift.sh test
bash scripts/build-app.sh --universal
~~~

Describe the final behavior, why it changes, and how it was verified. New cleanup rules require evidence, fresh action validation, clear recovery limits and synthetic fixtures. Never make a test clean a real project, reset an existing simulator or terminate an unrelated process.

The Component Gallery and --demo mode support UI review with synthetic data. Check shared layouts in light/dark mode and at the minimum window size. Retain native keyboard and accessibility behavior.

New source contributions use the MIT license. Preserve attribution for adapted code and update THIRD_PARTY_NOTICES.md when appropriate.
