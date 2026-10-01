# 12 — MonoCode: shell layout, navigation and design tokens

Date: 2026-09-30.

Sources:
- hardbeat920/monocode@cdc1441dc51e3709cd843e5c316608a123f323c6 (HEAD 2026-09-30 08:11 +0100). MIT. Tauri 2 + React 19 + Tailwind v4.
- https://usemono.dev
- Screenshots: README hero (v0.1.31, 3360x2100) and `M docs/screenshot.jpg` (v0.1.0, 3356x2098; the file is PNG data despite the .jpg name).
- Tailwind v4 default theme: https://github.com/tailwindlabs/tailwindcss/blob/fa81d697fe572a10ac150d18964a093a7a874081/packages/tailwindcss/theme.css (MonoCode depends on `tailwindcss ^4`, `M package.json:78`, and overrides only colors, fonts and leading, so radii, shadows, blur and type sizes are these defaults).
- gpui-pre 0.3.6 and gpui-kit 0.6.6 from the local Cargo registry, for what Pocket can express.
- Pocket worktree `orchestrator-research` @ 86deb13, for mapping only. Markdown/transcript styles are in `P docs/research-monocode-markdown-styles.md` and are not repeated here.

Citation legend: `M path:L` is a file and line in the MonoCode clone at the SHA above, path relative to the clone root. `P path:L` is a file and line in this Pocket worktree, path relative to the worktree root. `R crate/path:L` is a crate in the local Cargo registry (`~/.cargo/registry/src/index.crates.io-*/`). `TW:L` is a line in the Tailwind v4 `theme.css` linked above. `Z` (Zeron) is not used. "CL x.y.z" is that version's section in `M CHANGELOG.md`. "Shot" is the README hero screenshot. Hex values marked "computed" are derived by me from MonoCode's HSL tokens over the flat base colour. Where code and docs disagree, the code wins and the gap is flagged "code vs docs".

## TL;DR

- **One ink, two lightness inputs.** Every surface is `content` ink alpha-mixed over `background-base`. The inputs are hue 240, saturation 0%, base lightness 9% dark / 97% light, and ink lightness 92% / 18%. That gives dark #171717 / #ebebeb and light #f7f7f7 / #2e2e2e (computed). Borders are ink 7%. The five selection fills are 8/10/12/15/20% in dark and 5/6/7/10/14% in light. There is one accent, hsl(211 92% 62%) = #459bf7 (`M src/styles/index.css:25-116`).
- **Dark by default; glass is dark-only.** The macOS window is transparent with a private CGS blur (1–64, default 24). The sidebar tint sits at 0.85 alpha. Light mode is opaque (`M src/features/settings/model/appearance.ts`, `M src-tauri/src/macos.rs:43-54`). gpui already exposes `WindowBackgroundAppearance::Blurred` (`R gpui-pre-0.3.6/src/platform.rs:2449-2464`). Pocket is light-only, with static u32 constants (`P packages/desktop/crates/theme/src/theme.rs:9-62`).
- **Every chrome row is 40 / 36 / 32 / 28 px.**
  - 40: headers and the tab strip, aligned to a 40px traffic-light container.
  - 36: sub-toolbars and split headers.
  - 32: list rows and rail items.
  - 28: the footer and inputs.
  - Radius is 6px on nearly everything, 12px on popovers and settings groups, 16px on modals. Pocket is rounder: 8, 10, 14 and 26 (`P packages/desktop/crates/ui/src/ui.rs:33,439,739`; 14 at `P packages/desktop/crates/pocket/src/view.rs:377`).
- **Layout.** From left to right:
  - Project rail: 200px, range 180–360. It collapses to a 48px icon rail or hides.
  - "Workspace" column: 260px, range 260–560, capped at 50% of the window. It has Sessions / Inbox / Explorer / Changes tabs.
  - Main area: a 40px title bar whose tabs each hold a split tree of Sessions, a terminal dock, and a 28px usage footer.
  - With the compact rail on, the column opens as a push-drawer: 200ms open, 160ms close.
- **Tabs group Sessions; splits are a real tree.** `LayoutNode` leaf/split, direction right/down, fractional `sizes`, `MIN_SIZE 0.08`.
  - Dropping a split header or a sidebar Session row on a pane edge picks the dominant axis from the centre.
  - The sash is a 1px line with a 12px hit area.
  - Pocket's model is rows of panes with no sizes and no drag (`P packages/desktop/crates/workspace/src/workspace.rs:1-44`).
- **Session row = 3 lines + a coloured status word.**
  - Line 1: agent icon, model, and one of "Need approval" (amber-400), "Working..." (braille spinner, accent), "Done" (emerald-400), "Draft", or a relative time.
  - Line 2: 13px semibold title.
  - Line 3: `repo/branch` 11px.
  - A waiting Session gets a dashed border on a 20% fill. Folders tint at accent 18% and show an aggregate status when collapsed.
- **Palette: code vs docs.**
  - ⌘P "Go to File" and ⇧⌘P "Command Palette" are the same dialog. A leading `>` switches it to commands.
  - At HEAD the command list has exactly one entry, "Reload MonoCode" (`M src/features/files/ui/FilePicker.tsx:40-42`; CL 0.1.51 calls it "its first action").
  - ⌘K is a full-surface search over conversations, messages, files, content and projects, with per-group limits.
- **Settings is a full surface, not a sheet.**
  - 10 sections in 3 groups: App / Agents / Workspace. Each has a one-line description.
  - A header search jumps to a row and flashes it with accent.
  - Rows stack below 560px container width.
  - The keybindings table has command / keys / when-clause, click-to-record, and Delete to disable.
- **Motion tokens.**
  - Feedback: 120ms. Reorder: 160ms. Tab width: 200ms. All use `cubic-bezier(0.22,1,0.36,1)` except tab width, which uses `(0.3333,0.6667,0.6667,1)`.
  - Popovers take 170ms and modals 200ms, both `cubic-bezier(0.16,1,0.3,1)`.
  - Reduced motion turns all of these off.
- **Best ports, in order:**
  1. Dark theme via a derived token set (12-1).
  2. Split tree with sizes and a sash (12-6).
  3. Two-line session tabs with an agent status stack (12-4).
  4. Settings surface primitives (12-10).
  5. `>` command mode plus pointer guard in the palette (12-12).
  6. Motion tokens (12-2).

## Findings

### F1. Colour model (`M src/styles/index.css:6,25-116`)
- Custom dark variant: `dark (&:where(html:not(.theme-light), html:not(.theme-light) *))` (`:6`). Light is the class `html.theme-light` (`:102`).
- Inputs (`:71-96`): `--theme-hue 240`, `--theme-saturation 0%`, `--theme-dark-lightness 9%`, `--content-lightness 92%`. In light mode (`:102-116`) the base lightness is 97% and content lightness is 18%.
- `--color-background-base` = hsl(hue sat base-lightness). `--color-content` = hsl(hue sat content-lightness) (`:28-33`).
- `--color-stroke` = content 7% over transparent. Structural separators use only this (`:34-35`).
- Selection fills are content at a strength var (`:36-62`):

  | Fill | Dark | Light |
  |---|---|---|
  | subtle | 8% | 5% |
  | selection | 10% | 6% |
  | strong | 12% | 7% |
  | hover | 15% | 10% |
  | emphasis | 20% | 14% |

  A comment explains that light mode gets "gentler ink" (`:36-37`).
- Fixed colours:
  - accent hsl(211 92% 62%)
  - skill #e8c547, light #a07c10
  - mention #38bdf8, light #0284c7
  - link #7dd3fc, light hsl(211 92% 40%)
  - markdown heading #f9a8c9, light #be185d
- The accent does not change between light and dark. Users can override it: `applyAccentColor` sets `--user-accent-color` and `--user-accent-foreground`. The foreground is black when sRGB relative luminance is above 0.179, white otherwise (`M src/features/settings/model/appearance.ts` `accentForeground`).
- Status colours are raw Tailwind palette classes, not tokens: amber-400 needs approval, emerald-400 done and additions, red-400 deletions and errors, teal-400 done in tabs, fuchsia-300/65 orchestration (`M src/app/shell/Sidebar.tsx:2994-2998,2830-2834`; `M src/app/shell/TitleBar.tsx`).
- Tab-group colours: 9 HSL values. Index 0 is neutral hsl(210 8% 58%). A project name is hashed ×31 onto indices 1–8 (`M src/features/workspace/model/tabGroups.ts:6-24`).
- Sidebar tint `.sidebar-glass` (`:266-279`):
  - dark: base mixed 90% with black, i.e. slightly darker than the body
  - light: base
  - dark with native glass: base at `--sidebar-opacity` (default 0.85)
  - `.body-glass` = base (`:293-295`), or base × sidebar-opacity when "Main pane glass" is on (`:1215-1221`).
