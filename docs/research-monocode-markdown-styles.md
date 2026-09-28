# Research: monocode markdown styles, and mapping them to gpui-kit TextView

Date: 2026-09-28.
- monocode: `hardbeat920/monocode` at `c576783ac14a50a0998222146fd9838d85d52ffb`.
- Streamdown: `vercel/streamdown` tag `streamdown@2.5.0` = `15ba1aee86ab053ada83326f563e78b4d2c7f4f7`. @shikijs/themes 3.23.0.
- Our stack: gpui-kit 0.6.6.

## TL;DR

- **Repo** is https://github.com/hardbeat920/monocode ("A desktop UI for your coding agents"; Tauri 2 + React 19 + Tailwind v4). Confidence is high (§0).
- **Pipeline:** Streamdown 2.5 (react-markdown, remark-gfm, Shiki, lazy mermaid), plus monocode's own `code`/`a`/`img` renderers.
  - Streamdown's shadcn classes (`bg-muted`, `border-border`, `text-primary`, `bg-sidebar`, …) are never generated, because monocode defines none of those tokens.
  - So **the whole look comes from monocode CSS** (§1).
- **Palette:** one neutral `content` colour alpha-mixed over the page bg: 78% body, 65% labels, 45% icons, 35% line numbers, 20% quote bar, 10% borders, 6–8% fills. Dark is the default; light is a variant (§2).
- **Type:** 14/24 system-ui body at 78%, with bold/italic at 100%. Headings are weight 600 at h1 22 / h2 18 / h3 20 (h3 > h2 is a monocode quirk) (§3, §4).
- **Code blocks and tables share one card:** 10px radius, 6% fill, 1px 10% border.
  - Code blocks add a 36px header (file icon + lowercase mono 12/500 lang or path), a 1px divider, a 24px copy button with a morph animation, 12px code, 10px line numbers at 35%, and Shiki github-light/dark (§6).
  - Tables have 12px cells, 8/10 padding, 5% row rules and no column rules (§7).
- **Inline code** is a 6px-radius 8% pill at 0.8em. Links are sky-400/90 with no underline until hover. There are no anchors, TOC, callouts or math. Frontmatter is a collapsible "Properties" card (§5, §8).
- **Mapping:** component `TextViewStyle` can express heading sizes, paragraph gap, the code/table card, cell padding and the inline-code fill.
  - **Colours are not per-view:** they come from the global `Theme`. Use `gpui_kit::base` TextView for per-view colours.
  - **Gaps:**
    - code header and line numbers
    - inline pill box
    - link underline/hover
    - body 78% vs bold 100%
    - heading margins and weights
    - quote italic and 4px bar
    - 1px hr
    - table column rules

  The code header, line numbers and inline pill are feasible with `MarkdownPlugin`. The rest need gpui-kit patches (§11).

### Citation legend

- `M path:L` = monocode at SHA above.
- `S path:L` = streamdown repo at tag above.
- `C` = monocode's compiled CSS from `npx vite build` at the SHA (`dist/assets/index-DMoq2laI.css`, not in repo). These are Tailwind v4 defaults.
- `K` = `@shikijs/themes` 3.23.0 `dist/github-{light,dark}.mjs`, as installed by monocode's lockfile.
- `P path:L` = our repo path relative to `packages/desktop/`.
- `R crate/path:L` = `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/...`.
- Hex values marked ≈ are the colour-mix composited over the page bg at default settings.

## 0. Repo identification

- `hardbeat920/monocode`: about 1650 stars, a desktop GUI for Claude Code/Codex-style agents. It renders agent replies and `.md` files, which matches "copy design into our coding app". README screenshot: `M README.md:12`.
- Other GitHub "monocode" candidates had 0 to a few stars and nothing relevant: `tobi-techy/monocode`, `kaceper11/monocode`, `LandonDev/monocode-releases` (binaries only), `matterconi/monocode`, `aaronmbos/monocode`, `acary/monocode`. `Scientific-Computing-Lab/MonoCoder` is an ML model.

## 1. Pipeline

- **Renderer:** `<Streamdown>` from `streamdown` 2.5.0 with `plugins={ code, mermaid }` (`M src/features/sessions/ui/AgentMarkdown.tsx:1,19-26,63`).
  - `controls={false}` hides streamdown's copy/download buttons (:559).
  - `dir="auto"` (:560) wraps every block in `<div dir style="display:contents">` (`S packages/streamdown/index.tsx:364`).
