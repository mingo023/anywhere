// Command palette (⌘K): sessions, worktrees, files and actions over the screen you came from, which stays
// drawn behind it. Mirrors palette.rs.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS, FILES } = AW;

const STATUS = { needs: 'Needs you', working: 'Working', done: 'Done', failed: 'Failed' };
const FILE_LIST = [
  ['settings.ts', 'src', true], ['useTheme.ts', 'hooks', true], ['Sheet.tsx', 'components', true], ['tokens.ts', 'src/theme', true],
  ['app.tsx', 'src', false], ['store.ts', 'src', false], ['Button.tsx', 'components', false], ['package.json', '', false], ['README.md', '', false],
];
const PAGES = [['activity', 'Go to Activity'], ['board', 'Go to Board'], ['automations', 'Go to Automations'], ['inbox', 'Go to Inbox'], ['hosts', 'Go to Settings']];
const sounds = { needs: true, done: true, failed: true };
const SOUND_NAME = { needs: 'Needs you', done: 'Done', failed: 'Failed' };

let prev = 'sessions';
let backdrop = '';
let query = '';
let ix = 0;
let allProjects = true;
let entries = [];

const words = () => query.trimStart().replace(/^>/, '').split(/\s+/).filter(Boolean).map((w) => w.toLowerCase());
const matches = (ws, fields) => ws.every((w) => fields.some((f) => f.toLowerCase().includes(w)));
const mark = (text, ws) => {
  const lower = text.toLowerCase();
  const spans = ws.flatMap((w) => [...lower.matchAll(w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') || '$^', 'g')].map((m) => [m.index, m.index + w.length]));
  spans.sort((a, b) => a[0] - b[0]);
  const merged = [];
  for (const s of spans) (merged.at(-1) && s[0] <= merged.at(-1)[1]) ? (merged.at(-1)[1] = Math.max(merged.at(-1)[1], s[1])) : merged.push([...s]);
  let out = '';
  let at = 0;
  for (const [a, b] of merged) { out += esc(text.slice(at, a)) + `<mark>${esc(text.slice(a, b))}</mark>`; at = b; }
  return out + esc(text.slice(at));
};

function sessionEntry(api, r) {
  const where = [r.p.name, r.local ? null : r.label, AGENTS[r.agent][1], STATUS[r.status] ?? 'Idle'].filter(Boolean).join(' · ');
  const lead = r.status ? api.glyph(r) : `<span class="ag">${ic(AGENTS[r.agent][0], 13)}</span>`;
  return { lead, title: r.title ?? r.label, detail: where, pick: () => api.select(r) };
}

function sections(api) {
  const q = query.trimStart();
  const ws = words();
  const here = api.sel.p;
  const rows = api.all().filter((r) => !r.local && r.title && (allProjects || r.p === here));
  const found = (r) => matches(ws, [r.title, r.p.name, r.label]);
  const actions = [
    { lead: ic('sparkle', 13), title: `New session in ${here.name}`, keys: '⌘ N', pick: () => api.go('worktrees') },
    { lead: ic('forward', 13), title: 'Go to next Needs you', keys: '⌘ J', pick: () => { const r = api.needing()[0]; if (r) api.select(r); } },
    { lead: ic('forward', 13), title: 'Go to Up next', keys: '⌘ ⇧ J', pick: () => { const r = api.needing()[0]; if (r) api.select(r); } },
    ...PAGES.map(([id, title]) => ({ lead: ic(id === 'activity' ? 'activity' : id === 'board' ? 'blocks' : id === 'automations' ? 'bolt' : id === 'inbox' ? 'bell' : 'settings', 13), title, pick: () => api.go(id) })),
    ...Object.keys(sounds).map((k) => ({ lead: ic('bell', 13), title: `${SOUND_NAME[k]} sound: ${sounds[k] ? 'On' : 'Off'}`, pick: () => { sounds[k] = !sounds[k]; }, stay: true })),
  ].filter((a) => matches(ws, [a.title]));
  if (q.startsWith('>')) return [['Actions', actions]];
  const upNext = rows.filter((r) => ['needs', 'failed'].includes(r.status)).sort((a, b) => (a.status === 'needs' ? 0 : 1) - (b.status === 'needs' ? 0 : 1)).filter(found).slice(0, 5);
  const sessions = rows.filter(found).sort((a, b) => b.id - a.id).slice(0, ws.length ? 8 : 5);
  const locals = !ws.length ? [] : api.all().filter((r) => r.local && matches(ws, ['local', r.p.name])).slice(0, 5)
    .map((r) => ({ lead: ic('laptop', 13), title: 'Local', detail: r.p.name, pick: () => api.select(r) }));
  const trees = !ws.length ? [] : rows.filter(found).slice(0, 5).map((r) => ({ lead: ic('worktree', 13), title: r.label, detail: `${r.p.name} · ${r.label}`, pick: () => api.select(r) }));
  const files = (ws.length ? FILE_LIST.filter(([n, d]) => matches(ws, [`${d}/${n}`])) : FILE_LIST.filter((f) => f[2]).slice(0, 3)).slice(0, 8)
    .sort((a, b) => b[2] - a[2])
    .map(([n, d, changed]) => ({ lead: FILES[n.split('.').pop()] ? AW.fileIcon(n) : ic('file', 14), title: n, detail: [d, changed ? 'modified' : ''].filter(Boolean).join(' · '), pick: () => api.go('review') }));
  return [
    ['Up next', upNext.map((r) => sessionEntry(api, r))],
    ['Sessions', sessions.map((r) => sessionEntry(api, r))],
    ['Locals', locals],
    ['Worktrees', trees],
    ['Files', files],
    ['Actions', actions],
  ].filter(([, list]) => list.length);
}

const kbd = (keys, auto) => `<span class="pl-kbd${auto ? ' auto' : ''}">${keys.split(' ').map((k) => `<i>${k}</i>`).join('')}</span>`;

function results(api) {
  const secs = sections(api);
  const ws = words();
  entries = secs.flatMap(([, list]) => list);
  ix = Math.min(ix, Math.max(0, entries.length - 1));
  let n = 0;
  const body = secs.map(([label, list]) => `<div class="pl-label">${label}</div>${list.map((e) => {
    const i = n++;
    const keys = e.keys ?? (i === ix ? '↵' : '');
    return `<button class="pl-row${i === ix ? ' on' : ''}" data-act="pick" data-v="${i}"><span class="pl-lead">${e.lead}</span><span class="pl-title">${mark(e.title, ws)}</span><span class="pl-detail">${e.detail ? mark(e.detail, ws) : ''}</span>${keys ? kbd(keys, !e.keys) : ''}</button>`;
  }).join('')}`).join('');
  return body || '<div class="pl-none"><b>No matches.</b><span>Try a session title, project, worktree or branch.</span></div>';
}

const hint = (k, label) => `<span class="pl-hint"><b>${k}</b>${label}</span>`;
const scopeText = () => (allProjects ? 'Tab to filter by project' : 'Tab to search all projects');

// Shown when the palette is the first screen, with nothing behind it to keep.
function newSession(api) {
  const p = api.sel.p;
  return `<div class="pane dots"><div class="bar bare"><div class="crumbs">${esc(p.name)}<i>/</i><b>New session</b></div></div>
    <div class="home"><h2>What should we build next?</h2>
      <div class="composer empty"><textarea rows="2" placeholder="Ask, build, / for commands, @ for files" aria-label="Message"></textarea>
        <div class="foot"><button class="chip"><span class="ag claude">${ic('claude', 15)}</span><b>Claude Code</b>${ic('chevron-down', 12)}</button><button class="chip">${ic('shield', 15)}Agent's settings${ic('chevron-down', 12)}</button><span class="sp"></span><button class="ib">${ic('plus', 17)}</button><button class="send">${ic('arrow-up', 16)}</button></div></div>
      <div class="where-row"><button class="chip ghost"><span class="mini">${api.letter(p.name)}</span>${esc(p.name)}${ic('chevron-down', 12)}</button><button class="chip ghost">${ic('worktree', 14)}Worktree${ic('chevron-down', 12)}</button><button class="chip ghost mono">${ic('branch', 14)}main${ic('chevron-down', 12)}</button></div></div></div>${api.rail(null, {})}`;
}

function close(api) { api.go(prev); }

function choose(api, i) {
  const e = entries[i];
  if (!e) return;
  e.pick();
  if (e.stay) return close(api);
  // A pick that navigates has already left; one that doesn't (a sound) closes the palette.
  if (api.state.screen === 'palette') close(api);
}

function paint(api) {
  const list = api.$('.pl-results');
  if (list) list.innerHTML = results(api);
  const scope = api.$('.pl-scope');
  if (scope) scope.textContent = scopeText();
}

function scrollTo(api) {
  const on = api.$('.pl-row.on');
  on?.scrollIntoView({ block: 'nearest' });
}

D.register('palette', {
  enter(api) {
    const was = document.querySelector('.app')?.dataset.screen;
    if (was && was !== 'palette') prev = was;
    query = '';
    ix = 0;
    allProjects = true;
  },
  render(api) {
    const area = api.$('.area');
    if (area && !area.querySelector('.pl-back')) backdrop = area.innerHTML;
    const html = results(api);
    return `<div class="pl-back" aria-hidden="true">${backdrop || newSession(api)}</div>
      <div class="pl-layer"><div class="pl-scrim" data-act="close"></div>
        <div class="pl-box" role="dialog" aria-label="Command palette">
          <div class="pl-input">${ic('search', 17)}<input placeholder="Search sessions, files and actions…" value="${esc(query)}" aria-label="Search" autocomplete="off" spellcheck="false"><span>esc</span></div>
          <div class="pl-results">${html}</div>
          <div class="pl-foot">${hint('↑↓', 'Navigate')}${hint('↵', 'Open')}<span class="pl-scope">${scopeText()}</span>${hint('esc', 'Close')}</div>
        </div></div>`;
  },
  mount(root) {
    root.querySelector('.pl-input input')?.focus({ preventScroll: true });
  },
  act(act, v, el, api) {
    if (act === 'close') return close(api);
    if (act === 'pick') return choose(api, +v);
  },
  input(e, api) {
    if (!e.target.matches('.pl-input input')) return;
    query = e.target.value;
    ix = 0;
    paint(api);
    api.$('.pl-results')?.scrollTo(0, 0);
  },
  key(e, api) {
    const n = entries.length;
    if (e.key === 'Escape') { e.preventDefault(); return close(api); }
    if (e.key === 'Tab') { e.preventDefault(); allProjects = !allProjects; ix = 0; return paint(api); }
    if (!n) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      ix = (ix + (e.key === 'ArrowDown' ? 1 : n - 1)) % n;
      paint(api);
      return scrollTo(api);
    }
    if (e.key === 'Enter') { e.preventDefault(); choose(api, ix); }
  },
});

// A click on the sidebar's Search field or ⌘K opens the palette.
const app = () => document.querySelector('.app');
document.addEventListener('keydown', (e) => {
  if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== 'k') return;
  const a = app();
  if (!a || !(a.matches(':hover') || a.contains(document.activeElement))) return;
  if (a.dataset.screen === 'palette') return;
  e.preventDefault();
  D.go('palette');
});

// Moving the pointer over a row selects it, as the app's rows do.
document.addEventListener('mousemove', (e) => {
  const row = e.target.closest?.('.app .pl-row');
  if (!row || row.classList.contains('on')) return;
  const i = +row.dataset.v;
  ix = i;
  const a = app();
  a.querySelectorAll('.pl-row').forEach((r) => {
    const on = +r.dataset.v === i;
    r.classList.toggle('on', on);
    r.querySelector('.pl-kbd.auto')?.remove();
    if (on && !entries[i]?.keys) r.insertAdjacentHTML('beforeend', kbd('↵', true));
  });
});
})();