- Primary button `.primary-action` (`:1381-1425`):
  - dark: white background, black text, hover white 90%, disabled white 30% with 40% text
  - light: content background, base text
  - with a user accent: accent background with the computed foreground, hover mixes 88% with content

### F2. Type, spacing, radii, shadows, blur
- Fonts: `--font-sans` system-ui stack, `--font-mono` ui-monospace/SFMono stack (`M src/styles/index.css:64-70`). No bundled webfonts. `M package.json` has no font packages.
- `--leading-label: 1.4`, "including two 10px lines in a 30px tab" (`:26-27`).
- Tailwind defaults in use:

  | Token | Value | Source |
  |---|---|---|
  | text-xs | 12/16 | `TW:347-348` |
  | text-sm | 14/20 | `TW:349-350` |
  | text-lg | 18/28 | `TW:353-354` |
  | text-xl | 20/28 | `TW:355-356` |
  | spacing | 4px | `TW:325` |
  | weights | 500 / 600 | `TW:378-379` |

- Arbitrary sizes seen in code: 10, 11, 12, 13px.
- Radii: rounded 4, `rounded-[5px]` 5, md 6, lg 8, xl 12, 2xl 16 (`TW:398-402`).
- Shadows (`TW:408-412`): only shadow-sm (rail search), shadow-lg (error tips), shadow-xl (popovers), shadow-2xl (modals).
- Blur: backdrop-blur-xl 24px (`TW:480`) on every popover, modal and palette. backdrop-blur-sm 8px on the empty-search tile.
- Custom shadows:
  - Explorer drag preview: `0 4px 12px rgb(0 0 0/18%), 0 1px 2px rgb(0 0 0/20%)`, 26px high, radius 5, 12px text, border content 12% (`M src/styles/index.css:1296-1356`).
  - Light composer: `0 6px 24px content 9%, 0 2px 6px content 6%` (`:1373-1379`).
- Non-mac only: 6px scrollbars, thumb content 16% (32% on hover), fully rounded (`:132-160`). On mac the system scrollbars are kept.
- Root: `position: fixed`, `user-select: none`, `overscroll: none`. Text areas opt back in to selection (`:118-130,1223-1235`).

### F3. Motion
- Tokens (`M src/styles/index.css:71-76`):
  - `--motion-reorder-duration 160ms`
  - `--motion-tab-close-duration 200ms`
  - `--motion-feedback-duration 120ms`
  - `--motion-ease-out cubic-bezier(0.22,1,0.36,1)`
  - `--motion-tab-ease-out cubic-bezier(0.3333,0.6667,0.6667,1)`
- JS reads the same CSS variables so gestures and CSS stay in sync. The tab-close fallback is 180ms (`M src/shared/lib/motion.ts:1-20`).
- Reorder is FLIP-style `transform` using the tokens. After a drag, clicks are suppressed for duration + 400ms (`M src/shared/hooks/useAnimatedReorder.ts:86-88,226`).
- Tabs (`M src/styles/index.css:177-221`):
  - background, colour and opacity transition at 120ms.
  - `.tab-opening` / `.tab-closing` animate width over 200ms.
  - Default slot width is 14rem. Opening tabs have a min-width of 7rem. Collapsed tabs go to width 0 with margin-right −0.125rem.
  - `TabWidthMotion` waits two rAFs, toggles `data-collapsed`, then finishes after the token duration (`M src/app/shell/ClosingTab.tsx`).
- Session drawer (Web Animations, width, which pushes the main area): open 200ms `(0.22,1,0.36,1)`, close 160ms `(0.4,0,1,1)`. A reversal mid-slide starts from the current width (`M src/app/shell/Sidebar.tsx:773-812`).
- Popover in: 170ms `(0.16,1,0.3,1)` from opacity 0, scale .94, 8px offset toward the anchor. Transform origin follows side and align (`M src/styles/index.css:2572-2625`, `M src/shared/ui/Popover.tsx:358`).
- Modal: backdrop fade 160ms ease-out. Panel 200ms `(0.16,1,0.3,1)` from translateY(8px) scale(.98) (`M src/styles/index.css:2978-3013`).
- Never animate the glass layer, only its sibling content (`M src/app/shell/GlassBackdrop.tsx`).
- Toast in: 180ms `(0.22,1,0.36,1)` from translateY(−8px) scale(.98) (`:1930-1943`).
- Live indicators:
  - `.shimmer-text` gradient sweep, 2s linear infinite, base ink at 0.4 alpha (`:1964-1990`). The busy project name uses 1.4s (`M src/app/shell/ProjectRail.tsx:924-1060`).
  - Braille spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, 80ms per frame, 11px text (`M src/features/sessions/ui/TerminalSpinner.tsx:3-28`).
  - Running-terminal indicator: three 4x8px bars, #e39b4a, opacity .4 to .85, 3.2s loop (`M src/styles/index.css:3015-3085`).
  - Mascot beat: 460ms (`:1991-2058`).
- Press feedback: `active:scale-[0.97]` on rail and footer chips (`M src/app/shell/Sidebar.tsx:2550+`, `M src/app/shell/UsageProviderChip.tsx:224`).
- Reduced motion disables tab width motion (and hides closing tabs), drawer animation and reorder (duration 0) (`M src/styles/index.css:177-221`; `M src/app/shell/Sidebar.tsx:791-797`; `M useAnimatedReorder.ts:87`).
- There is also a user setting, General → "Tab animations" (`M src/features/settings/ui/SettingsView.tsx` GeneralPage).

### F4. Window and titlebar
- Window: 1280x800, minimum 800x520, `titleBarStyle "Overlay"`, `hiddenTitle`, `transparent`, `backgroundColor "#171717"`, `shadow true`, `macOSPrivateApi true` (`M src-tauri/tauri.conf.json:14-28`).
- Traffic lights are re-pinned with Auto Layout inside the 40px bar (`M src-tauri/src/macos.rs:1-21,45-55`):
  - `TAB_BAR_HEIGHT 40`, `BUTTON_SIZE 14`, `LEFT_MARGIN 12`, `BUTTON_SPACING 6`, `TOP_INSET (40−14)/2 = 13`.
  - The buttons therefore end at x = 66. Every left header reserves a `w-[78px]` spacer (`M src/app/shell/ProjectRail.tsx:333-351`).
  - The compact title bar uses `pl-[70px]` instead (`M src/app/shell/TitleBar.tsx:905`).
- Glass:
  - Private `CGSSetWindowBackgroundBlurRadius`, range 1–64, default 24.
  - An NSVisualEffectView backing at alpha 0.01 prevents chamfered corners.
  - The window launches opaque; glass turns on after first paint (`M src-tauri/src/macos.rs:1-21`).
  - Glass is enabled only when the scheme is dark (`M appearance.ts` `applyThemePreference` L358-373).
- Drag regions: `data-tauri-drag-region="deep"` on the title bar. Controls opt out with `="false"` (`M src/styles/index.css:1470-1485`).
- Non-mac: an in-app MenuBar and WindowControls. Linux shows a centred system title at 11.5px, content/40 (`M src/app/shell/TitleBar.tsx`).
- The native menu mirrors the shortcuts: File / View / Edit / Window / Help plus the app menu. User overrides are rewritten into the menu accelerators (`M src-tauri/src/menu.rs:40-115,453-539`).

### F5. Window layout (Shot; `M src/app/App.tsx:10476-10520`)

```
┌ rail 200 (180–360) ┬ Workspace col 260 (260–560, ≤50% win) ┬ main ────────────────────────────────┐
│ 40  ●●●  ← → ▣     │ 40  "Workspace"            ⌕  +        │ 40  tabs (two-line) …  ▣ ▦ ⌕ + ⚙     │
│ 32  Search ⌘K      │ 36  Sessions|Inbox|Explorer|Changes    │     split tree (per tab)              │
│ 32  Inbox • / Notes│ 36  [⌕ Search conversations...] [≡]    │     ├ 36 header ⠿ ● title  ×          │
│ Pinned / Groups /  │     folder groups, Session rows        │     └ body                            │
│ Projects +         │                                        │  terminal dock (⌘J)                   │
│ 32 project rows    │                                        │ 28  usage footer · terminal chip      │
│ footer: Settings ⌘,│                                        │                                       │
└────────────────────┴────────────────────────────────────────┴───────────────────────────────────────┘
```

- Root: `flex h-full flex-col`. With glass on it also gets `bg-background-base/40` (`M src/app/App.tsx:10516-10520`).
- Every tab mounts its own PaneTree. Inactive tabs are hidden, not unmounted, so terminals and scroll state survive.
- Search, Inbox, Notes, Automations and Settings are full-surface views. They replace the main column and the others are `hidden` + `inert` (`M src/app/App.tsx`).
- UsageFooter shows only when no full surface is open.
- Column separators are 1px `border-stroke`.
- Resize handles are 6px wide (`w-1.5 -right-px`), content/10 on hover and content/15 while dragging. Double-click resets to the default (`M src/app/shell/Sidebar.tsx:2104-2114`).
- Widths persist in localStorage: `monocode.projectRailWidth` and others (`M appearance.ts:11-34`). Column constants: `M src/app/shell/Sidebar.tsx:180-183`.