- **remark:** Streamdown defaults `remark-gfm` + `remarkCodeMeta` (`S index.tsx:250-268`), plus the custom `remarkWorkspaceFileLinks`.
- **rehype:** `raw` → `sanitize` → `harden` with `allowedImagePrefixes ["*"]`, `allowDataImages`, `imageBlockPolicy: "remove"` (`M AgentMarkdown.tsx:65-79`). While streaming, `rehypeWordFade` is added (:96-105).
- **Highlighting:** `@streamdown/code` runs Shiki `codeToTokens` with themes `["github-light","github-dark"]` and the JS regex engine (`S packages/streamdown-code/index.ts:156-159,226-231,259`). Token colours come from `--sdm-c`, or `--shiki-dark` under `.dark` (`S packages/streamdown/lib/code-block/body.tsx:158-163`).
- **Mermaid:** 11.16, lazy. `securityLevel: "strict"`; theme is `default` in light and `dark` in dark (`M AgentMarkdown.tsx:50-61,691`).
- **Dark mode:** `@custom-variant dark (&:where(html:not(.theme-light), html:not(.theme-light) *))` (`M src/styles/index.css:6`). Default preference is `"dark"` (`M src/features/settings/model/appearance.ts:78`).
- **Class generation:** Tailwind scans streamdown dist (`M index.css:1-4`). None of its shadcn-token utilities are generated, because the theme vars don't exist.
  - Effective streamdown classes are layout/size only: `my-*`, `mt-6 mb-2`, `font-semibold`, `text-*`, `px/py`, `border`, `rounded-*`.
  - The `pre` bg classes `bg-[var(--sdm-bg,inherit]` are malformed, so no Shiki bg is painted (`S body.tsx:97-98`).
- **Line heights:** `--tw-leading` is declared `@property … inherits: false` (C line 9846). Elements carrying their own `text-*` class therefore use Tailwind's default line-height, not the root `leading-6`.

## 2. Palette

Tokens are defined at `M index.css:25-72` (`@theme`), `:74-97` (`:root`, dark) and `:103-116` (`html.theme-light`). Hue and saturation are user-configurable (defaults 240 / 0%, so neutral grey), as is dark bg lightness (0–30%, default 9%) (`M appearance.ts:112-122,275-297`).

| Token | Dark | Light |
|---|---|---|
| bg `--color-background-base` | `hsl(240 0% 9%)` #171717 | `hsl(240 0% 97%)` #f7f7f7 |
| `--color-content` | `hsl(240 0% 92%)` #ebebeb | `hsl(240 0% 18%)` #2e2e2e |
| content 78% (p, li) | ≈#bcbcbc | ≈#5a5a5a |
| content 70% (frontmatter body) | ≈#ababab | ≈#6a6a6a |
| content 65% (code header, path) | ≈#a1a1a1 | ≈#747474 |
| content 60% (frontmatter summary) | ≈#969696 | ≈#7e7e7e |
| content 45% (copy icon, toggle off) | ≈#767676 | ≈#9d9d9d |
| content 35% (line numbers) | ≈#616161 | ≈#b1b1b1 |
| content 20% (quote bar) | ≈#414141 | ≈#cfcfcf |
| content 10% (card border, hr, code divider) | ≈#2c2c2c | ≈#e3e3e3 |
| content 8% (inline code) | ≈#282828 | ≈#e7e7e7 |
| `--color-stroke` content 7% (`:35`) | ≈#262626 | ≈#e9e9e9 |
| content 6% (code/table/mermaid fill) | ≈#242424 | ≈#ebebeb |
| content 5% (table row rules) | ≈#222222 | ≈#ededed |
| content 3% (frontmatter fill) | ≈#1d1d1d | ≈#f1f1f1 |
| `--color-selection-strong` (toggle on) | content 12% | content 7% |
| markdown link | `sky-400/90` = `oklch(74.6% .16 232.661 / .9)` ≈#00bcfe e6 | same, no variant |
| markdown link hover | `sky-300` = `oklch(82.8% .111 230.318)` ≈#74d4ff | same |
| `--link-color` (code-path hover only) | #7dd3fc | `hsl(211 92% 40%)` |
| `--color-markdown-heading` (source view) | #f9a8c9 | #be185d |

