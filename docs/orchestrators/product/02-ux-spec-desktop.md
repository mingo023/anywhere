# 02 — Desktop UX spec (macOS, `packages/desktop`)

Date: 2026-09-30. Inputs: R04, R05, R06, R07, R12, R13, R14, R16, R17 (`docs/orchestrators/research/`), `CONTEXT.md`, PO decisions D1–D13, Zeron and MonoCode screenshots. Pocket @ 5bc8ea8; main is now `f8f7293` — resolve every path through 05-roadmap §0 Path map.

**Overrides.** Where this spec disagrees with 00-prd.md or 05-roadmap.md, the 05-roadmap §7.1 Spec overrides table wins (D42). Plans copy its rows verbatim.

**Legend**
- `[R04 §F4]` = finding F4 in report 04. `[R17 §S1]` = its UI spec item S1. `[R04 §Spec]` = its "UI/UX spec to copy" section. `[R04 idea 04-2]` = idea 04-2.
- `[P path:L]` = Pocket code at 5bc8ea8. Prefixes:
  - `th` = `d/theme/src/theme.rs`
  - `ui` = `d/ui/src/ui.rs`
  - `keys` = `d/keys/src/keys.rs`
  - `ag` = `d/agents/src/agents.rs`
  - `pk/` = `d/pocket/src/`
  - `d/` = `packages/desktop/crates/`
- **Main has moved on.** main (`f8f7293`) split `pocket` into feature modules (b223e99) and opened files and changes as tabs (8499bcf). Approximate path map (the full one, with lines, is 05-roadmap §0):

  | At 5bc8ea8 | On main |
  |---|---|
  | `pk/view.rs` | `sidebar/{column,panel,rail,row_menu,sessions,usage}.rs`, `desktop/chrome.rs`, `terminal_view/{tabs,tab_menu,pane}.rs` |
  | `pk/overlay.rs` | `palette.rs`, `modals/{more,confirm}.rs` |
  | `pk/forms.rs` | `modals/{form,new_session,add_project}.rs` |
  | `pk/changes.rs`, `pk/diff.rs` | `git_ui/{changes,diff}` |
  | `pk/termview.rs` | `terminal_view.rs` |
  | `pk/inbox.rs` | `inbox/{list,detail}.rs` |
  | `pk/main.rs` `cards()` | `desktop/project.rs` |

- **new** = a choice made here that no report settles. Each one is listed in §8.
- **Z** = Zeron (GPUI), **M** = MonoCode (web), **P** = Pocket today.
- Terms follow CONTEXT.md. UI copy says **Session** where the code says agent, and **Timeline view** for the rendered Conversation. Needs you, Working, Done, Failed, Idle, Seen and Attached are used exactly as CONTEXT.md defines them.
- Scope: the macOS desktop app (D13). The terminal is in scope (D9). R16 supersedes R06 §9.
- **Strategy-agnostic.** No strategy is picked yet. The spec therefore gives every attached agent session a Timeline view with a composer, and keeps its terminal one click away. Which one opens by default is a setting (§9 Q1). **Decided since:** strategy C spine + A cockpit (D14); Terminal is the default, no setting (D22).
- **Token conflicts**, Zeron vs MonoCode, are resolved by these rules, in order:
  1. GPUI-proven in Zeron beats web-only MonoCode.
  2. Text meets WCAG AA (4.5:1) in both modes.
  3. Pocket's value stays when neither is clearly better.
  4. Every token has a light and a dark value.

---

## 1. Principles

1. **Status first, order stable.** One glance at the sidebar answers "does anything need me?", and rows never move under the pointer. D1 colours plus a distinct glyph per status mean colour is never the only signal.
   - Lists keep a stable order (D2).
   - Only attention surfaces sort by urgency: ⌘J, the palette's Up next group, the Inbox, the Dock badge and notifications.
   - Refs: [R04 idea 04-1; R12 idea 12-3].
2. **The terminal is always one click away.** The Timeline view and the composer are views on the same Session, never a replacement for its terminal (D9) [R05 idea 05-23; R14 idea 14-1].
3. **Render costs what's visible.** Every `cx.notify()` redraws the whole window [CLAUDE.md §Desktop performance].
   - Lists that grow with the Project are virtualized.
   - Git, file IO and highlighting stay off the UI thread.
   - Hovers stay instant. No animation runs for longer than the interaction it explains.
4. **Keyboard reaches everything; no key collides.** The existing keys stay, and new keys are checked against every context (D6, §5).
5. **One token set, two modes, AA text.** Light and dark come from one `Palette`. Information-bearing text is ≥4.5:1 on its surface, and glyphs are ≥3:1 [R07 §F15, idea 07-3].
6. **Never lose work by accident.**
   - Drafts survive navigation.
   - Enter never stops a Working agent [R05 idea 05-2].
   - Closing a Working terminal asks first [R13 idea 13-13].
   - Destructive dialogs state facts, and deleting a worktree keeps its branch (D8).

---

## 2. Tokens

> **Owner's dark theme wins.** Where this section conflicts with the owner's dark-theme plan (`docs/plans/2026-09-30-dark-theme.md`), the plan wins; 05-roadmap §7.1 lists the rows. Tokens are the plan's `theme::Token { light, dark }` consts, not a `Palette` struct or `p(cx)`. WHITE splits into the plan's ON_TEXT, CUTOUT and WHITE on coloured fills, not ON_SOLID/PAPER. Dark uses the plan's MonoCode values, is Blurred and translucent, keeps its shadows, and ships with the plan rather than at S1. The Zeron chrome literals map to the plan's roles. What remains here for E16 is the status tokens and glyph shapes (§2.2), the AA fixes, radii (§2.5), motion (§2.8) and copy; see `docs/designs/2026-09-30-look.md`.

### 2.0 Mechanics (R07 idea 07-1, 07-2)

- **Palette struct.** Replace the `pub const u32` set [P th:9-62] with `pub struct Palette { … }` and two statics, `LIGHT` and `DARK`. `pub fn p(cx: &App) -> &'static Palette` picks one from the gpui-kit `ThemeMode`.
  - Call sites change mechanically: `rgba(TEXT)` → `rgba(p(cx).text)`.
  - That is about 436 sites plus about 10 raw literals, listed in §2.1 "Chrome literals" [R07 §Open questions 9].
- **Split `WHITE`** into `on_solid`, `on_accent` and `paper`, and pick one per call site.
- **`theme::init`** [P th:188-203] sets the gpui-kit `Theme` fields for both modes and re-runs `highlight_theme()` per mode. Without that, gpui-kit inputs and scrollbars stay light [R07 §Open questions 6].
- **Appearance follows macOS by default.**
  - Observe the window appearance the same way `follow_reduce_motion` observes Reduce Motion [P pk/main.rs:1041-1045].
  - Settings can override it with System / Light / Dark (§3.10) [R07 §Spec].
  - Switching is instant, with no crossfade.
- **Contrast unit test** (new file `d/theme/src/contrast.rs`) asserts, for both palettes:
  - ≥4.5 for `text`, `text_body`, `text_2`, `waiting_text`, `success_text`, `failed_text` and `accent` on `window`, `surface`, `surface_sunken` and `side`.
  - ≥3.0 for the status glyph colours on the same surfaces.
  - ≥4.5 for `on_solid` on `solid` and `failed`, and for `on_accent` on `accent`.

  Refs: [R07 idea 07-3].

### 2.1 Colour

Values are hex; 8-digit values end in alpha. Contrast is measured on `SURFACE` / `WINDOW`, light mode.

**Surfaces and chrome**

| Role | Light | Dark | Src | Pocket const |
|---|---|---|---|---|
| Window background | `#F3F3F5` | `#0D0D0D` | Z [R07 §F16] | `WINDOW` (replace `F4F4F5`) |
| Page surface (main area, pickers) | `#FFFFFF` | `#060606` | Z | `SURFACE` |
| Sunken (session page body, terminal, code) | `#FAFAFA` | `#090909` | Z | `SURFACE_SUNKEN` |
| Raised (chips, field fill on dark) | `#EDEDF0` | `#343438` | Z | add `SURFACE_RAISED` |
| Dialog card | `#FEFEFE` | `#151516` | Z | add `DIALOG` |
| Menu, popover, palette card | `#FFFFFF` | `#1B1B1C` | Z | add `OVERLAY` |
| Side chrome (aside, column, rail) | `#FAFAFBB3` | `#0D0D0D` | P [P ui:27-29] / Z | add `SIDE` |
| Glass (floating buttons, footers) | `#FFFFFF9E` | `#1B1B1CCC` | P [P ui:42-44] / Z | add `GLASS` |
| Field and trigger fill | `#FFFFFFD9` | `#343438B8` | P [P ui:702, ui:810] / Z | add `FIELD` (both literals) |
| Top-edge highlight (pop, glass, primary) | `#FFFFFFF2` | `#FFFFFF17` | P / Z | add `HIGHLIGHT` |
| Scrim base (palette / sheet / dialog α) | `#28241E` α `1F` / `2E` / `40` | `#000000` α `3A` / `56` / `78` | P [P ui:708-710; pk/overlay.rs:321-343] / Z | add `SCRIM` + `backdrop(id, Scrim::{Palette,Sheet,Dialog})` |
| Floating-sidebar dim (Compact) | `#1111131A` | `#00000056` | P [P pk/view.rs:569] / new dark | `SCRIM` (`Sheet` α in dark) |

**Text tiers**

| Role | Light | Dark | Src | Pocket const |
|---|---|---|---|---|
| Primary | `#111113` | `#E8E8EA` | P / Z | `TEXT` |
| Body | `#3F3F46` | `#DCDCE0` | P / Z | `TEXT_BODY` |
| Secondary: the AA tier for meta text, section labels, empty states and palette detail | `#62626A` (6.05 / 5.45) | `#A9A9AE` | Z | `TEXT_2` (replace `6F6F78`) |
| Tertiary: icons, placeholders, text ≥18 px; never essential small text | `#797981` (4.32 / 3.90) | `#85858A` | Z | `TEXT_3` (replace `8B8B94`) |
| Quaternary: kbd hints, disabled, decorative | `#97979F` | `#6B6B72` | Z | `TEXT_4` (replace `A1A1AA`) |
| Line numbers, faint glyphs | `#B4B4BC` | `#525255` | P / Z | `TEXT_5` |
| Hairline glyphs | `#D4D4D8` | `#39393B` | P / Z | `TEXT_6` |
| Text on `SOLID` / `FAILED` buttons | `#FFFFFF` | `#000000` | Z | add `ON_SOLID` (split `WHITE`) |
| Text on accent | `#FFFFFF` (6.15) | `#000000` (6.31; white is 3.33) | Z | add `ON_ACCENT` |
| Paper: rings behind badges, preview canvases | `#FFFFFF` | `#090909` | Z | add `PAPER` |
| Solid: Primary button fill | `#111113` | `#EBEBEF` | P [P ui:70-72] / Z | add `SOLID` (Z light `232328` not adopted, rule 3) |

- Rule 2 moves information-bearing text drawn in `TEXT_3` / `TEXT_4` in light mode to `TEXT_2`. That covers:
  - the row meta line [P ui:430]
  - section headers [P pk/view.rs:193-262]
  - the empty line [P pk/view.rs:115-117]
  - palette detail [P ui:743]
  - the trigger label [P ui:816]
  - the pill "Not attached" [P ui:241]
  - field labels [P ui:691]
- Icons keep `TEXT_3`, which clears 3:1 [R07 §Open questions 7].

**Fills and borders**

| Role | Light | Dark | Src | Pocket const |
|---|---|---|---|---|
| Fill 1 (row hover) | `#11111308` | `#FFFFFF0F` | P / Z | `FILL_1` |
| Fill 2 (hover on chrome) | `#1111130B` | `#FFFFFF14` | P / Z | `FILL_2` |
| Fill 3 (selected row, chip) | `#1111130E` | `#FFFFFF1A` | P / Z | `FILL_3` |
| Fill 4 (selected tab / sidebar row, kbd) | `#11111311` | `#FFFFFF1F` | P / Z | `FILL_4` |
| Hairline | `#11111312` | `#FFFFFF14` | P / Z | `HAIRLINE` |
| Separator | `#11111317` | `#FFFFFF1A` | P / Z | `SEPARATOR` |
| Separator strong (focused-terminal ring, tracks) | `#1111131F` | `#FFFFFF2E` | P / Z | `SEPARATOR_STRONG` |

**Accent** (one violet for desktop and phone [03 §2.1])

