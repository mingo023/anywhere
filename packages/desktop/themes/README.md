# Anywhere themes

Every theme the desktop app ships is a JSON file in this folder. To make your own, copy the one closest to what you want, change its colours, and import it from **Settings › Appearance › Custom themes**.

`graphite-light.json` and `graphite-dark.json` are the defaults and list every key, so start from one of them.

## The file

```json
{
  "name": "My theme",
  "appearance": "dark",
  "accent": "#bd93f9",
  "colors": {
    "text": "#f8f8f2",
    "window": "#282a36d9"
  }
}
```

| Field | What it is |
|---|---|
| `name` | What Settings calls the theme. Importing a theme with a name you already imported replaces it. |
| `appearance` | `"light"` or `"dark"`. A theme is one look; make two files for a pair. |
| `accent` | Optional. The colour of selection, focus rings and primary buttons. |
| `colors` | The colours, by key. A key you leave out keeps the default theme's colour for the same appearance. Keys the app doesn't know are ignored. |
| `backdrop` | Optional. A colour, or a `["top", "bottom"]` pair for a gradient, painted behind a chat's messages. |
| `bubble` | Optional. `{ "radius": 0–32 }` rounds the corners of your messages in a chat. |
| `scene` | Optional. `{ "sprites": [{ "art": ["..ab"], "colors": { "a": "#hex" }, "pixel": 4, "count": 3, "speed": 30 }] }`: pixel art that swims across a chat behind its messages. In `art` each character names a colour, and `.` or a space is clear. `pixel` is 1–16 points, `count` 0–24 copies, `speed` 0–400 points a second. It moves 12 times a second, and stands still under Reduce motion. |

A colour is `#RRGGBB`, or `#RRGGBBAA` with an alpha.

The app copies the file into `themes/` beside its settings and reads it again whenever its window comes back to the front, so you can edit the copy and switch back to see the change.

A file that isn't JSON, or has a colour or an appearance the app can't read, is refused with a message that names the problem. The app doesn't check contrast: an imported theme is yours to make legible.

## What a theme doesn't set

- Agent and brand colours, the six project colours, and the git graph's lanes.
- The terminal's ANSI colours, which **Settings › Terminal** sets.
- Syntax colours inside a chat's or a markdown file's code blocks.
- Anything Settings overrides: **Accent color**, **Syntax theme**, **Diff colors** and **Custom diff colors** win over the theme until they are set back to **From theme**.

## `accent`

With an `accent`, the app works out these keys from it and ignores what the file says for them: `selection`, `accent.bg`, `accent.ring`, `accent.tint`, `accent.glow`, `action` and `action.on`. Primary buttons take the accent too, with white or near-black text, whichever reads better. Without one they take `text` and `text.on`.

## Dark themes and glass

The dark window is glass: the desktop shows through it as far as **Window opacity** allows. Graphite Dark gets that by leaving `page`, `canvas`, `side` and `surface.sunken` clear (`#00000000`) and by making `surface`, `well` and the `fill.*` keys a little of the text colour (`#ebebeb0d` to `#ebebeb1f`). Give those keys solid colours and the glass is covered.

`window`'s alpha is ignored in dark themes; the slider sets it. A dark theme's `sidebar` shows only with **Window opacity** at 100%; below that the sidebar is clear glass.

## Keys

### Text

| Key | Where it shows |
|---|---|
| `text` | Titles and anything primary |
| `text.body` | Body copy |
| `text.2` … `text.6` | Secondary text down to the faintest placeholder |
| `text.on` | Text on a `text`-coloured fill, such as a primary button with no accent |
| `shimmer` | The band of light crossing a working label |
| `link` | Links, which are also underlined |
| `accent.text` | Small marks of emphasis: unread dots, drop targets |

### Surfaces

| Key | Where it shows |
|---|---|
| `window`, `window.solid` | The window; `window.solid` where nothing may show through |
| `surface`, `surface.solid` | Cards; `surface.solid` where nothing may show through |
| `surface.sunken` | The terminal, the diff and other recessed panes |
| `page`, `canvas` | The page behind a diff, a commit or the git graph, and behind the Board and Activity |
| `well` | A recessed block inside a page, such as a chat's code block |
| `side` | Side panels |
| `sidebar`, `sidebar.cutout`, `sidebar.notice` | The left sidebar, the ring around a badge on it, and its "What's new" card |
| `popover`, `popover.glass`, `glass` | Menus and floating panels |
| `highlight`, `shadow`, `cutout` | The top-edge sheen and shadow of a raised surface, and the ring around a badge |
| `fill.1` … `fill.4` | Hover and pressed fills, faintest first |
| `hairline`, `separator`, `separator.strong` | Lines |
| `selection` | Selected text |
| `terminal.cursor` | The terminal's block cursor |
| `accent.bg`, `accent.ring`, `accent.tint`, `accent.glow` | The selected row, focus rings, a found row and a diff line being pointed at |
| `action`, `action.on` | The Board's buttons and their text |

### Status

| Key | Where it shows |
|---|---|
| `waiting`, `waiting.text`, `waiting.bg`, `waiting.dot`, `waiting.on` | A session that needs you: glyph, text, tint, the badge and the text on it |
| `failed`, `failed.text`, `failed.bg` | Errors |
| `success`, `success.text`, `success.bg` | Things that passed |
| `working` | A running session |
| `merged` | A merged pull request |
| `goal.text`, `goal.bg`, `goal.bg.strong` | An active goal |
| `modified` | A changed file's mark |

### Chat

| Key | Where it shows |
|---|---|
| `chat.rail` | The line a reply's steps hang from |
| `chat.bubble` | Your messages |
| `chat.mention`, `chat.command` | An `@mention` and a `/command` in a prompt |
| `agent-map.card`, `agent-map.card.hover` | A node of the agent map |
| `diagram.hud` | The buttons over a diagram |

### Code

| Key | Where it shows |
|---|---|
| `diff.added.bg`, `diff.added.text`, `diff.added.word` | Added lines: the row, the text and a changed word |
| `diff.removed.bg`, `diff.removed.text`, `diff.removed.word` | Removed lines |
| `syntax.keyword`, `syntax.function`, `syntax.string`, `syntax.comment` | Code in diffs, the editor and file previews |

### Markdown

| Key | Where it shows |
|---|---|
| `markdown.text`, `markdown.text.2` | A rendered markdown file's text |
| `markdown.link` | Its links |
| `markdown.grid` | Table lines and rules |
| `markdown.aside` | Quotes and callouts |
| `markdown.code.bg`, `markdown.code.border` | Code blocks |
| `markdown.page` | The page behind it |
