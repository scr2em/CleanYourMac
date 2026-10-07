# CleanYourMac brand

CleanYourMac uses warm cream surfaces, a mint and teal palette, rounded display type, and an original leaf-and-sparkle mark. The mark suggests a little room to breathe. It stays legible at app-icon and sidebar sizes.

[tokens.json](tokens.json) is the shared visual contract for the Swift design system and Figma. Colors have explicit light and dark values. Semantic names belong to the shared system; features reuse them rather than adding colors or spacing values.

[logo.svg](logo.svg) is the app icon and public project mark. [glyph.svg](glyph.svg) is the leaf without its outer tile. Both are original vector artwork released under this repository's MIT license. They contain no MacPaw artwork or third-party imagery.

Use the logo on plain surfaces with at least 12% of its width as clear space. Do not stretch it, rotate it, or add effects. The teal tile is part of the app-icon mark; the leaf can appear independently inside the app.

SF Pro, SF Pro Rounded, and SF Mono are platform fonts. The repository does not redistribute font files. Figma should use those families and verify the exact installed style names before creating text. Rounded type is reserved for page headings, section headings, and metrics. Paths use the monospace style.

Cards use a 20-point radius; controls use 12 points; badges use capsules. Shadows stay faint. Selected and hovered controls use semantic fills, visible focus, and text labels. Motion is short, interruptible, and respects Reduce Motion.

Mint, coral, lavender, and amber help distinguish storage categories. Every chart includes text values. A coral highlight alone never communicates a destructive action: review screens name the action and its consequences.
