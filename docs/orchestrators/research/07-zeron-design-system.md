# 07 — Zeron: design system tokens mapped to Pocket

Date: 2026-09-30.

Sources:
- Zeron `zeronsh/comet` @ `ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5` (MIT), local clone `/Users/mingo/tmp/orchestrators/zeron`. https://zeron.sh was not fetched, so every Zeron claim comes from the clone.
- Pocket worktree `orchestrator-research` @ `86deb13`. `main` @ `b9d14a1` differs in theme only by 4 extra icons (discard, list-flat, list-tree, minus).
- Rust deps from `~/.cargo/registry`: `gpui-pre 0.3.6` and `gpui-component 0.6.6`, both pulled in via `gpui-kit 0.6.6` (P packages/desktop/Cargo.toml:25).

Citation legend:
- `Z` paths are relative to the Zeron clone root, with these shorthands:
  - `Z ui/` = `crates/ui/src/`
  - `Z th/` = `crates/theme/src/`
  - `Z proto/` = `crates/proto/src/`
  - `Z ios/` = `apps/ios/Zeron/`
- `P` paths are relative to the Pocket repo root, with these shorthands:
  - `P th` = `packages/desktop/crates/theme/src/theme.rs`
  - `P ui` = `packages/desktop/crates/ui/src/ui.rs`
  - `P pk/` = `packages/desktop/crates/pocket/src/`
  - `P app/` = `packages/app/src/`
- `R gpui/` = `gpui-pre-0.3.6/src/`; `R gc/` = `gpui-component-0.6.6/src/`.
- `M` is unused (MonoCode is out of scope).
- Hex values marked **(c)** are computed. I re-ran Zeron's own color math (Z th/lib.rs:107-185, Z th/builtins.rs:97-169) on the cited seeds, so these values never appear literally in the source.
- Contrast ratios use the WCAG 2.x formula.
- `[code≠doc]` marks places where code and comments/docs disagree. The code value is the one reported.

## TL;DR

- **Source of truth.**
  - Runtime colors come from the `zeron-theme` crate variants `zeron-dark` / `zeron-light` through `Theme::from_variant` (Z ui/theme.rs:1408-1507).
  - The hand-authored oklch `Theme::dark()` / `light()` (Z ui/theme.rs:1147-1314) is only a fallback and differs from the crate (accent `#7c86ff` vs `#8b7cf6`).
- **Palette.**
  - Neutral near-black / near-white with one violet accent: dark `#8b7cf6`, light `#5b43e8`.
  - Status colors are Tailwind hues: danger `#f87171` / `#dc2626`, warning `#facc15` / `#a16207`, success `#34d399` / `#15803d`.
  - Muted text is contrast-checked to ≥4.5:1 against background; accent primary only to ≥3.0 (Z th/lib.rs:358). Light `faint` `#797981` is not checked and sits at 4.32.
- **Status semantics.** Zeron and Pocket assign colors differently:

  | State | Zeron | Pocket |
  |---|---|---|
  | Working | accent pixel-glyph spinner | green |
  | Awaiting input | accent dot α0.6 | amber |
  | Errored | danger α0.65 | red |
  | Done | success check α0.9 | accent dot |
  | Idle | ink α0.14 | — |

  In Zeron, amber means queued or offline only (Z ui/shell/spaces.rs:2245-2262, Z ui/shell.rs:6956-7010).
- **Type.**
  - Geist and Geist Mono are bundled.
  - The user UI scale runs 12–20, default 16, applied through `ui_rems`.
  - Markdown 14/22, code 12.5/18, terminal 13 (Z ui/typography.rs:84-128, Z ui/markdown/render.rs:31-44).
- **Geometry.**
  - Spacing 4/8/12/16.
  - Radii: control 6, panel 10, popover 12, bubble / dialog 16.
  - Header 44, titlebar 38, control 24 (Z ui/theme.rs:815-846, Z ui/surface_chrome.rs:7-12).
- **Frost** (macOS/Windows).
  - Window is `Blurred`, surfaces at α0.80.
  - Menus get a 16px in-scene backdrop blur, which requires Zeron's gpui fork.
  - No drop shadows while frosted (Z ui/theme.rs:795-813, 1130-1138, Z ui/frost.rs:20).
- **Motion.**
  - 6 named CSS béziers (plus the `EASE_RESORT` alias) and 16 named specs: menu 140/100ms, dialog 180, collapse 180, hover fade 150, entrance 500 expo.
  - `ReduceMotion` System/On/Off, plus a `ZERON_MOTION_SCALE` env var (Z ui/motion.rs:298-426, 812-889).
- **Sound.**
  - 3 session cues (done / request / attention) plus appshot, as embedded WAVs played through `afplay` or `paplay` etc.
  - Priority: Errored > AwaitingInput > Done. 5s startup quiet (connectivity alerts only), 250ms coalesce (Attention cue only), per-cue toggles, all default on (Z ui/sound.rs).
- **Themes.** 19 families / 30 variants, 7 accent presets, opt-in wallpaper tint (Z th/builtins.rs:15-61, Z th/lib.rs:277-325).
- **Pocket today.**
  - Light-only `u32` consts (P th:9-62) used at ~436 call sites.
  - WCAG failures: TEXT_3 3.38, TEXT_4 2.56, FAILED text 3.91, `count_badge` 1.80.
  - A stray iOS-blue button shadow, no dark mode, no sound.
  - F16 is a drop-in `{light, dark}` token patch. F17 maps the phone palette to Zeron's iOS palette.

## Findings

### F1. Theme architecture and color math

- **Pipeline.** `ThemeVariant` seeds → `variant()` derives `ThemeColors` → `AccentRoles` (Z th/builtins.rs:97-169) → `Theme::from_variant` (Z ui/theme.rs:1408-1507).
- **Seeds.**
  - Zeron: `zeron_dark` (Z th/builtins.rs:239-271), `zeron_light` (273-305). Both use `SurfaceTreatment::Frosted`.
  - VS Code Dark+ (L307) and Light+ (L344) use Opaque.
- **Math** (Z th/lib.rs:107-185):
  - `with_alpha` rounds α×255.
  - `mix` rounds per channel.
  - `ensure_contrast(bg, r)` mixes toward black or white in 20 steps.
  - `best_on_color` picks black or white by WCAG luminance.
- **Derivation rules** (Z th/builtins.rs:106-145):

  | Role | Rule |
  |---|---|
  | `muted` | `ensure_contrast(background, 4.5)` |
  | `faint` | seed, **not checked** |
  | `dialog` | `card.mix(raised, 0.18 dark / 0.04 light)` |
  | `overlay` | `card.mix(raised, 0.34 / 0.02)` |
  | `hover` | white/black α0.11 / 0.06 |
  | `active` | accent α0.18 / 0.10 |
  | `border` | α0.10 / 0.12 |
  | `border_strong` | α0.18 / 0.22 |
  | `solid` | `rgb(235,235,239)` / `rgb(35,35,40)` |
  | `on_solid` | `best_on_color` |
  | `*_muted` | `danger.mix(text, 0.28)`; `warning` / `success` `.mix(text, 0.25)` |
  | `input` | dark `raised` α0.72, light `card` |
  | `cursor` | text α0.40 / 0.55 |
  | `diff_add` / `diff_delete` | = `success` / `danger` |
  | `diff_hunk` | accent α0.08 / 0.07 |
  | terminal fg | `text.ensure_contrast(term_bg, 4.5)` |
  | terminal selection | α0.22 / 0.16 |

- **`AccentRoles::derive`** (Z th/lib.rs:357-390): `primary.ensure_contrast(background, 3.0)` (every preset passes unchanged), `on_accent = best_on_color`, and `strong = primary` unless `on` vs primary is below 4.5.

### F2. Zeron palette (runtime, crate variants)

Seeds are from Z th/builtins.rs:239-305. Derived values are **(c)**. Contrast is measured against `background`.

