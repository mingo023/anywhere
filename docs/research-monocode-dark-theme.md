# Research: monocode dark theme and window translucency

Date: 2026-09-30.
- monocode: `hardbeat920/monocode` at `cdc1441dc51e3709cd843e5c316608a123f323c6`.
- Our stack: gpui-kit 0.6.6, gpui-pre 0.3.6.

## TL;DR

- **Palette:** bg and ink are `hsl(hue sat L)` from three variables. Every other surface mixes ink into transparent by a percentage, with no grey ramp (§1).
- **The "opacity" is pane alpha over a blurred desktop.** In dark mode the window is transparent, root paints nothing, and panes paint bg at `--sidebar-opacity` (default 0.85) (§2).
- **Blur is native, not CSS.** On macOS it is the private `CGSSetWindowBackgroundBlurRadius` (radius 1–64, default 24). On Windows it is Acrylic (§3).
- **Light mode is always opaque.** monocode's reason: pale desktops make translucent UI illegible (§3).
- **Mapping:** GPUI has `WindowBackgroundAppearance::Blurred` with fixed blur and no radius. Our theme is light-only `const u32`, so dark needs a runtime theme (§5).

### Citation legend

- `M path:L` = monocode at SHA above.
- `P path:L` = our repo path relative to `packages/desktop/`.
- `R crate/path:L` = `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/...`.
- Hex values marked ≈ are composited over the opaque bg at default settings.

## 1. Palette

- Dark is the default (`M src/features/settings/model/appearance.ts:78`). Light is `html.theme-light`, and the Tailwind `dark` variant is `html:not(.theme-light)` (`M src/styles/index.css:6`).
- Tokens: `@theme` at `M index.css:25-72`, dark `:root` at `:74-97`, light at `:103-116`.
- Hue and saturation (defaults 240 / 0%, so neutral grey) tint both schemes. Dark bg lightness is 0–30%, default 9% (`M appearance.ts:112-122,275-297`).

| Token | Dark | Light |
|---|---|---|
| bg `--color-background-base` | `hsl(h s 9%)` #171717 | `hsl(h s 97%)` #f7f7f7 |
| ink `--color-content` | `hsl(h s 92%)` #ebebeb | `hsl(h s 18%)` #2e2e2e |
| `stroke` (separators) | ink 7% ≈#262626 | ink 7% |
| `selection-subtle` | ink 8% ≈#282828 | ink 5% |
| `selection` | ink 10% ≈#2c2c2c | ink 6% |
| `selection-strong` | ink 12% ≈#303030 | ink 7% |
| `selection-hover` | ink 15% ≈#373737 | ink 10% |
| `selection-emphasis` | ink 20% ≈#414141 | ink 14% |
| accent | `hsl(211 92% 62%)` | same |

Because fills are ink-over-transparent, they stay correct over a translucent pane with no extra tokens.

## 2. Translucent surfaces

- `html`, `body` and `#root` paint bg (`M index.css:118-131`). With native glass in dark mode they paint `transparent` (`:166-170`).
- `.has-native-glass` is set on macOS and Windows (`M src/platform/tauri/platform.ts:9`, `M appearance.ts:301-304`).
- **Sidebar and project rail** (`.sidebar-glass`, `M src/app/shell/ProjectRail.tsx:336`, `M src/app/shell/Sidebar.tsx:2437`):
  - dark + glass: `hsl(h s L / var(--sidebar-opacity))` (`M index.css:275-280`)
  - dark, no glass: bg mixed 90% with black, ≈#151515 (`:267-269`)
  - light: plain bg (`:271-273`)
- **Main pane and session list** (`.body-glass`, `M src/app/App.tsx:10626`, `M Sidebar.tsx:1587`):
  - plain bg by default (`M index.css:293-295`)
  - dark + glass + "Main pane glass" on: bg at `sidebar-opacity × 100%` (`:1215-1221`)