| Role | Light | Dark | Src | Pocket const |
|---|---|---|---|---|
| Accent: Working, links, focus, selection | `#5B43E8` (6.15 / 5.54) | `#8B7CF6` (6.09) | Z | `ACCENT` (replace `5B5BD6`) |
| Accent bg (selected palette row, Draft) | `#5B43E81F` | `#8B7CF638` | Z | `ACCENT_BG` |
| Accent ring (input focus, keyboard focus) | `#5B43E83D` | `#8B7CF659` | Z | `ACCENT_RING` |
| Accent tint (Working pill, hunk header) | `#5B43E81A` | `#8B7CF62E` | Z | `ACCENT_TINT` |
| Accent glow (Accent button shadow) | `#5B43E866` | `#8B7CF666` | Z | `ACCENT_GLOW`; also replaces literal `0x0a84ff59` [P ui:91] |

Accent choice: M's `#459BF7` is web-only (rule 1). Z's violet is AA in both modes, stays close to Pocket's hue, and matches the phone.

**Status (D1)**. Glyph colours need 3:1 and text colours need 4.5:1 (§2.2).

| Role | Light | Dark | Src | Pocket const |
|---|---|---|---|---|
| Needs you: glyph, count badge fill | `#A16207` (4.93 / 4.44) | `#FACC15` | Z [R07 §F16 warning] | `WAITING` (replace `FFB224`: 1.8:1 on white fails 3:1) |
| Needs you: text | `#855612` (5.67 on WINDOW) | `#FACC15` | Z `warning_muted` | `WAITING_TEXT` (replace `AD5700`) |
| Needs you: tint | `#A1620714` | `#FACC1514` | Z | `WAITING_BG` |
| Working: glyph and text | = `ACCENT` | = `ACCENT` | D1 | delete `RUNNING`, `RUNNING_TEXT` |
| Working: tint | = `ACCENT_TINT` | = `ACCENT_TINT` | D1 | delete `RUNNING_BG` |
| Done: glyph | `#15803D` (5.02 / 4.52) | `#34D399` | Z | add `SUCCESS` |
| Done: text | `#1C6C3B` (5.8 on WINDOW) | `#34D399` | Z `success_muted` | add `SUCCESS_TEXT` |
| Done: tint | `#15803D14` | `#34D39914` | Z | add `SUCCESS_BG` |
| Failed: glyph, Danger button fill | `#DC2626` (4.83) | `#F87171` | Z | `FAILED` (replace `E5484D`) |
| Failed: text | `#B91C1C` (5.89 on WINDOW) | `#F87171` | Z | add `FAILED_TEXT` |
| Failed: tint | `#DC262614` | `#F8717114` | Z | `FAILED_BG` |
| Idle | no glyph; label `TEXT_2` | same | D1 | — |
| Git M / A / D letter | `MODIFIED` `#A16207` / `SUCCESS_TEXT` / `FAILED_TEXT` | `#FACC15` / `#34D399` / `#F87171` | Z | `MODIFIED` (replace `AD5700`); `git_color` [P ui:465-470] |
| Teal (secondary tag) | `#0E7490`, bg α`14` | `#22D3EE`, bg α`14` | Z | `TEAL`, `TEAL_BG` |

**Diff** [R07 idea 07-11; R06 §Spec]

| Role | Light | Dark | Src | Pocket const |
|---|---|---|---|---|
| Added row wash | `#15803D0E` | `#34D3990E` | Z | `DIFF_ADD_BG` (replace `30A46C1C`) |
| Deleted row wash | `#DC26260E` | `#F871710E` | Z | `DIFF_DEL_BG` (replace `E5484D17`) |
| Added word | `#15803D40` | `#34D39940` | Z | `DIFF_ADD_WORD` |
| Deleted word | `#DC262638` | `#F8717138` | Z | `DIFF_DEL_WORD` |
| Added sign / stat text | `#15803D` | `#34D399` | Z | `DIFF_ADD_TEXT` |
| Deleted sign / stat text | `#DC2626` | `#F87171` | Z | `DIFF_DEL_TEXT` |
| Change bar, 3 px | `#15803D8C` / `#DC26268C` | `#34D3998C` / `#F871718C` | Z | add `DIFF_ADD_BAR`, `DIFF_DEL_BAR` |
| Hunk header | `#5B43E812` | `#8B7CF614` | Z | add `DIFF_HUNK` |

**Syntax** (7 new roles; `syntax_color` [P th:64-72] maps every capture) [R07 idea 07-10]

| Role | Light | Dark | Pocket const |
|---|---|---|---|
| Keyword, boolean, preproc | `#5B43E8` | `#8B7CF6` | `SYN_KEYWORD` (replace `8E4EC6`) |
| Function, constructor | `#2563EB` | `#60A5FA` | `SYN_FN` |
| String | `#15803D` | `#34D399` | `SYN_STRING` |
| Comment | `#6B7280` (4.63) | `#92929A` | `SYN_COMMENT` (replace `A1A1AA`, 2.6:1) |
| Number, constant | `#A16207` | `#FACC15` | add `SYN_NUMBER` |
| Type, enum | `#7E22CE` | `#C084FC` | add `SYN_TYPE` |
| Property, tag | `#BE185D` | `#F472B6` | add `SYN_PROPERTY` |
| Variable | `#303035` | `#E8E8EA` | add `SYN_VARIABLE` |
| Punctuation | `#52525B` | `#A1A1AA` | add `SYN_PUNCT` |
| Attribute | `#0E7490` | `#22D3EE` | add `SYN_ATTR` |
| Invalid | `#B91C1C` | `#F87171` | add `SYN_INVALID` |

**Terminal** [R07 §F2]

| Role | Light | Dark | Pocket const |
|---|---|---|---|
| Foreground / background | `#303035` / `#FAFAFA` | `#E8E8EA` / `#090909` | add `TERM_FG`, `TERM_BG` |
| Selection (inactive = half α) | `#00000029` | `#FFFFFF38` | add `TERM_SELECTION` |
| Cursor (filled) | `#3030358C` | `#E8E8EA66` | add `TERM_CURSOR` |

ANSI 16 values:
- Light: `#1F1F1F #DC2626 #16A34A #B45309 #2563EB #9333EA #0E7490 #3F3F46 / #71717A #B91C1C #15803D #92400E #1D4ED8 #7E22CE #155E75 #18181B`.
- Dark: `#242424 #F87171 #4ADE80 #FACC15 #60A5FA #C084FC #22D3EE #D4D4D8 / #52525B #FCA5A5 #86EFAC #FDE047 #93C5FD #D8B4FE #67E8F9 #FAFAFA`.
- Stored as `ANSI: [u32; 16]` per palette. This depends on the shim exposing a palette setter [R07 §Open questions 4].

**Identity**

| Role | Light | Dark | Src | Pocket const |
|---|---|---|---|---|
| Claude Code / Codex | `#D97757` / `#0F9D8A` | same | P | `AGENT_CLAUDE`, `AGENT_CODEX` |
| Project tiles (8) | `475569 2563EB 7C3AED BE123C A16207 047857 0F766E C2410C` | `94A3B8 93C5FD C4B5FD FDA4AF FCD34D 6EE7B7 5EEAD4 FDBA74` | Z iOS [R07 §F14] | `PALETTE` (replace the 6-colour set [P th:96]); matches the phone |

**Chrome literals to tokenize**

| Literal | Site | Becomes |
|---|---|---|
| `0xfafafbb3` | `side` [P ui:28] | `SIDE` |
| `0xffffff9e` | `glass` [P ui:43] | `GLASS` |
| `0xffffffd9` | `field_box` [P ui:702] | `FIELD` |
| `0xffffffb3` | `trigger_field` [P ui:810] | `FIELD` |
| `0xffffff8c` | usage card [P pk/view.rs:449] | `GLASS` |
| `0xfffffff2` | `pop`, `glass` [P ui:33, ui:43] | `HIGHLIGHT` |
| `0x0a84ff59` | Accent button [P ui:91] | `ACCENT_GLOW` |
| `0x1111131a` | Compact dim [P pk/view.rs:569] | `SCRIM` |
| `0xffffff1a`, `0xffffff33`, `0xffffff8c` | commit split button on SOLID [P pk/changes.rs:270-287] | `ON_SOLID` at α `1A` / `33` / `8C` |
| `WHITE` ring 2 on badges | [P ui:289-309] | `PAPER` |

### 2.2 Status mapping (D1)

| Status | Glyph (sidebar mark, tab, rail, palette lead) | Label (row, pill, palette detail) | Pill bg | Sound (D11) | Banner |
|---|---|---|---|---|---|
| Needs you | 7 px dot `WAITING` | "Needs you" `WAITING_TEXT` | `WAITING_BG` | Needs you cue | yes, if not Seen |
| Working | spinner 11 px `ACCENT` | "Working" `ACCENT` | `ACCENT_TINT` | — | no |
| Done | check 11 px `SUCCESS` | "Done" `SUCCESS_TEXT` + diffstat | `SUCCESS_BG` | Done cue | yes, if not Seen |
| Failed | ×-bold 11 px `FAILED` | "Failed" `FAILED_TEXT` | `FAILED_BG` | Failed cue | yes, if not Seen |
| Idle | none | "Idle" `TEXT_2` (palette, Inbox only) | none | — | no |
| Not attached | none | "Not attached" `TEXT_2` | `FILL_3` | — | no |

- **Shape carries status too.** The dot, spinner, check and × differ in shape, so status reads without colour. That matters for amber vs red under deuteranopia (new).
- **Glyphs are drawn at full token α on desktop.** Zeron's α0.55–0.9 [R07 §F6] drop the light glyphs to about 2.5:1 on `SIDE`. The phone keeps its own α [03 §2.2].
- **Zeron's "amber = queued/offline" is not adopted (D1).** Amber only ever means Needs you.
- **Sites that change** (Done was accent, Working was green):
  - [P ui:239] Done pill dot → check `SUCCESS`
  - [P ui:283] `alert_color` Done → `SUCCESS`
  - [P ui:373-378] indicator spinner → `ACCENT`
  - [P ui:457] "Running" → "Working"
  - [P ui:467] git A → `SUCCESS_TEXT`
  - [P ui:213] diffstat `+N` → `DIFF_ADD_TEXT`
  - [P pk/view.rs:809] tab mark Done → check `SUCCESS`; Working → spinner `ACCENT`
  - [P pk/view.rs:459-561] rail badges: bottom → `ACCENT`, top → `alert_color`
- **`count_badge`** [P ui:584-601] becomes `WAITING` fill with `ON_SOLID` text: 4.93 in light, black on `FACC15` in dark. Today it is white on `FFB224`, 1.8:1.

### 2.3 Type scale

Fonts: `.SystemUIFont` (UI) and Geist Mono (code, terminal, kbd) [P th:6-7]. gpui-kit base 14 / mono 13 [P th:192, th:200].

| Role | Size / line | Weight | Used by | Src | Pocket const |
|---|---|---|---|---|---|
| Display | 20/26 | Bold | modal title, Settings page h1 | P [P ui:713-726]; M 20/600 [R12 §F12] | add `type::DISPLAY` |
| Title | 16/22 | Bold | column header, sheet header, Inbox title (17 → 16) | P [P pk/view.rs:636] | add `type::TITLE` |
| Canvas prompt | 15/23.25 | Regular | new-session composer | R17 §S1 | add `type::PROMPT` |
| Row title / Body | 14/20 row, 14/22 prose | SemiBold row / Regular prose | session title; Timeline markdown, composer, blank page | P [P ui:442]; R07 §Spec | add `type::BODY` |
| UI | 13.5/18 | Regular / Medium | menus, tree and worktree rows, trigger | P [P ui:760, ui:825] | add `type::UI` |
| Control | 13/18 | Medium | buttons, column tabs, inputs, Settings row label | P [P ui:84]; M 13/500 | add `type::CONTROL` |
| Small | 12.5/16 | Medium | terminal tabs, links, bar error, tool-group header | P [P pk/view.rs:862] | add `type::SMALL` |
| Meta | 12/16 | Regular | row meta line, hints, Settings descriptions | P [P ui:430]; M 12 | add `type::META` |
| Caption | 11.5/15 | SemiBold | status pills, section labels, pick heads | P [P ui:229] | add `type::CAPTION` |
| Micro | 10/12 | Medium, mono | ⌘1–9 jump chips, badges | Z [R07 §F6] | add `type::MICRO` |
| Mono UI | 11/16 | Regular | kbd, diffstat, hunk header, split header 11.5 | P [P ui:210, ui:262] | add `type::MONO_UI` |
| Code | 12.5/22 | Mono | diff lines, file view | P [P pk/diff.rs:319-320] | add `type::CODE` |
| Code block | 12.5/18 | Mono | fenced code in the Timeline view | R07 §Spec | — |
| Terminal | 13/22; split 12/19 | Mono, ligatures off | terminal grid | P [P pk/termview.rs:11-12]; R16 idea 16-3 | keep `MAIN`, `SMALL` |

- Markdown headings: h1 19/27, h2 16/24, h3 15/22. Block gap 12 [R07 §Spec; R05 §Spec].
- Values are Pocket's except Micro. R07 finds Pocket's scale coherent [R07 §F9]. Consts are added so new code stops inventing sizes; old sites migrate when touched.