| Role | Dark | Light | Notes |
|---|---|---|---|
| background | `#060606` | `#ffffff` | main content |
| shell | `#0d0d0d` | `#f3f3f5` | sidebar / chrome |
| card | `#0e0e0e` | `#ffffff` | |
| raised | `#343438` | `#ededf0` | |
| dialog (c) | `#151516` | `#fefefe` | |
| overlay (c) | `#1b1b1c` | `#ffffff` | |
| hover (c) | `#ffffff1c` | `#0000000f` | |
| active (c) | `#8b7cf62e` | `#5b43e81a` | |
| border (c) | `#ffffff1a` | `#0000001f` | |
| border_strong (c) | `#ffffff2e` | `#00000038` | |
| text | `#e8e8ea` 16.56 | `#303035` 13.13 | |
| text_muted | `#a9a9ae` 8.66 | `#62626a` 6.04 | |
| text_faint | `#85858a` 5.52 | `#797981` **4.32** | light fails AA |
| solid / on_solid | `#ebebef` / `#000000` | `#232328` / `#ffffff` | |
| danger / muted (c) | `#f87171` / `#f49293` | `#dc2626` 4.83 / `#ac292a` | |
| warning / muted (c) | `#facc15` / `#f6d34a` | `#a16207` 4.92 / `#855612` | |
| success / muted (c) | `#34d399` / `#61d8ad` | `#15803d` 5.02 / `#1c6c3b` | |
| input (c) | `#343438b8` | `#ffffff` | |
| cursor (c) | `#e8e8ea66` | `#3030358c` | |
| diff_hunk (c) | `#8b7cf614` | `#5b43e812` | |
| accent primary = strong | `#8b7cf6` 6.09 | `#5b43e8` 6.15 | |
| on_accent (c) | **`#000000`** | `#ffffff` | white on `#8b7cf6` = 3.33 |
| wash (c) | `#8b7cf638` | `#5b43e81f` | |
| selection (c) | `#8b7cf659` | `#5b43e83d` | |
| glyph light / mid / dark (c) | `#aba1f9` / `#8b7cf6` / `#7266ca` | `#7965ec` / `#5b43e8` / `#4332ac` | |
| terminal bg / fg | `#090909` / `#e8e8ea` | `#fafafa` / `#303035` | |
| terminal selection (c) | `#ffffff38` | `#00000029` | |

**ANSI 16** (Z th/builtins.rs:229-237):
- **Dark:** `#242424 #f87171 #4ade80 #facc15 #60a5fa #c084fc #22d3ee #d4d4d8 / #52525b #fca5a5 #86efac #fde047 #93c5fd #d8b4fe #67e8f9 #fafafa`
- **Light:** `#1f1f1f #dc2626 #16a34a #b45309 #2563eb #9333ea #0e7490 #3f3f46 / #71717a #b91c1c #15803d #92400e #1d4ed8 #7e22ce #155e75 #18181b`

**Syntax, 12 slots** (seeds Z th/builtins.rs:263-266, 297-300; slot order L171-185). Values are dark / light:

| Slot | Dark | Light |
|---|---|---|
| comment | `#92929a` | `#6b7280` |
| keyword | `#8b7cf6` | `#5b43e8` |
| string | `#34d399` | `#15803d` |
| number / boolean | `#facc15` | `#a16207` |
| type | `#c084fc` | `#7e22ce` |
| function | `#60a5fa` | `#2563eb` |
| property | `#f472b6` | `#be185d` |
| variable | `#e8e8ea` | `#303035` |
| punctuation | `#a1a1aa` | `#52525b` |
| tag | `#f472b6` | `#be185d` |
| attribute / stringSpecial / escape | `#22d3ee` | `#0e7490` |
| invalid | `#f87171` | `#b91c1c` |

### F3. Hand-authored fallback and ink helpers (Z ui/theme.rs)

**Fallback theme** (`Theme::dark` L1147-1214, `light` L1228-1314, oklch neutrals L87-131). It differs from the crate in these values:
- accent `#7c86ff`, with an oklch `strong`
- dark border white 0.08, border_strong 0.14
- raised `neutral(0.235)`, faint `neutral(0.556)`
- `text_dim` `#989898`
- danger `rgb(255,100,103)`, warning `rgb(255,185,0)`

Pocket should follow the crate values; the fallback is not what users see.

**Ink helpers** (L1692-1800):

| Helper | Dark | Light |
|---|---|---|
| `ink(a)` | white α | black α |
| `hairline(a)` | white α | black `min(α×1.35, 0.5)` |
| `wash` | hsl L0.92 | hsl L0.10 |
| `scrim` (`SCRIM_ALPHA_DARK` L1737) | black 0.60 | black 0.32×(a/0.60) |
| `band` | black 0.16 | black 0.045 |
| `glass_selected_bg` (L1781) | wash 0.11 | wash 0.06 |
| `user_bubble_bg` (L1792) | 0.08 | 0.04 |

- `card_selected_shadows` is an inset ring (L1842).
- Scrollbar thumb: text α0.30 rest / 0.42 hover / 0.55 drag (~L1630).

**Semantic comments:** warning = "amber (offline notices, awaiting-input)" (L682).

### F4. Accent presets (Z th/lib.rs:277-325)

`strong = primary` for every preset. `on_accent` is black in dark and white in light for every preset.

| Preset | Dark | Light |
|---|---|---|
| Zeron | `#8b7cf6` | `#5b43e8` |
| Orange | `#fb923c` | `#c2410c` |
| Amber | `#fbbf24` | `#a16207` |
| Green | `#4ade80` | `#15803d` |
| Cyan | `#22d3ee` | `#0e7490` |
| Blue | `#60a5fa` | `#2563eb` |
| Pink | `#f472b6` | `#be185d` |

### F5. Surfaces, elevation, frost

- **Glass alpha.** `GLASS_ALPHA` 0.80 and `GLASS_ALPHA_LIGHT` 0.80 on macOS/Windows; 1.0 elsewhere (Z ui/theme.rs:795-813).
- **`glass()`** (L852-900) multiplies surface alpha by a contrast-checked factor. The adverse backdrop is white for dark and black for light; text must stay ≥4.5 and muted ≥3.0. The effective alpha is 0.80 in both modes **(c)**.
- **Helpers.**
  - `is_glass()`
  - `panel_bg` = bg×0.4 (L906)
  - `is_frost` (L921)
  - Toolbar bg = `surface` α0.26 when glass (Z ui/surface_chrome.rs:42)
  - Surface input field = `ink(0.035)`, h24, px8, r6, gap6, 11.5px (Z ui/surface_chrome.rs:15-27)
- **Window background** (`window_background_appearance`, L1130-1138): Linux uses `Transparent`, glass uses `Blurred`, everything else `Opaque`.
  - `[code≠doc]` The comment at L1118-1129 implies light mode is opaque ("restores vibrancy when user switches back to dark"), but `GLASS_ALPHA_LIGHT` = 0.80.
- **Menus.** `MENU_BLUR` = 16px in-scene backdrop blur via `paint_backdrop_blur` (Z ui/frost.rs:20, 24, 84). This lives in Zeron's zui gpui fork; `gpui-pre 0.3.6` has no backdrop blur or `EdgeFade`.
  - `gpui-pre` does have `WindowBackgroundAppearance {Opaque, Transparent, Blurred, MicaBackdrop, MicaAltBackdrop}` (R gpui/platform.rs:2449-2468) and `Window::set_background_appearance` (R gpui/window.rs:2901).
- **Shadows.** Popover and dialog cards use gpui `shadow_lg()` only when `!is_frost()` (Z ui/popover.rs:334, 1185). Palette cards always use it (Z ui/popover.rs:1010). Frosted surfaces rely on hairline borders instead: popover `theme.border`, dialog/palette `hairline(0.10)`.
- **Wallpaper tint** (opt-in, default false, Z ui/settings.rs:981).
  - Surfaces mix wallpaper color 0.88 / 0.94 (Z ui/settings/wallpaper_colors.rs:52-54).
  - hover accent α0.09, active 0.15, border 0.14, border_strong 0.30 (L71-74).
  - The Frosted override uses white 0.09 / 0.15 for hover / active (Z ui/theme.rs:1394-1400).

