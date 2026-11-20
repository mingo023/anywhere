# Anywhere mods

A mod is JavaScript that hooks the app's events. Each lives in its own folder, `mods/<id>/mod.js`, beside the app's settings (`themes/` sits there too). The app runs every mod when it starts, in id order, in a QuickJS engine of its own: no DOM, no Node, no file or network access.

```js
export function register(on) {
    on('start', $ => {
        $.theme.set({ name: 'Paper', appearance: 'light', backdrop: ['#fdf6e3', '#f3e5c0'], bubble: { radius: 4 } });
    });
}
```

`on(event, hook)` adds a hook, called as `hook($, e)`. `e.event` names the event, and `e` holds its fields.

| | What it does |
|---|---|
| `start` | The app is starting. |
| `prompt` | The user sent `e.text` in a chat. The app reads the mods from disk again first, so a mod an agent just wrote takes part. |
| `$.theme.set(look)` | Paints `look` in place of the theme chosen in Settings. A look is a theme file's fields as an object (see `../themes/README.md`). The last look any mod sets wins. |

A mod that throws, sets a look that isn't a theme, or runs past 250ms or 16MB changes nothing; the app writes why to its log as `mod <id>: <why>`.

Each event runs in a fresh engine, so a mod keeps nothing between events.

`paper/` is a whole mod that works out its colours from one hue. `koi/` is a pond of pixel koi that turns red and fills up when a prompt mentions a bug, and yellow for a test. To try it, copy the folder into `mods/` beside the app's settings and restart the app.