### 2.4 Spacing

- **Grid.** 4 px, with half steps: 2, 4, 6, 8, 10, 12, 16, 20, 24, 32 [R07 §F9]. No new consts: sizes stay inline `px()` as today.
- **Fixed paddings:**
  - session row px10 py10 gap4
  - lists p8 gap2
  - menus p6
  - modal pt22 px24 pb20
  - terminal pt10 px20 pb14 [R16 §Spec]
  - page bar pl24 pr10 (pl91 in Focus) [P pk/view.rs:729-738]

### 2.5 Radii

| Role | Value | Src | Pocket const |
|---|---|---|---|
| Sidebar row / session, tree, menu row | 9 / 8 | P [P ui:313-327, ui:428-450] | keep |
| Icon button, close, kbd | 7 / 5 / 5 | P | keep |
| Chip, column tab | 8 | P | keep |
| Button, pill, segmented | h/2 (16, 18, 10, 12) | P [P ui:74-102] | keep (Z/M use 6; the pill is Pocket's identity, R07 §F9 calls the grid coherent) |
| Field, trigger | 11 / 12 | P | keep |
| Popover, menu, picker, tab menu | **12** (was 26 / 16 / 14 / 10) | Z + M agree [R07 §Spec; R12 §F2] | add `R_POPOVER`; `pop()` default |
| Dialog (Confirm, AddRepo, project settings) | **16** (was 26) | Z + M agree | add `R_DIALOG` |
| Palette card | **16** (was 22) | Z [R04 §F5] | `R_DIALOG` |
| Composer (canvas and Timeline view) | 14 | P [P pk/forms.rs:601] = R17 §S1 | keep |
| Card (usage, Inbox detail), group box | 12 | P; M 12 | keep |

### 2.6 Chrome heights

| Element | Value (px) | Src |
|---|---|---|
| Top bars (aside top, column header, page bar) | 42 | P (Z 44, M 40; rule 3) |
| Tab strip / terminal tab | 40 / 28 | P [P pk/view.rs:826-923] |
| Column tabs | 30 | P |
| Session search row | 44 | P |
| Sidebar row (project, worktree) | 32 | P |
| Session row | 80 (3 lines) | P (Z 45/61, rule 3: Pocket shows provider, status word and branch) |
| Tree row (Explorer, Changes) | 28 | P; R06 27 |
| Split header (only when 2+ terminals) | 30 | P (M 36) |
| Palette input / row / footer | 60 / 40 / 40 | P [P pk/overlay.rs:174-237] |
| Menu row | 32 | P |
| Button / large / icon | 32 / 36 / 28 | P |
| Diff line / hunk header / file header | 22 / 28 / 38 | P [P pk/diff.rs:14-16]; R06 |
| Composer toolbar / send | 28 / 28 | R13 §Spec |
| Rail width | 56 | P (M 48) |
| Column seam hit / visible | 20 / 1 (was 5 / 0) | Z [R04 §F8] |
| Terminal sash hit / visible | 10 / 1 | R16 §Spec |
| Window default / min | 1440×900 / 900×600 (none today) | P [P pk/main.rs:1074-1083] / Z [R04 idea 04-7] |
| Traffic lights | (14, 14) | P |

### 2.7 Shadows and frost

- **No blur anywhere.** gpui-pre 0.3.6 has no in-scene backdrop blur, and `Blurred` window appearance is not always supported [R07 §F5; P pk/view.rs:576]. Surfaces are opaque with hairlines. Frost (07-8, 07-15) and native blur (12-15) are out.
- **Light keeps Pocket's elevation:**
  - `pop` [P ui:33]: ring `SEPARATOR` 0.5, `HIGHLIGHT` inset 1, `#00000024` y18 b50, `#0000000F` y2 b6
  - `row_shadow` [P ui:17-19]
  - `glass` [P ui:42-44]
  - `primary` [P ui:70-72]
  - floating sidebars `#11111324` b48 [P pk/view.rs:577]
- **Dark drops drop-shadows** and keeps the ring and `HIGHLIGHT` `#FFFFFF17`, as Zeron does when opaque [R07 §F5, §F16]. The shadow helpers read the mode and return rings only in dark.

### 2.8 Motion

Easings:

| Token | Curve | Status |
|---|---|---|
| `QUINT` | (0.22, 1, 0.36, 1) | existing [P ui:137] |
| `EASE_OUT` | (0, 0, 0.58, 1) | add [R07 §F11] |

Add them as `theme::motion::{…}` consts together with the durations below.

| Token | Duration | Easing | Use | Src |
|---|---|---|---|---|
| `HOVER` | 0 (instant) | — | every hover | P. Z 150 fades need per-element state, and each frame redraws the window (principle 3) |
| `SWAP` | 120 | QUINT | icon-group swap | P [P ui:125-137] |
| `MENU_IN` | 150, translate −4 → 0, fade | QUINT | every menu, popover and picker (today only More animates) | P [P pk/overlay.rs:321-343]; Z 140/−2 is equivalent, rule 3 |
| `MENU_OUT` | 0 | — | close | new: GPUI has no exit transition without keeping the element mounted |
| `DIALOG_IN` | 200, translate +8 → 0, fade | QUINT | Confirm, AddRepo, Settings sheets | P; Z 180/2 is equivalent, rule 3 |
| `PALETTE` | 0 | — | palette open | P = Z [R07 §Spec] |
| `CANVAS` | 0 | — | new-session canvas | R17 §S7 (P1; the 420 glide is later) |
| `TAB_SLIDE` | 150 | EASE_OUT | sibling slide during tab drag | Z [R16 idea 16-12] |
| `SASH_SNAP` | 200 | EASE_OUT | double-click resets a sash | R16 §Spec |
| `SPINNER` | 1000 per turn | linear | Working | P [P th:141-145] |
| `CURSOR_BLINK` | 530 on / 530 off | step | terminal cursor | R16 §F5 |
| `SCROLLBAR` | linger 1400, fade 260 | EASE_OUT | terminal and list scrollbars | Z [R16 §Spec] |
| `TOOLTIP_DELAY` | 350 | — | every tooltip | Z [R04 §Spec] |
| `JUMP_HINT_DELAY` | 280 after ⌘ is held alone | — | ⌘1–9 chips | new (Z badge hover delay [R04 §Spec]) |
| `FIT_DEBOUNCE` | 80, trailing | — | terminal resize; suspended during sash drag | R16 idea 16-11 |

- **Folds are instant.** Tree rows, tool groups and diff files open and close without an animation, because virtualized lists have fixed or re-measured heights. Only the chevron swaps.
- **No FLIP re-sort (RESORT 260 [R04 §F2]).** D2 keeps order stable, so nothing reorders.
- **Reduced motion.** Pocket already mirrors macOS Reduce Motion into `App::reduce_motion` [P pk/main.rs:1041-1045, 1060]. Settings adds System / On / Off [R07 §Spec]. When on:
  - one-shots snap to their end (GPUI does this)
  - the spinner shows a static frame
  - the cursor does not blink [R16 idea 16-16]
  - tabs don't slide

### 2.9 Sound cues (D11)

| Cue | Plays when a non-Seen session enters | File | Default | Priority |
|---|---|---|---|---|
| Needs you | Needs you | `request.wav` | on | 2 |
| Done | Done: fresh (≤45 s since the turn ended), not an interrupt | `done.wav` | on | 3 |
| Failed | Failed | `attention.wav` | on | 1 |

- **When cues play.** Only on transitions of sessions that are not Seen, whether or not the window is focused (D11).
  - A 250 ms coalesce window plays one cue, the highest priority [R05 idea 05-5; R07 §F12].
  - The baseline is silent: the first snapshot after launch or after a pocketd reconnect never plays. Subagents are silent.
  - Send, commit and every other frequent action is silent [R07 §F12].
- **Playback.**
  - `afplay` spawned on the background executor.
  - Zeron's WAVs (MIT) are embedded with attribution in `THIRD_PARTY.md` [R07 §Open questions 5].
- **Per-cue toggles**, default on (D11). This horizon: palette actions "Needs you sound: On/Off", "Done sound: On/Off", "Failed sound: On/Off", persisted in `desktop.json` (D25). Settings › Notifications waits for S1.

---

## 3. Window layout

All measurements are px.

### 3.1 Shell regions

```
Sidebars (default). Window 1440×900, min 900×600, title "Coding Pocket", transparent titlebar.
x=0          272          606                                                  1440
┌────────────┬────────────┬──────────────────────────────────────────────────────┐ y=0
│ aside      │ column     │ page bar 42: tabs · status · context · actions       │
│ SIDE       │ SIDE       ├──────────────────────────────────────────────────────┤ 42
│ w 272      │ w 334      │ body (flex)                                          │
│ 200–420    │ 280–600    │  session page: Terminal │ Timeline view │ Changes    │
│            │            │  else: diff │ file │ canvas │ blank page             │
│            │            │  SURFACE_SUNKEN (session page) / SURFACE (others)    │
└────────────┴────────────┴──────────────────────────────────────────────────────┘ 900
             ↑ seam        ↑ seam: hit 20 (±10), line 1 px SEPARATOR_STRONG on hover,
                             col-resize, double-click resets to 272 / 334

Compact (⌘. from Sidebars)                   Focus (⌘. from Compact)
┌──┬─────────────────────────────────────┐   ┌──────────────────────────────────────┐
│56│ page bar 42 (pl 24)                 │   │ ●●● [⇥28] page bar 42 (pl 91)        │
│  ├─────────────────────────────────────┤   ├──────────────────────────────────────┤
│  │ body                                │   │ body                                 │
└──┴─────────────────────────────────────┘   └──────────────────────────────────────┘
⌘\ in Compact floats aside + column over the body: sidebar shadow, SCRIM dim;
Esc or a click outside closes [P pk/view.rs:564-579].
```

- The widths, the layout mode and the window bounds persist in `desktop.json`, saved 500 ms after the last change [R04 idea 04-7; R14 idea 14-15].
- At 900 px with default widths the main area is 294 px. ⌘. gives it room (§9 Q7).

### 3.2 Projects sidebar (aside)

```
┌ 272, px8 pb10 ───────────────────────────────┐
│ ●●●                                  [🔔28]³ │ h42 drag area; bell → Inbox; count_badge = Needs you count
│ ┌──────────────────────────────────────────┐ │
│ │ 🔍15  Search                         ⌘K  │ │ trigger h32 r10, 13.5 TEXT_2; kbd 11.5 TEXT_4 → palette
│ └──────────────────────────────────────────┘ │
│ Projects                               [+22] │ 12 SemiBold TEXT_2; + = Add project
│ ▾ [P18] pocket                        ◌ ···  │ project row h32 (anatomy below)
│      ⎇ main                              ●   │ worktree row h32, pl25
│      ⎇ feat-login                        ✓   │
│ ▸ [A18] api                              ×   │ collapsed: mark = roll-up of all its worktrees
│ …                                            │ body scroll, gap 1 (not virtualized: bounded by projects)
│ ┌ usage card (hidden until the window is known, D7) ┐ │
│ [ Add project        ] h36 r17 GLASS  [⚙34]  │ foot
└──────────────────────────────────────────────┘
```

**Project row** (h32, r9):
```
│pl4│chevron 14 (only if it has worktrees)│gap7│tile 18 r5│gap8│name 14.5 SemiBold, flex, truncate│trail│pr6│
trail at rest: status mark (roll-up)   ·   hover / menu open: [+22 r6] [···22 r6]
```
- The tile shows initials on a `PALETTE` colour. With a Needs you or Failed session inside, it gets a 9 px badge at the top-right, ringed 2 px in `PAPER` [P ui:289-309].
- The roll-up mark is the most urgent status among the Project's sessions: Needs you > Failed > Done > Working [P pk/view.rs:96-103; pk/status.rs:5-12].
- Drag reorders projects: `DragProject` with an `ACCENT` 2 px `drop_line` [P ui:419-421].

**Worktree row** (h32, pl25): icon slot 22 (glyph 13; `TEXT_2` selected, else `TEXT_3`), then name 13.5 (SemiBold `TEXT` selected, 450 `TEXT_BODY` otherwise), then trail.
- Hover shows `[+22]` (New session in this worktree, ⌘N) and `[···22]`.
- The main checkout row shows no "Delete worktree…".

**States**: rest none · hover `FILL_2` · selected `FILL_4` · drag target `drop_line` · menu open keeps the trail buttons visible [P ui:394-417].

**Host line** (aside foot, above the foot row; 12 `TEXT_2`, h24, px8; from `hello.ok.host`, PRD FR 04-3, 16-5) [R15 idea 15-7]:
- "Keeping Mac awake" while pocketd holds the idle-sleep assertion (an Agent is Working or Needs you).
- "Phone access needs Tailscale" + a docs link, when `tailnet` is false.
- Hidden otherwise. Both can show; Tailscale goes first.

**Context menus.** Right-click opens the same menu as `···`, at the pointer. Menus are w216, r12, p6, `MENU_IN` [R04 §F6; R04 idea 04-5, 04-14].

| Project menu | Worktree menu | Session menu (row in §3.3) |
|---|---|---|
| New session ⌘N | New session ⌘N | Copy path |
| New worktree… ⌘⇧N | New terminal tab ⌘T | Copy resume command |
| — | — | — |
| Settings… ⌘, (or "Keep in Pocket" when not kept) | Open in editor | Close session… (danger) |
| Reveal in Finder | Reveal in Finder | |
| Copy path | Copy path | |
| — | Copy branch name | |
| Remove from Pocket (danger) | — | |
| | Delete worktree… (danger; not on main) | |

### 3.3 Sessions column

```
┌ 334, SIDE ──────────────────────────────────────┐
│ feat-login                               [+28]  │ h42: worktree name 16 Bold (was "Workspace"); + = New session ⌘N
├─────────────────────────────────────────────────┤
│ [ Sessions ] [ Explorer ] [ +12 −3 ]            │ p8 gap4; tabs flex_1 h30 r8 13; FILL_4 + SemiBold selected
├─────────────────────────────────────────────────┤
│ 🔍14  Search sessions…                          │ h44 pl18 pr14, input 13
├─────────────────────────────────────────────────┤
│ ┌ row 80, r8, px10 py10 gap4 ─────────────────┐ │
│ │ ●6 Claude Code                    2m | ⌘1   │ │ line h16, 12 TEXT_2: provider dot + name · right: age, or jump chip
│ │ Fix the login redirect loop                 │ │ title 14/20 SemiBold TEXT, truncate
│ │ ⎇12 feat-login                ● Needs you   │ │ line h16: branch · status label (§2.2)
│ └─────────────────────────────────────────────┘ │
│   … p8 gap2, stable order                       │
└─────────────────────────────────────────────────┘
```

- **Order (D2): newest created first.** Today `cards()` sorts by `updated_at` [P pk/main.rs:496; pk/status.rs:77], which moves a row whenever its status changes. Sort by `created_at` [P ag:25] instead, and delete the status sorts:
  - [P pk/view.rs:665] Sessions list
  - [P pk/view.rs:481] rail
  - [P pk/overlay.rs:92] palette Sessions group

  The Up next group replaces the palette sort.
- **Needs you row** (D1, 12-3). Row fill `WAITING_BG` under the hover and selected fills; the status slot shows the 7 px dot + "Needs you" in `WAITING_TEXT`. Shape and label carry the status without colour (deuteranopia).
- **Jump chips** [R04 §F4, idea 04-2].
  - After ⌘ has been held alone for 280 ms, rows 1–9 swap their age for a chip "⌘1"…"⌘9": h16, px4, r4, `FILL_4`, Micro mono 10 Medium, `TEXT_2`.
  - Rows 10+ keep their age.
  - Chips hide as soon as another modifier joins, and never show while an overlay is open.
  - Numbering (D40): the visible sidebar list after filters, across expanded Worktrees; in Compact, the nth rail item.
- **Hover** shows `[···20]` in the age slot. The menu is in §3.2. Selected is `FILL_3`, hover `FILL_1`, and selected never blends into hover [R04 §F3].
- **Not attached** rows show the pill "Not attached" in the status slot [P pk/view.rs:670-682].
- **Virtualization.** The list is small per worktree (bounded by Sessions, not files), so it stays a plain `div`. If a worktree ever exceeds 200 rows, switch it to `uniform_list` at 80 + 2 per row (principle 3).

### 3.4 Top bar (page bar) and tab strip

```
page bar h42, pl24 (Focus: pl91 + toggle 28), pr10, gap6, drag area, SURFACE_SUNKEN
│[⇥]│ tab strip h40 ─────────────────────────────────────┐ │ status │ ◔16 │ [Timeline│Terminal] │ [⫿ ⊟] │ [···] │
    │ ┌ tab h28 pl10 pr6 r7 gap7, 12.5 ─────┐ ┌───────────┐ [+28] [⌄20×28]
    │ │ ●7 Fix login redirect   ✓   ×20 │ │ ▸ zsh   │
    │ └─────────────────────────────────────┘ └───────────┘
```

- **Tab lead.**
  - Agent tab: provider dot 7, then the session title, max 150, truncated [R04 idea 04-11]. The provider name is the fallback.
  - Shell tab: prompt icon, then the OSC title, then the foreground command, then "Terminal" [R16 §F5].
  - Changes tab: branch icon + "Changes".
  - The label " · N panes" is dropped [P pk/view.rs:798-824]. The split headers name each terminal.
- **Tab mark** after the label follows §2.2. A bell shows a 6 px `ACCENT` dot until the tab is focused [R16 idea 16-15].
- **Tab states.**
  - Selected: `SURFACE` + `row_shadow` + SemiBold `TEXT`.
  - Other: Medium `TEXT_2`, hover `FILL_2`.
  - Exited: opacity 0.55 [R16 idea 16-12].
  - Close (20, r5, × 11 `TEXT_4`) shows on hover. Middle-click closes too. The tooltip reads "Close terminal".
- **Plus and tab menu.** `[+28]` = New shell (⌘T), tooltip "New terminal". `[⌄]` opens the tab menu (§3.13).
- **Drag reorder** [R16 idea 16-12]: arms after 5 px, shows a ghost at 0.9, siblings slide `TAB_SLIDE`, and the drop commits the order to `workspace` tabs [P ws:3-5].
- **Status cluster.**
  - Error: 12.5 `FAILED_TEXT`, truncated.
  - Diff meta "+12 −3" 12: a click opens Changes [P pk/view.rs:751-796].
- **Context ring** (D7), 16 px:
  - Hidden while the agent hasn't reported its window.
  - Delete the fixed `CONTEXT_WINDOW` 200k [P ag:57].
  - Colour by share used: `TEXT_3` below 75 %, `WAITING` from 75 %, `FAILED` from 90 %.
  - Hover card: "Context window" / "184,000 / 200,000 tokens" / "16,000 tokens remaining" [R05 §Spec, idea 05-7].
  - The usage card [P pk/view.rs:436-455] and the rail bars follow the same rule.
- **View switch** (Phase 6).
  - A segmented control h24 (item r10, 12 px) with "Timeline" and "Terminal". It shows only on agent tabs whose session is Attached and has a Conversation.
  - The choice is remembered per session. The default is Terminal, with no setting (D22).
  - ⌘⇧T and the palette actions "Show Timeline" / "Show Terminal" switch too (D23). The switch never resizes the PTY.
- **Actions.** `icon_group` [⫿ Split right ⌘D][⊟ Split down ⌘⇧D] and [··· More] [P ui:125-137].
- **Observe-only banner** (D20, PRD FR 03-6). When this desktop runs inside a Pocket Terminal: a strip h28 under the page bar, 12.5 `WAITING_TEXT` on `WAITING_BG`, "Observe only — pocketd is managed elsewhere". Actions that need the owner principal are disabled.

### 3.5 Terminal area (tabs and splits)

```
body SURFACE_SUNKEN = TERM_BG. A tab holds rows; each row holds terminals side by side [P ws:3-5]
┌ split header h30 pl16 pr6 (only when the tab has 2+ terminals) ────────────────────┐
│ 1 · Claude Code — ~/pocket/feat-login        mono 11.5; TEXT focused, else TEXT_2 ×22│
├─────────────────────────────────────────────────────────────────────────────────────┤
│ banner px16 py6, 12 TEXT_2 on FILL_2 (status::banner, only when set)                │
│ pt10                                                                                │
│ px20   grid: Geist Mono 13 / line 22, ligatures off                           ▐ ←─┐ │
│        cursor: filled block TERM_CURSOR, blink 530; unfocused: hollow 1 px        │ │
│        selection TERM_SELECTION (inactive: half α)                                │ │
│                                                     ┌─────────────────────┐   rail│ │
│                                                     │ Jump to bottom  ↓   │ h28 r14, 12, GLASS
│                                                     └─────────────────────┘ 12 from right/bottom
│ pb14                                                                              │ │
├──── sash: 1 px SEPARATOR, hit 10, row-resize; min 160, max 55 %, double-click → 280 ┘─┤
│ 2 · zsh — ~/pocket/feat-login                                                    ×22│
│ …                                                                                   │
└─────────────────────────────────────────────────────────────────────────────────────┘
Split right adds a terminal to the row (divider 0.5 SEPARATOR). Split down adds a row (280 tall).
Focused terminal: inset ring SEPARATOR_STRONG 0.5 [P pk/view.rs:1016-1089].
```

- **Scrollbar rail** [R16 idea 16-7]:
  - 10 px hit strip inside the right padding
  - thumb 3 px, 5 px on hover or drag, min 24, inset 4
  - thumb colour `TEXT_4` at α0.5, 0.85 on hover, 0.68 while dragging
  - `SCROLLBAR` linger and fade
- **Scrollback** 10 000 lines. The view follows the bottom until the user scrolls. Input snaps it back to the bottom. The pill shows while the view is scrolled up [R16 idea 16-1, 16-2].
- **Wheel routing** [R16 idea 16-2]:
  1. Shift → viewport.
  2. Mouse-reporting mode → encoder.
  3. Alt-screen with 1007 → arrow keys.
  4. Else → viewport.
- **No smooth scroll.**
- **Selection** [R16 idea 16-4]:
  - 1, 2 or 3 clicks select a cell, word or line.
  - A drag arms after 2 px. Shift+click extends.
  - Autoscroll at the edges every 24 ms, 1–3 lines.
- **Links** [R16 idea 16-8]: hovering with ⌘ held underlines OSC 8 links, URLs and `path:line` in `ACCENT`. ⌘-click opens them.
- **IME** [R16 idea 16-10]: preedit is drawn underlined at the cursor, and `bounds_for_range` returns the cursor cell.
- **State lines** stay at [P pk/view.rs:1016-1089]:
  - "Connecting…"
  - "This session is not running."
  - "Process exited with code {c}" in 12 `FAILED_TEXT` when non-zero

### 3.6 Timeline view (Phase 6)

```
body SURFACE; content column max_w 760 centred, px24; `list` (variable height), keyed by item id
┌──────────────────────────────────────────────────────────────────────┐
│                                   ┌──────────────────────────────┐   │ user bubble: right-aligned, max 80 %
│                                   │ Fix the login redirect loop  │   │ px16 py10 r14, 14/22 TEXT, FILL_2
│                                   └──────────────────────────────┘   │ > 5 lines / 400 chars: "Show more"
│ ▸ Ran 3 commands · edited 2 files · 1 failed                         │ tool group header h26, 12.5 TEXT_2, chevron 12
│    ├ Run   pnpm test                                        ×        │ rows 12/18, verb TEXT_2 + mono chip; guide at x12.5
│    └ Edit  src/login.ts                                 +4 −1        │ failed row: × FAILED + error, expandable
│ Assistant markdown 14/22 TEXT_BODY, block gap 12                     │
│ ◌ Working… 1m 12s                                                    │ trailer 12 TEXT_2, spinner 11 ACCENT
│                                          ┌──────────────────────┐    │
│                                          │ ↓  Scroll to bottom  │    │ pill h30 r15, 13, GLASS; 36 above the composer, right 10
│                                          └──────────────────────┘    │
├──────────────────────────────────────────────────────────────────────┤
│ composer (§3.7) or approval panel                                    │
└──────────────────────────────────────────────────────────────────────┘
```

- **Tool groups** [R05 idea 05-1, 05-10; R13 idea 13-9]:
  - open while streaming, folded when the turn settles
  - summary grammar: "Ran N command(s) · edited N file(s) · read N file(s) · searched N time(s) · fetched N page(s) · updated todos · called N tool(s) · N failed"
  - edited paths are deduplicated
- **Trailer** [R05 idea 05-8]:
  - while Working: "Working… {elapsed}", in the format `Ns`, `Nm Ns`, `Nh Nm`, `Nd Nh`
  - after: "Worked for {elapsed}"
  - Zeron's rotating flavour words are dropped (new)
- **Scroll** [R05 idea 05-17]:
  - stick to the bottom within 70 px
  - pill when more than 320 px from the bottom, hidden within 2 px
- **User bubble** [R05 idea 05-9]:
  - an optimistic bubble at opacity 0.65 until pocketd acks
  - on failure: "Not delivered — click to retry" in `FAILED_TEXT` 12
- **Failed turn** ends in a row: × `FAILED` + "Failed" `FAILED_TEXT` + the error line.
- **Data** comes from the full v3 timeline decode [R14 idea 14-3]. Markdown uses the existing gpui-kit renderer and mermaid [P pk/explore.rs:155-156]. Highlighting runs off the UI thread and is cached per block (principle 3).

### 3.7 Composer and approval panel (Phase 6)

```
┌ r14, SURFACE, ring SEPARATOR_STRONG 0.5 (focus: ACCENT_RING 1) — max_w 760, mb16 ─────┐
│ Message Claude Code…                    14/22; grows 1 → 8 lines (176), then scrolls  │ px14 pt12
│                                                                                        │
│                                                              ◔16  gap8  (↑ 28 circle) │ toolbar h28, pb10
└────────────────────────────────────────────────────────────────────────────────────────┘
```

The button morphs by state [R05 idea 05-2; R05 §Spec]:

| Session | Field | Button | Enter |
|---|---|---|---|
| not Working | empty | Send, disabled at 0.4 | nothing |
| not Working | text | Send: 28 circle `ACCENT`, arrow-up 14 `ON_ACCENT` | send |
| Working | text | Per provider (D37): "Queue" where the E01 step-0 check shows the text queues, else "Send". "Steer" (Codex via app-server, D4) only after the E10 spike | queue / send |
| Working | empty | Stop: 28 circle `SOLID`, 11 r3 square `ON_SOLID` | nothing: Enter never stops |
| Needs you | — | the approval panel replaces the composer | — |

- Shift+Enter inserts a newline.
- Drafts live per session, in memory [R05 idea 05-11].
- Stop maps to `turn/interrupt` for Codex (D4) and to pocketd's interrupt for Claude.

**Approval panel** (Needs you with a structured ask) [R14 idea 14-11; R05 idea 05-4]:
```
┌ r14, WAITING_BG, ring WAITING 0.5 ─────────────────────────────────────────────────────┐
│ ●7  Needs you · Bash                                          11.5 SemiBold WAITING_TEXT│
│ rm -rf node_modules && pnpm install                    mono 12.5, max 6 lines, scroll   │
│ [1 Allow]  [2 Allow for this session]  [3 Deny]                     [Open terminal]     │ buttons h28, kbd 1–9
└──────────────────────────────────────────────────────────────────────────────────────────┘
```
- Option labels come verbatim from the provider.
- A question gets one page per question, with a "1/3" chip and plain 1–9 keys, and auto-advances after 220 ms [R05 idea 05-4].
- An unstructured ask shows "Answer in the terminal" + [Open terminal], which switches the view. The phone shows "Answer on your Mac" with no button until S7 (PRD FR 12-2).
- **Plan variant** (`ExitPlanMode`; Plan first). Head "Needs you · Plan"; the rendered plan markdown (max_h 360, scroll); an optional feedback field; buttons [1 Approve] [2 Keep planning]. Feedback is sent with Keep planning (PRD FR 12-3, 12-4).

### 3.8 Changes and diff

Changes list (column tab "Changes"). It keeps Pocket's layout [P pk/changes.rs]:
- header h40: "Changes" 13.5 SemiBold + Push
- commit box: textarea max 120, commit split button h30 r9
- sections h28: "Staged Changes" / "Changes"
- rows h28 indent 14 in `uniform_list`, with `w_full()` and a `vertical_scrollbar`

```
Diff (main area; on main it opens as a tab)
┌ header h40 px16 gap10 ─────────────────────────────────────────────────────────────────┐
│ src/login.ts  mono 12.5   +4 −1 (mono 11) │ [Working tree ⌄] h24 │ [Unified│Split] │ ☐ Viewed │
├────────────────────────────────────────────────────────────────────────────────────────┤
│ @@ -12,6 +12,8 @@ fn redirect      h28 DIFF_HUNK, mono 11 TEXT_2   hover: [Stage hunk][Revert change] h22 │
│▌ 12 │ 12 │   │ let url = …                                            line h22        │ bar 3 │ old 44 │ new 44 │ sign 18 │ code 12.5 pl12 pr24
│▌ 13 │    │ − │ return url;                                            DIFF_DEL_BG     │
│▌    │ 13 │ + │ return safe(url);                                      DIFF_ADD_BG     │
│ ⋯ 34 unchanged lines                     h28, 12 TEXT_2; hover: "Expand upward" / "Expand downward" (20 lines) │
│ Diff truncated — showing first 5,000 of 12,400 lines       notice h24, 11 TEXT_2      │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Scope menu** (w240, r12). Refs: [R06 idea 06-1, 06-3, 06-16].
  - "Working tree" is the default.
  - "Branch changes": the header becomes `{branch} → {base ⌄}`, and the ref picker opens with "Current worktree" and branch search.
  - "Latest turn" is disabled with "No turn recorded yet — send a message first" until pocketd snapshots turns.
- **Hunk actions** [R13 idea 13-2]: "Stage hunk" and "Revert change". Revert confirms with "Revert this change?" / "This can't be undone.".
- **Comments.**
  - Click a line number to open the comment field (existing). ⌘↵ adds the comment [P pk/diff.rs:423].
  - Several comments collect in a "N comments" chip in the header, which sends them as one prompt [R06 idea 06-2].
  - The prompt quotes each line as `path:N` and tags "(deleted line)" [R13 idea 13-3].
- **File notices** [R06 §Spec]: "New file", "Deleted file", "Renamed from {from}", "Binary file — contents not shown".
- **Performance (non-negotiable)** [CLAUDE.md]:
  - uniform rows of 22
  - `load_diff` / `apply_diff` with a staleness check
  - highlighting in the background
  - Budget: the Changes frame stays ≤ 3 ms with 1137 files, and click → diff ≤ 100 ms.

### 3.9 New-session canvas (Phase 4, R17 §S1–S7)

The canvas replaces the modal sheet [P pk/forms.rs:544-645] and renders in the main area.

```
main area (834 at default width)
┌ page bar 42: crumb "New session" ────────────────────────────────────────────────────────┐
│                                                                                           │
│                                                      [▣18 pocket ⌄]  ← target row h20,    │
│                                                                  28 above the composer,   │
│                                                                  right inset 26           │
│   ┌ composer max_w 640, r14, SURFACE + ring SEPARATOR_STRONG 0.5 ─────────────────────┐   │
│   │ Describe what the agent should do…         15/23.25; min 105 → max 260, scroll    │   │
│   │                                                                                    │   │
│   │ [●Claude Code · opus · high ⌄] [Ask ⌄] [Plan first ×]                    (↑ 32)   │   │ chips h30 pl10 pr8 gap7 r8 FILL_2 (hover FILL_3)
│   └────────────────────────────────────────────────────────────────────────────────────┘   │
│    [▢ Current checkout ⌄] on main              ⌘↵ to start · esc to cancel               │ bottom slot h24 px10
│                                                                                           │
└───────────────────────────────────────────────────────────────────────────────────────────┘
Y of the composer = (pane_h − composer_h) × 0.5 + 8.
The optional headline "What should we work on in {project}?" (M) sits above; off by default.
```

- **Bottom slot** [R17 §S1]: small chips h20 px8 gap6 r6, 12 `TEXT_2`, icon 12, chevron 12 at 0.5.
  - New worktree: `[⎇ New worktree ⌄] [From {base} ⌄]`, then "branch" + an inline name field in mono 12.5. Name errors show inline in `FAILED_TEXT`.
  - Current checkout: `[▢ Current checkout ⌄] on {branch}`, read-only. It never runs checkout. On a worktree the chip reads "Current worktree".
- **Send** is a 32 round `ACCENT` button, disabled until the session is ready.
- **The "Plan first" pill** shows while Plan is on, with a `WAITING_BG` tint. Its tooltip reads "Turn off Plan first".
- **Menus** are instant, have no motion and open above when less than 180 px is free below [R17 §S2–S4]:
  - **Agent/model card** (w300):
    - one section per provider, with a provider mark
    - rows h34: model 13 Medium, effort meta `TEXT_3`, check 16, "Default" badge
    - "Effort" › submenu: low / medium / high / xhigh / max
    - a missing CLI dims to 0.5, with the note "Install the Codex CLI to enable. Install with `npm install -g @openai/codex`"
    - search appears above 12 rows
    - the card never shows "Default model"
  - **Access menu** (w288): two-line rows with the labels "Ask", "Auto-accept edits", "Auto", "Full access" (in `WAITING_TEXT`), plus the toggle "Plan first" / "Review a plan before building." [R17 idea 17-3].
  - **Checkout menu** (w240): head "Checkout"; rows "Current checkout" (meta: branch) and "New worktree" (meta: "from {base}").
  - **Base menu** (w300): "Default" / "Recent"; search above 20 rows.
  - **Project menu** (w280): 18 px tiles; footer "Add project…".
- **Defaults** [R17 §S6]:
  - provider Claude Code
  - effort High
  - access Ask; Full access is never remembered
  - Plan off
  - project = the selected sidebar Project
  - checkout: Current checkout via ⌘N, New worktree via ⌘⇧N
  - base: `cfg.base` → main → master → current
- **LaunchSpec.** Start sends one `agent.create` with `{project, checkout, provider, model, effort, access, plan, prompt}`, and pocketd builds argv (D5) [R17 idea 17-10]. The draft survives navigation and spawn errors, and the canvas keeps the prompt on failure [R17 idea 17-4, 17-9].

### 3.10 Settings surface

```
⌘, → Settings replaces aside + column + main (window chrome stays)
┌ nav, width = aside width (272), SIDE ──┬ page, SURFACE ──────────────────────────────────────┐
│ ●●●                                    │ Settings / Notifications          [🔍 Search settings]│ h42; crumb 13; search h28 w192
│ App                     11.5 SB TEXT_2 │                                                      │
│   General              row h32 r9      │  Notifications                    Display 20/26 Bold │ page px32 py32, max_w 720
│   Appearance                           │  Choose when Pocket gets your attention.  13 TEXT_2  │ max_w 576
│   Notifications   ← FILL_4 selected    │                                                      │
│   Keybindings                          │  ┌ group r12, FILL_1 bg, ring HAIRLINE 0.5 ───────┐  │ group pt32
│ Agents                                 │  │ Desktop notifications                 [●━━]    │  │ row px16 py14 gap24, 13 Medium
│   Claude Code                          │  │ Banners for sessions you aren't viewing.       │  │ description 12 TEXT_2
│   Codex                                │  ├────────────────────────────────────────────────┤  │ row divider HAIRLINE
│ Projects                               │  │ Session sounds                        [●━━]    │  │ toggle 36×20, knob 16
│   pocket                               │  │   Needs you · Done · Failed           [●━━] ×3 │  │
│   api                                  │  └────────────────────────────────────────────────┘  │
│                                        │                                                      │
│ [‹ Back]                h36, pinned    │                                                      │
└────────────────────────────────────────┴──────────────────────────────────────────────────────┘
```

Pages [R12 §F12, idea 12-10]:

| Page | Rows (control) |
|---|---|
| General | "Open agent sessions in" (segmented Timeline / Terminal, Phase 6) |
| Appearance | "Appearance" (System / Light / Dark); "Reduce motion" (System / On / Off) |
| Notifications | "Desktop notifications"; "Session sounds" (master) with "Needs you", "Done", "Failed" (D11, all on) |
| Keybindings | read-only table Command / Keybinding 160 / When 112, rows h44 12 px, filter w176 (Phase 7); recording is later [R12 idea 12-13] |
| Claude Code, Codex | CLI status ("Found at {path}" / "Not installed" + install line); default model, effort and access (R17 idea 17-5) |
| {Project} | the existing project settings: "Default base branch", "Worktrees folder", "Copy into each worktree"; "Remove from Pocket" (danger) [P pk/forms.rs:772-948] |

- **Controls.** Segmented p2, 12 px, items r10. Select max w208, popover max h320. Controls take at most 60 % of the row.
- **Search** (Phase 7): up to 8 hits in a 300 × 320 popover. Enter jumps to the row and flashes it once, `ACCENT_BG` fading over 600 ms (snapped under Reduce motion) [R12 idea 12-11].
- **Esc** or "Back" returns to the previous screen.

### 3.11 Command palette

```
scrim SCRIM(Palette); card w660, top 120, r16, OVERLAY, pop shadow (light) / ring (dark); no entrance motion
┌──────────────────────────────────────────────────────────────────────────┐
│ 🔍  Search sessions, files and actions…                            esc   │ h60, 17 px
├──────────────────────────────────────────────────────────────────────────┤
│ Up next                                         11.5 SemiBold TEXT_2 px12 │
│ [●] Fix login redirect     pocket · feat-login · Needs you      row h40 r10│ lead 24 r7 FILL_3; title 14 Medium
│ [×] Migrate schema         api · main · Failed                            │ detail 12.5 TEXT_2
│ Sessions                                                                  │
│ [●] Add rate limiter       pocket · main · Claude Code · Working          │
│ Worktrees · Files · Actions …                         results max_h 460  │
├──────────────────────────────────────────────────────────────────────────┤
│ ↑↓ Navigate   ↵ Open   Tab to search all projects   esc Close           │ footer h40, 11.5 TEXT_2
└──────────────────────────────────────────────────────────────────────────┘
```

Groups, in order:

| Group | Cap | Order | Contents |
|---|---|---|---|
| Up next | 5 | urgency: Needs you > Failed > Done (non-Seen), then oldest transition first | attention (D2) |
| Sessions | 8 | stable, newest created | every Session in scope |
| Worktrees | 5 | sidebar order | worktrees of Projects in scope |
| Files | 8 | changed first | `git ls-files`, loaded in the background [P pk/overlay.rs:61-78] |
| Actions | all matches | fixed | see §6 |

- **Matching.** Every whitespace-separated word must be a case-insensitive substring of title, Project, worktree, branch or provider name [R04 idea 04-4]. Matched runs are drawn in `ACCENT` [R12 idea 12-12]. The total stays ≤ 30 rows.
- **An empty query** shows Up next, the 5 newest Sessions, 3 changed files and all Actions.
- **Prefix `>`** limits results to Actions [R12 idea 12-12].
- **Keys.**
  - ↑/↓ wrap.
  - Tab toggles this Project / all Projects (existing).
  - Esc, a second ⌘K or a click outside closes and restores focus.
  - Pointer guard: hover selects only after the pointer moves [R12 idea 12-12].
- **Scope** of the filter work: at most 30 rows are built per frame (principle 3).

### 3.12 Inbox (attention surface; exists)

- **Layout.** A list in the column (width 348) and the detail in the main area [P pk/inbox.rs].
- **Sections** "NEEDS YOU", "FAILED" (new) and "DONE", each sorted oldest transition first (D39; was (status, newest) [P pk/inbox.rs:45]). Keys j/k [P pk/inbox.rs:72] and ⌘↵ open.
- **Mark all seen** marks every FAILED and DONE item Seen; NEEDS YOU items stay until answered.
- **Header.** "Inbox" moves to the Title role (17 → 16).
- **Copy changes:** "Mark all read" → "Mark all seen" [P pk/inbox/list.rs:34]. The "Answer in the terminal · J K" hint stays for unstructured asks [P pk/inbox/detail.rs:51].
- **Answer panel** (detail; PRD FR 12-4). Reuses the §3.7 approval card for permissions (including "Allow for this session"), questions and plans. 1–9 choose, Enter sends, Esc closes. This is the only place longer permission options appear; banners offer Allow once / Deny and stop.

### 3.13 Menus, dialogs, notifications

- **Menu / popover** [R04 §F6; R07 §Spec]:
  - card: `OVERLAY`, r12, p6, max_h 360 then scroll
  - rows: h32 px10 r8 gap10, icon 14 `TEXT_2`, 13.5, kbd right
  - heading 11.5 SemiBold `TEXT_2`, px10 pt6 pb4; divider 0.5 mx8 my4
  - hover `FILL_2`; the keyboard highlight is `FILL_3`, so the two stay distinct
  - destructive rows go last, behind a divider, in `FAILED_TEXT` with `FAILED_BG` hover
  - submenus open after 300 ms and flip left when right + 244 > the viewport
- **Tab menu** (w264) [P pk/view.rs:931-984]: head "New tab in {branch}", then "New shell ⌘T", a separator and Claude Code / Codex rows. Those rows launch with the remembered picks (R17 idea 17-11).
- **Dialog.**
  - card: `DIALOG`, r16, w440, top 160, pt22 px24 pb20; title Display 20 Bold
  - facts list 13/19 `TEXT_BODY`
  - footer "Cancel" (Secondary, large h36) + the action (Danger `FAILED` / `ON_SOLID`, large h36)
  - opens with `DIALOG_IN`; Esc cancels and Enter confirms
  - Refs: [P ui:713-726; pk/overlay.rs:270-309]
- **macOS notification.** Shown by `sync_alerts` [P pk/main.rs:339-352; pk/status.rs:120-124]:
  - Title: the session title, or the provider name as fallback.
  - No subtitle: gpui-pre 0.3.6 has no subtitle API (D30).
  - Body line 1: "{project} · {worktree}". Line 2: the status label, or the ask ≤ 240 chars when known ("Approve: {tool}", a question's prompt, or the reply's first paragraph) [R13 idea 13-20].
  - Actions, on permission asks only: "Allow" = allow once; "Deny and stop" = deny + interrupt, so the Session goes Idle (P pd/internal/daemon/daemon.go:148).
  - Notifications fire only from a bundle; dev runs use `scripts/bundle-dev.sh` (gpui-pre-macos-0.3.6 system_notifications.rs:118-127).
  - A click focuses the session.
  - Banners stay suppressed for Seen sessions (D11). One notification per session: a new one replaces the old (D3 parity).
- **Dock badge.** The Needs you count; empty at 0 [R14 idea 14-10].

### 3.14 Empty and loading states

Empty copy renders only after the first snapshot arrives, so it never flashes during load [R12 idea 12-16].

| Surface | Condition | Copy (13.5–14, `TEXT_2`) | Action |
|---|---|---|---|
| Main | pocketd down (PRD FR 04-5) | "Starting Pocket's terminal service…" | — (retries with backoff) |
| Session row, tab banner | after a pocketd restart (PRD FR 09-6) | "Resumed" · "Interrupted by restart" · "Full access resumed as Ask" · "Couldn't resume" | — |
| Main | no Project | "Add a project to begin." | [Add project] (new button) |
| Main | Project, no tabs | "Pick a session, or start a new one." | [New session ⌘N] |
| Sessions list | no Project | "Add a project with + to start." | — |
| Sessions list | worktree has none | "No sessions yet." | [New session ⌘N] ghost |
| Sessions list | search misses | "No matching sessions" | — |
| Palette | no results | "No matches." / "Try a session title, project, worktree or branch." | — |
| Changes | clean, per scope | "No uncommitted changes" · "No changes vs {base}" · "No changes this turn" | — |
| Changes | not git | "Not a git repository." | — |
| Diff | loading > 150 ms | "Preparing diff…" | — |
| Inbox | none | "Nothing needs you." / "You're all caught up." | — |
| Terminal | connecting / dead | "Connecting…" / "This session is not running." | — |
| Timeline view | no turns yet | "No messages yet." | — |
| Canvas | pocketd down | "pocketd disconnected — sessions can't start." | — |
| Canvas | no CLI | "No agents available" / "Install Claude Code or Codex, then reopen." | — |
| Canvas | branches loading | "Loading branches…" | — |
| Settings search | no hits | "No matching settings" | — |

---

## 4. Interaction spec per component

Common rules (unless a row says otherwise):
- Hover is instant.
- Disabled = opacity 0.4, no pointer, no hover.
- Keyboard focus on inputs = ring `ACCENT_RING` 1 px.
- Loading uses the 11 px spinner in `TEXT_3`, never a skeleton.
- Errors show inline in 12 `FAILED_TEXT`, in place of the control's hint.

| Component | Hover | Active / selected | Focused | Disabled | Loading | Error | Keyboard | Motion |
|---|---|---|---|---|---|---|---|---|
| Project row | `FILL_2`; the trail swaps the mark for [+][···] | `FILL_4` | — | — | tile + "Setting up…" [P ui:380-391] | — | ⌘1–9 does not target it | chevron swap only |
| Worktree row | `FILL_2`; [+][···] | `FILL_4`, name SemiBold | — | — | "Setting up…" | "Couldn't create worktree: {git error}" | — | none |
| Session row | `FILL_1`; age → [···] | `FILL_3` | — | — | — | Failed status label | ⌘1–9, ⌃Tab / ⌃⇧Tab, right-click menu | chips appear after 280 ms |
| Column tabs | `FILL_2` | `FILL_4` + SemiBold | — | — | — | — | — | none |
| Terminal tab | `FILL_2`, close shown | `SURFACE` + `row_shadow` | — | exited 0.55 | — | exit code in the split state line | ⌘⇧[ / ⌘⇧], ⌘W, ⌘T | `TAB_SLIDE` on drag |
| Split header / sash | sash: 1 px line → `SEPARATOR_STRONG`, row-resize | focused title `TEXT` | focused terminal ring | — | — | — | ⌘⌥arrows, ⌘D / ⌘⇧D, ⌘W | `SASH_SNAP` on double-click |
| Terminal | ⌘-hover link underline | selection `TERM_SELECTION` | cursor filled + blinking; unfocused hollow | — | "Connecting…" | "Process exited with code {c}" | §5 Terminal rows; Tab goes to the PTY [P keys:3-8] | blink 530; `FIT_DEBOUNCE` |
| Column seam | line 1 px `SEPARATOR_STRONG`, col-resize | dragging keeps the line | — | — | — | clamps at min/max | double-click resets | none |
| Button (all variants) | Secondary `FILL_4`, Ghost `FILL_3`, Primary / Accent unchanged | pressed opacity 0.85 | ring `ACCENT_RING` 2 | 0.4 | spinner replaces the icon, label stays | — | Enter / Space when focused | none |
| Icon button | `FILL_3` | `FILL_4` when its menu is open | — | 0.4 | — | — | tooltip after 350 | none |
| Menu / popover | row `FILL_2` | keyboard row `FILL_3` | — | row `TEXT_4`, no hover | — | — | ↑/↓ clamp, ↵, Esc, type-ahead off | `MENU_IN` |
| Palette | row `FILL_2` (after the pointer moves) | `ACCENT_BG` | input always focused | — | files arrive async; the group appears later | "No matches." | ↑/↓ wrap, ↵, Tab, Esc, `>` | none |
| Dialog | — | — | first field, else the action | action disabled while its facts load | "Deleting…" on the action | error inline above the footer, dialog stays | Enter confirms, Esc cancels | `DIALOG_IN` |
| Field / input | — | — | ring `ACCENT_RING` 1 | 0.4 | — | ring `FAILED` 1 + message below | platform text keys (gpui-kit Input) | none |
| Changes row | `FILL_2` | `FILL_4` | — | — | — | — | ↑/↓ when the list is focused (Phase 5) | none |
| Diff hunk | hunk actions appear | — | — | actions hidden in Branch / Latest-turn scopes | "Staging…" | "Couldn't stage hunk: {e}" | — | none |
| Comment field | — | — | ring `ACCENT_RING` | Comment disabled when empty | "Sent" pill [P pk/diff.rs:504] | inline error | ⌘↵ add, Esc cancel | none |
| Canvas composer | — | — | ring `ACCENT_RING` 1 | Send disabled until ready | Send spinner while spawning | inline `FAILED_TEXT` under the composer, prompt kept | ⌘↵ start, Esc (menu, else discard), ⌘/ | none (P1) |
| Canvas chips / pickers | `FILL_3` | open: `FILL_3` + chevron up | — | missing CLI 0.5 | "Loading branches…" | "No matching branches" | ↑/↓ (model wraps, access clamps), 1–9, Space (Plan), ↵, Esc | instant |
| Timeline composer | — | — | ring `ACCENT_RING` 1 | Send 0.4 when empty | "Sending…" in the trailer | "Not delivered — click to retry" | ↵ send / queue / steer, ⇧↵ newline, ⌘/ | none |
| Approval panel | option `FILL_2` | — | first option | — | "Sending…" on the chosen option | "Couldn't send the answer" + retry | 1–9 pick, ↵ first option, Esc returns focus | none |
| Tool group | header `FILL_1` | open | — | — | streaming: open, spinner | failed segment in `FAILED_TEXT` | ↵ / Space toggles | chevron only |
| Settings row | — | — | control ring | 0.4 + reason in the description | — | inline | Tab between controls, Esc back | search flash 600 |
| Context ring | hover card after 350 | — | — | hidden when unknown | — | ≥90 % `FAILED` | — | none |

---

## 5. Keymap (D6)

- **Scope.** "Global" = no key context. "Terminal" = `CONTEXT = "Terminal"` [P keys:3]. "Input" = a gpui-kit text input.
- **⌘ never reaches the PTY.** `key_bytes` has no platform arm [P keys], so no ⌘ binding shadows a shell key.
- **Precedence.** Bindings run before `on_term_key` [P pk/main.rs:985-993]. A global ⌃Tab therefore wins over the PTY's `\t`.
- **Collision check.** Every row is unique per context. A unit test asserts that no two `KeyBinding`s share a keystroke and a context (Phase 2).

| Key | Action | Context | Status | Ref |
|---|---|---|---|---|
| ⌘K | Open palette | Global | existing | [P pk/main.rs:1061] |
| ⌘P | Go to file (palette, files) | Global | existing | [P pk/main.rs:1062] |
| ⌘N | New session: canvas, Current checkout (sheet until Phase 4) | Global | existing | D6 |
| ⌘⇧N | New worktree: canvas, New worktree | Global | existing | D6 |
| ⌘J | Go to next Needs you: oldest transition first, wraps | Global | existing [P pk/desktop.rs:253] | D2, D6, D24 |
| ⌘⇧J | Go to Up next: the top-ranked Up next item (Needs you > Failed > Done, non-Seen) other than the current Session, recomputed per press | Global | **new** | D24, D39 |
| ⌘⇧T | Switch Terminal ⇄ Timeline | Global (agent tab, Attached with a Conversation) | **new** | D23 |
| ⌘T | New shell tab in the selected worktree | Global | existing | D6 |
| ⌘\ | Toggle the floating sidebars (Compact) | Global | existing | [P pk/main.rs:960-983] |
| ⌘. | Cycle layout Sidebars → Compact → Focus | Global | existing | [P pk/main.rs:960-983] |
| ⌘, | Settings surface (project settings until Phase 2 ships it) | Global | existing, retargeted | R12 idea 12-10; macOS convention |
| ⌘↵ | Open the selected Inbox item | Global (Inbox) | existing | [P pk/inbox.rs] |
| ⌘↵ | Commit / add comment / start session | Input (commit field, comment field, canvas) | existing | [P pk/main.rs:218-223; pk/forms.rs:209] |
| Tab / ⇧Tab | Sent to the PTY | Terminal | existing | [P keys:3-8] |
| Esc | Close picker → menu → overlay → canvas draft | Global | existing | [P pk/view.rs:1092-1142] |
| ↑ ↓ ↵ Tab Esc | Palette navigation | Palette | existing | [P pk/overlay.rs:152-172] |
| j / k | Inbox next / previous | Inbox | existing | [P pk/inbox.rs:71-73] |
| ⌘1 … ⌘9 | Nth Session in the visible sidebar list after filters, across expanded Worktrees; Compact: the nth rail item | Global; off while an overlay is open | **new** | D6, D40; R04 idea 04-2 |
| ⌃Tab / ⌃⇧Tab | Next / previous session in the same order, wraps; with none selected, first / last | Global (wins in Terminal) | **new** | R04 idea 04-3 |
| ⌘⇧] / ⌘⇧[ | Next / previous terminal tab | Global (session page) | **new** | macOS tab convention; ⌃Tab is taken by sessions |
| ⌘D / ⌘⇧D | Split right / split down (new shell) | Global (session page) | **new** | [P pk/view.rs:751-796] buttons |
| ⌘W | Close the focused terminal: split first, else the tab. Confirms when Working or busy; never closes the window | Global (session page) | **new** | R12 idea 12-8; R13 idea 13-13 |
| ⌘⌥← → ↑ ↓ | Focus the neighbouring terminal | Terminal | **new** | R12 idea 12-8 |
| ⌘C | Copy the selection; no-op without one (never sends ^C) | Terminal | **new** | R16 idea 16-5 |
| ⌘V | Paste, bracketed; multi-line confirm when unsafe | Terminal | **new** | R16 idea 16-6 |
| ⌘A | Select all scrollback | Terminal | **new** | R16 §F5 |
| ⌘F | Find in terminal (↵ next, ⇧↵ previous, Esc close) | Terminal | **new** | R16 idea 16-14 |
| ⌥← / ⌥→ | Send ESC b / ESC f | Terminal | **new** | R13 idea 13-12 |
| ⌘← / ⌘→ | Send ^A / ^E | Terminal | **new** | R13 idea 13-12 |
| ⌘⌫ | Send ^U | Terminal | **new** | R13 idea 13-12 |
| ⌘-click | Open link / `path:line` | Terminal | **new** | R16 idea 16-8 |
| ⌘/ | Open the agent/model card | Canvas, Timeline composer | **new** | R17 idea 17-14 |
| 1 … 9 | Pick the Nth option | open picker card, approval / question panel | **new** | D6 (plain digits); R05 idea 05-4 |
| Space | Toggle "Plan first" | Access menu | **new** | R17 §S3 |
| ↵ / ⇧↵ | Send (or Queue / Steer) / newline; never Stop | Timeline composer | **new** | R05 idea 05-2 |

**Rejected**, so no collisions:

| Key | Proposed as | Why not |
|---|---|---|
| ⌘B | sidebar | ⌘\ exists |
| ⌘J | M terminal / Z drawer | ⌘J is Go to next Needs you (D6) |
| ⌘K | M clear | ⌘K is the palette |
| ⌘1–9 | tabs [R09 idea 09-12] | D6 gives them to sessions |
| ⌘⇧↑ / ⌘⇧↓ | sessions [R09 idea 09-12] | extends selection in inputs; ⌃Tab covers it |
| ⌘⇧U | newest Done [R15 idea 15-15] | ⌘⇧J reaches Done (D24) |
| Mod+Shift+A | archive | no archive |
| ⌘+ / ⌘− | interface scale | 12-18 rejected |

---

## 6. Copy

Tags: **E** = existing, kept · **C** = changed · **N** = new.

**Status and shell**

| Where | Text | Tag | Ref |
|---|---|---|---|
| Status labels | "Needs you" · "Working" · "Done" · "Failed" · "Idle" · "Not attached" | E; C: "Running" → "Working" [P ui:457] | D1 |
| Column header | "{worktree name}" | C (was "Workspace" [P pk/view.rs:636]) | CONTEXT |
| Column tabs | "Sessions" · "Explorer" · "Changes" / "+N −M" | E | |
| Session search | "Search sessions…" | E | |
| Aside | "Search" "⌘K" · "Projects" · "Add project" · "context left" | E | |
| Blank page | "Add a project to begin." + "Add project" · "Pick a session, or start a new one." + "New session ⌘N" | E; N button | R17 §S5 |
| Project menu | "New session" · "New worktree…" · "Settings…" · "Keep in Pocket" · "Reveal in Finder" · "Copy path" · "Remove from Pocket" · "Remove" | E + N | R04 idea 04-14 |
| Worktree menu | "New session" · "New terminal tab" · "Open in editor" · "Reveal in Finder" · "Copy path" · "Copy branch name" · "Delete worktree…" | E + N | |
| Session menu | "Copy path" · "Copy resume command" · "Close session…" | N | R04 idea 04-5 |
| More menu | "New terminal tab" · "Close session" · "Open in editor" · "Reveal in Finder" | E | [P pk/overlay.rs:239-268] |
| Inbox | "Inbox" · "Mark all seen" · "NEEDS YOU" · "FAILED" · "DONE" · "Open session" · "Answer in the terminal ·" | C ("Mark all read"); N "FAILED" | CONTEXT (Seen) |
| Context card | "Context window" · "{used} / {window} tokens" · "{left} tokens remaining" | N | R05 §Spec |
| Aside host line | "Keeping Mac awake" · "Phone access needs Tailscale" | N | PRD FR 16-5 |
| Observe-only banner | "Observe only — pocketd is managed elsewhere" | N | D20 |
| Reconnect | "Starting Pocket's terminal service…" | N | PRD FR 04-5 |
| Restore notices | "Resumed" · "Interrupted by restart" · "Full access resumed as Ask" · "Couldn't resume" | N | PRD FR 09-6 |
| Pair phone dialog | "Pair phone" · "Scan with the iPhone Camera" · "Or enter this code: {code}" · "Paired" | N | PRD FR 03-7 |

**Terminal**

| Where | Text | Tag | Ref |
|---|---|---|---|
| Tab menu | "New tab in {branch}" · "New shell" · "Claude Code" · "Codex" | E | |
| Tooltips | "New terminal" · "Close terminal" · "Split right" · "Split down" | N | R16 §Spec |
| Pill | "Jump to bottom ↓" | N | R16 §Spec |
| States | "Connecting…" · "This session is not running." · "Process exited with code {c}" | E | |
| Banners | the two `status::banner` strings | E | [P pk/status.rs:101-110] |
| Find | "Find" · "{i} of {n}" · "No results" | N | R16 idea 16-14 |
| Paste confirm | "Paste {n} lines into {title}?" · "Paste" · "Cancel" | N | R16 idea 16-6 |
| Close confirm (agent) | "\"{title}\" is still working in {worktree}. Close this terminal anyway?" · "Close terminal" · "Cancel" | N | R13 idea 13-13 |
| Close confirm (shell) | "\"{command}\" is still running in {worktree}. Close this terminal anyway?" | N | R13 idea 13-13 |

**Timeline view and composer**

| Where | Text | Tag | Ref |
|---|---|---|---|
| View switch | "Timeline" · "Terminal" | N | |
| Placeholder | "Message {provider name}…" | N | |
| Tool summary | "Ran N commands · edited N files · read N files · searched N times · fetched N pages · updated todos · called N tools · N failed" | N | R05 idea 05-1 |
| Folding | "Show more" · "Show less" · "Show full output ({n} KB)" | N | R05 idea 05-12, 05-16 |
| Trailer | "Working… {elapsed}" · "Sending…" · "Worked for {elapsed}" | N | R05 idea 05-8 |
| Delivery | "Not delivered — click to retry" | N | R05 idea 05-9 |
| Scroll pill | "Scroll to bottom" | N | R05 idea 05-17 |
| Button tooltips | "Send" · "Queue" · "Steer" · "Stop" | N | R05 idea 05-2 |
| Approval | "Needs you · {tool}" · provider option labels · "Answer in the terminal" · "Open terminal" | N | R14 idea 14-11 |
| Plan | "Needs you · Plan" · "Approve" · "Keep planning" · "Feedback (optional)" | N | PRD FR 12-3 |
| Question | "{i}/{n}" · "Select one or more options." · "Next" · "Submit" · "Type your own answer, or pick an option above" | N | R05 §Spec |

**Changes and diff**

| Where | Text | Tag | Ref |
|---|---|---|---|
| Changes | "Changes" · "Staged Changes" · "Commit" · "Commit All" · "Commit & Push" · "Amend Last Commit" · "Push" · "Committing…" · "Pushing…" · "Amending…" · "Couldn't write a message: {e}" | E | [P pk/changes.rs] |
| Diff header | "Unified" · "Split" · "Viewed" | E | |
| Scopes | "Working tree" · "Branch changes" · "Latest turn" · "No turn recorded yet — send a message first" | N | R06 §Spec |
| Scope headers | "{n} Changed files vs {base}" · "{n} Changed files this turn" · "{n} Uncommitted changes" | N | R06 §Spec |
| Folds | "{n} unchanged lines" · "Expand upward" · "Expand downward" | E + N | R13 idea 13-16 |
| Hunks | "Stage hunk" · "Revert change" · "Revert this change?" · "This can't be undone." | N | R13 idea 13-2 |
| Notices | "New file" · "Deleted file" · "Renamed from {from}" · "Binary file — contents not shown" · "Diff truncated — showing first {n} of {m} lines" · "Preparing diff…" | N | R06 idea 06-6 |
| Comments | "Line N" · "Lines a–b" · "Comment" · "Cancel" · "Sent" · "Resolve" · "No session" · "{n} comments" · "Send {n} comments" | E + N | R06 idea 06-2 |
| Discard | "Discard changes to {file}?" · "Discard changes to N files?" · "Unstaged edits can't be restored" · "Deletes N untracked files" · "Couldn't discard changes" · "The worktree changed since you opened this. Review and try again." | E + N | R06 idea 06-7 |
| Push safety | "Push to default branch \"{branch}\"?" · "Push" · "Cancel" | N | R13 idea 13-6 |

**Dialogs**

| Where | Text | Tag | Ref |
|---|---|---|---|
| Remove project | "Remove {name}?" · "Its files stay on disk" | C (was "The repository stays on disk"; CONTEXT) | |
| Delete worktree | "Delete {tree}?" · "Deletes the folder {~path}" · "Keeps the branch {branch}" · "{n} uncommitted files will be lost." · "{n} commits on {branch} aren't on a remote. They stay on the branch." | E + N | D8; R13 idea 13-15 |

**New-session canvas** [R17 §S1–S7]

| Where | Text | Tag |
|---|---|---|
| Canvas | "New session" · "Describe what the agent should do…" · "⌘↵ to start · esc to cancel" · "What should we work on in {project}?" (optional) | E + N |
| Chips | "{Provider} · {model} · {effort}" · "Ask" · "Auto-accept edits" · "Auto" · "Full access" · "Plan first" · "Review a plan before building." · "Turn off Plan first" | C: "Permissions" / "Auto-edit" / "Plan only" [P pk/forms.rs:506-507] are replaced |
| Checkout | "Checkout" · "Current checkout" · "Current worktree" · "New worktree" · "From {base}" · "on {branch}" · "branch" | C: picker head "Workspace" → "Checkout" |
| Base | "Default" · "Recent" · "Search branches…" · "No matching branches" · "Showing 20 of {n} branches" | E + N |
| Model | "Effort" · "Default" (badge) · "Search models…" · "No models found" · "Not in the current model list" | N; C: "Default model" is never shown |
| Project | "Projects" · "Add project…" · "No projects match." | N |
| States | "pocketd disconnected — sessions can't start." · "No agents available" · "Install Claude Code or Codex, then reopen." · "Install the Codex CLI to enable. Install with `npm install -g @openai/codex`" · "Loading branches…" | N |
| Name errors | "A worktree or branch with this name already exists" · "Use letters, digits, - _ or ." | E |

**Palette**

| Where | Text | Tag |
|---|---|---|
| Input | "Search sessions, files and actions…" · "esc" | E |
| Groups | "Up next" · "Sessions" · "Worktrees" · "Files" · "Actions" | C (adds Up next, Worktrees) |
| Row detail | "{project} · {worktree} · {status}" (sessions) · "{dir} · modified" (files) | C (adds worktree) |
| Actions | "New session in {project}" ⌘N · "New worktree in {project}" ⌘⇧N · "New terminal tab" ⌘T · "Split right" ⌘D (was "Open selected in a split") · "Go to next Needs you" ⌘J (was "Jump to next waiting session") · "Go to Up next" ⌘⇧J · "Show Timeline" / "Show Terminal" ⌘⇧T · "Pair phone…" · "Phone access level…" · "Needs you sound: On/Off" · "Done sound: On/Off" · "Failed sound: On/Off" · "Toggle layout" ⌘. · "Settings" ⌘, · "Add project…" · "Appearance: Match system" · "Appearance: Light" · "Appearance: Dark" (Appearance: S1) | C + N |
| Footer | "↑↓ Navigate" · "↵ Open" · "Tab to search all projects" / "Tab to filter by project" · "esc Close" | E + N ("esc Close") |
| Empty | "No matches." · "Try a session title, project, worktree or branch." | E + N |

**Settings** [R12 §F12]

| Where | Text | Tag |
|---|---|---|
| Nav | "App" · "General" · "Appearance" · "Notifications" · "Keybindings" · "Agents" · "Projects" · "Back" | N |
| Page | "Settings / {Section}" · "Search settings" · "No matching settings" | N |
| Rows | "Open agent sessions in" · "Appearance" · "System" · "Light" · "Dark" · "Reduce motion" · "On" · "Off" · "Desktop notifications" · "Banners for sessions you aren't viewing." · "Session sounds" · "Plays when a session you aren't viewing changes status." · "Needs you" · "Done" · "Failed" | N (CONTEXT names instead of Zeron's "Task completed" / "Input required" / "Errors and disconnections") |
| Keybindings | "Shortcuts" · "Command" · "Keybinding" · "When" | N |
| Agents | "Found at {path}" · "Not installed" | N |
| Project page | "Default base branch" · "Worktrees folder" · "Copy into each worktree" · "Save" · "Cancel" · "Remove from Pocket" | E |

**Notifications** [R13 idea 13-20]

| Where | Text | Tag |
|---|---|---|
| Title | "{session title}" (fallback provider name) | E |
| Body line 1 | "{project} · {worktree}" (no subtitle API, D30) | N |
| Body line 2 | "Needs you" / "Done" / "Failed", or the ask / first paragraph ≤ 240 chars | C |
| Actions | "Allow" · "Deny and stop" (permission asks only) | N |
| Dock | "{n}" Needs you count | N |

---

## 7. Phased adoption

Every phase must pass these gates:
- `cargo clippy` and `cargo test -p theme -p ui -p pocket -p keys`
- capture-mode screenshots of the touched screens in light and dark (`cargo run --release -p pocket --features capture -- --capture <dir> …`)
- the CLAUDE.md baselines: Changes frame ≤ 3 ms at 1137 files; click → diff ≤ 100 ms

| Phase | Scope (desktop) | Dependency | Tests / checks |
|---|---|---|---|
| **0 Fix now (D12)**, 1 day | Codex Auto-edit `--full-auto` → `-s workspace-write -a on-request` [P pk/forms.rs:412] (R17 idea 17-1). Terminal ligatures off (`liga` / `calt` / `dlig` = 0) (R16 idea 16-3). Agent self-approval fix is pocketd's (R18), no UI | none | argv unit test per provider × access; capture of `--yolo` spacing |
| **1 Tokens + dark**, weeks 1–2 (first visible value) | §2.0 Palette + `p(cx)` + both-mode `init`; WHITE split; literals → tokens; follow macOS appearance; palette "Appearance: …" actions (persisted). D1 remap §2.2 incl. "Running" → "Working", count badge. Radii: popover 12, dialog 16, palette 16. Light AA pass (TEXT_3 → TEXT_2 sites). D7: delete `CONTEXT_WINDOW` [P ag:57], hide usage card and rail bars when unknown. Copy fixes tagged C in §6 that need no new feature. Terminal fg / bg / selection / cursor per mode (ANSI if the shim allows) | none | `contrast.rs` (both palettes); status → glyph / colour mapping test; screenshot pairs of aside, Sessions, session page, palette, dialogs, diff |
| **2 Shell**, weeks 3–4 | D2: sort by `created_at`, delete the three status sorts; ⌘1–9 + chips; ⌃Tab; ⌘J = next Needs you, ⌘⇧J = Up next; palette v2 (§3.11); right-click and session menus; seams 20 px + double-click reset; persist widths, layout and window bounds; min window 900×600; `MENU_IN` on all menus. Notifications: body lines (D30) + Allow / Deny and stop, Dock badge, sounds (§2.9). Settings surface v1 (Appearance, Notifications, Projects list opening the existing modal); ⌘, → Settings | none | order test (a status change keeps the order); Up next ranking; sound gate (coalesce, priority, silent baseline, Seen); keymap uniqueness test; jump-hint modifier test |
| **3 Terminal (D9)**, weeks 5–7 | 16-1 scrollback, 16-2 wheel, 16-4 selection, 16-5 ⌘C, 16-6 ⌘V + confirm, 13-12 Mac keys, 16-11 fit debounce; then 16-7 scrollbar, 16-8 links, 16-12 tab drag / middle-click / exited, 16-16 cursor, 16-17 sash; ⌘D / ⌘⇧D / ⌘W / ⌘⌥arrows / ⌘⇧[ ]; close confirm 13-13; OSC title + bell dot 16-15; then 16-14 find, 16-9 mouse, 16-10 IME | libghostty shim additions (R16 §F2) | `key_bytes` tests for ⌥←/→, ⌘←/→, ⌘⌫; paste-safety classifier; wheel routing table; capture with 10k lines of scrollback |
| **4 New-session canvas (D5)**, weeks 8–9 | §3.9: canvas replaces the sheet; access picker 17-2 / 17-3; model card 17-7; checkout / base chips 17-6; CLI probe 17-8; offline / spawn states 17-9; remembered picks 17-5; tab-menu quick rows 17-11; `claude -n` 17-12; ⌘/ 17-14 | `agent.create` LaunchSpec + pocketd argv builder (17-10) | LaunchSpec from chip state; defaults; Full access never remembered |
| **5 Changes / diff**, weeks 10–11 | wash + bar (colours shipped in 1); scope menu (Working tree, Branch changes); hunk stage / revert 13-2; fold steps 13-16; batch comments 06-2 + quoted prompts 13-3; caps + notices 06-6; staleness-guarded discard 06-7; push safety 13-6; delete-worktree unpushed fact 13-15 | "Latest turn" needs pocketd turn snapshots (06-1) | hunk-apply index text test; comment prompt format test; perf gate at 1137 files |
| **6 Timeline view + composer** (strategy-dependent) | §3.6–3.7: view switch, Timeline view (`list`), tool groups, trailer, scroll pill, optimistic bubble; composer morph; approval panel; context ring in the composer | 14-3 full timeline decode; WS prompt / interrupt / resolve (14-2); D4 Codex app-server; 14-11 structured asks; 14-21 reported window (D7) | summary grammar; composer morph table; context thresholds 74 / 75 / 89 / 90 / unknown |
| **7 Settings v2** | search + flash 12-11; Keybindings table (read-only); Agents pages; Project pages move into the surface (the modal retires) | none | settings search ranking; every binding appears in the table |

Order constraints:
- 0 comes before everything.
- 1 comes before everything else, because every new component reads `p(cx)`.
- 2 ships the Settings surface that D11's toggles need, before sounds reach users.
- 4 comes before 6: the Timeline composer reuses the canvas composer and pickers.
- Phone spawn and remote reach stay behind R18 (D10). This spec adds neither.

**This horizon deviates** (05-roadmap §7.1, D25, D26, D43): Phase 1 ships light tokens only (E16 PR1), before the D1 remap (E08 PR1); dark is S1. Sounds ship with palette toggles before any Settings surface (S1). The canvas is S10, so the Timeline composer (E14) is a plain input with no pickers, and the New session sheet takes the §6 chips (E06 PR4).

---

## 8. Decisions made here (not settled elsewhere)

- **Tokens.**
  - Accent `#5B43E8` / `#8B7CF6`, with `ON_ACCENT` black in dark.
  - `WAITING` light moves from `#FFB224` to `#A16207`, so one amber serves glyph and badge fill with white text (4.93). `WAITING_TEXT` light is `#855612`.
  - Text-tier tokens (`*_TEXT`) are split from glyph tokens so both pass AA / 3:1 on `WINDOW`.
- **Status glyphs.** Full-strength α on desktop (not Zeron's 0.55–0.9). Failed keeps Pocket's × glyph, so shape differs per status.
- **Order.** Sessions sort by `created_at` desc, because `updated_at` moves rows on every status change (D2). The rail and palette status sorts go.
- **Keys.**
  - ⌘J goes to the next Needs you (D6). ⌘⇧J walks Up next (Needs you > Failed > Done, non-Seen) (D24), which makes ⌘⇧U unnecessary.
  - ⌘⇧↑ / ⌘⇧↓ are dropped (text-selection conflict).
  - ⌘⇧[ / ⌘⇧] cycle terminal tabs. ⌘D / ⌘⇧D split. ⌘W closes a terminal, never the window.
  - ⌘, moves from project settings to the Settings surface, which holds each Project's settings.
- **Chrome.**
  - Pocket's chrome heights (42 bars, 80 session rows, 30 split headers, 56 rail) and pill buttons are kept.
  - Only the popover (12), dialog (16) and palette (16) radii change.
  - Hovers stay instant. Folds are instant.
  - Menus gain Pocket's existing 150 ms / −4 px entrance. The palette and canvas stay instant.
- **Blur.** None, in either mode. Dark drops drop-shadows.
- **Sidebar structure.** Pocket's two-column layout stays: Projects aside → worktrees, then a Sessions column for the selected worktree. Zeron's single tree is not adopted, because Pocket already separates worktree choice from session choice.
- **Tabs.** Agent tab labels show the session title. " · N panes" is dropped.
- **Up next.** The palette gets an "Up next" group (name shared with the phone). The Inbox gets a "FAILED" section. "Mark all read" → "Mark all seen".
- **Sounds.** Named after CONTEXT statuses. The baseline stays silent on reconnect as well as launch.
- **Notifications.** Body line 1 "{project} · {worktree}" (no subtitle API, D30). The Dock badge counts Needs you only.
- **Timeline view.** Content max width 760. No rotating trailer words.
- **Composer.**
  - The composer placeholder is "Message {provider name}…".
  - Stop is a separate state of the send button and is never bound to Enter.
- **Empty states.** "No sessions yet." for an empty worktree, and an "Add project" button on the no-Project blank page.
- **Copy.** "Remove {name}?" fact reads "Its files stay on disk" (CONTEXT: not "repository").
- **Settings nav width** equals the aside width, so nothing shifts on open.

## 9. Open questions

1. **Default view for agent sessions.** Answered: Terminal, no setting (D22).
2. **Spinner cost.** `with_animation(...).repeat()` requests a frame every vsync. While any Working spinner is on screen, the whole window redraws at display rate (principle 3).
   - Measure in capture mode with 1137 files and 3 spinners.
   - If it's over budget, step all spinners on one shared ~12 fps clock [R07 idea 07-14].
3. **Terminal line height.** 22 (today) or 18 (Zeron)? [R16 §Open questions].
4. **ANSI palette.** Does the libghostty shim let Pocket set the 16-colour palette, fg / bg and selection per mode? [R07 §Open questions 4].
5. **Black on dark accent** for Accent buttons [R07 §Open questions 3]. Should Accent buttons in dark use `SOLID` instead? Decide from the Phase 1 screenshots.
6. **Queue vs Steer on Claude's PTY.** Answered per provider by the E01 step-0 check (D37); Steer waits for the E10 spike.
7. **Narrow windows.** At the 900 px minimum with default widths, the main area is 294 px. Auto-Compact below a threshold, or leave it to ⌘.?
8. **Pins and custom sections** (R04 idea 04-6, 04-15) are left out. Revisit after Phase 2 if stable order alone doesn't keep long-running sessions reachable.