### F6. Status tokens

- **Model.** `Indicator {None, Working, AwaitingInput, Errored}` (Z proto/view.rs:31-36). `SESSION_STALE_MS` = 45000 (L41).
- **Dot colors** (`status_dot_color`, Z ui/shell/spaces.rs:2245-2262):

  | State | Color |
  |---|---|
  | Working | `busy` α0.55 |
  | AwaitingInput | `accent` α0.6 |
  | Errored | `danger` α0.65 |
  | Completed | `success` α0.9 |
  | Idle | `ink(0.14)` |

  `[code≠doc]` The comment says AwaitingInput is "Blue"; the code uses accent.
- **Sidebar row status** (Z ui/shell.rs:6956-7010, 7099-7130):
  - Label overrides: undelivered → danger "Failed"; queued → warning "Queued".
  - Labels: `Working` / `Input` / `Failed` / `Done`.
  - Glyphs: Working = `mini_glyph_spinner(2.0, theme.glyph)`; Done = `CHECK` icon 11px; others = 6px round dot.
  - Slot 13px, label 10px (`ui_rems`) MEDIUM in the status color, gap 4.
- **Status strip** (Z ui/shell.rs:10398-10467):
  - h24 (`STATUS_STRIP_HEIGHT`), px `SPACE_LG`+8, gap 8, text 11.
  - Errored → "Run failed" in danger.
  - Sending → `gradient_spinner` 2.5 + "Sending…" 12px muted.
  - Working and AwaitingInput leave the strip empty.
- **Badge recipe** (Z ui/change_requests.rs:158-172):
  - h16 (composer 20), px4 (7), r4 (6).
  - bg tone α0.08, hover α0.16.
  - text 10px (11) MEDIUM, tone α0.85.
  - Tones (L24-26): Open = success, Merged = `code_text` (accent), Closed = danger.
- **Jump hint** (Z ui/shell.rs:7018-7040): 16px, px4, r4, fill α0.08, text α0.85, mono 10 MEDIUM.
- **iOS mirror** (Z ios/Design/StatusGlyph.swift:3-11): working 0.55, input 0.6, failed 0.65, done 0.9, idle 0.14, time muted 0.5. See also trailer tints L59, 0.75s spin L159, PR badge L187-203.

### F7. Diff tokens (Z ui/changes.rs)

- **Geometry:**
  - `DIFF_LINE_HEIGHT` 21 (L74)
  - notice 24
  - `MARKER_WIDTH` 28 (L80)
  - `ACCENT_BAR_WIDTH` 3 (L82)
  - `DIFF_TEXT_SIZE` 12 (L88)
- **Row colors:**
  - row wash = tone α0.055 (L4446-4450)
  - bar tone α0.55
  - line numbers tone α0.9 (L4452-4466)
  - context marker faint α0.5
- **Warning notice** bg α0.08 (L4319).
- **iOS equivalents** (Z ios/Design/Palette.swift:58-94): diffAddWash 0.055, diffAddBar 0.55, diffHunk 0.07 / 0.08.

### F8. Typography

- **Families.** 16 bundled faces in `Z crates/ui/assets/fonts`: Geist and Geist Mono, each in Regular, Medium, SemiBold, Bold with italics. iOS registers 12 faces (Z ios/Core/Fonts.swift:6-20).
- **UI size.** `UiFontSize::ALL` = [12, 13, 14, 15, 16, 18, 20], default 16. Sizes scale through `ui_rems` (Z ui/typography.rs:84-117).
- **Fixed sizes:** `CODE` 12.5, `TERMINAL` 13, clamp `MIN` 8 / `MAX` 32 (L125-128).

**Role scale:**

| Role | Size / line height | Weight | Source |
|---|---|---|---|
| Markdown body | 14 / 22, block gap 12 | Regular | Z ui/markdown/render.rs:31-34 |
| h1 / h2 / h3 / h4+ | 19/27, 16/24, 15/22, 14/22 | SemiBold | render.rs:758-765 |
| Code block | 12.5 / 18, pad 12×10, header 28, action 22 | Mono | render.rs:36-44 |
| Table | cell pad 12, divider 1, header BOLD | | render.rs:54-58 |
| Composer input | 14 / 22.75 | | Z ui/composer.rs:99-100 |
| User message | line height 22 | | Z ui/transcript.rs:168 |
| Tool row | 12, icon 16, output line height 18 | | transcript.rs:108-109, 791 |
| Menu / popover | 13 | | Z ui/popover.rs:339 |
| Dialog title / body | 15 SemiBold / 13 on 19 | | popover.rs:1194, 1203-1204 |
| Badges (context) | h24, pill r8, icon 12, text 12, card w320, hover delay 280ms | | Z ui/badges.rs:57-63 |
| Row status label | 10 | Medium | Z ui/shell.rs:7099-7130 |
| Status strip | 11 | | Z ui/shell.rs:10398-10467 |

### F9. Spacing, radii, layout (Z ui/theme.rs unless noted)

| Token | Value | Line |
|---|---|---|
| `SPACE_XS/SM/MD/LG` | 4 / 8 / 12 / 16 | 839-842 |
| `TEXT_STACK_GAP` | 1 | 846 |
| `HEADER_HEIGHT` | 44 | 815 |
| `TITLEBAR_HEIGHT` | 38 | 819 |
| `TOP_PAD` | 4 | 822 |
| `STATUS_STRIP_HEIGHT` | 24 | 825 |
| `TRANSCRIPT_FADE_BAND` | 24 | 831 |
| `BUBBLE_RADIUS` | 16 | 833 |
| `PANEL_RADIUS` | 10 (queue panel 16, Z ui/queue.rs:82) | 835 |
| `CONTROL_RADIUS` | 6 | 837 |
| Surface chrome | header 38, control 24, r6, icon 14, gap 4, edge inset 8 | Z ui/surface_chrome.rs:7-12 |
| Popover card | r12, inset 4, item gap 2, item r7, max h320 | Z ui/popover.rs:307-315, 374 |
| Palette item | r10, action rows 32 | Z ui/popover.rs:315, `docs/screenshots/command-palette/README.md` |
| Dialog card | w360, p20, r16 | Z ui/popover.rs:1177-1181 |
| Dialog field | px12, py8 | popover.rs:1209-1215 |
| Menu scrollbar | track inset 4, hit 10, thumb 3 → 5 on hover, min 24, linger 1400ms, fade 260ms | popover.rs:1369-1381 |

### F10. Icons

- **UI icons.** Solar Icons, Linear style, CC BY 4.0, by 480 Design (Z ui/icons.rs:4-7, 55). 115 SVGs in `crates/ui/assets/icons`.
- **File icons.** `miguelsolorio/vscode-symbols` (`crates/ui/assets/file-icons/README.md:4`).
  - Dark recolor map at Z ui/file_icons.rs:25+.
  - Icon well bg α0.32 when frosted, else 0.16 (L44-45).
- **Sizes:** chrome 14, activity 16, badge 12, done check 11, status dot 6.

### F11. Motion (Z ui/motion.rs)

**Easings** (L298-310):

| Name | Curve |
|---|---|
| `EASE_OUT_EXPO` | (0.16, 1, 0.3, 1) |
| `EASE_OUT` | (0, 0, 0.58, 1) |
| `EASE` | (0.25, 0.1, 0.25, 1) |
| `EASE_OUT_QUINT` = `EASE_RESORT` | (0.22, 1, 0.36, 1) |
| `EASE_IN_OUT` | (0.42, 0, 0.58, 1) |
| `EASE_TAILWIND` | (0.4, 0, 0.2, 1) (L408) |

**Specs** (L373-415):