Base text colour is `content` on `body` (`M index.css:118-127`).

## 3. Typography and spacing

- **Root:** `agent-markdown min-w-0 font-sans text-sm leading-6`, i.e. 14px/24px (`M AgentMarkdown.tsx:557`).
  - `--font-sans: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", …`
  - `--font-mono: ui-monospace, SFMono-Regular, Menlo, …` (`M index.css:66-71`). On macOS these are SF Pro and SF Mono.
- **Colour:** `p, li` are content 78% (`M index.css:1408-1411`). `strong`/`em` inside `p`/`li` are content 100% `!important` (:1438-1450). `strong` weight is 600 (`S lib/components.tsx:249`).
- **Block gaps:** streamdown's root `space-y-4` targets the `display:contents` wrappers, so it paints nothing (`S index.tsx:804`). monocode re-adds some gaps (`M index.css:1413-1436`).
- **Effective gaps.** Margins collapse across the contents wrappers.

  | Transition | Gap | Source |
  |---|---|---|
  | p → p | 16 | `M :1423` |
  | → list | 8 | `M :1433-1436` |
  | → heading | 24 | heading `mt-6` |
  | heading → next | 16 (max of 8 and 16) | |
  | → code | 16 | `M :1128-1134` |
  | → table | 16 | `S lib/table/index.tsx:32` `my-4` |
  | → blockquote | 16 | `S components.tsx:594` |
  | → hr | 24 | `S :233` |
  | quote p → p | 16 | `M :1429-1431` |

- **First heading:** it keeps its 24px top margin, because `[&>*:first-child]:mt-0` hits the wrapper.

## 4. Headings

All headings use `mt-6 mb-2 font-semibold` (24 / 8, weight 600) and `content` 100%. There are no anchors or ids.

| Level | Size | Line-height (Tailwind default) | Source |
|---|---|---|---|
| h1 | 22px | 1.2 → 26.4px | `S components.tsx:371` `text-3xl`, overridden at `M index.css:1283-1285` |
| h2 | 18px | 1.333 → 24px | `S :388` `text-2xl`, overridden at `M :1287-1289` |
| h3 | 20px | 28px | `S :405` `text-xl` |
| h4 | 18px | 28px | `S :422` `text-lg` |
| h5 | 16px | 24px | `S :439` `text-base` |
| h6 | 14px | 20px | `S :456` `text-sm` |

The quirk is that monocode only shrinks h1/h2, so **h3 renders larger than h2**. When copying, either keep it or use a monotonic scale such as 22/18/16/15/14/14.

## 5. Inline code, links, emphasis

- **Inline code** (`M AgentMarkdown.tsx:289`): `inline-flex items-center gap-1 rounded-md bg-content/8 px-1.5 min-h-6 max-w-full [overflow-wrap:anywhere] align-baseline font-mono text-[0.8em] text-content`.
  - That is a pill: radius 6, horizontal padding 6, min-height 24, font 0.8em (11.2px in body), fill 8%, text 100%, mono.
  - **File-like text** gets a 14px `FileTypeIcon`. It becomes clickable, with hover `sky-300` + underline (:290, :311-315).
- **Links** (`M :213-261`, class at :234): `text-sky-400/90 hover:text-sky-300 hover:underline`. There is no underline at rest and no light-mode variant.
  - Streamdown's default `font-medium text-primary underline` is replaced (`S components.tsx:341`), so links use the inherited weight (400).
- **Bold** is 600 at 100%. **Italic** is at 100%. Strikethrough is the default GFM `del` with inherited colour.

## 6. Fenced code

DOM (`M AgentMarkdown.tsx:321-355`). Rules are in `M index.css`.