### F6. Project rail (`M src/app/shell/ProjectRail.tsx`, `M src/app/shell/RailAction.tsx`)
- `nav.sidebar-glass border-r`. The 40px header holds the 78px mac spacer and TabVisitNav: back, forward, sidebar toggle (`:333-351`).
- Visit history is browser-style, capped at 50 entries (`M src/features/workspace/model/tabVisitHistory.ts:3`).
- Top actions (`:362-400`, `px-2 pb-2 pt-0.5 gap-px`):
  - RailSearch "Search ⌘K": `h-8 rounded-md border border-content/8 shadow-sm px-1.5` (`M RailAction.tsx:109`).
  - Inbox, with an unread dot and a right-click menu for notifications.
  - Notes (only if enabled) and Automations.
- RailAction row (`M RailAction.tsx:56-78`):
  - `h-8 rounded-md px-2 gap-2`. Active is `bg-selection`; otherwise content/50, hover content/10.
  - Icon size-4 at opacity .7. Label text-sm medium. Shortcut 11px content/40.
  - Dot: size-2 accent. Badge: min-w-4 accent pill, 10px semibold white, "99+" cap.
- Sections "Pinned", "Groups" (FolderPlus add) and "Projects" (+ open folder / remote) (`:618-680`):
  - Header `px-3 pb-1.5 pt-1`, label text-xs content/50. Add buttons size-5.
  - Empty state: "No projects yet", 11px content/40.
- Project row (`:924-1060`):
  - `h-8 rounded-md px-2`. Selected is `bg-selection-strong`; unselected rows sit at opacity .65.
  - 16px logo slot: a project image, or a 12px pixel mascot in the group colour that animates while busy.
  - Name: `text-sm font-medium leading-tight truncate` (`:838`), shimmering while busy.
  - Diff stat: "+N" emerald-400 / "−N" red-400, 11px semibold tabular.
  - Remote: globe icon with an emerald online dot. Muted: BellOff amber-400.
  - On hover: "…" (size-6) at the right, and pin/unpin replaces the logo. Padding shifts to pr-6 over 150ms.
  - Context menu or Shift+F10 opens the menu. Drag-reorder on the y axis.
- Footer: LiveAgentsPreview (shown when 2 or more agents are live; see report 09), update footer "Check for updates v0.1.31" (Shot), GitHub star prompt, "Settings ⌘,".
- Collapsed mode (`M src/features/settings/model/settings.ts:585-590`): `"compact" | "hidden"`, default compact.
  - The compact rail is `w-12` (48px) (`M Sidebar.tsx:2437`). Top 40px is a drag region.
  - Actions in order: expand, project picker, the vertical workspace tabs (Sessions / Inbox / Explorer / Changes), Search, Inbox, Notes, Automations. Settings sits at the bottom.
  - CompactRailAction: `size-8 rounded-md active:scale-[0.97]`, icon size-4, dot size-1.5 at top-1.5 right-1.5 (`M Sidebar.tsx:2550+`).
  - The icon under the dot is masked with a 2px gap: the 6px dot sits at (23, 9) in the 32px button (`M src/styles/index.css:281-291`).
- Drawer mode = compact rail visible and session column closed (`M Sidebar.tsx:749-752`). A compact-rail tab icon slides the column in (F3). Escape or an outside click dismisses it.

### F7. Workspace column: tabs, search, Session rows, folders (`M src/app/shell/Sidebar.tsx`)
- Frame: `aside.body-glass border-r border-stroke`. Header `h-10` holds "Workspace" (text-sm medium, `pl-3 pr-1.5`) plus search and new-session icons (`:1584-1657`).
- Tab list: `h-9 border-b px-2 gap-px` (`:1545-1580`).
  - Buttons `h-6 rounded-md px-2 text-[12px]`. Active `bg-selection text-content`, otherwise content/50.
  - Labels: "Sessions", "Inbox", "Explorer", "Changes" (`:189-201`). The Changes tab carries a diff stat.
  - Reorderable. Order persists in `monocode.sidebarTabOrder`, default sessions, inbox, files, changes.