| Spec | ms | Easing | Use |
|---|---|---|---|
| `FADE_IN` | 500 | expo | entrance, translateY 4→0 |
| `FADE_QUICK` | 150 | ease | quick fades |
| `WALLPAPER_CROSSFADE` | 180 | (1/3, 1, 2/3, 1) | wallpaper swap |
| `MENU_IN` | 140 | ease | popover, translateY −2 |
| `MENU_OUT` | 100 | ease | popover close |
| `DIALOG_IN` | 180 | ease | dialog |
| `SPLASH_OUT` | 500 + 150 delay | ease | boot splash, −6px |
| `RESIZE` | 200 | ease-out | sidebar/pane size |
| `TAB_SLIDE` | 150 | ease-out | terminal tab reorder |
| `COLLAPSE` | 180 | ease-out | diff file collapse |
| `NEW_THREAD_TRANSITION` | 420 | quint | composer handoff |
| `CHEVRON` | 200 | ease | crossfade (no rotation) |
| `SCROLL_GLIDE` | 500 | ease-in-out | scroll-to-row |
| `HOVER_FADE` | 150 | tailwind | every hover wash |
| `ZERON_PULSE` | 2400 | ease | loader |
| `GRADIENT_SPIN` | 750 | ease | working wave |

- `[code≠doc]` The catalog (L1-30) says `menu-in` / `dialog-in` "scale 0.96". The code approximates scale with fade + translate because gpui divs have no scale transform (L26-30).
- **Resize edge:** nudge 5px, 220ms, out fraction 0.32 (L424-426).
- **Clocks:** `PULSE_TICK` 33ms (L56), lease 300ms (L61). Hover state is stored in a `thread_local` (L738).
- **Global controls.**
  - `ZERON_MOTION_SCALE` env var (L812-819).
  - `ReduceMotion {System, On, Off}` plus pause-in-background (L842-889).
  - Settings label "Reduce motion" (Z ui/settings/appearance.rs:3014).
- **Loader math** (Z proto/motion.rs):
  - 5 cells, 3×3 matrix (L19-21)
  - opacity min 0.08, scale min 0.9 (L24-26)
  - stagger 0.15 / 2.4 (L28)
  - row tints `#B6D3EF #EDB185 #F888A0`, dim 0.1 (L32-34)
  - mark 34 cells, spread 0.55 (L97-106)
- **Loaders** (Z ui/loaders.rs): `zeron_mark_loader` L30, `zeron_loader` 74, `gradient_spinner` 115, `mini_glyph_spinner` 152, `mini_mono_spinner` 165.
- **iOS glass:** `UIGlassEffect`, appear 0.35s / disappear 0.25s, honors reduce motion (Z ios/Design/Glass.swift:35).
- **Pocket compatibility.** `gpui-pre` already honors `App::reduce_motion` in `with_animation` (R gpui/elements/animation.rs:301, 407; setter R gpui/app.rs:1144). Its `ease_out_quint` is the polynomial 1-(1-t)^5, not a bézier (R gpui/elements/animation.rs:526-528).

### F12. Sound (Z ui/sound.rs)

- **Cues.** `Sound {Done, Request, Attention}` (L31-37).
  - Assets `appshot.wav`, `attention.wav`, `done.wav`, `request.wav` are embedded with `include_bytes` (L23-27).
  - Kill switch: `ZERON_DISABLE_SOUND` env var (L19).
- **Players:**
  - macOS: `afplay` (L224-227).
  - Windows: PowerShell `Media.SoundPlayer.PlaySync` (L229-253).
  - Linux: `paplay` → `pw-play` → `aplay -q` → `ffplay` → `mpv` (L255-273).
  - Wait is bounded at 10s (L275+).
- **Triggers** (`sound_since`, L339-356), first match wins:
  1. Errored → Attention
  2. AwaitingInput → Request
  3. New `last_completed_turn` with no pending send → Done
  - Connectivity Offline / Reconnecting → Attention (L361-370).
- **Gates:** `STARTUP_QUIET` 5s (L383) arms only connectivity alerts (`ConnectivityNotificationState`); `COALESCE` 250ms (L417, `AttentionSoundGate`) applies only to the Attention cue (Z ui/shell.rs:2633-2636). Done / Request are not gated by either.
- **Settings** (Z ui/settings.rs:792-798, 932-935, 1515-1520): master `sound_enabled` plus completion / input / attention toggles, all default true.
  - Copy (Z ui/settings/notifications.rs:190-259): "Session sounds", "Task completed", "Input required", "Errors and disconnections".
- **Design notes** (`docs/sound-design/README.md:1-35`; `docs/sound-design/auditions/README.md`):
  - The four-cue signature is synthesized with the Python stdlib.
  - Format: 48kHz 16-bit stereo, peaks −18.7 to −20.1 dBFS, 0.30–0.79s.
  - "Frequent actions such as Send and Undo stay silent."

### F13. Built-in themes (Z th/builtins.rs:15-61)

19 families, 30 variants:
- zeron (light, dark)
- vscode-default (light, dark)
- catppuccin (latte, mocha)
- tokyo-night (light, dark)
- dracula
- github (light, dark)
- ayu (light, dark, mirage)
- gruvbox (light, dark)
- rose-pine (dawn, moon)
- nord
- one-dark-pro
- atom-one-dark
- night-owl (light, dark)
- winter-is-coming (light, dark-blue)
- palenight
- synthwave-84
- shades-of-purple
- cobalt2
- andromeda

Every variant carries a provenance hash (sha256 of the resolved definition, L162-167). Only zeron uses Frosted.

### F14. Zeron iOS palette (Z ios/Design/Palette.swift)

`[code≠doc]` The comment at L3-4 says "single teal"; the code accent is violet.

| Role | Light | Dark |
|---|---|---|
| background (L13) | `#F3F3F5` | `#060606` |
| elevated | `#FFFFFF` | `#111113` |
| rowActive | white 0.9 | white 0.06 |
| text | `#27272C` | `#E8E8EA` |
| secondary | `#62626A` | `#A9A9AE` |
| tertiary | `#97979F` | `#6B6B72` |
| textFaint | `#797981` | `#85858A` |
| hairline | `#E2E2E6` | `#1E1E22` |
| accent / accentSoft α0.12 | `#5B43E8` | `#8B7CF6` |
| controlFill α0.075 | `#27272C` | `#E8E8EA` |
| danger / success / warning | `#DC2626` / `#15803D` / `#A16207` | `#F87171` / `#34D399` / `#FACC15` |
| userBubble | `#FFFFFF` | `#19191C` |
| codeBackground / codeBorder | `#FAFAFB` / `#E4E4E8` | `#0B0B0D` / `#1F1F23` |
| chip (L31) | `#E7E7EB` | `#1C1C20` |
| inlineCodeText | `#3F3F46` | `#DCDCE0` |

Alpha roles:
- quoteBar accent 0.45
- toolBadge 0.06
- agentCard 0.03
- subline muted 0.5 (L43)

projectDots, 8 pairs light / dark (L36-40):
- `475569/94A3B8`
- `2563EB/93C5FD`
- `7C3AED/C4B5FD`
- `BE123C/FDA4AF`
- `A16207/FCD34D`
- `047857/6EE7B7`
- `0F766E/5EEAD4`
- `C2410C/FDBA74`

### F15. Pocket current state

**Desktop tokens** (P th). All are light-only `u32` consts:
- Fonts: `SANS` = `.SystemUIFont` (L6), `MONO` = Geist Mono (L7). Geist and Geist Mono are bundled in 4 weights without italics (L124-133), but UI text uses the system font.
- Text: `TEXT #111113`, `TEXT_BODY #3f3f46`, `TEXT_2 #6f6f78`, `TEXT_3 #8b8b94`, `TEXT_4 #a1a1aa`, `TEXT_5 #b4b4bc`, `TEXT_6 #d4d4d8`, `WHITE` (L9-16).
- Surfaces: `WINDOW #f4f4f5`, `SURFACE #fff`, `SURFACE_SUNKEN #fafafa` (L18-20).
- Fills and lines: `#111113` at α `08/0b/0e/11` for fills; hairline `12`, separator `17`, strong `1f` (L22-28).
- Accent: `#5b5bd6`; BG `1c`, RING `33`, TINT `12`, GLOW `66` (L30-34).
- Status (L36-43):
  - WAITING `#ffb224`, text `#ad5700`, bg α`2e`
  - RUNNING `#30a46c`, text `#18794e`, bg `21`
  - FAILED `#e5484d`, bg `1c`
