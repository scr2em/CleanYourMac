# CleanYourMac design system specification

Design specification and implementation baseline, 7 October 2026.

The app uses a self-contained SwiftUI design system so its visual language remains consistent as features grow. Feature modules extend discovery and actions. Tokens, components, states, and page layouts are controlled centrally. The implemented API lives in Sources/CleanYourMacDesignSystem/DesignSystem.swift, with a native Component Gallery. The broader catalog below remains the specification for future components.

## Visual direction

Use a calm, native macOS utility interface with clear typography, compact data tables, a familiar sidebar, and a persistent inspector. Give storage amounts, item ownership, and action consequences more prominence than decorative graphics. Use the system font, SF Symbols, native controls, and platform-aware surfaces. One restrained accent identifies primary actions and selection; warning and destructive colors have explicit semantic roles.

Our interface and assets are independently designed. MacPaw's UI, artwork, components, and binaries are not inputs or dependencies. Apple platform guidance is a reference for familiar desktop behavior. [Apple macOS design guidance](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos)

## Package boundary

CleanYourMacDesignSystem owns semantic tokens, component styles, accessibility behavior, motion presets, and reusable layout primitives. It depends on SwiftUI/AppKit and has no dependency on cleanup logic or specific feature modules.

CleanYourMacUI translates Core findings and plans into those components. Specialized worktree or simulator inspectors are UI adapters built from the same approved primitives. Backend modules return data and capabilities; they do not provide arbitrary SwiftUI views.

Expose a small public API with explicit component variants. Keep raw palette values, typography implementation, and private layout constants internal. A new feature may choose existing variants; a new visual pattern must be added centrally with its states and gallery examples. This is an enforceable development convention, not a security boundary for arbitrary downloaded code.

## Tokens

The implementation uses system colors and fonts, spacing 2/4/8/12/16/24/32, a 10-point panel radius, a 220-point sidebar, a 320-point resizable inspector and result rows of at least 52 points. Native controls retain platform sizing and behavior. The broader families below govern future additions; unimplemented roles are not exposed merely to expand the API.

| Family | Public roles | Proposed behavior |
| --- | --- | --- |
| Color | canvas, surface, elevatedSurface, separator, textPrimary, textSecondary, textDisabled, accent, selection, info, success, warning, destructive | Resolve semantic roles for light/dark mode, increased contrast, and reduced transparency. Use system colors where suitable. Status always includes text or an icon. |
| Typography | pageTitle, sectionTitle, body, secondary, caption, metric, code | Use system text styles and appropriate weights. Metrics use monospaced digits; paths/identifiers can use a monospaced role. Preserve readable text when system settings or localization increase its width. |
| Spacing | none, xxs, xs, sm, md, lg, xl, xxl | Proposed scale: 0, 2, 4, 8, 12, 16, 24, 32 points. Choose named roles in feature UI. |
| Radius | control, panel, sheet | Proposed app-owned values: 6, 10, 14 points. Native control and sheet chrome use their platform shapes. |
| Icon | small, standard, prominent | Proposed sizes: 12, 16, 24 points. SF Symbol selection comes from a centrally maintained semantic icon map. |
| Density | standard, compact | Standard is the default. Compact is reserved for dense result tables, with a centrally defined row height and usable interaction areas. |
| Layout | sidebarWidth, inspectorWidth, contentInset, sectionGap, tableRowMinimum | Proposed sidebar 220 points and inspector 320 points, both resizable. Content inset 24 points, section gap 24 points, app-owned standard result rows at least 44 points. |
| Motion | immediate, feedback, stateChange, panel | Immediate input feedback; proposed 120 ms feedback and 180 ms state changes. Use native panel transitions and centrally specified restrained springs only where movement aids orientation. Reduced motion replaces movement with a short fade or immediate change. |

Keep storage units, date formatting, path shortening, and pluralization in shared presentation formatters. Display a consistent storage unit convention. Paths may shorten visually in the middle, while accessibility, copy, and reveal actions retain the full path. Separate logical size, allocated size, bytes moved to Trash, and measured free-space change.

## Component catalog

| Component | Responsibilities | Required variants or states |
| --- | --- | --- |
| AppShell and SidebarItem | Resizable navigation and selected category | Selected/unselected, count, unavailable category |
| PageHeader and SectionHeader | Title, concise description, scan action, scope | Idle, scanning, partial, completed |
| ActionButton | Native button behavior with a semantic role | Primary, secondary, destructive; enabled, disabled, pressed, focused, busy |
| SearchField and FilterBar | Search and approved size/age/type filters | Empty, filled, active filters, disabled |
| MetricTile and StorageBar | Storage summaries with clear labels | Known, estimated, unavailable; text equivalents |
| ScanOrb | The overview's scan control: an orb in an orbiting particle field that is the Scan button at rest, shows a progress ring while scanning and shrinks to a compact Scan again button beside the findings | Idle, scanning (progress or indeterminate), compact, disabled; frozen with Reduce Motion |
| ResultTable and FindingRow | Shared results, sorting, item selection, cleanup checkboxes; Command-click toggles a row, Shift-click selects the eligible rows from the last clicked one | Unselected, selected, checked, protected, ineligible, stale, loading |
| PathLabel and SizeLabel | Path/size formatting, full-path copy/reveal | Long path, uncertain size, unavailable data |
| StatusBadge and RiskBadge | Human-readable status and action consequences | Complete, partial, active, unknown; review needed, rebuild required, permanent |
| InspectorSection and KeyValueRow | Owner, evidence, tool state, and consequences | Available, missing, unknown, permission denied |
| ScanStatus and ProgressSummary | Progress, cancel, coverage, and warnings | Determinate, indeterminate, partial, cancelled, failed |
| SelectionSummary | Selected item count and overlap-aware size totals | Empty, eligible, mixed eligibility, changed selection |
| ActionReview | Selected paths, consequences, apply/cancel | Reversible file action, tool action, irreversible action, stale plan |
| EmptyState and ErrorState | Explain an empty result or a recoverable failure | No results, filtered out, missing tool, missing access, partial scan |
| ActionHistoryRow | Per-item outcome and eligible restore affordance | Applied, skipped, failed, restore available, restore unavailable |