- Search row: `h-9 px-2`, input `h-7` "Search conversations...", 12px search icon at 50%, ListFilter button (`:1694-1712`, Shot).
- List: `ul flex-col gap-0.5 p-1.5`. Order (`M src/features/sessions/model/sessionFolders.ts:110-150`): Reminders, then folders (empty ones hidden), then Pinned, then ungrouped.
- Groups (Pinned, Reminders #f59e0b, folders):
  - Container: `rounded-md bg-content/5`, or a folder tint of `color-mix(accent 18%)` (`M sessionFolders.ts:365-372`); `mb-1.5`.
  - Inner `ul gap-px p-1` of compact rows.
  - Footer `border-t border-stroke p-1` with a "+ New session" button (Plus size-3, 13px semibold, content/45).
- Folder row (`:2734-2835`):
  - `h-8 px-2 gap-1.5`. The 16px icon slot shows the folder icon and swaps to a chevron on hover.
  - Name 13px semibold. Count 11px tabular.
  - When collapsed it also shows the worst child status: CircleAlert amber-400, spinner accent, or Check emerald-400.
  - F2 renames; the input is `bg-content/10 ring-1 ring-accent/40`. Drag-reorder shows a `h-0.5 bg-accent` line.
- Folder model: `{id, name, sessionIds, collapsed, colorIndex?, customColor?}`, key `monocode.sessionFolders`. New names are "New folder", "New folder 2", … (`M sessionFolders.ts:8-22`).
- Session row, `SessionCard` (`:2921-3480`):
  - Frame `rounded-md px-2.5 border`, py-2 (py-1.5 when compact inside a group).
  - Line 1: agent icon size-3.5, model 11px content/50, status at the right.
  - Line 2: optional pin, then title 13px semibold `line-clamp-1` (mt-1).
  - Line 3: GitBranch + "repo/branch" 11px content/45. Hover reveals an Archive button (size-5). Linked PR/issue "#N" in accent. Automation Zap in amber.
  - A linked-update dot: size-1.5 accent.
  - Status word (`:2990-3040`), 11px tabular:

    | State | Icon | Colour | Copy |
    |---|---|---|---|
    | needs approval | CircleAlert | amber-400 | "Need approval", or "Needs input" for orchestration |
    | busy | spinner | accent | "Working..." |
    | done | Check (2.25 stroke) | emerald-400 | "Done" |
    | draft | CircleDashed | content/55 | "Draft" |
    | otherwise | none | content/45 | relative time |

  - Frame states, in precedence order (`:3256-3273`):

    | State | Style |
    |---|---|
    | drop target | `bg-accent/20` overlay |
    | multi-selected | `bg-accent/15` |
    | needs approval | `bg-content/20 border-content/30 border-dashed` |
    | active | `bg-selection` |
    | draft | `border-content/25 border-dashed` |
    | default | content/80, hover content/5 |
    | dragging | opacity .4 |

  - Keys: Enter / Space select, F2 renames, Delete / Backspace deletes.
  - Multi-select: Shift selects a range, ⌘/Ctrl toggles (`:1343-1375`).
  - Hover prefetches the transcript after 120ms (`:2919`).
  - Dragging starts after 5px. Drop onto a pane edge to split, or onto a folder. Escape cancels.
- Empty and status copy (`:1689-1749`):
  - "No project folder"
  - "Couldn’t load sessions"
  - "No matching sessions"
  - "No sessions match these filters"
  - "This project’s machine isn’t connected on this computer."
  - The first load stays blank on purpose, so a placeholder never flashes.
- SessionsEmpty (`M src/features/sessions/ui/SessionsEmpty.tsx`):
  - A 16x12 pixel-art terminal SVG with a 0.4-opacity glow scatter, `w-24 text-content/25`.
  - Copy "Sessions you start will show up here", 13px content/45, `gap-5 px-6 py-10`.

### F8. Title-bar tabs = groups of Sessions (`M src/app/shell/TitleBar.tsx`)
- A tab holds one or more Sessions and files in a split tree. Fields: project, title, `more[]`, sessionCount, harnesses, busy/done harnesses, files, multiPane, dirty, terminal, preview (`:52-80`).
- Bar: `h-10 border-b border-stroke`, drag region.
- Strip: `gap-0.5 pl-1.5 pr-2.5`, scrollbar hidden. The wheel converts vertical scroll to horizontal.
- Overflow chevrons: size-6.5 `bg-content/10 backdrop-blur-xl`. Each click scrolls max(60% of width, 112px).
- Slot: `w-56 min-w-28 shrink` (224px, shrinking to 112px) (`:992`).
- Tab button: `h-7.5 rounded-md px-2 gap-1.5`, pr-7 when closable (`:333`). Active `bg-selection`, otherwise content/50 with hover content/5.
- Two-line layout when the container is at least 11rem (176px) (`:366,381`):
  - headline: 10px medium, the conversation title, file, or "New session"
  - meta: 10px content/45, the other title, "N sessions", or a file (`tabCopy` `:118-162`)
  - Below 176px it collapses to a single 13px line.
  - Preview tabs are italic. The dirty dot is size-1.5 content/70.
- Close: X size-5 at right-1, visible on hover. Middle-click closes. Double-click pins a preview tab. The last blank tab is not closable (`titleTabClosable`).
- Agent stack (TabHarnesses):
  - Up to 3 icons overlapped −0.125rem, then "+N".
  - Busy = spinner in accent. Done-unseen = CheckCircle teal-400. Otherwise the agent icon.
  - Opacity .55 on inactive tabs.
- Context menu, 244px wide (`:1070`; items `:764-815`):
  - "Close Tab ⌘W", "Close Other Tabs", "Close Tabs to the Right", "Close Tabs to the Left"
  - "Archive" — "All N conversations in this tab"
  - "Delete" (danger) — "Permanently delete all N conversations in this tab"
- Drop hint when a split is dragged into the strip: `w-0.5 bg-accent shadow-[0_0_8px_accent]` between slots (`:1001`).
- Right-side buttons: "Toggle Sidebar (⌘B)", "Toggle Session Sidebar (⌘⇧B)", "Go to File (⌘P)", "New session (⌘T)", "Settings (⌘,)" (`:591,873-936`).
- With no project, a CwdPicker labelled "No project" appears.
- Tab groups by project:
  - Each group has a colour, label, logo and mascot.
  - Menu: "New tab in group", "Move group to new window", "Close group", "Delete group" (`M src/features/workspace/ui/TabGroupMenu.tsx:96-118`).
  - Storage keys: `monocode:tab-group:colors|custom-colors|labels|logos|mascots` (`M tabGroups.ts:26-31`).
- Per-pane file tabs (SurfaceTabs): `h-9 border-b`, with a grip "Drag to reorder pane" `h-7.5 w-5` (`M src/features/workspace/ui/SurfaceTabs.tsx:268-289`).

### F9. Splits (`M src/features/workspace/model/layout.ts`, `M src/features/workspace/ui/PaneTree.tsx`)
- Model (`layout.ts:11-33`):
  - `LayoutNode` = leaf | split `{dir: right|down, children, sizes}`.
  - Same-direction splits share one group with equal sizes.
  - `MIN_SIZE 0.08` of the parent (`:105`). A pair clamps to min(MIN_SIZE, pair/2) (`:1013-1027`).
- Drag rules (`:11-20`):
  - Dropping on the same axis reorders.
  - Dropping on the perpendicular axis nests a new split.
  - Dropping from another group relocates.
  - Sidebar Session rows use the same edges.
- `paneEdgeFromPoint` (`:1182-1191`): take the normalized offset from the centre; the dominant axis wins; the sign picks left/right or top/bottom. So each quarter-triangle of a leaf maps to one edge.
- Leaves are positioned absolutely by rect percentage. The dragged leaf sits at opacity .4 (`PaneTree.tsx`).
- Drop hint (`PaneTree.tsx:560-583`): a half-leaf `bg-accent/15` wash plus a 2px accent line on the target edge.
- Sash (`PaneTree.tsx:585-697`):
  - A visible 1px `bg-stroke` line with a hit area of ±6px (`-left-1.5 -right-1.5`).
  - col-resize or row-resize cursor. The preview is rAF-throttled. Escape cancels. `role=separator`.
  - `html.is-resizing` forces the cursor globally (`M src/styles/index.css:1351-1352`).
- Split header, shown only when the tab has more than one leaf (`M src/features/sessions/ui/SessionPane.tsx:709-753`):
  - `h-9 border-b px-2 gap-1.5 cursor-grab`.
  - GripVertical size-3.5 at content/35. Focus dot `size-2` accent when focused, transparent otherwise.
  - Title text-xs. Close button "Close Pane (⌘W)" size-5.
- Keys: ⌘D splits right. ⇧⌘D splits down. Both are `!editorFocus`. ⌘⌥arrows move focus. ⌘W closes the pane (`M settings.ts:938-1036`).

### F10. Search (⌘K), Go to File (⌘P), Command Palette (⇧⌘P)
- **FilePicker = both ⌘P and ⇧⌘P.** ⇧⌘P just preloads the query ">" (`M src/app/App.tsx:9545-9561`; `M src/features/files/ui/FilePicker.tsx:83-84`).
  - Frame (`FilePicker.tsx:215-247`):
    - fixed, z `LAYER.dialog` 90, no dim.
    - `top-[12%] w-[min(560px,100vw−24px)] rounded-lg border border-content/10 bg-content/5 backdrop-blur-xl`.
  - Input row: `border-b px-2 py-2.5`, search icon size-3.5 (1.75 stroke), 13px input, placeholder "Go to File (type > for commands)".
  - List: `max-h-[min(380px,50vh)] px-1.5 pb-1.5`. Rows `h-8 rounded-md px-2 gap-2 text-sm`; highlighted row is `bg-selection`.
  - File rows: type icon, name, and the directory at the right (mono 11px content/40, max 45%) (`:416-456`).
  - Match characters are `text-accent` (`M src/shared/ui/MatchText.tsx:22`).
  - Command rows end in a kbd hint: `rounded border-content/10 bg-content/5 px-1.5 py-0.5 mono 10px content/50` (`:356`).
  - Keys:
    - ↑/↓ wrap around. Enter runs. Tab is swallowed.
    - Escape closes, in the capture phase (`:150-212`).
    - Recents rank first: recently opened files, then files currently open (`:73-82`).
  - Pointer guard: hover moves the highlight only after a real mouse move. It re-arms on list change or keyboard move. This stops the list scrolling under a still cursor from stealing selection (`:381-409`).
  - Empty copy (`:290-296`):
    - "No matching commands"
    - "Open a project to search files"
    - "Indexing files…"
    - "No files found"
    - "No matching files"
    - "Type a file name to search"
  - **Code vs docs:** `ACTIONS` = `[{id:"reload", label:"Reload MonoCode"}]` only (`:40-42`). The keybinding table and CL 0.1.51 present a "Command Palette", but it runs one command.
- **Search surface (⌘K)** (`M src/features/search/ui/SearchView.tsx`):
  - A full surface: header `h-10` with the input "Search everything..." (13px), and a spinner while loading (`:385-415`).
  - Scope row `h-9 px-3 gap-px`: "All", "Conversations", "Files", "Projects". Buttons `rounded-md px-2 py-1 text-[12px]`, selected `bg-selection` (`:53-58,422-440`).
  - Remote queries (session messages, file content) are debounced 200ms (`:211-279`).
  - Group limits (`M src/features/search/model/appSearch.ts:92-123`):

    | Scope | Conversations | Messages | Files | Content | Projects |
    |---|---|---|---|---|---|
    | All | 8 | 8 | 10 | 12 | 6 |
    | Conversations | 24 | 24 | – | – | – |
    | Files | – | – | 40 | 48 | – |
    | Projects | – | – | – | – | 24 |

  - When truncated: "Results limited to the first matches" (11px content/45). Other copy: "No results"; errors in red-400 (`:375,454-457`).
  - Empty state (`:476-509`):
    - A 27x19 grid of 3px dots, gap 7px, opacity .14, masked by `radial-gradient(ellipse 72% 68%, #000 18%, transparent 76%)`.
    - A centred 56px tile: `rounded-2xl bg-content/6 backdrop-blur-sm` with a 24px search icon.
    - Copy "Find files, conversations, messages, and projects." 13px content/45.
- ⇧⌘F "Find in Files" opens Explorer with its search field focused (`M src/app/App.tsx:10241-10242`, `onFindInProject`).

### F11. Keyboard
- The full table is in report 09 (§UI/UX, from `M settings.ts:938-1036`). Additions from this pass:
  - Rows are `{command, keys, when}`. The when-clauses used are: `sessionFocus && !overlay` (Archive ⇧⌘A), `!overlay && (!textFocus || emptyComposer)` (⌘↑/↓ within a tab), `!editorFocus` (⌘D, ⇧⌘D), and "Anywhere" (Quick Composer ⇧⌘Space).
  - Extra bindings: ⌃Tab / ⌃⇧Tab cycle tabs; ⌘+ / ⌘− / ⌘0 zoom; ⌘F / ⌥⌘F editor find/replace; ⇧⌘G toggle workspace in the composer; Escape stops the focused turn (`M src/features/workspace/model/tabKeys.ts:1-31`).
  - App shortcuts ignore IME composition. User overrides are checked first (`M src/features/settings/model/appShortcuts.ts`).
  - Overrides live in `monocode.keybindingOverrides` with the change event `monocode:keybindings-change`. The native menu re-reads them (`M src-tauri/src/menu.rs:40-115`).
- Pocket conflicts, restated from 09: ⌘J (NextWaiting vs dock), ⌘. (ToggleFocus vs model), ⌘, (ProjectSettings vs app Settings) (`P packages/desktop/crates/pocket/src/main.rs:1046-1057`).

### F12. Settings (`M src/features/settings/model/settings.ts`, `M src/features/settings/ui/SettingsView.tsx`, `M src/app/shell/SettingsRail.tsx`)
- The surface replaces the main column. The project rail turns into SettingsNav (`M ProjectRail.tsx`).
  - SettingsNav: groups have a `text-xs font-semibold text-content/35` label. Rows `px-2 py-1.5 rounded-md`, icon size-4 at .7, text-sm medium.
  - "Back" (ArrowLeft) is pinned at the bottom (`M SettingsRail.tsx:51-69`).
- Header `h-10 border-b`: breadcrumb "Settings / <Section>" at 13px ("Settings" content/45, "/" content/25).
  - At the right: "Restore defaults" (Appearance only) and the settings search (`M SettingsView.tsx:480-512`).
  - Escape closes unless a dialog handles it first.
- Sections (`M settings.ts:50-135`):

  | Group | Section | Description (verbatim) | Rows in the search index (`:156-410`) |
  |---|---|---|---|
  | App | General | "The build you are running, how MonoCode reaches you, and the panels it shows." | Version, Sounds, Notifications, Notes, Quick composer, Working agents, File tabs, Tab animations, Close to tray |
  | App | Connections | "Connect your machines and run agents remotely through SSH." | Your machines |
  | App | Appearance | "Theme, tint, translucency, workspace layout, and conversation backgrounds." | Theme, Accent color, Hue, Saturation, Dark-mode lightness, Sidebar opacity, Blur radius, Main pane glass, Interface scale, Collapsed project rail, Show excluded files, Chat background |
  | App | Keybindings | "Every shortcut the workspace handles, from the app menu and the key handler." | (table) |
  | Agents | Chat | "How transcripts read, what the composer does with a follow-up, how files save, and how diffs open." | Transcript layout, Anchor prompts to top, Follow-up behavior, Model controls, Composer mascot, Format on save, Diff view, Empty session games |
  | Agents | Providers | "Provider accounts, agent CLIs MonoCode can drive, and the model new sessions start with." | Agent CLIs, Provider accounts, Claude Code hooks |
  | Agents | Skills | "Discover and manage file skills from project, personal, and harness folders." | – |
  | Workspace | Inbox | "Manage Inbox services and notification preferences for each project." | Project notifications, GitHub, GitLab, ADO, Jira, Linear |
  | Workspace | Archive | "Projects and conversations you have archived." | Show archived in the sidebar |
  | Workspace | Worktrees | "Manage additional worktrees for each project." | Project worktrees |

- Appearance page groups (`M SettingsView.tsx:2032-2226`), verbatim:
  - **Theme** — "Dark and light share the same tint, so the color settings below apply to both."
    - Theme: Dark / Light / System, "System follows the OS appearance."
    - Accent color: "Used for the composer send button and your message bubbles."
  - **Color** — "Hue and saturation tint every surface. Lightness only moves the dark theme."
    - Hue 0–360, default 240
    - Saturation 0–100, default 0
    - Dark-mode lightness 0–30, default 9
  - **Translucency**
    - Sidebar opacity 0.15–1, default 0.85, "Applies to the project rail and the other glass panes."
    - Blur radius 1–64, default 24
    - Main pane glass, default on
  - **Layout**
    - Collapsed project rail: compact / hidden
    - Interface scale 0.5–2, step 0.1, default 1, applied with webview `setZoom` (`M src/features/settings/model/uiScale.ts`)
    - Show excluded files
  - Chat background: effects none / dither / ascii / halftone / scanlines / "Haze", opacity 0.05–0.65, default 0.24 (`M appearance.ts`).
- Every value lives in localStorage `monocode.*` (`M appearance.ts:11-34`). Nothing syncs across machines.
- Primitives (`M SettingsView.tsx:3854-4335`):

  | Primitive | Spec |
  |---|---|
  | Page | `max-w-5xl` (1024px), `px-5 py-6 pb-16`; at ≥560px container width `px-8 py-8` (`:534`) |
  | PageHeader | h1 20px semibold, tight leading; description 13px content/45, `max-w-xl` (576px), relaxed leading, mt-1.5, pb-4 |
  | Group | `pt-8` (none if first). Title 13px semibold, description 12px content/45, `pb-2.5`. Box `rounded-xl border border-content/10 bg-content/3` |
  | Row | `flex items-start gap-6 px-4 py-3.5 border-b border-content/5` (none on the last row). Label 13px medium; description 12px content/45 mt-1. Control column `max-w-[60%]` right-aligned, gap-2 |
  | Row, narrow | below 560px the row stacks with gap 0.75rem; switch-only rows stay inline with gap 1.5rem (`M src/styles/index.css:8-23`) |
  | Segmented | `inline-grid gap-0.5 rounded-md border border-content/10 p-0.5 text-[12px]`; item `rounded-[5px] px-2.5 py-1`; on = `bg-selection`, off = content/50 |
  | Slider | 224px track + value `w-10` 12px tabular; 4px track (`M src/styles/index.css:1493-1510`) |
  | Toggle | 36x20 pill; 16px white knob, left 2px / 18px; on `bg-accent`, off content/20; plays a "switch" sound cue |
  | Select | trigger `max-w-52 rounded-md border border-content/10 bg-content/5 px-2 py-1 12px` + chevron that rotates 180°; popover maxHeight 320, options `rounded-lg px-2 py-1.5 12px` with a Check. Used instead of a native select because the OS popup is unreadable in dark mode on Windows/Linux |

- Settings search:
  - Input `h-7 w-48 rounded-md border border-content/10`, placeholder "Search settings".
  - Results popover: 300px wide, max 320px tall, up to 8 hits. Each shows the label plus the section in 11px content/40 (`M SettingsView.tsx:589-700`; `M settings.ts:432-435`).
  - Picking a hit opens the section, scrolls to `data-setting-id`, and flashes the row `bg-accent/10` (or the group border `accent/60`) (`M SettingsView.tsx:3893-3945`).
- Keybindings page (`M SettingsView.tsx:2393-2720`):
  - Title "Shortcuts" — "Click a shortcut to record new keys. Press Delete while recording to disable it."
  - Filter input `w-44` (176px) "Filter", with a count.
  - Header row: 11px semibold uppercase, tracking .08em, content/40. Columns: Command (flex) / Keybinding (160px) / When (112px).
  - Rows `h-11` (44px), 12px. The When column is mono 11px content/40.
  - Key chip: `h-6 w-28 rounded-md border mono 11px`, border accent while focused.
    - Recording shows "Record…" or the live preview.
    - A disabled binding shows "Disabled" with a dashed border.
    - A reset (RotateCcw) sits beside it.
  - Keys while recording: Escape cancels; Delete / Backspace alone disables.
  - Conflicts show a red tooltip under the chip, `shadow-lg`.

### F13. Overlays and layers
- LAYER (`M src/shared/lib/layers.ts:9-17`):

  | Layer | z |
  |---|---|
  | popover | 80 |
  | submenu | 81 |
  | dialog (modals and palette) | 90 |
  | dialogPopover | 91 |
  | toast | 100 |

  Panels stay below 50.
- Popover (`M src/shared/ui/Popover.tsx:62,345-358`; `M src/shared/lib/popover.ts:49-50`):
  - `isolate overflow-hidden rounded-xl border border-content/10 shadow-xl` over GlassBackdrop.
  - maxHeight 400. Gap 6px, viewport padding 8px. It flips to the opposite side when that has more room.
- GlassBackdrop: `absolute inset-0 z-0 rounded-[inherit] backdrop-blur-xl` plus `.popover-backdrop`. That is content 2% in dark and opaque base in light (`M src/app/shell/GlassBackdrop.tsx:9-19`; `M src/styles/index.css:2572-2580`).
- Modal (`M src/shared/ui/Modal.tsx:12-13,59,87-144`):
  - Sizes: sm 420px at top 22%, md 560px at top 10%, both min(…, 100vw−24px).
  - Panel `rounded-2xl border border-content/7 shadow-2xl`, glass `bg-background-base/55`. Backdrop `bg-black/40`.
  - Header `px-4 pt-3`, title text-xl medium, description 12px content/50.
  - Close `size-7 rounded-md`, X at 14px. Escape closes in the capture phase. `fitViewport` caps height at 100dvh − 32px.

### F14. Icons
- Hugeicons free set: `@hugeicons/core-free-icons ^4.3.0` + `@hugeicons/react ^1.1.10` (`M package.json:48-49`).
  - 98 glyphs are deep-imported so the 5MB catalog is not bundled (`M src/shared/ui/icons.tsx:1`).
  - Each is renamed to a lucide-style name (Check = Tick02, ChevronDown = ArrowDown01, …) (`:160-173`).
- The default stroke is 1.75 (`:114`). Checks use 2.25 for weight at small sizes (`M Sidebar.tsx:2834`). Hand-drawn fold icons use 1.5 (`:134-155`).
- Sizes: 12px inline, 14px buttons and lists, 16px rail and nav, 24px empty states.

### F15. Footer and empty Session
- Footer `h-7 border-t border-stroke px-3 gap-1.5 text-[11px] text-content/55`, scrolling horizontally (`M src/app/shell/UsageFooter.tsx:313`).
  - Usage chips per provider, `h-5 rounded px-1`, e.g. "9% 6d 5h" (Shot; details in report 09).
  - A refresh button `size-4.5`.
  - A running-terminal chip at the right with the three-bar indicator (`:294-296,405-408`).
- Empty Session (`M src/features/sessions/ui/EmptySession.tsx:43-68`):
  - Headline "What should we work on in <project>?" (or "What should we work on?"), text-lg.
  - The composer sits in the same `max-w-4xl p-1.5` box as the docked composer, so it keeps its width when the first message docks it.
  - Behind it: a 6px-cell / 1px-gap dot grid, border opacity .06, peak .72, 33ms frames, with optional pac-man / snake idle games (`M src/features/terminal/ui/TerminalGridBackground.tsx:19-32`). Toggle: Chat → "Empty session games".
- Composer placeholder: "Ask, build, / for commands, @ for references... " (`M src/features/sessions/ui/Composer.tsx:2087-2100`). Report 09 quotes the older "/ for skills".

### F16. Pocket today (mapping baseline)
- Theme (`P packages/desktop/crates/theme/src/theme.rs`):
  - Light only.
  - Text ramp TEXT…TEXT_6 = #111113 → #d4d4d8 (`:9-15`). WINDOW #f4f4f5, SURFACE #fff (`:18-20`).
  - FILL_1–4 = ink alpha 0x08 / 0x0b / 0x0e / 0x11 (`:22-25`). Hairline and separators 0x12 / 0x17 / 0x1f (`:26-28`).
  - Accent #5b5bd6 with alpha variants (`:30-34`). Waiting #ffb224, running #30a46c, failed #e5484d (`:36-43`).
  - Fonts: SANS `.SystemUIFont`, MONO "Geist Mono", with bundled Geist and Geist Mono ttf (`:6-7,124-133`).
  - 40 SVG icons (`:143-147`).
  - The main checkout has uncommitted Material-icons work (`theme/build.rs`, `theme/assets/material/`); it is not in this worktree.
- UI primitives (`P packages/desktop/crates/ui/src/ui.rs`):

  | Primitive | Spec |
  |---|---|
  | `side` | `#fafafbb3`, 0.5px right hairline (`:27`) |
  | `pop` | radius 26, 4-layer shadow (`:32`) |
  | `glass` | (`:42`) |
  | `segmented` | (`:146`) |
  | `status` pill | 20px tall, labels "Needs you" / "Working" / "Failed" / "Sent" / "Draft" / "Not attached" (`:217-243`) |
  | `session_row` | 10px padding, radius 8, 14px semibold title (`:428-450`) |
  | `status_label` | (`:453`) |
  | `modal` | title 20 bold (`:713`) |
  | `palette_row` | 40px, radius 10, selected `ACCENT_BG` (`:728-746`) |

- Layout (`P packages/desktop/crates/pocket/src/view.rs`):
  - Rail 56px (`:535`). Projects column 200–420 and Sessions column 280–600, resized with a 5px handle and no double-click reset (`:160-190`). Defaults: Projects 272px (`:248`), Sessions 334px (`:616`; 348px when it shows the Inbox list, `:584`).
  - Header rows are 42px (`:195,617`), the tab strip 40px (`:913`), the session search 44px (`:638`).
  - Traffic lights at (14, 14) (`P packages/desktop/crates/pocket/src/main.rs:1065`).
- Tabs and splits: `Tab::Term(Vec<Vec<String>>)`. Split right appends to the last row; split down adds a row. No sizes, no drag, no focus navigation (`P packages/desktop/crates/workspace/src/workspace.rs:1-44`).
- Palette ⌘K: Sessions (top 5, sorted by status then recency) + files + actions, placeholder "Search sessions, files and actions…" (`P packages/desktop/crates/pocket/src/overlay.rs:79-110`; `P packages/desktop/crates/pocket/src/main.rs:188`). ⌘P is GoToFile.
- Overlays deliberately have no entrance motion for the palette and new-session sheet. Others use 200ms / 150ms `ease_out_quint` with an 8 / −4px rise (`P packages/desktop/crates/pocket/src/overlay.rs:311-330`). Reduced motion is followed (`P packages/desktop/crates/pocket/src/main.rs:1027`).
- Settings: only a per-project settings sheet on ⌘, (`P packages/desktop/crates/pocket/src/forms.rs:474`).
- gpui can do: blurred window background (`R gpui-pre-0.3.6/src/platform.rs:2449-2464`, `window_background` at `:2226`), system appearance (`R gpui-pre-0.3.6/src/app.rs:1494`), and gpui-component ThemeMode light/dark (`R gpui-component-0.6.6/src/theme/mod.rs:119`).

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 12-1 | Derived two-scheme tokens: base + ink lightness, ink-alpha ramp (stroke 7%, 5 selection steps per scheme), fixed accent; Light / Dark / System | Dark mode for a tool used all day; one change retints everything | `M src/styles/index.css:25-116`; `M appearance.ts` | `theme` crate: adapt. Replace `pub const` u32s with a `Tokens` global built from (scheme, hue, sat), read via `cx.global`; follow `cx.window_appearance()` for System; retint `highlight_theme()` and gpui-component `ThemeMode` | L | Call-site sweep across `pocket` / `ui` (237 `rgba(` call sites in about 7k lines) |
| 12-2 | Motion tokens: feedback 120, reorder 160, tab width 200, popover 170, modal 200, drawer 200/160; two easings + reduced-motion zeroing | Consistent, quick feel; one place to tune | `M src/styles/index.css:71-76,2572-2625,2978-3013`; `M src/shared/lib/motion.ts` | `theme`: new `motion` module (Durations + `fn ease_out_expo(t)` for `cubic-bezier(0.22,1,0.36,1)`); `ui`/`pocket`: port `ease_out_quint` call sites | S | None |
| 12-3 | Waiting Session row treatment: dashed 1px border + 20% ink fill, status word on line 1 (amber "Needs you" / spinner "Working" / green "Done" / relative time) instead of a pill | Needs-you rows pop out of a long list without colour-only cues | `M src/app/shell/Sidebar.tsx:2990-3040,3256-3273` | `ui::session_row` / `status_label`: adapt (`P packages/desktop/crates/ui/src/ui.rs:428-460`) | S | None |
| 12-4 | Two-line tabs: headline 10px medium + meta 10px (other Session title / "N sessions"), collapsing to one 13px line under 176px; stack of up to 3 agent icons (spinner = Working, check = Done unseen) + "+N" | See which tab has a working or finished agent without opening it | `M src/app/shell/TitleBar.tsx:52-162,302-400,333,366-381` | `pocket` view.rs tab strip: adapt (`P packages/desktop/crates/pocket/src/view.rs:905-918`); state from existing Session status | S | Seen state per Session |
| 12-5 | Tab close/open width motion (200ms) + FLIP reorder (160ms) + drag-to-tab drop hint (2px accent + 8px glow) | Tabs don't jump; clear drop target | `M src/styles/index.css:177-221`; `M TitleBar.tsx:1001`; `M useAnimatedReorder.ts:86-88` | `pocket` tab strip: new `with_animation` on width; `ui::drop_line` exists (`P ui.rs:420`) | M | 12-2 |
| 12-6 | Split tree with fractional sizes: leaf/split(right/down), MIN 0.08, 1px sash with 12px hit area, rAF preview, Esc cancel | Real side-by-side Terminals at useful ratios | `M src/features/workspace/model/layout.ts:11-33,105,1013-1027`; `M PaneTree.tsx:585-697` | `workspace` crate: port the model (replace `Vec<Vec<String>>`); `pocket` termview: sash; send pocketd `resize` on commit only | L | None |
| 12-7 | Edge-drop to split: drag a split header or a sidebar Session row onto a Terminal edge; dominant-axis rule; half wash accent/15 + 2px edge line | Build layouts by dragging, no menu | `M layout.ts:1182-1191`; `M PaneTree.tsx:560-583`; `M Sidebar.tsx` drag 5px threshold | `pocket`: new drag payload + drop overlay; `workspace`: `insert_at(edge)` | M | 12-6 |
| 12-8 | Split header only when a tab has 2+ Terminals: 36px, grip, 8px accent focus dot, 12px title, close (⌘W); ⌘⌥arrows focus neighbour | Always know which Terminal gets keystrokes | `M src/features/sessions/ui/SessionPane.tsx:709-753`; `M settings.ts:938-1036` | `pocket` termview.rs: new header; `workspace`: `neighbour(dir)`; bind `cmd-alt-left/right/up/down` | S | 12-6 for spatial focus |
| 12-9 | Compact 48px rail + push-drawer for the Sessions column (200ms open / 160ms close; Esc or outside click closes) | Reclaim width on laptops and still reach Sessions in one click | `M Sidebar.tsx:749-812,2365-2560`; `M settings.ts:585-590` | `pocket` view.rs: adapt the 56px rail (`:535`) to host column toggles; animate width | M | 12-2 |
| 12-10 | Settings as a full surface: nav grouped App / Agents / Workspace, "Settings / Section" crumb, Group + Row primitives, Toggle 36x20, Segmented, Slider, Select; stack under 560px | Room for growing prefs (theme, notifications 09-2, keys) with one consistent layout | `M settings.ts:32-135`; `M SettingsView.tsx:3854-4335`; `M src/styles/index.css:8-23` | `ui`: new `setting_group`, `setting_row`, `toggle`, `slider`; `pocket`: new settings view replacing the main area; ⌘, → app settings, project settings as a section | M | Decide the ⌘, conflict (F11) |
| 12-11 | Settings search → jump + one-shot accent flash of the row | Find a toggle by name | `M settings.ts:156-410,432-450`; `M SettingsView.tsx:3893-3945` | `pocket` settings view: static index `(id, section, label, keywords)` + scroll-to + 1s flash | S | 12-10 |
| 12-12 | Palette: `>` prefix switches to commands; fuzzy positions highlighted in accent; wrap-around ↑/↓; pointer guard (hover selects only after a real mouse move); precise empty copy ("Indexing files…", "No matching files", …) | Keyboard-first control without a second shortcut; no stolen selection | `M src/features/files/ui/FilePicker.tsx:40-42,83-104,150-212,290-296,381-409` | `pocket` overlay.rs: adapt `palette_sections` (`:79`); `ui::palette_row` gains match highlighting | S | None |
| 12-13 | Keybindings page: Command / Keybinding / When table, click-to-record, Delete disables, Esc cancels, per-row reset; overrides persisted and applied to gpui `KeyBinding` with context predicates | Resolve Pocket vs habit conflicts (⌘J, ⌘., ⌘,) per user | `M SettingsView.tsx:2393-2720`; `M settings.ts:938-1036`; `M src-tauri/src/menu.rs:40-115` | `pocket`: new page; `store` crate: overrides; rebuild the keymap on change (`P main.rs:1046-1057`) | M | 12-10 |
| 12-14 | Session folders: tinted group (accent 18%), 13px semibold name + count, aggregate worst status when collapsed, "+ New session" footer, F2 rename | Park and triage 20+ Sessions per Worktree | `M Sidebar.tsx:2734-2835`; `M sessionFolders.ts:8-22,110-150,365-372` | pocketd store: folder model (so the phone agrees); desktop `ui`: `section_header` variant (`P ui.rs:670`); app: same grouping | M | 09-15 |
| 12-15 | Native blurred sidebar: `WindowBackgroundAppearance::Blurred`, sidebar alpha 0.85; main pane glass is a toggle, on by default | Depth and context without extra chrome | `M src-tauri/src/macos.rs:1-21`; `M appearance.ts:136` (`BODY_GLASS_DEFAULT = true`); `R gpui-pre-0.3.6/src/platform.rs:2449-2464` | `pocket` main.rs `WindowOptions.window_background`; `ui::side` alpha from tokens | S | 12-1 (MonoCode enables glass in dark only) |
| 12-16 | Empty states: one small illustration (pixel terminal / dot grid) + one 13px line at 45% ink; never flash a placeholder on first load | Calm first run; no flicker | `M SessionsEmpty.tsx`; `M SearchView.tsx:476-509`; `M Sidebar.tsx:1689-1749` | `pocket` view.rs `empty()` (`:115`): adapt; SVG assets in `theme` | S | None |
| 12-17 | Full-surface search with scopes All / Conversations / Files / Projects, per-group caps, 200ms debounce, "Results limited to the first matches" | Find an old Session by what was said, not its title | `M SearchView.tsx:53-58,211-279`; `M appSearch.ts:92-123` | pocketd: new transcript text index; `pocket`: new surface; app: search tab | L | Transcript access in pocketd |
| 12-18 | Interface scale 0.5–2, step 0.1, ⌘+ / ⌘− / ⌘0 | Readability on large or small displays | `M uiScale.ts`; `M settings.ts:938-1036` | gpui `set_rem_size` works only for `rems()`; Pocket uses `px()` everywhere, so it needs a unit sweep | L | Unit sweep; low demand |

## UI/UX spec to copy

**Layout grid (px)**

| Element | MonoCode | Pocket now |
|---|---|---|
| Window default / min | 1280x800 / 800x520 | 1440x900 / none (`P main.rs:1059`) |
| Traffic lights | 14 btn, x 12, gap 6, y 13 in a 40 bar; reserve 78 | (14,14); rail 56 |
| Header / tab strip | 40 | 42 / 40 |
| Sub-toolbar (tabs, search, split header, file tabs) | 36 | 44 search |
| List row / rail item / palette row | 32 | Session row about 70; palette 40 |
| Input | 28 | – |
| Tab button | 30 high; slot 224 → 112; two-line at ≥176 container | 40 strip |
| Footer | 28 | none |
| Compact rail | 48; buttons 32; icons 16 | 56 |
| Project rail | 200 (180–360) | 272 default (200–420) |
| Sessions column | 260 (260–560, ≤50% win) | 334 default (280–600) |
| Resize hit | 6 (visible 1) | 5 |
| Sash hit | 12 (visible 1) | none |
| Popover gap / edge padding | 6 / 8 | 8 edge (`P ui.rs:37`) |
| Modal widths | 420 @22% top, 560 @10% | `modal(width, top)` |
| Palette | 560 @12% top, list ≤ min(380, 50vh) | overlay |
| Settings page | max 1024, 20/24 → 32/32 padding at ≥560 | – |

**Colour tokens** (computed hex over flat base; dark / light)

| Token | Dark | Light |
|---|---|---|
| background-base | #171717 | #f7f7f7 |
| content (ink) | #ebebeb | #2e2e2e |
| sidebar-glass (opaque) | base 90% + black | #f7f7f7 |
| ink 3% (settings group fill) | #1d1d1d | #f1f1f1 |
| ink 5% (hover, group box) | #222222 | #ededed |
| stroke = ink 7% | #262626 | #e9e9e9 |
| ink 10% (popover border, hover-strong) | #2c2c2c | #e3e3e3 |
| selection-subtle | #282828 (8%) | #ededed (5%) |
| selection | #2c2c2c (10%) | #ebebeb (6%) |
| selection-strong | #303030 (12%) | #e9e9e9 (7%) |
| selection-hover | #373737 (15%) | #e3e3e3 (10%) |
| selection-emphasis | #414141 (20%) | #dbdbdb (14%) |
| ink 25% (empty art) | #4c4c4c | #c5c5c5 |
| ink 35% (grip, faint icons) | #616161 | #b1b1b1 |
| ink 40% (placeholder, kbd) | #6c6c6c | #a7a7a7 |
| ink 45% (secondary text) | #767676 | #9d9d9d |
| ink 50% (inactive tab, icon) | #818181 | #929292 |
| ink 55% (footer) | #8c8c8c | #888888 |
| accent | hsl(211 92% 62%) = #459bf7 | same |
| link | #7dd3fc | hsl(211 92% 40%) = #0863c4 |
| needs approval | amber-400 | amber-400 |
| done / add | emerald-400 | emerald-400 |
| delete / error | red-400 | red-400 |
| done unseen (tab) | teal-400 | teal-400 |
| running terminal bars | #e39b4a | #e39b4a |
| reminders group | #f59e0b | #f59e0b |
| modal backdrop | black 40% | black 40% |

**Type scale** (system-ui; px / weight / ink)
- 20 / 600: settings h1. 20 / 500: modal title.
- 18 / 400: empty-Session headline.
- 14 / 500: rail and nav labels, project names. 14 / 400: palette rows.
- 13 / 600: Session title, folder name, settings group title.
- 13 / 500: settings row label. 13 / 400: inputs, search rows, breadcrumbs, empty copy at 45%.
- 12: segmented, settings descriptions (45%), select, popover rows, split title.
- 11: model line, branch line, counts, status words, footer, shortcut hints, keybinding header (600, uppercase, tracking .08em).
- 10: two-line tab text (500 / 45%), kbd chips (mono), badges (600).
- Label leading 1.4; descriptions use relaxed leading (1.625).
- Mono: ui-monospace for paths (11px, 40%), kbd, and the When column.

**Radii**
- 4: kbd, footer chips.
- 5: segmented item, drag preview.
- 6: rows, buttons, tabs, Session rows, inputs.
- 8: palette frame, select options.
- 12: popover, settings group.
- 16: modal, empty tile.
- Full: toggle, dots, badges.

**Elevation** (dark relies on borders; shadows are Tailwind defaults)
- Popover: border ink 10% + shadow-xl `0 20px 25px -5px /.1, 0 8px 10px -6px /.1` + 24px backdrop blur.
- Modal: border ink 7% + shadow-2xl `0 25px 50px -12px /.25`.
- Rail search: shadow-sm.
- Drag preview: `0 4px 12px /.18, 0 1px 2px /.2`.

**Motion**

| Use | Duration | Easing | From |
|---|---|---|---|
| Hover / selected fill | 120ms | cubic-bezier(0.22,1,0.36,1) | – |
| Reorder (FLIP) | 160ms | cubic-bezier(0.22,1,0.36,1) | – |
| Tab open / close width | 200ms | cubic-bezier(0.3333,0.6667,0.6667,1) | width 0 |
| Drawer open / close | 200 / 160ms | (0.22,1,0.36,1) / (0.4,0,1,1) | width 0 |
| Popover in | 170ms | cubic-bezier(0.16,1,0.3,1) | opacity 0, scale .94, 8px toward anchor |
| Modal panel / backdrop | 200 / 160ms | (0.16,1,0.3,1) / ease-out | translateY 8, scale .98 |
| Toast in | 180ms | (0.22,1,0.36,1) | translateY −8, scale .98 |
| Tailwind `transition-*` | 150ms | cubic-bezier(0.4,0,0.2,1) (`TW:492-493`) | – |
| Spinner | 80ms/frame × 10 | steps | – |
| Shimmer | 2s (busy project 1.4s) | linear infinite | – |
| Press | instant | – | scale .97 |

**States**
- Row hover: ink 5%. Active row: selection. Selected project: selection-strong. Unselected project: opacity .65.
- Multi-selected: accent 15%. Drop target: accent 20%. Dragging: opacity .4.
- Needs approval: ink 20% + dashed ink-30% border. Draft: dashed ink-25% border.
- Focus ring: `outline-2 outline-accent` / `ring-2 ring-accent`. Rename input: ink 10% + `ring-1 accent/40`.
- Disabled: opacity .4–.5.

**Shortcuts**
- See report 09 for the full list. Keep these in Pocket as-is: ⌘D / ⇧⌘D split, ⌘⌥arrows focus, ⌘W close split, ⌘1–8 / ⌘9 tabs, ⇧⌘[ / ⇧⌘] tab cycling, ⌘[ / ⌘] visit history, F2 rename, Shift+F10 menu, middle-click close.

**Copy** (verbatim)
- Tabs: "New session", "N sessions", "Close Tab", "Close Other Tabs", "Close Tabs to the Right", "Close Tabs to the Left", "All N conversations in this tab", "Permanently delete all N conversations in this tab".
- Status: "Need approval", "Needs input", "Working...", "Done", "Draft".
- Empty: "Sessions you start will show up here", "No matching sessions", "No sessions match these filters", "Find files, conversations, messages, and projects.", "No results", "Results limited to the first matches", "What should we work on in <project>?".
- Palette: "Go to File (type > for commands)", "Indexing files…", "Type a file name to search", "No matching files", "No matching commands".
- Settings: "Search settings", "Restore defaults", "Back", "Click a shortcut to record new keys. Press Delete while recording to disable it.", "Record…", "Disabled".

## Open questions / risks

- **Q1. Dark-mode cost in Pocket.** Every colour is a `pub const u32` used directly (`P theme.rs:9-62`). A runtime token global touches most render code. Should Pocket do a single sweep, or ship dark first on new surfaces only? gpui-component widgets (Input, Textarea) also need `ThemeMode::Dark` colours to match.
- **Q2. Glass is dark-only in MonoCode.** It uses private CGS APIs and alpha-0.01 backing hacks (`M macos.rs:1-21`). gpui's `Blurred` is documented as "Not always supported" (`R gpui-pre-0.3.6/src/platform.rs:2463`). Test on macOS 26 before committing; the fallback is an opaque tint.
- **Q3. Tabs group Sessions in MonoCode, while Pocket tabs hold Terminals per Worktree.** The two-line tab (12-4) needs a rule for the meta line when a tab holds a Login shell rather than an agent Session.
- **Q4. The split tree (12-6) changes the persisted layout format.** It needs a migration from `Vec<Vec<String>>`. PTY resize should fire on drag end, not per frame, to avoid pocketd `resize` storms. Pocket only dedupes identical (cols, rows) today (`P packages/desktop/crates/pocket/src/main.rs:722-724`).
- **Q5. The palette is thinner than the docs suggest** (one command, F10). Copy the `>` mechanism and pointer guard, not a command inventory. Pocket's ⌘K palette already mixes Sessions, files and actions, which is ahead of MonoCode here.
- **Q6. Where settings live.** MonoCode keeps everything in per-machine localStorage. Pocket needs to decide which prefs pocketd owns (so the phone agrees, e.g. folders and notification mutes) and which stay desktop-local (theme, widths, keys).
- **Q7. Shortcut conflicts** (⌘J, ⌘., ⌘,) need a product call before 12-10 / 12-13 re-map ⌘, to app settings.
- **Q8. Interface scale (12-18)** is effectively blocked by Pocket's px-only sizing. It is low value compared with the macOS system text size.
- **Q9. Pace risk.** MonoCode changes quickly (report 09: about 1.5 releases per day). Measurements are pinned to cdc1441 and may drift; copy values, not their component structure.

## Verification

Date: 2026-09-30. Claims checked: 18. Corrected: 9.

Confirmed at source: clone HEAD cdc1441; colour inputs and all five selection strengths (dark and light); computed hexes #171717 / #ebebeb / #f7f7f7 / #2e2e2e / #459bf7; motion tokens and drawer 200/160ms; popover 170ms and modal 200ms easings; traffic-light constants and 78px spacer; window 1280x800 / 800x520; column widths 200 (180–360) and 260 (260–560, ≤50%); compact rail `w-12`; `LayoutNode`, `MIN_SIZE 0.08`, `paneEdgeFromPoint`, sash hit area; Session status words and frame precedence; tab slot `w-56 min-w-28` and the 11rem two-line switch; FilePicker `ACTIONS` = one "Reload MonoCode" and `>` preload on ⇧⌘P; pointer guard; search group limits; LAYER z values; modal sizes; 10 settings sections in 3 groups; glass enabled only in dark; keybindings ⌘J = Terminal dock, ⌘. = Switch Model; Pocket theme constants, `Tab::Term(Vec<Vec<String>>)`, palette sections Sessions / Files / Actions, overlay entrance timings, window 1440x900, traffic lights (14,14), ⌘J / ⌘. / ⌘, bindings, gpui `Blurred` "Not always supported".

Corrections:
- Pocket bundles 40 SVG icons, not 41 (`P theme.rs:143-147`).
- Pocket Sessions column default is 334px (348px only for the Inbox list); Projects default is 272px (`P view.rs:248,584,616`). Updated F16 and the layout grid.
- 12-6: removed "double-click equalize". PaneTree's sash has no double-click handler; double-click reset exists only on the sidebar resize handle.
- 12-15: "body opaque" was wrong. Main pane glass defaults on (`M appearance.ts:136`, `BODY_GLASS_DEFAULT = true`).
- F9 / F11: ⌘D split right is also `!editorFocus`, not only ⇧⌘D (`M settings.ts:1022-1027`).
- 12-1 prerequisite: "about 6k lines of `rgba(CONST)`" replaced with the measured 237 `rgba(` call sites in about 7k lines of `pocket` + `ui`.
- Pocket radii citation fixed: 26 / 8 / 10 at `P ui.rs:33,439,739`; 14 is at `P view.rs:377`, not in `ui.rs`.
- Citation fixes: settings Page padding at `M SettingsView.tsx:534`; tab context menu width at `M TitleBar.tsx:1070`; macOS constants at `M macos.rs:45-55`.
- Q4: noted that Pocket already dedupes identical (cols, rows) resizes (`P main.rs:722-724`), but has no drag-end gating.