- Agents: `AGENT_CLAUDE #d97757`, `AGENT_CODEX #0f9d8a` (L45-46).
- Diff: add bg `30a46c1c` / text `18794e`; del bg `e5484d17` / text `cd2b31`; word α`40` / `38` (L48-53).
- Other: `MODIFIED #ad5700`, `TEAL #0e7c86` (L55-57).
- Syntax, only 4 roles (L59-72): keyword `#8e4ec6`, fn `#3e63dd`, string `#18794e`, comment `#a1a1aa`.
- `PALETTE`: 6 repo colors (L96).
- Spinner: 1s rotation (L118-122).
- `ICONS`: 40 SVGs, 24 viewBox, stroke 1.8, round caps (L143-147).
- `init`: font 14, mono 13, gpui-kit theme fields (L163-178).

**Components** (P ui):

| Component | Spec | Lines |
|---|---|---|
| `side` | bg `#fafafbb3`, 0.5px border | L27-29 |
| `pop` | r26, ring 0.5, highlight `#fffffff2`, shadows `00000024 18/50` + `0000000f 2/6` | L32-34 |
| `glass` | `#ffffff9e` | L42-44 |
| button | h32 px12 gap7 r16 13px; large h36 r18; Primary = `TEXT` bg | L70-102 |
| Accent button | shadow **`0x0a84ff59`** (iOS system blue, not the accent) | L91 |
| `icon_button` | 28, r7 | L117-119 |
| segmented swap | 120ms `ease_out_quint` | L134-137 |
| status pills | h20 r10 11.5 SemiBold; NeedsYou amber, Working green + spinner, Failed red ×, Done accent dot | L195-243 |
| `sidebar_row` | h32 r9; selected `FILL_4`, hover `FILL_2`, no hover transition | L313-327 |
| `session_row` | px10 py10 r8; title 14/20 SemiBold; meta 12 `TEXT_3` | L428-450 |
| `tree_row` | h28 r8 13.5 | L474-503 |
| `count_badge` | `WAITING` bg + `WHITE` 10 bold (1.80:1) | L584-601 |
| `section_header` | 12/15 SemiBold `TEXT_3` | L670-685 |
| `field_box` | h38 r11 `#ffffffd9` | L695-705 |
| `backdrop` | `0x28241e` + α | L708-710 |
| modal title | 20 bold | L713-726 |
| `palette_row` | h40 r10 | L728-746 |
| `menu_item` | h32 r8 13.5 | L748-763 |
| `trigger_field` | h36 r12 `#ffffffb3` | L800-818 |

**Motion in `P pk/overlay.rs:311-330`:**
- Palette and NewSession have no entrance ("motion would only slow them").
- AddRepo 200ms / 8px, More 150ms / −4px, Confirm 200ms / 8px, all `ease_out_quint`.
- Backdrop alphas `1f` / `2e` / `40`.

**Other desktop:**
- Mermaid: in-use 250ms, fade 200ms (P pk/mermaid.rs:15-16).
- Window (P pk/main.rs:1060-1068): transparent titlebar, traffic lights at 14,14, Opaque background.
- Terminal (P pk/termview.rs): 13/22 main, 12/19 small (L11-12); ink `TEXT` on paper `WHITE` (L42-43). It runs on libghostty-vt (P packages/desktop/crates/term/src/shim.c:1, 61-62).
- No sound code anywhere.
- ~436 token references across the pocket, ui, and storybook sources (rg word count of the 46 `u32` const names); ~10 raw `rgba(0x…)` literals in pocket/ui.
- gpui-component provides `Theme::sync_system_appearance` / `Theme::change` / `ThemeMode` (R gc/theme/mod.rs:228, 261, 701).

**Contrast audit** (c). Ratios are on `#fff` / on `WINDOW #f4f4f5`:

| Token | On white | On WINDOW | Verdict |
|---|---|---|---|
| TEXT | 18.86 | 17.16 | pass |
| TEXT_BODY | 10.44 | 9.50 | pass |
| TEXT_2 | 4.98 | 4.53 | pass |
| TEXT_3 | 3.38 | 3.07 | **fails as meta text** |
| TEXT_4 | 2.56 | 2.33 | **fails** |
| TEXT_5 | 2.06 | 1.87 | decorative only |
| ACCENT | 5.37 | 4.88 | pass |
| WAITING_TEXT | 5.07 | 4.61 | pass |
| RUNNING_TEXT | 5.41 | 4.92 | pass |
| FAILED as text | 3.91 | 3.56 | **fails** |
| white on FAILED | 3.91 | | **fails** |
| `count_badge` white on `#ffb224` | 1.80 | | **fails** |

**Phone** (P app/):
- `design.ts:1-35` is dark-only. It is used by App, ChatScreen, Composer, Timeline, ToolGroup, TaskList, DiffView, Markdown, PlanCard, syntax.
  - Neutrals: bg `#0B0C0E`, text `#EDEBE6` (16.43), body `#D9D7D2` (13.61), muted `#A1A4AA` (7.83), faint `#7E828A` (5.08), lineNo `#5B5F66` (3.05)
  - Lines and cards: rule `#1D2024`, card `#15171A`, stroke `#272A30`
  - Status and chat: green `#5BE38A`, red `#F07167`, teal `#5CC8BE`, bubble `#1C3326` / `#BFF5D2`
  - Diff: add `#12251A` / `#8BF0AD`, del `#2A1616` / `#F5A39B`
  - Syntax: keyword `#5CC8BE`, typeName `#E7C98B`
  - Fonts: Geist and GeistMono, 400/500/600
- A second, separate palette lives in `theme.ts:1-14` (bg `#0b0d10`, accent `#7aa2f7`, ok `#3fb950`, error `#f85149`, warn `#d29922`, radius 12). Only ConnectScreen, AgentsScreen, and PermissionSheet use it.

### F16. Mapping table / theme patch (desktop)

**Patch shape.** Replace the `pub const X: u32` items in P th with a `Palette` struct holding the same field names, plus two statics `LIGHT` and `DARK`. `pub fn p(cx: &App) -> &'static Palette` selects between them from gpui-kit `ThemeMode`. Call sites change mechanically from `rgba(TEXT)` to `rgba(p(cx).text)`.

The light column keeps Pocket's identity unless a contrast fix or Zeron alignment is needed. The dark column is Zeron's crate value, or derived from it with Zeron's rules. Contrast figures are **(c)**.

