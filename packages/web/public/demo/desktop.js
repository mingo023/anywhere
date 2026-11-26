// A working copy of the desktop window, drawn at the app's 1440×900pt size and scaled to fit its frame.
// The shell owns the sidebar, the shared demo data and the router. Each feature is a screen in demo/screens/
// that registers itself with AnywhereDesktop.register and draws the area right of the sidebar (or, with
// `full`, the whole window).

(() => {
const { ic, esc, AGENTS } = AW;

// Demo data only: never the names of real projects. A row is a worktree (or the project's Local) and the
// session running in it; the sidebar and the tab show the session's title, the chat's footer its branch.
const PROJECTS = [
  { name: 'orbit-web', open: true, rows: [
    { label: 'feat/dark-mode', title: 'Add dark mode to settings', status: 'working', chat: true, add: 48, del: 6,
      log: [['you', 'Add a dark mode toggle to the settings screen, and remember the choice.'], ['say', "I'll add a theme setting and wire it to the existing tokens."], ['tool', 'Read', 'src/settings.ts']],
      script: [['tool', 'Read', 'src/theme/tokens.ts'], ['tool', 'Write', 'hooks/useTheme.ts'], ['tool', 'Edit', 'src/settings.ts'], ['tool', 'Bash', 'pnpm test settings'], ['say', 'Done. Here\'s what changed:\n\n- Added a `theme` setting with **Light**, **Dark** and **System**\n- `useTheme` reads it and sets `data-theme` on the root\n- The choice is saved in `localStorage`\n\n```mermaid\nflowchart LR\n  A[Settings toggle] --> B[useTheme]\n  B --> C[localStorage]\n  B --> D[data-theme on root]\n```\n\nAll **48 tests pass**.']] },
    { label: 'fix/flaky-checkout', title: 'Fix flaky checkout test', status: 'needs', agent: 'codex', add: 12, del: 9,
      log: [['you', 'The checkout test fails about one run in five. Find out why.'], ['tool', 'Read', 'tests/checkout.spec.ts'], ['say', 'It waits a fixed 500ms for the cart total. I want to run it 20 times to confirm.']],
      ask: { cmd: 'pnpm test checkout --repeat 20', rule: 'pnpm test',
        after: [['tool', 'Bash', 'pnpm test checkout --repeat 20'], ['err', '4 failed, 16 passed'], ['tool', 'Edit', 'tests/checkout.spec.ts'], ['tool', 'Bash', 'pnpm test checkout --repeat 20'], ['out', '20 passed'], ['say', 'Fixed. The test now waits for the cart total instead of a timer.']] } },
    { label: 'chore/react-19', title: 'Migrate to React 19', pr: 'open', chat: true, add: 84, del: 51,
      log: [['you', 'Upgrade us to React 19'], ['tool', 'Bash', 'pnpm up react@19 react-dom@19'], ['tool', 'Edit', 'src/app.tsx'], ['tool', 'Bash', 'gh pr create --fill'], ['say', 'Opened **PR #214**: Migrate to React 19. Checks are running.']] },
  ] },
  { name: 'fox-api', open: true, rows: [
    { label: 'feat/upload-limit', title: 'Rate-limit the upload route', status: 'done', chat: true, add: 21, del: 2,
      log: [['you', 'Rate-limit POST /uploads to 10 a minute per user'], ['tool', 'Edit', 'src/routes/uploads.ts'], ['tool', 'Bash', 'pnpm test uploads'], ['say', 'Done. Over the limit, the route now returns `429` with `Retry-After`.']] },
    { label: 'perf/invoice-cache', title: 'Cache invoice queries', status: 'failed', agent: 'codex', add: 33, del: 4,
      log: [['you', 'Cache the invoice list queries'], ['tool', 'Edit', 'src/invoices/list.ts'], ['tool', 'Bash', 'pnpm test invoices'], ['err', '2 failed: cached totals are stale after a refund']] },
    { label: 'Local', local: true },
  ] },
  { name: 'garden-mobile', open: true, rows: [
    { label: 'fix/resume-crash', title: 'Crash when resuming a download', status: 'needs', chat: true, add: 7, del: 3,
      log: [['you', 'The app crashes when a paused download resumes'], ['tool', 'Read', 'src/downloads/queue.ts'], ['say', 'The resume handler reads a task that was already freed. I want to reproduce it with the queue tests.']],
      ask: { cmd: 'pnpm test queue --runInBand', rule: 'pnpm test',
        after: [['tool', 'Bash', 'pnpm test queue --runInBand'], ['err', '1 failed: resume after cancel'], ['tool', 'Edit', 'src/downloads/queue.ts'], ['tool', 'Bash', 'pnpm test queue --runInBand'], ['say', 'Fixed. Resuming a cancelled download no longer touches the freed task.']] } },
    { label: 'Local', local: true },
  ] },
  { name: 'docs-site', open: false, rows: [{ label: 'Local', local: true }] },
];
let ids = 0;
PROJECTS.forEach((p) => p.rows.forEach((r) => Object.assign(r, { id: ids++, p, agent: r.agent ?? 'claude', log: r.log ?? [] })));

const all = () => PROJECTS.flatMap((p) => p.rows);
const needing = () => all().filter((r) => r.status === 'needs');

// The app's tile letter: the first letter of the name's last dash-separated word ("orbit-web" is W).
const letter = (name) => (name.split('-').filter(Boolean).pop() ?? name)[0].toUpperCase();

// The 3×3 gradient spinner: dots tinted along the diagonal, a pulse sweeping from the top-left.
const GRADIENT = ['#8fb8ff', '#a9aafa', '#c39cf5', '#e19db7', '#ff9f7a'];
const spinner = (size = 11) => `<span class="gspin" style="--z:${size}px">${[0, 1, 2].map((row) => [0, 1, 2].map((col) =>
  `<i style="background:${GRADIENT[row + col]};animation-delay:${-((8 - row - col) / 8) * 800}ms"></i>`).join('')).join('')}</span>`;

const CROSS = [1, 0, 1, 0, 1, 0, 1, 0, 1];
const matrix = (status, size = 11) => `<span class="mtx ${status}" style="--z:${size}px">${CROSS.map((x) =>
  `<i${status !== 'failed' || x ? ' class="on"' : ''}></i>`).join('')}</span>`;

const glyph = (r) =>
  r.status === 'working' ? spinner()
  : ['needs', 'failed', 'done'].includes(r.status) ? matrix(r.status)
  : r.local ? `<span class="dim">${ic('laptop', 13)}</span>`
  : `<span class="dim">${ic('worktree', 13)}</span>`;

const diffstat = (r) => (r.add || r.del ? `<span class="stat"><b class="add">+${r.add}</b><b class="del">−${r.del}</b></span>` : '');

const PAGES = [['activity', 'Activity', 'activity'], ['board', 'Board', 'blocks'], ['automations', 'Automations', 'bolt']];

function sidebar() {
  const n = api.alerts();
  const page = screens[state.screen]?.page;
  return `
    <div class="side-head">
      <button class="ib bell" data-act="go" data-v="inbox" aria-label="Inbox">${ic('bell', 17)}${n ? `<b class="badge">${n}</b>` : ''}</button>
      <button class="ib" aria-label="Hide sidebar">${ic('sidebar-collapse', 17)}</button>
    </div>
    <button class="field" data-act="go" data-v="palette">${ic('search', 15)}<span>Search</span><kbd>⌘K</kbd></button>
    <div class="pages">${PAGES.map(([id, name, icon]) => `<button class="row page${page === id ? ' on' : ''}" data-act="go" data-v="${id}"><span class="glyph">${ic(icon, 15)}</span><span class="name">${name}</span></button>`).join('')}</div>
    <div class="label">Sessions<span class="tools"><button class="ib" data-act="new" aria-label="New session">${ic('plus', 15)}</button></span></div>
    <div class="label">Projects<span class="tools"><button class="ib" aria-label="Filter projects">${ic('search', 14)}</button><button class="ib" aria-label="Sort">${ic('arrow-down-wide-narrow', 14)}</button><button class="ib" aria-label="Add project">${ic('folder-plus', 14)}</button></span></div>
    <div class="projects">${PROJECTS.map((p, i) => `
      <button class="row proj" data-act="fold" data-v="${i}"><span class="tile">${letter(p.name)}</span><span class="name">${p.name}</span></button>
      ${p.open ? p.rows.map((r) => `<button class="row wt${!page && r === state.sel ? ' on' : ''}" data-act="sel" data-v="${r.id}"><span class="glyph">${glyph(r)}</span><span class="name">${esc(r.title ?? r.label)}</span>${diffstat(r)}</button>`).join('') : ''}`).join('')}</div>
    <button class="row settings" data-act="go" data-v="hosts"><span class="glyph">${ic('settings', 15)}</span><span class="name">Settings</span></button>`;
}

// The right rail, shared by session screens: chat list, files, changes and pull requests.
const RAIL = [['sessions', 'sessions', 'Sessions'], ['explorer', 'explorer', 'Files'], ['changes', 'branch', 'Changes'], ['prs', 'github', 'Pull requests']];
const rail = (on, counts = {}) => `<div class="rail">${RAIL.map(([id, icon, label]) =>
  `<button class="${on === id ? 'on' : ''}" data-act="rail" data-v="${id}" aria-label="${label}">${ic(icon, 17)}${counts[id] ? `<b class="count">${counts[id]}</b>` : ''}${id === 'sessions' && needing().length ? '<i class="dot"></i>' : ''}</button>`).join('')}</div>`;

const tab = (r) => {
  const [icon, name] = AGENTS[r.agent];
  return `<div class="tab"><span class="ag ${r.agent}">${ic(icon, 15)}</span><span class="tt"><b>${esc(r.title ?? r.label)}</b><small>${name}</small></span></div>`;
};

const screens = {};
const listeners = new Set();
const win = document.querySelector('.window.desktop');
const state = { screen: win.dataset.screen || 'sessions', sel: PROJECTS[0].rows[0] };

const app = document.createElement('div');
app.className = 'app';
const $ = (q) => app.querySelector(q);

const api = {
  PROJECTS, all, needing, glyph, spinner, matrix, diffstat, letter, rail, tab, state, $, ic, esc, AGENTS,
  go, draw,
  // The bell's count; the inbox replaces it with Needs you plus unseen Failed and Done.
  alerts: () => needing().length,
  get sel() { return state.sel; },
  select(r) { state.sel = r; go('sessions'); },
  // Screens that show statuses (inbox, activity) subscribe; a session's status change redraws them.
  changed() { listeners.forEach((f) => f()); draw(); },
  onChange(f) { listeners.add(f); },
};

// A field's place in the window as child indexes, so the same field can be found after a redraw.
const pathTo = (el) => {
  const path = [];
  for (; el && el !== app; el = el.parentElement) path.unshift([...el.parentElement.children].indexOf(el));
  return path;
};
const atPath = (path) => path.reduce((el, i) => el?.children[i], app);

function draw() {
  const s = screens[state.screen];
  if (!s) return;
  const keep = $('.area .scroll')?.scrollTop;
  // A redraw rebuilds every element; a field the visitor is typing in keeps its focus, text and caret.
  const typing = app.contains(document.activeElement) && document.activeElement.matches('input, textarea')
    ? { path: pathTo(document.activeElement), tag: document.activeElement.tagName, value: document.activeElement.value, from: document.activeElement.selectionStart, to: document.activeElement.selectionEnd }
    : null;
  const body = s.render(api);
  app.classList.toggle('full', !!s.full);
  app.dataset.screen = state.screen;
  app.innerHTML = s.full ? body : `<div class="side">${sidebar()}</div><div class="area">${body}</div>`;
  if (keep != null && s.keepScroll && $('.area .scroll')) $('.area .scroll').scrollTop = keep;
  s.mount?.(app, api);
  const field = typing && atPath(typing.path);
  if (typing && field?.tagName === typing.tag) {
    // A redraw the visitor caused (sending, submitting) draws the field as the screen wants it; one from a timer
    // keeps what they typed.
    if (!acting && field.value !== typing.value) field.value = typing.value;
    field.focus({ preventScroll: true });
    try { field.setSelectionRange(typing.from, typing.to); } catch {}
  }
}

function go(id) {
  if (!screens[id]) return;
  const before = state.screen;
  state.screen = id;
  screens[id].enter?.(api);
  draw();
  if (before !== id) document.dispatchEvent(new CustomEvent('desktop:screen', { detail: screens[id].tour ?? id }));
}

// True while a click, key or input from the visitor is being handled.
let acting = false;
const during = (f) => (e) => { acting = true; try { f(e); } finally { acting = false; } };

app.addEventListener('click', during((e) => {
  const t = e.target.closest('[data-act]');
  if (!t || !app.contains(t)) return;
  const { act, v } = t.dataset;
  switch (act) {
    case 'go': return go(v);
    case 'fold': PROJECTS[v].open = !PROJECTS[v].open; return draw();
    case 'sel': {
      state.sel = all().find((r) => r.id === +v);
      const s = screens[state.screen];
      return s?.sessionScreen ? draw() : go('sessions');
    }
  }
  screens[state.screen]?.act?.(act, v, t, api, e);
}));
app.addEventListener('input', during((e) => screens[state.screen]?.input?.(e, api)));
app.addEventListener('keydown', during((e) => screens[state.screen]?.key?.(e, api)));

window.AnywhereDesktop = {
  register(id, screen) { screens[id] = screen; },
  go,
  api,
};

// The app keeps its own width (1440pt, narrower on a phone) and scales to the window's.
new ResizeObserver(([e]) => win.style.setProperty('--s', e.contentRect.width / app.offsetWidth)).observe(win);
win.append(app);
win.classList.add('live');
// `#demo=<screen>` shows only the window, unscaled, on that screen: the harness that compares it with the app's captures.
const only = location.hash.match(/demo=([\w-]+)/);
if (only) {
  document.documentElement.classList.add('demo-only');
  state.screen = only[1];
}
// Screens register from later scripts; draw once they all have.
addEventListener('DOMContentLoaded', () => { screens[state.screen]?.enter?.(api); draw(); });
})();
