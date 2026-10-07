# Figma handoff

Figma publication is blocked by connector authentication. The design agent tried the account lookup three times. The first two responses requested a retry after authentication; the third returned `UNAUTHORIZED` / `TRIGGER_REAUTHENTICATION`. No Figma file or URL was created.

The saved source artifacts are:

- `docs/brand/tokens.json`: canonical Comfy light/dark colors, spacing, radii and typography.
- `docs/brand/logo.svg` and `glyph.svg`: original leaf/sparkle branding.
- `design/figma/screens.json`: screen content and fixture data.
- `design/figma/build.js`: editable Figma Plugin API builder for foundations, components, screens and audit phases.
- `design/figma/prepare-plugin.py`: prepares a local development-plugin bundle under `build/figma-plugin/`.
- `design/figma/render-boards.py` and `docs/design-concepts/`: local design boards, clearly separate from actual app screenshots.

These assets were prepared while the connection was unavailable. The builder has **not been executed or verified in Figma**, and the boards are **not evidence of a published Figma file**.

After reconnecting Figma, the next agent should read the Figma create-file, use, generate-library, generate-design and SwiftUI skills. Create the new file, run the builder incrementally, inspect each screen and component, verify variable bindings and font availability, and record the actual file URL here.

For a local development-plugin attempt:

~~~sh
python3 design/figma/prepare-plugin.py
~~~

Then import the generated manifest through Figma's development-plugin UI. The builder requires the declared SF Pro, SF Pro Rounded and SF Mono fonts; it does not silently substitute fonts. The generated plugin is an unverified handoff artifact.