| Pocket token | Zeron source | Now (light) | Proposed light | Proposed dark |
|---|---|---|---|---|
| `WINDOW` | shell | `#f4f4f5` | `#f3f3f5` | `#0d0d0d` |
| `SURFACE` | background | `#ffffff` | `#ffffff` | `#060606` |
| `SURFACE_SUNKEN` | terminal.background | `#fafafa` | `#fafafa` | `#090909` |
| new `SURFACE_RAISED` | raised | — | `#ededf0` | `#343438` |
| new `DIALOG` | dialog | — | `#fefefe` | `#151516` |
| new `OVERLAY` | overlay | — | `#ffffff` | `#1b1b1c` |
| `TEXT` | text | `#111113` | keep `#111113` (17.02 on shell) | `#e8e8ea` (15.88 on shell) |
| `TEXT_BODY` | iOS inlineCodeText | `#3f3f46` | keep | `#dcdce0` (14.82) |
| `TEXT_2` | text_muted | `#6f6f78` | `#62626a` (5.45 on shell) | `#a9a9ae` (8.30) |
| `TEXT_3` | text_faint | `#8b8b94` | `#797981` (4.32 on white, 3.90 on shell) | `#85858a` (5.29) |
| `TEXT_4` | iOS tertiary | `#a1a1aa` | `#97979f` (decorative) | `#6b6b72` (3.68) |
| `TEXT_5` | faint.mix(bg, 0.4) (c) | `#b4b4bc` | keep | `#525255` |
| `TEXT_6` | faint.mix(bg, 0.6) (c) | `#d4d4d8` | keep | `#39393b` |
| `WHITE` → `ON_SOLID` | on_solid | `#ffffff` | `#ffffff` | `#000000` |
| `WHITE` → `ON_ACCENT` | on_accent | `#ffffff` | `#ffffff` | `#000000` (6.31) |
| `WHITE` → `PAPER` | terminal bg | `#ffffff` | `#ffffff` | `#090909` |
| new `SOLID` (Primary button, avatar, checkbox) | solid | `TEXT` | `#232328` | `#ebebef` |
| `FILL_1` | light α × 1.83 (hover 0.11/0.06) | `#11111308` | keep | `#ffffff0f` |
| `FILL_2` | same | `#1111130b` | keep | `#ffffff14` |
| `FILL_3` | same | `#1111130e` | keep | `#ffffff1a` |
| `FILL_4` | ≈ hover | `#11111311` | keep | `#ffffff1f` |
| `HAIRLINE` | fallback border 0.08 | `#11111312` | keep | `#ffffff14` |
| `SEPARATOR` | border | `#11111317` | keep (Zeron `#0000001f`) | `#ffffff1a` |
| `SEPARATOR_STRONG` | border_strong | `#1111131f` | keep (Zeron `#00000038`) | `#ffffff2e` |
| `ACCENT` | accent.primary | `#5b5bd6` | `#5b43e8` (6.15) | `#8b7cf6` (6.09) |
| `ACCENT_BG` | wash | `#5b5bd61c` | `#5b43e81f` | `#8b7cf638` |
| `ACCENT_RING` | selection | `#5b5bd633` | `#5b43e83d` | `#8b7cf659` |
| `ACCENT_TINT` | active | `#5b5bd612` | `#5b43e81a` | `#8b7cf62e` |
| `ACCENT_GLOW` | α0.40 | `#5b5bd666` | `#5b43e866` | `#8b7cf666` |
| `WAITING` (dot / fill) | warning | `#ffb224` | keep `#ffb224` | `#facc15` |
| `WAITING_TEXT` | warning | `#ad5700` | `#a16207` (4.92) | `#facc15` (13.23) |
| `WAITING_BG` | badge tone α0.08 | `#ffb2242e` | `#a1620714` | `#facc1514` |
| `RUNNING` | busy / accent, dot α0.55 | `#30a46c` | `#5b43e8` | `#8b7cf6` |
| `RUNNING_TEXT` | accent | `#18794e` | `#5b43e8` | `#8b7cf6` |
| `RUNNING_BG` | accent α0.08 | `#30a46c21` | `#5b43e814` | `#8b7cf614` |
| new `SUCCESS` (Done) | success | — | `#15803d` (5.02) | `#34d399` (10.54) |
| new `SUCCESS_BG` | α0.08 | — | `#15803d14` | `#34d39914` |
| `FAILED` | danger | `#e5484d` | `#dc2626` (4.83; use `#b91c1c` 5.89 on `WINDOW`) | `#f87171` (7.32) |
| `FAILED_BG` | α0.08 | `#e5484d1c` | `#dc262614` | `#f8717114` |
| new `*_MUTED` | danger / warning / success_muted | — | `#ac292a` / `#855612` / `#1c6c3b` | `#f49293` / `#f6d34a` / `#61d8ad` |
| `AGENT_CLAUDE` / `AGENT_CODEX` | none (brand marks) | `#d97757` / `#0f9d8a` | keep | keep |
| `DIFF_ADD_BG` | success α0.055 | `#30a46c1c` | `#15803d0e` | `#34d3990e` |
| `DIFF_DEL_BG` | danger α0.055 | `#e5484d17` | `#dc26260e` | `#f871710e` |
| `DIFF_ADD_TEXT` | diff_add | `#18794e` | `#15803d` | `#34d399` |
| `DIFF_DEL_TEXT` | diff_delete | `#cd2b31` | `#dc2626` | `#f87171` |
| `DIFF_ADD_WORD` | keep α0.25 | `#30a46c40` | `#15803d40` | `#34d39940` |
| `DIFF_DEL_WORD` | keep α0.22 | `#e5484d38` | `#dc262638` | `#f8717138` |
| new `DIFF_ADD_BAR` / `DIFF_DEL_BAR` | tone α0.55, 3px | — | `#15803d8c` / `#dc26268c` | `#34d3998c` / `#f871718c` |
| new `DIFF_HUNK` | diff_hunk | — | `#5b43e812` | `#8b7cf614` |
| `MODIFIED` | warning | `#ad5700` | `#a16207` | `#facc15` |
| `TEAL` | attribute / cyan | `#0e7c86` | `#0e7490` (5.36) | `#22d3ee` (11.21) |
| `TEAL_BG` | α0.08 | `#0f9d8a14` | `#0e749014` | `#22d3ee14` |
| `SYN_KEYWORD` | keyword | `#8e4ec6` | `#5b43e8` | `#8b7cf6` |
| `SYN_FN` | function | `#3e63dd` | `#2563eb` (4.95 on sunken) | `#60a5fa` (7.83) |
| `SYN_STRING` | string | `#18794e` | `#15803d` | `#34d399` |
| `SYN_COMMENT` | comment | `#a1a1aa` (2.46) | `#6b7280` (4.63) | `#92929a` (6.45) |
| new `SYN_NUMBER` | number / boolean | — | `#a16207` | `#facc15` |
| new `SYN_TYPE` | type | — | `#7e22ce` | `#c084fc` |
| new `SYN_PROPERTY` / `SYN_TAG` | property / tag | — | `#be185d` | `#f472b6` |
| new `SYN_VARIABLE` | variable | — | `#303035` | `#e8e8ea` |
| new `SYN_PUNCT` | punctuation | — | `#52525b` | `#a1a1aa` |
| new `SYN_ATTR` | attribute / escape | — | `#0e7490` | `#22d3ee` |
| new `SYN_INVALID` | invalid | — | `#b91c1c` | `#f87171` |
| `highlight_theme` editor bg / line number / active line | term bg / faint / hover | `SURFACE_SUNKEN` / `TEXT_5` / `FILL_1` | same tokens | same tokens (resolve via palette) |
| terminal fg / bg | terminal | `TEXT` / `WHITE` | `#303035` / `#fafafa` | `#e8e8ea` / `#090909` |
| terminal selection / cursor | terminal | ghostty default | `#00000029` / `#3030358c` | `#ffffff38` / `#e8e8ea66` |
| terminal ANSI 16 | ANSI_LIGHT / DARK | ghostty default | F2 list | F2 list |
| `PALETTE` (repo tiles) | iOS projectDots | 6 colors | 8 light values (F14) | 8 dark values (F14) |

**`ui.rs` literals:**

| Literal (P ui line) | Zeron source | Proposed light | Proposed dark |
|---|---|---|---|
| side bg `#fafafbb3` (L27) | shell α0.80 if Blurred window, else shell | keep / `#f3f3f5cc` | `#0d0d0d` / `#0d0d0dcc` |
| glass `#ffffff9e` (L42) | overlay α0.80 + hairline 0.10 | keep | `#1b1b1ccc` + `#ffffff1a` |
| pop bg `SURFACE` + ring (L32) | dialog + border | `#fefefe` + `SEPARATOR` | `#151516` + `#ffffff1a` |
| pop highlight `#fffffff2` (L32) | ink top edge | keep | `#ffffff17` |
| pop / button shadows (L5-19, L32-34) | `shadow_lg` only when not frosted | keep | drop drop-shadows; keep ring |
| Accent shadow `0x0a84ff59` (L91) | accent α0.35 | `#5b43e859` | `#8b7cf659` |
| Accent button text `WHITE` | on_accent | `#ffffff` | `#000000` |
| field_box `#ffffffd9` / trigger `#ffffffb3` (L695, L800) | input | keep | `#343438b8` |
| backdrop `0x28241e` α `1f` / `2e` / `40` (L708; P pk/overlay.rs) | scrim, dark = light × 1.875 | keep | black α `3a` / `56` / `78` |
| `count_badge` text `WHITE` (L599) | best_on_color | `#000000` (11.65 on `#ffb224`) | `#000000` |