```
div.markdown-code-shell                 relative; margin-block 16                       :1128-1134
  FileTypeIcon 16px                     abs top 10 left 10, h 16                        :1229-1237
  MarkdownCodePath?                     abs top 10 left 34 right 12 (42 w/ copy);       :1239-1259
                                        mono 12/500, 65%, ellipsis; hover --link-color + underline :1271-1274
  CodeCopyButton                        abs top 6 right 8                               :1145-1227
  [data-streamdown=code-block]          card: border 1px 10%, radius 10, bg 6%,         :1109-1122
                                        gap 0, padding 0, overflow hidden
    [code-block-header]                 min-h 36, pad 8/10 (left 34), 65%               :1309-1324
      span                              lang: mono lowercase 12/500 (S lib/code-block/header.tsx:11,15)
                                        hidden when a path is shown                     :1276-1281
    [code-block-body]                   border-top 1px 10%, pad 10 8 10 0, 12px,        :1343-1362
                                        bg transparent, overflow-x auto
      pre > code > span.line            white-space pre, width max-content              :1364-1376
        ::before (line number)          w 24, text-right, mr 8, 10px mono, 35%          :1378-1383 + S body.tsx:15-27
```

- **Code metrics:** 12px mono. Line-height is ≈17.1px (12 × 1.4286 from `text-sm`'s non-inherited leading, C).
- **Header** is always visible, because the copy button always exists (`:1326-1334`). With no language the label is empty.
- **Fence meta:**
  - `startLine:endLine:path` or a bare path fills the header path (`M AgentMarkdown.tsx:771-807`).
  - `startLine=N` offsets the line numbers (:781).
  - `noLineNumbers` disables them (:331).
  - The icon comes from the path, or from `code.<ext>` via `LANGUAGE_FILE_NAMES` (:175-209).
- **Line numbers** are on by default. The body's left padding is 0, so the 24px number column sits flush against the card edge.
- **Copy button:**
  - 24×24, radius 6, transparent bg, 45% colour.
  - Hover/focus: bg 10%, colour 100%.
  - Custom SVG at 15px, stroke 1.6. It morphs pages → check over 260ms `cubic-bezier(0.22,1,0.36,1)`, and the copied state lasts 1500ms (`M index.css:1145-1227`; `M AgentMarkdown.tsx:357-397,379`).
- **Syntax colours (K).** The Shiki bg is not painted; the card's 6% fill shows instead.

| Scope | github-light | github-dark |
|---|---|---|
| fg | #24292e | #e1e4e8 |
| keyword, storage | #d73a49 | #f97583 |
| entity / function name | #6f42c1 | #b392f0 |
| string | #032f62 | #9ecbff |
| constant | #005cc5 | #79b8ff |
| variable | #e36209 | #ffab70 |
| tag | #22863a | #85e89d |
| comment | #6a737d | #6a737d |

## 7. Blocks

- **Blockquote:**
  - `border-inline-start: 4px` content 20%, `padding-inline-start: 1rem` (`M index.css:859-865`).
  - Italic with `my-4` (`S components.tsx:594`; its muted colour is not generated).
  - Text is the inner `p` at 78%.
- **Lists:**
  - Markers sit outside (disc / decimal), with a 24px indent, nested 24px too (`M index.css:867-878`). This overrides streamdown's `list-inside` (`S :173,:213`).
  - `li` has `py-1` (4px) and `[&>p]:inline` (`S :194`). Colour is 78%, and markers inherit it.
- **Task lists:** remark-gfm's native `<input type=checkbox disabled>`, with no styling.
- **Tables:**
  - Wrapper: the same card as code (border 1px 10%, radius 10, bg 6%, padding 0, gap 0, overflow hidden), `my-4` (`M index.css:1109-1126`; `S lib/table/index.tsx:32`).
  - Inner scroller: border 0, bg transparent (`M :1385-1389`; `S table/index.tsx:52`).
  - Row rules are 1px content 5%, both head/body and between rows (`M :1391-1394`; `S table/index.tsx:56`, `S components.tsx:518`).
  - Cells: padding 8/10, 12px (`M :1396-1400`).
  - `th` is 600, left-aligned, nowrap (`S :555`). `td` has no extra style (`S :575`).
  - Cell line-height is ≈17.1px (`text-sm`). There are no column rules and no header fill (`bg-muted/80` is not generated, `S :500`).
  - `td` text is content 100%, because only `p`/`li` get 78%.
- **hr:** 1px content 10% (`M index.css:1402-1406`), margin 24 (`S :233`).
- **Images:** only `data:image/`, note assets and inbox media render. Remote images return `null` (`M AgentMarkdown.tsx:437-453`). No styling beyond Tailwind preflight (block, `max-width:100%`, `height:auto`).
- **Footnotes:** default GFM output. Streamdown only drops empty ones (`S components.tsx:645-776`), and there is no CSS.
- **Callouts/GitHub alerts, math:** not supported (no plugin at `M AgentMarkdown.tsx:63`; none in streamdown).
- **Mermaid:**
  - Card: `rounded-[10px] border border-content/10 bg-content/6 p-3 overflow-x-auto` (`M AgentMarkdown.tsx:731`).
  - Loading placeholder is `h-32 animate-pulse` in the same card (:725).
  - The SVG is centred with `max-width:100%` (`M index.css:1466-1471`).
  - While incomplete or on error it falls back to a code block (`M AgentMarkdown.tsx:706-721`).
- **Streaming:** words fade in over 320ms ease-out (`M index.css:2044-2052`).

## 8. Frontmatter, TOC, anchors, preview shell

- **Frontmatter:** the leading `---` block is split off without YAML parsing (`M src/shared/lib/markdownFrontmatter.ts:7-20`). It renders as a `<details>` that is closed by default (`M src/features/sessions/ui/MarkdownDocumentPreview.tsx:27-41`):
  - **Card:** `mb-6`, radius 8, 1px content 10% border, bg 3%.
  - **Summary:** 12px, 60% (100% on hover), padding 8/12. A chevron, 14px at 50%, flips right/down.
  - **Body:** `pre` mono 12/20 at 70%, padding 8/12, `pre-wrap`, with a top border in `stroke` (7%).
  - **Label:** "Properties" (`M src/features/files/ui/FileEditor.tsx:467`), or "Skill metadata" (`M src/features/skills/ui/SkillDocumentPreview.tsx:5`).
- **TOC and heading anchors:** none (no slug/autolink plugin in `M AgentMarkdown.tsx:63-79`).
- **Preview shell:** a full-height scroller with an inner `px-6 py-8` (24 / 32), no max-width, on the page bg (`M AgentMarkdown.tsx:590-624,611-613`).
- **Source view:** `pre` mono 13/20 at 85%, `px-4 py-3`. ATX heading lines are tinted with `--color-markdown-heading` (`M AgentMarkdown.tsx:626-665,641`; `M index.css:1041-1043`).
- **Preview/Source toggle:** absolute at top-right (`M index.css:836-838`).
  - Container: radius 6, 1px 10% border, bg 10%, padding 2, `backdrop-blur-md`.
  - Tabs: 11px, padding 8/2, radius 4. On: `selection-strong` bg + 100% text. Off: 45% text, 80% on hover (`M src/features/sessions/ui/MarkdownModeToggle.tsx:37,67-71`).

## 9. Screenshots

- README hero: https://github.com/user-attachments/assets/2cd4a6ec-eb1e-4b45-8627-a76442ea3874 (`M README.md:12`). The repo also has `M docs/screenshot.jpg`.
- It shows a dark transcript: grey 78% body, white bold, and inline paths on faint pills. There is a diff code card with a file icon + path header and line numbers.

## 10. Token table (copy-ready)

"c N%" = `color-mix(content N%, transparent)`. Hex values are in §2.

| Element | Property | Value | Source |
|---|---|---|---|
| page | bg / text | bg / content | `M index.css:118-127` |
| body | font | system-ui 14px / 24px, weight 400 | `M AgentMarkdown.tsx:557`, `index.css:69-71` |
| p, li | color | c 78% | `M index.css:1408-1411` |
| strong / em | color / weight | c 100% / 600 | `M index.css:1438-1450`, `S components.tsx:249` |
| paragraph | gap before | 16px | `M index.css:1423` |
| list | gap before | 8px | `M index.css:1433-1436` |
| h1–h6 | size | 22 / 18 / 20 / 18 / 16 / 14 px | `M index.css:1283-1289`, `S components.tsx:371-456` |
| h1–h6 | line-height | 26.4 / 24 / 28 / 28 / 24 / 20 px | C |
| h1–h6 | weight / color / margin | 600 / c 100% / 24 top, 8 bottom | `S components.tsx:371-456` |
| inline code | box | radius 6, px 6, min-h 24, gap 4 | `M AgentMarkdown.tsx:289` |
| inline code | bg / text | c 8% / c 100%, mono 0.8em | `M AgentMarkdown.tsx:289` |
| inline code (file) | icon / hover | 14px file icon / sky-300 + underline | `M AgentMarkdown.tsx:290,311-315` |
| link | color / hover / underline | sky-400 @90% / sky-300 / hover only | `M AgentMarkdown.tsx:234` |
| code card | border / radius / bg | 1px c 10% / 10px / c 6% | `M index.css:1109-1122` |
| code card | margin | 16px block | `M index.css:1128-1134` |
| code header | height / padding | min 36 / 8 10, left 34 | `M index.css:1309-1324` |
| code header | label | mono 12px 500 lowercase, c 65% | `M index.css:1313-1320`, `S header.tsx:15` |
| code header | icon | 16px @ top 10 left 10 | `M index.css:1229-1237` |
| code path | text | mono 12px 500, c 65%, ellipsis; hover `--link-color` | `M index.css:1239-1274` |
| code body | divider / padding | top 1px c 10% / 10 8 10 0 | `M index.css:1343-1356` |
| code body | font | mono 12px / ≈17.1px | `M index.css:1350`, C |
| line number | box / text | w 24 right-aligned, mr 8 / 10px mono c 35% | `M index.css:1378-1383`, `S body.tsx:15-27` |
| copy button | box | 24×24 @ top 6 right 8, radius 6 | `M index.css:1145-1160` |
| copy button | color | c 45%; hover bg c 10%, c 100% | `M index.css:1161-1175` |
| copy button | icon / motion | 15px stroke 1.6 / 260ms cubic-bezier(.22,1,.36,1), 1500ms copied | `M index.css:1177-1227`, `AgentMarkdown.tsx:379` |
| syntax | theme | Shiki github-light / github-dark (§6) | `S streamdown-code/index.ts:156-159`, K |
| blockquote | bar | 4px c 20% | `M index.css:859-865` |
| blockquote | padding / style / margin | start 16 / italic / 16 block | `M index.css:861`, `S components.tsx:594` |
| list | indent / markers | 24px (nested 24) / outside disc, decimal | `M index.css:867-878` |
| li | padding | 4px block | `S components.tsx:194` |
| table card | border / radius / bg / margin | 1px c 10% / 10px / c 6% / 16 | `M index.css:1109-1126`, `S table/index.tsx:32` |
| table rows | rule | 1px c 5%, no column rules | `M index.css:1391-1394` |
| th, td | padding / size | 8 10 / 12px (≈17.1 lh) | `M index.css:1396-1400` |
| th | weight / align / wrap | 600 / left / nowrap, no fill | `S components.tsx:555` |
| td | color | c 100% | inherits body |
| hr | rule / margin | 1px c 10% / 24 block | `M index.css:1402-1406`, `S components.tsx:233` |
| mermaid | card | radius 10, 1px c 10%, bg c 6%, p 12 | `M AgentMarkdown.tsx:731` |
| frontmatter | card | radius 8, 1px c 10%, bg c 3%, mb 24 | `M MarkdownDocumentPreview.tsx:27` |
| frontmatter | summary / body | 12px c 60%, pad 8 12 / mono 12/20 c 70%, top rule c 7% | `M MarkdownDocumentPreview.tsx:28-41` |
| preview shell | padding / width | 24 / 32, no max-width | `M AgentMarkdown.tsx:613` |

## 11. Mapping to gpui-kit 0.6.6

### 11.1 Today

- **Preview:** `TextView::new(&self.md).plugin(Mermaid(..))` inside `pane().px(40).py(20).bg(SURFACE)` (`P crates/pocket/src/explore.rs:322-340`). The style sets:
  - `highlight_theme`
  - `inline_code` bg `FILL_2`
  - `code_block` bg `SURFACE_SUNKEN`, radius 8, p 12
- **Theme:** light only.
  - Geist / Geist Mono at 14 / 13 (`P crates/theme/src/theme.rs:6-7,169-176`).
  - `foreground` TEXT #111113, `muted_foreground` TEXT_2, `link` ACCENT #5b5bd6 (:171-177).
  - Syntax is recoloured from `default_light` (:78-96).
- **What the component `TextViewStyle` exposes** (`R gpui-component-0.6.6/src/text/style.rs:13-69`): `paragraph_gap`, `heading_base_font_size`, `heading_font_size`, `highlight_theme`, `code_block`, `table`, `table_head`, `table_cell`, `inline_code`, `is_dark`.
- **Colours come from the global `Theme`:**
  - `foreground`, `muted_foreground`, `link`, `selection`
  - `muted` → code/table-head bg
  - `border`
  - `accent` → inline-code bg

  They cannot be set per view (`R gpui-component-0.6.6/src/text/mod.rs:34-62`, `compat.rs:293-329`).
- **Per-view colours:** use `gpui_kit::base::text::TextView` with the base `TextViewStyle::with_foreground/with_muted_foreground/with_link/with_code_background/with_border` (`R gpui-base-0.6.6/src/text/style.rs:103-213`). It has the same `plugin`, `code_block_actions` and `markdown_extensions` API (`R gpui-base-0.6.6/src/text/text_view.rs:291-398`).
  - Unverified: whether the base view picks up the component-installed highlighter via `TextViewDefaults` (`R gpui-component-0.6.6/src/text/mod.rs:64-66`).

### 11.2 Mapping

ok = expressible today; part = close but not exact; gap = needs a plugin or a gpui-kit patch.

| monocode | gpui-kit knob | Status | Notes |
|---|---|---|---|
| 14px body, system-ui | `Theme.font_family/font_size` (`P theme.rs:169-170`) | ok | `.SystemUIFont` / "SF Mono" (`/System/Library/Fonts/SFNSMono.ttf`) if copying fonts too; otherwise keep Geist |
| 24px line-height | `.line_height(px(24.))` on the TextView (it is `Styled`) | part | Cascade unverified |
| p 78% vs bold 100% | one `foreground`; bold = `FontWeight::BOLD` (700), same colour (`R gpui-base-0.6.6/src/text/node.rs:1663-1690`) | gap | Choose foreground ≈ TEXT_BODY (body look) or TEXT (heading look) |
| paragraph gap 16 | `paragraph_gap: rems(1.)` (default) | ok | Applied after every block (`R node.rs:2865-2869`) |
| list gap 8, li 4px, 24 indent | fixed layout (`R node.rs:2372-2425,2936-2967`; `utils.rs:12`) | gap | |
| heading sizes | `heading_font_size: Some(Arc::new(\|lvl, _\| ..))` | ok | Overrides the rem table (`R node.rs:2891-2904`) |
| heading weight 600 all | h1 700, h2–h5 600, h6 500 (`R node.rs:2891-2899`) | part | |
| heading margin 24 / 8 | `pb(0.3rem)`, no top (`R node.rs:2906-2910`) | gap | Space before = previous block's gap (16); after ≈4.8px vs 16 |
| inline code fill / text / mono | `inline_code: HighlightStyle` (`R node.rs:1663-1690`) | ok | |
| inline code pill (radius 6, px 6, 0.8em, min-h 24) | `HighlightStyle` has no box or size | gap | Inline `MarkdownPlugin` on `mdast::Node::InlineCode` returning an `InlineElement` (`R gpui-base-0.6.6/src/text/markdown_ext.rs:45-75`). Atomic, so selection and wrapping inside the pill are at risk |
| file-like inline code icon + click | none | gap | Same plugin |
| link colour | `Theme.link` (global) | ok | |
| link no underline, hover colour | always a 1px underline, `style.link()` (`R node.rs:1059-1064`) | gap | Patch |
| code card: 10 radius, 6% fill, 1px 10% border, 12px | `code_block` refinement (`R node.rs:1569-1574`) | ok | `.bg().border_1().border_color().rounded().text_size()` |
| code header (icon, lang/path, 36px, divider) | none built in | gap | Block `MarkdownPlugin` claiming `mdast::Node::Code`, as our Mermaid does (`P crates/pocket/src/mermaid.rs:190-200`). We then own highlighting (tree-sitter via gpui-kit) |
| line numbers 10px 35% | none | gap | Same plugin |
| copy button 24×24 @ 6/8 | `code_block_actions` (`R gpui-component-0.6.6/src/text/compat.rs:118-125`), placed `absolute top_2 right_2` over a `code_background` fill (`R node.rs:1600-1604`) | part | Position 8/8, and the forced fill is visible |
| github-light/dark syntax | `highlight_theme` (`P theme.rs:78-96`) | part | Recolour with the §6 values |
| table card 10 / 6% / 1px 10% | `table` refinement on the frame (`R node.rs:2732-2739`) | ok | Frame bg defaults to `surface`; override |
| row rules 5% | `style.border()` (`R node.rs:2703-2704`) = `Theme.border` | part | Global colour, shared with hr and quote |
| no column rules | `border_r_1` on every non-last cell (`R node.rs:2693-2695`) | gap | A 5% border makes them faint; removal needs a patch |
| cells 8/10, 12px | `table_cell` refinement (`R node.rs:2691-2696`) | ok | |
| th 600, no fill | `table_head` refinement over the `code_background` fill (`R node.rs:2710-2714`) | ok | Set bg transparent and weight 600 |
| blockquote 4px c 20%, start 16, italic, 78% | `border_l_3`, `style.border()`, `px_4`, `muted_foreground` text (`R node.rs:2919-2935`) | part | 3px, shared colour, no italic, extra 16 right |
| hr 1px c 10%, 24 margin | 2px `style.border()` + paragraph gap (`R node.rs:2980-2986`) | part | |
| task checkbox | 14px square, foreground border, filled + check (`R node.rs:2389-2422`) | ok | Different but acceptable |
| images | `img` max-w 100%, contain (`R node.rs:1810-1814`) | ok | |
| footnotes | not verified | n/a | Low priority |
| frontmatter "Properties" `<details>` | `MarkdownExtensions::frontmatter()` (`R gpui-base-0.6.6/src/text/markdown_ext.rs:272`) + `FrontmatterPlugin`, which renders a `DescriptionList` (`R gpui-component-0.6.6/src/text/frontmatter.rs:39-82`) | part | Own block plugin on `mdast::Node::Yaml` for the collapsible card |
| mermaid card | our frame: radius 8, `HAIRLINE` border, `SURFACE` bg, my 8 (`P mermaid.rs:213`) | ok | Restyle to radius 10, 6% fill, 10% border |
| per-view colours | component style can't; base style can | gap | See §11.1 |
| dark mode | light-only theme | gap | |
| shell 24 / 32, page bg | `pane().px(40).py(20).bg(SURFACE)` (`P explore.rs:323-326`) | ok | Change the values |

### 11.3 Starter style (sketch, not compiled; light, alphas on TEXT #111113)

```rust
TextViewStyle {
    highlight_theme: cx.theme().highlight_theme.clone(),
    heading_font_size: Some(Arc::new(|level, _| px(match level { 1 => 22., 2 => 18., 3 => 20., 4 => 18., 5 => 16., _ => 14. }))),
    inline_code: HighlightStyle { background_color: Some(rgba(0x11111314).into()), color: Some(rgba(TEXT).into()), ..Default::default() },
    code_block: StyleRefinement::default().bg(rgba(0x1111130f)).border_1().border_color(rgba(0x1111131a)).rounded(px(10.)).px(px(12.)).py(px(10.)).text_size(px(12.)),
    table: StyleRefinement::default().bg(rgba(0x1111130f)).border_color(rgba(0x1111131a)).rounded(px(10.)),
    table_head: StyleRefinement::default().bg(transparent_black()).font_weight(FontWeight::SEMIBOLD),
    table_cell: StyleRefinement::default().px(px(10.)).py(px(8.)).text_size(px(12.)),
    ..Default::default()
}
```

The alpha bytes are 5% `0d`, 6% `0f`, 8% `14`, 10% `1a`, 20% `33`, 35% `59`, 45% `73`, 65% `a6`, 78% `c7`.

Nearest existing tokens:

| monocode | Our token |
|---|---|
| 78% | TEXT_BODY |
| 65% | TEXT_2 |
| 45% | TEXT_4 |
| 35% | TEXT_5 |
| 6% | FILL_4 |
| 10% | SEPARATOR |
| 5% | FILL_3 |

Global `Theme.border` should be about 5–10% for rows/hr/quote. It is shared with every component.

### 11.4 Gaps by effort

1. **Block plugin for fenced code:** header, icon, lang/path, line numbers and copy. This is the biggest visual win, and it reuses the `mdast::Node::Code` hook. It needs our own highlighting.
2. **Frontmatter plugin:** a collapsible "Properties" card.
3. **Inline-code plugin:** the pill and file icon. Selection and wrap need verifying.
4. **gpui-kit patches or fork:**
   - link underline/hover
   - bold colour
   - heading margins/weights
   - list spacing
   - quote italic/width/colour
   - hr thickness
   - table column rules