Use native toggles, checkboxes, menus, tables, tooltips, and alerts through shared styles or wrappers as appropriate. Customize only where the app needs a consistent behavior or presentation. Do not build a separate control system for each feature.

## Page layouts

The primary workspace has a sidebar, results area, and optional inspector. At narrower widths, collapse the inspector into a detail destination and allow the sidebar to collapse through native navigation. Tables choose fewer columns before shrinking important text. Secondary metadata belongs in the inspector.

Standardize these page patterns:

- Overview: storage summary, enabled scan groups, recent actions, and a clear scan entry point.
- Finder: scope chooser, scan status, filters, result table, inspector, and selection summary. Worktrees, Node projects, simulators, and orphan processes use this pattern. The process scope is the current user; its summary shows count, CPU, and memory rather than disk bytes.
- Storage explorer: folder navigation and sizes, with visual storage mapping added after the basic explorer is useful.
- Action review: exact selected items, total estimate, consequences, recovery limits, and a single commit action.
- Activity: local action outcomes, explanations, and eligible restore actions.
- Settings: scan roots, protected paths, modules, display preferences, and required access.

Features fill these patterns with their data. A new page layout requires central review and catalog coverage, rather than a feature-local design fork.

## Interaction and copy

Table row focus and cleanup checkbox selection are separate states. Selecting a row opens its inspector; it does not select that item for cleanup. Sorts and incremental scan updates preserve focus and selection by resource identity. Avoid automatically reordering visible results on every streamed finding; provide stable insertion or an explicit sort refresh.

Use precise action labels: Scan, Review Selection, Move to Trash, Reset Simulator, Delete Simulator, Remove Worktree, Empty Trash, Terminate, and Force Quit. Process states include requesting termination, still running, exited, skipped, and failed. Force Quit remains a separately reviewed action. Show irreversible consequences in the review state. A keyboard shortcut for deletion opens the same review workflow; it never commits removal directly. Do not assign a commit shortcut that conflicts with common macOS commands.

Use familiar keyboard navigation, visible focus, standard copy/search shortcuts, context menus, and Reveal in Finder where relevant. Busy operations keep cancellation and navigation responsive. Progress only uses percentages when a meaningful total is known. A failure describes what happened and gives a useful next action; missing access and incomplete scans are not presented as a clean bill of health.

Use restrained motion for state changes and orientation. No decorative scan loops, bouncing destructive actions, or celebratory animations that conceal results. Respect reduced motion, reduced transparency, and increased contrast through shared environment handling. [SwiftUI reduced motion environment](https://developer.apple.com/documentation/swiftui/environmentvalues/accessibilityreducemotion)

## Accessibility and adaptability

Every interactive component supplies an accessible name, role, state, and keyboard path. Icons need labels when their meaning is not already present in text. Announce meaningful scan completion and action outcomes without reading every streamed row. Avoid color-only meanings; badges combine text with shape/icon cues. Prefer native controls' accessibility behavior.

Test light/dark mode, increased contrast, reduced transparency, reduced motion, long paths, large size values, mixed statuses, missing values, long translations, and right-to-left layouts. Preserve critical action text and allow explanatory text to wrap. Dense views may hide optional columns, but must keep the primary item and consequence legible.

## Governance and acceptance

Create a development-only SwiftUI component gallery with each public component, its variants, and realistic fixture data. Include complete worktree, Node, simulator, empty, permission-denied, partial-scan, and destructive-review pages. The gallery is built with our own components and contains no user filesystem data.

Use visual comparisons for shared components and interaction tests for important flows. Review behavior and accessibility manually as well; screenshots alone cannot verify either. Add a scoped CI lint check that flags new raw color/font/radius/motion constants in feature UI. Exceptions, such as a data-driven chart dimension, must be named and reviewed. Code review enforces component reuse where simple linting cannot.

Version the public component API and document migrations. Every new component must include its purpose, variants, empty/error/loading behavior where applicable, keyboard/accessibility behavior, and gallery examples. New tokens belong to semantic families; feature modules cannot create their own theme.

The native gallery, shared Finder/Action Review patterns and production scanners are connected. --demo provides Node, worktree, simulator, storage and orphan fixture data with actions disabled. Light/dark appearance and native accessibility-tree inspection are part of local verification. A complete VoiceOver, localization and assistive-settings audit remains a public-release check.