Metrics that stay as they are: Pocket radii and heights are already on a coherent 4px grid. Only the popover radius (26 vs Zeron 12) and the dialog radius stand out; see the UI/UX spec.

### F17. Phone mapping (P app/design.ts → Zeron iOS)

| design.ts key | Now | Proposed dark | Proposed light (new) |
|---|---|---|---|
| bg | `#0B0C0E` | `#060606` | `#F3F3F5` |
| card | `#15171A` | `#111113` | `#FFFFFF` |
| text | `#EDEBE6` | `#E8E8EA` | `#27272C` |
| body | `#D9D7D2` | `#DCDCE0` | `#3F3F46` |
| muted | `#A1A4AA` | `#A9A9AE` | `#62626A` |
| faint | `#7E828A` | `#85858A` | `#797981` |
| lineNo | `#5B5F66` | `#6B6B72` | `#97979F` |
| rule | `#1D2024` | `#1E1E22` | `#E2E2E6` |
| cardRule / stroke | `#1F2226` / `#272A30` | `#1F1F23` | `#E4E4E8` |
| chip | `#1D2024` | `#1C1C20` | `#E7E7EB` |
| green / greenChip | `#5BE38A` / `#14261B` | `#34D399` / success α0.12 | `#15803D` / α0.12 |
| red | `#F07167` | `#F87171` | `#DC2626` |
| teal | `#5CC8BE` | `#22D3EE` | `#0E7490` |
| bubble / bubbleText | `#1C3326` / `#BFF5D2` | `#19191C` / text | `#FFFFFF` / text |
| code | `#0F1113` | `#0B0B0D` | `#FAFAFB` |
| addBg / addText | `#12251A` / `#8BF0AD` | success α0.055 / `#34D399` | α0.055 / `#15803D` |
| delBg / delText | `#2A1616` / `#F5A39B` | danger α0.055 / `#F87171` | α0.055 / `#DC2626` |
| keyword / typeName | `#5CC8BE` / `#E7C98B` | `#8B7CF6` / `#C084FC` | `#5B43E8` / `#7E22CE` |
| new accent | — | `#8B7CF6` | `#5B43E8` |

Notes:
- Fold `theme.ts` into `design.ts`: accent `#7aa2f7` → accent; `ok` / `error` / `warn` → success / danger / warning.
- Pick the variant with RN `useColorScheme`.

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 07-1 | Runtime `Palette {light, dark}` struct replacing `u32` consts | Enables dark mode and theming without touching every widget twice | Z th/builtins.rs:97-169; P th:9-62 (~436 refs) | `desktop/crates/theme` new; mechanical migration in `ui`, `pocket`, `storybook` | L | none |
| 07-2 | Dark palette + System/Light/Dark appearance | Dark mode; follows the OS | F2, F16; R gc/theme/mod.rs:228, 261 | `theme` port values; `pocket` settings adapt | M | 07-1 |
| 07-3 | Contrast fixes + a contrast unit test on the palette | Readable meta text and legible badges; blocks regressions | F15 audit; Z th/lib.rs:107-185 (`ensure_contrast`) | `theme` adapt: `FAILED` → `#dc2626`, `TEXT_3` → `#797981`, `count_badge` text black, `SYN_COMMENT` `#6b7280`; test asserts ≥4.5 for text roles | S | none (07-1 for dark) |
| 07-4 | Status semantics per Zeron | Consistent meaning of color: accent = live, amber = needs you, green = done, red = failed | Z ui/shell/spaces.rs:2245-2262; Z ui/change_requests.rs:158-172 | `ui` adapt `State` pills, `indicator`, `alert_color`; α tiers 0.55 / 0.6 / 0.65 / 0.9 / 0.14; badge bg 0.08, text 0.85, hover 0.16 | S | product call (open question 2) |
| 07-5 | 7 accent presets | Personalization at near-zero cost | Z th/lib.rs:277-325 | `theme` port table; settings picker | M | 07-1 |
| 07-6 | Session sounds (done / request / attention) with gates | Hear when an agent finishes or needs you without watching | Z ui/sound.rs:31-37, 224-273, 339-417 | `pocket` new `sound.rs`; reuse MIT WAVs with attribution; `afplay`; 5s startup quiet for connectivity alerts, 250ms coalesce for Attention; 4 toggles | M | session state events |
| 07-7 | Motion catalog constants + hover fade + reduced motion | Consistent, calmer feel; accessibility | Z ui/motion.rs:298-426, 842-889; R gpui/elements/animation.rs:301 | `theme` new `motion` module (CSS bézier fn, specs); `ui` hover wash 150ms; `pocket` settings "Reduce motion" → `cx.set_reduce_motion` | M | none |
| 07-8 | Frosted window (`Blurred`) with α0.80 chrome | macOS-native depth; matches Zeron look | Z ui/theme.rs:795-813, 1130-1138; R gpui/platform.rs:2449-2468 | `pocket/main.rs` `window_background_appearance`; `ui` side / glass / pop α0.80; no drop shadows when frosted | M | 07-1, 07-2 |
| 07-9 | UI font-size scale 12–20 + role type scale | Accessibility; dense or roomy preference | Z ui/typography.rs:84-128; render.rs:31-58, 758-765 | `theme` new `ui_rems`-style scale; switch `SANS` to bundled Geist (optional) | M | 07-1 |
| 07-10 | 12-role syntax + ANSI 16 + terminal colors | Richer code highlighting; terminal matches theme | Z th/builtins.rs:171-200, 229-237 | `theme` `syntax_color` / `highlight_theme` adapt; `term` pass palette to libghostty | S/M | ghostty palette API (open question 4) |
| 07-11 | Diff row wash 0.055 + 3px change bar | Calmer diffs; add/del readable at a glance | Z ui/changes.rs:74-88, 4446-4466 | `pocket/diff.rs` (worktree; `pocket/changes.rs` on `main`) + `theme` DIFF_* tokens | S | none |
| 07-12 | Phone: Zeron iOS dynamic palette; merge `theme.ts` into `design.ts` | Light mode on phone; one palette; brand parity with desktop accent | Z ios/Design/Palette.swift:3-94; P app/design.ts, theme.ts | `packages/app` adapt | M | 07-2 values |
| 07-13 | Theme families / VS Code theme import | Power-user theming | Z th/builtins.rs:15-61 | `theme` new | XL | 07-1, 07-5 |
| 07-14 | Pixel-glyph working spinner on a shared 30fps clock | Recognizable "working" signal; less CPU than per-row timers | Z proto/motion.rs:19-106; Z ui/loaders.rs:152; Z ui/motion.rs:56 | `ui` new loader; replace 1s rotate (P th:118-122) | M | 07-4 |
| 07-15 | In-scene backdrop blur for menus + EdgeFade | Frosted menus over content | Z ui/frost.rs:20-84 | needs a gpui fork (not in `gpui-pre 0.3.6`) | L | fork decision |

## UI/UX spec to copy

**Appearance setting**
- Options "System / Light / Dark", default System. Zeron renders them as option cards with a miniature theme preview each (Z ui/appearance.rs:32-49; Z ui/settings/appearance.rs:1259-1275, 2630-2648).
- System follows `Theme::sync_system_appearance`.
- Switching is instant, no crossfade. Zeron crossfades only wallpapers (180ms).

**Status (sidebar row / session row)**

