# Figma

The Comfy design system lives in Figma: https://www.figma.com/design/LwVoq8tzYDa6dqRG6dm1m4

The file is still named "Untitled" because the plugin API cannot rename documents. Rename it by hand.

| Page | Node | Contents |
| --- | --- | --- |
| 00 · Cover | `0:1` | Cover |
| 01 · Foundations | `14:2` | Light and dark palette, type, space, radius, elevation, layout, brand |
| 02 · Components | `14:3` | Icons, Checkbox, ActionButton (kind × state), StatusBadge, Panel, PageHeader, MetricTile, KeyValueRow, StorageBar, ResultRow, EmptyState, plus app-shell pieces |
| 03 · App screens | `14:4` | Node Dependencies finder and Review sheet, in light and dark |

## Variables and code names

Collections: Comfy · Primitives, Comfy · Colors (Light and Dark modes), Comfy · Dimensions, Comfy · Motion and Comfy · Typography. Semantic variables carry iOS code names that match the SwiftUI tokens: `Palette.<name>`, `Space.<name>` and `Layout.<name>`. `docs/brand/tokens.json` is the shared source, and `Sources/CleanYourMacDesignSystem/DesignSystem.swift` mirrors it.

## Fonts

Apple fonts do not render in the Figma environment used to build the file, so text styles use variables with two modes:

| Mode | Headings | UI text | Code |
| --- | --- | --- | --- |
| Figma substitute | Newsreader | Inter | Roboto Mono |
| Apple SF | New York | SF Pro | SF Mono |

Switch the Comfy · Typography collection to "Apple SF" on a Mac that has those fonts installed. Icon components are named after SF Symbols and drawn with outlined stand-in glyphs.

## Not yet done

Code Connect mappings, and screens beyond the two examples.

`design/figma/build.js` and `docs/design-concepts/` are earlier offline artifacts kept for reference.
