# Figma

The Comfy design system lives in Figma: https://www.figma.com/design/LwVoq8tzYDa6dqRG6dm1m4

The file is still named "Untitled" because the plugin API cannot rename documents. Rename it by hand.

| Page | Node | Contents |
| --- | --- | --- |
| 00 · Cover | `0:1` (cover frame `33:641`) | Cover |
| 01 · Foundations | `14:2` (frame `31:2`) | Light and dark palettes, TypeStyle, space, radius, elevation, layout, sizes, brand |
| 02 · Components | `14:3` | Icons, BrandIcon, controls, finder pieces, sidebar, window chrome, Settings controls |
| 03 · App screens | `14:4` | Build Artifacts finder (light and dark), Overview, Settings (General, Appearance) |

## Components

Every component is built with auto layout, and every fill, stroke, radius, padding, gap and text style is bound to a variable or style. Instances in screens carry only text, variant and instance-swap overrides.

| Component | Node | SwiftUI source | Variants and properties |
| --- | --- | --- | --- |
| Icon/* | `20:5` (grid) | `Image(systemName:)` | One 16 pt component per SF Symbol, named after the symbol |
| BrandIcon/* | `58:6` (section) | `BrandIcon`, `RowIcon.brand` | nextdotjs, swift, react, flutter, rust, python, gradle, pnpm, git, xcode; 24 pt with a `Space.xxs` inset |
| Divider | `60:11` | `Divider()` | orientation |
| Checkbox | `60:29` | `.toggleStyle(.checkbox)` | state off / on / mixed × disabled |
| ActionButton | `60:45` | `ActionButton` | kind primary / secondary / destructive × state enabled / disabled; Label |
| StatusBadge | `60:57` | `StatusBadge` | kind info / warning; Title |
| SearchField | `60:65` | `TextField(...).textFieldStyle(.roundedBorder)` | state placeholder / filled |
| PopupButton | `60:78` | `Picker` (menu style) | size regular / small; Label, Value, Show label |
| StatChip | `60:88` | `StatChip` | emphasized; Value, Label |
| PageHeader | `61:17` | `PageHeader` | Title, Subtitle |
| Panel | `61:24` | `Panel` | Slot for content |
| MetricTile | `61:27` | `MetricTile` | Title, Value, Detail |
| KeyValueRow | `61:34` | `KeyValueRow` | Label, Value |
| EmptyState | `61:40` | `EmptyState` | Symbol (swap), Title, Message |
| FindingRow | `62:77` | `FindingRow` + `RowIconView` ("ResultRow" in older docs) | state default / active / checked / checkedActive / ineligible; Icon (instance swap: SF Symbol or BrandIcon), Title, Subtitle, Value, Badge |
| SelectionBar | `62:103` | `SelectionBar` | state empty / partial / all; Title, Count |
| SelectionFooter | `62:119` | `SelectionFooter` | state empty / selected; Count, Summary |
| ScopeRow | `62:123` | `ScopeView` | Symbol, Scope, Choose folders |
| SidebarRow | `63:43` | sidebar `Label` | state default / selected (accent fill, onAccent content); Icon, Label |
| SidebarSectionHeader | `63:44` | sidebar `Section` header | Title |
| TrafficLights | `63:50` | window controls | |
| ToolbarButton | `63:54` | toolbar `Button` | Symbol |
| ModuleRow | `63:60` | Overview module button | Symbol, Name, Summary, Count |
| Switch, Radio, SettingsTab, FormRow, PaletteOption | `67:53`, `67:57`, `67:66`, `67:89`, `67:107` | `PreferencesView`, `ThemeSwatch` | Settings controls |
| StorageBar | `27:77` | `StorageBar` | fraction |

## Screens

| Screen | Node | Notes |
| --- | --- | --- |
| Build Artifacts · Light | `65:6` | Walnut Light. Three rows checked, `.next` inspected, partial SelectionBar, footer actions, inspector |
| Build Artifacts · Dark | `66:254` | Same frame with Comfy · Colors set to Walnut Dark and Comfy · Brand set to Dark |
| Overview · Light | `66:340` | Header, four MetricTiles, scope, Scan enabled modules, ModuleRows per category |
| Settings · General · Light | `67:784` | 640 × 540 tabbed window: Scan roots, Menu bar |
| Settings · Appearance · Light | `67:853` | Palette radio list; each ThemeSwatch is bound to that palette's primitives |

Windows are `layout/windowWidth` × `layout/windowHeight` (1120 × 760). Lists clip at the window edge, as a scroll view would.

## Variables and code names

Collections: Comfy · Primitives, Comfy · Colors (Light and Dark modes for Walnut, Honey, Ember, Terracotta and Mauve), Comfy · Brand (Light and Dark), Comfy · Dimensions, Comfy · Motion and Comfy · Typography. Semantic variables carry iOS code names that match the SwiftUI tokens: `Palette.<name>`, `Space.<name>` and `Layout.<name>`. `docs/brand/tokens.json` is the shared source, and `Sources/CleanYourMacDesignSystem/DesignSystem.swift` mirrors it. Where the two disagree, the Figma values follow DesignSystem.swift.

- `color/accentSymbol` (was `accentIcon`) maps to `Palette.accentSymbol`. `color/onDestructive` maps to `Palette.onDestructive`.
- `size/rowIcon` (24) maps to `Layout.rowIcon`. `space/controlPaddingY` (6) is `Space.sm - Space.xxs`, the vertical padding of buttons, text fields and sidebar rows.
- Comfy · Brand holds the ecosystem logo colors. It is the only place raw brand hex values are allowed. Near-black or near-white marks (Next.js, Rust, and Gradle in dark mode) alias `color/ink`, as `BrandIcon` does in code.
- Comfy · Colors is at Figma's 10-mode limit, so Linen & Rose has no mode. Its swatches bind to `primitive/rose/*` directly.

## Fonts

The app uses SF Pro only, with no serif headings, on the compact `TypeStyle` scale (pageTitle 24 semibold down to caption 11), plus SF Mono for paths. Apple fonts do not render in the Figma environment used to build the file, so the text styles use variables with two modes:

| Mode | UI text | Code |
| --- | --- | --- |
| Figma substitute | Inter | Roboto Mono |
| Apple SF | SF Pro | SF Mono |

Switch the Comfy · Typography collection to "Apple SF" on a Mac that has those fonts installed. Icon components are named after SF Symbols and drawn with outlined stand-in glyphs. Brand logos come from Simple Icons (CC0), using the same paths as `BrandIcons.swift`.

## Not yet done

- Code Connect mappings.
- Screens for the other modules, the Review sheet, and the Settings Protection and Modules tabs.

`design/figma/build.js` and `docs/design-concepts/` are earlier offline artifacts kept for reference.