| State | Glyph | Color | Label | Label style |
|---|---|---|---|---|
| Working | 13px slot with a 2px-cell glyph spinner (or Pocket spinner) | accent α0.55 | "Working" | 10px Medium, status color |
| Needs input | 6px dot | Zeron accent α0.6; Pocket keeps amber `WAITING` | "Input" | same |
| Failed | 6px dot | danger α0.65 | "Failed" | same |
| Done | 11px check icon | success α0.9 | "Done" | same |
| Queued | dot | warning | "Queued" | same |
| Idle | dot | `ink` α0.14 | none | — |

- Gap between glyph and label: 4px.

**Badges**
- Standard: h16, px4, r4, 10px Medium. Composer variant: h20, px7, r6, 11px.
- bg tone α0.08 → α0.16 on hover; text tone α0.85.
- PR tones: Open = success, Merged = accent, Closed = danger.

**Status strip**
- h24, text 11.
- Failed: "Run failed" in danger.
- Sending: spinner + "Sending…" 12px muted.
- Otherwise empty.

**Popover / menu**
- Card r12, inset 4, item gap 2, item r7, text 13, max h320, border hairline.
- Motion: in 140ms `EASE` fade + translateY −2; out 100ms.
- Scrollbar thumb 3 → 5 on hover, fades out 260ms after a 1400ms linger.
- Pocket `pop` r26 is 2× Zeron. Keep it if intentional; otherwise move to 12–16.

**Dialog**
- w360, p20, r16, hairline α0.10 border; `shadow_lg` only when opaque.
- Title 15 SemiBold; body 13/19; field px12 py8.
- In 180ms `EASE`, fade + 2px rise (Z ui/motion.rs:566-573). Pocket uses 200ms / 8px `ease_out_quint`.
- Scrim: dark black α0.60, light α0.32.

**Command palette** (`docs/screenshots/command-palette/README.md`; screenshot `command-palette/light.png`)
- Placeholder "Type a command or search chats…"; trailing "Ctrl K" (macOS "⌘K") chip.
- Section "Actions": "New chat", "New project", "Open settings".
- Action rows 32px; chat rows with a 2px gap; item r10.
- Footer: "↑ ↓ Navigate  ↵ Select  Esc Close" in code (Z ui/shell/command_palette.rs:428-431); `[code≠doc]` the screenshot shows "Open".
- Keys: Cmd/Ctrl+K opens; Up/Down wrap; Enter opens; Esc, a second Cmd/Ctrl+K, or clicking outside closes.
- No entrance motion; Pocket already does this (P pk/overlay.rs:311-330).

**Hover**
- Every hover wash animates bg over 150ms, `cubic-bezier(0.4, 0, 0.2, 1)`.
- Dark hover = white α0.11 (≈ `FILL_4` dark).

**Diff**
- Line height 21, text 12 mono, marker column 28.
- Add/del row wash α0.055, 3px left bar α0.55, line numbers tone α0.9.
- Hunk bg accent α0.07–0.08.
- File collapse 180ms ease-out; chevron 200ms.

**Sound**
- Settings copy:
  - "Session sounds" (master)
  - "Task completed"
  - "Input required"
  - "Errors and disconnections"
- All default on.
- Cue priority: failed > input > done.
- Connectivity alerts stay silent for the first 5s of observation; Attention cues within 250ms coalesce into one.
- Send and Undo are silent.

**Motion settings**
- "Reduce motion": System / On / Off.
- When on, one-shot animations snap to the end state and loaders show a static frame. gpui does this automatically (R gpui/elements/animation.rs:301, 407).

**Typography defaults**
- UI 14 (Pocket) vs 16 (Zeron default of the 12–20 scale).
- Keep Pocket's 14 and offer a scale of 12/13/14/15/16/18/20.
- Markdown 14/22; h1 19/27, h2 16/24, h3 15/22; code 12.5/18.
- Terminal 13; Zeron's code line height is 18 against Pocket's 22.

## Open questions / risks

1. **Light identity.** Keep Pocket's `#5b5bd6` accent and `#111113` text, or move light to Zeron's `#5b43e8` / `#303035`? F16 keeps text and moves the accent.
2. **Status color swap.** Zeron uses accent for working and green for done; Pocket uses green for running and accent for done. The swap changes learned meaning, so it needs a product decision before 07-4.
3. **Dark accent text.** Dark `on_accent` is black on `#8b7cf6`, because white is only 3.33:1. Is black-on-violet acceptable for primary buttons, or should the button use `SOLID` instead?
4. **Terminal palette.** Does the libghostty-vt shim expose a way to set the 16-color palette, fg/bg and selection (P packages/desktop/crates/term/src/shim.c)? Unverified.
5. **Asset licenses.**
   - Sound WAVs are MIT (repo license) and need attribution.
   - Solar icons are CC BY 4.0, which requires attribution if Pocket adopts any.
   - vscode-symbols file icons carry their own license, not checked.
6. **gpui-kit components in dark mode.** gpui-kit components (inputs, highlighter, scrollbars) read gpui-component `Theme` fields. `init` (P th:163-178) must set both modes, or components stay light.
7. **Light faint below AA.** Zeron's light `faint` `#797981` is 4.32 on white and 3.90 on shell. Using it for `TEXT_3` improves on today's 3.38 but still fails AA for small meta text; `TEXT_2` is the safe tier.
8. **Frost risks.**
   - Frost needs `Blurred` plus translucent roots everywhere; any opaque root breaks it.
   - Zeron notes a macOS vibrancy re-apply quirk (Z ui/theme.rs:1118-1129).
   - No in-scene menu blur without a gpui fork.
9. **Migration size.** ~436 call sites plus ~10 raw rgba literals; `WHITE` is overloaded as paper, on-solid and on-accent and must be split per site.
10. **Web not checked.** zeron.sh was not fetched, so marketing-site tokens (if different) are not covered.

## Verification

Date: 2026-09-30. Claims checked: 18. Corrected: 8.

Confirmed against source: crate seeds and accent pair (Z th/builtins.rs:239-305), derivation rules (L96-145), fallback oklch accent (Z ui/theme.rs:92-93), 7 accent presets, 19 families / 30 variants with only zeron Frosted, `status_dot_color` alphas and "Blue" comment, sidebar status labels / glyphs, geometry constants (Z ui/theme.rs:815-846), `GLASS_ALPHA*` 0.80, `window_background_appearance`, `MENU_BLUR` 16, popover/dialog `shadow_lg` gating, badge recipe, diff constants and washes, sound cues / players / priority / settings copy, `ReduceMotion`, gpui-pre `WindowBackgroundAppearance` / `reduce_motion` / polynomial `ease_out_quint`, gpui-component `sync_system_appearance`, Pocket theme consts, `0x0a84ff59` accent shadow, `count_badge` colors, status pills, overlay motion, termview metrics, phone `design.ts` / `theme.ts` users, and every contrast ratio re-run.

Corrections:
- Motion counts: 6 named béziers plus an alias and 16 specs, not "5 béziers and 17 specs".
- Sound gates: `STARTUP_QUIET` covers only connectivity alerts and `COALESCE` only the Attention cue; TL;DR, F12, 07-6 and the Sound spec implied both gate every cue.
- Accent contrast: primary is checked to ≥3.0, not ≥4.5 (TL;DR, F1).
- Appearance setting: Zeron has `AppearanceMode {System, Light, Dark}` option cards with previews; the old citation (settings/appearance.rs:1988-1990) was a per-theme variant label.
- Dialog entrance rise is 2px (Z ui/motion.rs:566-573), not 8px; Pocket's 200ms / 8px differs.
- Palette footer in code reads "↵ Select"; "Open" is only in the screenshot. Added the second-Cmd/Ctrl+K and click-outside dismissals.
- 07-11 mapping: `pocket/changes.rs` does not exist at worktree `86deb13`; diff view is `pocket/src/diff.rs` (`changes.rs` exists on `main`).
- Token reference count is approximate (~436 by rg, ~10 raw `rgba(0x…)`), not an exact 434 / 9.