- `--sidebar-opacity` ranges 0.15–1, default 0.85 (`M appearance.ts:124-126,410-414`). "Main pane glass" defaults on (`:136,447-450`).
- Separate from the theme: an optional chat background image at opacity 0.24 (0.05–0.65) (`M appearance.ts:138-144`, `M index.css:297-470`).

## 3. Native window

- Tauri window: `transparent: true`, `titleBarStyle: "Overlay"`, `backgroundColor: "#171717"`, `macOSPrivateApi: true` (`M src-tauri/tauri.conf.json:13,22-26`).
- **Scheme switch:** JS calls `set_window_glass_enabled(scheme === "dark")` on every scheme change (`M appearance.ts:358-377`). The command is at `M src-tauri/src/window.rs:129-152`.
- **macOS glass on** (`M src-tauri/src/macos.rs:215-239`):
  - `setOpaque(false)`.
  - Window bg is clear at alpha 0.01. Fully clear plus a shadow draws a chamfered gap at the corners (`:19-21,234-235`).
  - A 0.01-alpha `NSVisualEffectView` (`UnderWindowBackground`, `BehindWindow`) sits below the WKWebView. Without it, hover repaints and captures briefly show unblurred content (`:241-279`).
  - `CGSSetWindowBackgroundBlurRadius(connection, windowNumber, radius)` is resolved with `dlsym` (`:281-298,411-413`).
- **macOS glass off:** blur 0, opaque `#f7f7f7` (`:221-226`).
- **Launch:** the window is opaque `#171717` during the dock bounce (`:74-77,189-212`). Glass turns on after the first UI paint (`M appearance.ts:379-383`).
- **Windows:** `Effect::Acrylic` with a clear bg. When off, effects are cleared and bg is `#f7f7f7` (`M window.rs:141-150`).
- **Blur radius** is 1–64, default 24, and is stored and applied only while glass is on (`M macos.rs:53-55,159-165`, `M appearance.ts:128-130,433-437`).

## 4. Settings exposed

`M src/features/settings/ui/SettingsView.tsx:2040-2175`:
- Hue and Saturation: "Hue and saturation tint every surface. Lightness only moves the dark theme."
- Dark-mode lightness.
- Translucency, disabled in light mode:
  - Sidebar opacity ("Applies to the project rail and the other glass panes.")
  - Blur radius ("Blur costs more to composite the higher it goes.")
  - Main pane glass.

## 5. Mapping to anywhere

- **Theme:** colours are light-only `const u32` (`P crates/theme/src/theme.rs:7-60`). Dark needs runtime values: the monocode model (bg, ink, ink-alpha fills) or a second constant set chosen at runtime.
- **Window:**
  - `WindowOptions.window_background` (`R gpui-pre-0.3.6/src/platform.rs:2226`) takes `Opaque | Transparent | Blurred | MicaBackdrop | MicaAltBackdrop` (`:2449-2469`).
  - `window.set_background_appearance` switches it at runtime.
  - We open with the default, `Opaque` (`P crates/pocket/src/main.rs:42-50`).
- **GPUI `Blurred` on macOS:**
  - `setOpaque(false)` and window bg alpha 0.0001: the same shadow trick as monocode.
  - An `NSVisualEffectView` subclass (material `Selection`) goes below the content. Its desktop tint layer and saturation filter are removed, as in Zed (`R gpui-pre-macos-0.3.6/src/window.rs:1857-1895,3735-3790`).
  - The blur amount is AppKit's and is not adjustable. A radius setting like monocode's needs our own `CGSSetWindowBackgroundBlurRadius` call via objc.
- **Opacity:** root paints nothing, and panes paint bg as `Hsla` with alpha 0.85. Keep light `Opaque`, as monocode does.
- **Cost:** every `cx.notify()` redraws the whole window, and a blurred transparent window adds WindowServer compositing. Measure in release before shipping.
