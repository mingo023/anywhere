// Review: a session with the Changes, Pull request or Files panel open from the rail. Changes lists the
// worktree's files, a click opens the diff in the main pane, and the commit box commits, pushes and opens a PR.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, FILES } = AW;

// ---- Diffs -------------------------------------------------------------------------------------------------
// A diff is written as git prints it, but a hunk header is `@ <old start> <new start> <context>`.

const parseDiff = (src) => {
  const lines = [];
  let o = 0;
  let n = 0;
  let end = 0;
  for (const raw of src.replace(/^\n/, '').replace(/\n$/, '').split('\n')) {
    const k = raw[0];
    const text = raw.slice(1);
    if (k === '@') {
      const [, a, b, ...ctx] = raw.split(' ');
      o = +a;
      n = +b;
      lines.push({ k: 'h', hidden: Math.max(0, n - (end + 1)), ctx: ctx.join(' ') });
    } else if (k === '+') lines.push({ k: 'a', text, n: n++ });
    else if (k === '-') lines.push({ k: 'd', text, o: o++ });
    else lines.push({ k: 'c', text, o: o++, n: n++ });
    if (k !== '@' && k !== '-') end = n - 1;
  }
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].k !== 'd') continue;
    const ds = [];
    const as = [];
    while (lines[i]?.k === 'd') ds.push(lines[i++]);
    while (lines[i]?.k === 'a') as.push(lines[i++]);
    i--;
    for (let j = 0; j < Math.min(ds.length, as.length); j++) words(ds[j], as[j]);
  }
  return lines;
};

// The changed span of a replaced line: what stays between its common start and end.
function words(del, add) {
  const x = del.text;
  const y = add.text;
  let s = 0;
  while (s < x.length && s < y.length && x[s] === y[s]) s++;
  let e = 0;
  while (e < x.length - s && e < y.length - s && x[x.length - 1 - e] === y[y.length - 1 - e]) e++;
  const word = /[\w$]/;
  while (s > 0 && word.test(x[s - 1])) s--;
  while (e > 0 && word.test(x[x.length - e])) e--;
  if (s + e < Math.min(x.length, y.length) * 0.3) return;
  if (x.length - e > s) del.w = [s, x.length - e];
  if (y.length - e > s) add.w = [s, y.length - e];
}

const KW = new Set('import export from const let var function return if else type interface as new async await default for of in class extends typeof void throw try catch switch case break enum'.split(' '));
const TOKEN = /(\/\/.*$)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)|([A-Za-z_$][\w$]*)(?=\s*\()|([A-Za-z_$][\w$]*)|(\s+)|(.)/g;

// Syntax colours are the theme's four tokens; word marks lay a tint over the changed span.
function code(text, w) {
  const out = [];
  const put = (cls, s, at) => {
    const cut = w ? [w[0], w[1]].filter((p) => p > at && p < at + s.length).map((p) => p - at) : [];
    const parts = [];
    let from = 0;
    for (const p of [...cut, s.length]) {
      parts.push([from, p]);
      from = p;
    }
    for (const [a, b] of parts) {
      const hot = w && at + a >= w[0] && at + b <= w[1];
      const c = [cls, hot ? 'w' : ''].filter(Boolean).join(' ');
      out.push(c ? `<span class="${c}">${esc(s.slice(a, b))}</span>` : esc(s.slice(a, b)));
    }
  };
  for (const m of text.matchAll(TOKEN)) {
    const [s, com, str, fn, id] = m;
    put(com ? 'cm' : str ? 'st' : fn ? (KW.has(fn) ? 'kw' : 'fn') : id && KW.has(id) ? 'kw' : '', s, m.index);
  }
  return out.join('') || '&nbsp;';
}

// ---- Data --------------------------------------------------------------------------------------------------

const SETS = {
  'feat/dark-mode': [
    ['src/settings.ts', 'M', `
@ 1 1
 import { store } from './store';
+import { useTheme, type Theme } from '../hooks/useTheme';
 import { Sheet } from '../components/Sheet';

-type Section = 'account' | 'notifications';
+type Section = 'account' | 'appearance' | 'notifications';

 export function openSettings(section: Section = 'account') {
   return Sheet.open({ title: 'Settings', section });
@ 24 25 export function SettingsPanel() {
   const user = store.user();
-  const dark = false;
+  const { theme, setTheme } = useTheme();

+  const options: Theme[] = ['light', 'dark', 'system'];
+
   return (
-    <Panel title="Account">
+    <Panel title="Appearance">
+      <label className="row">
+        Theme
+        <select value={theme} onChange={(e) => setTheme(e.target.value as Theme)}>
+          {options.map((o) => (
+            <option key={o}>{o}</option>
+          ))}
+        </select>
+      </label>
+      <small>System follows your OS setting.</small>
       <Avatar user={user} />
       <Name value={user.name} />
-      <Toggle on={dark} />
+      <Divider />
+      <Sync keys={['theme']} />
     </Panel>
   );
 }
`],
    ['hooks/useTheme.ts', 'A', `
@ 0 1
+import { useEffect, useState } from 'react';
+
+export type Theme = 'light' | 'dark' | 'system';
+
+const KEY = 'theme';
+
+// Remembers the choice; System follows the OS.
+export function useTheme() {
+  const [theme, setTheme] = useState<Theme>(
+    () => (localStorage.getItem(KEY) as Theme) ?? 'system',
+  );
+
+  useEffect(() => {
+    const media = matchMedia('(prefers-color-scheme: dark)');
+    const dark = theme === 'system' ? media.matches : theme === 'dark';
+    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
+    localStorage.setItem(KEY, theme);
+  }, [theme]);
+
+  return { theme, setTheme };
+}
+
+export const themes: Theme[] = ['light', 'dark', 'system'];
+`],
    ['components/Sheet.tsx', 'M', `
@ 18 18 export function Sheet({ title, children }: Props) {
   return (
     <div className="sheet">
-      <h2>{title}</h2>
+      <header className="sheet-head">
+        <h2>{title}</h2>
+        <Close onClick={close} />
+      </header>
       {children}
     </div>
   );
`],
    ['src/theme/tokens.ts', 'M', `
@ 9 9 export const tokens = {
   radius: 8,
-  surface: '#ffffff',
+  surface: 'var(--surface)',
+  surfaceDark: '#1a1a1c',
+  textDark: '#ebebeb',
   text: '#171717',
 };
`],
  ],
  'fix/flaky-checkout': [
    ['tests/checkout.spec.ts', 'M', `
@ 12 12 test('shows the cart total', async ({ page }) => {
   await page.goto('/cart');
-  await page.waitForTimeout(500);
-  expect(await page.textContent('.total')).toBe('$42.00');
+  await expect(page.locator('.total')).toHaveText('$42.00');
 });
`],
    ['tests/helpers/cart.ts', 'M', `
@ 3 3
 export async function fillCart(page: Page) {
-  await page.click('#add');
+  await page.getByRole('button', { name: 'Add' }).click();
 }
`],
  ],
  'chore/react-19': [
    ['package.json', 'M', `
@ 10 10 "dependencies": {
-    "react": "^18.3.1",
-    "react-dom": "^18.3.1",
+    "react": "^19.0.0",
+    "react-dom": "^19.0.0",
     "zustand": "^4.5.2"
`],
    ['src/index.tsx', 'M', `
@ 1 1
-import { render } from 'react-dom';
+import { createRoot } from 'react-dom/client';
 import { App } from './app';

-render(<App />, document.getElementById('root'));
+createRoot(document.getElementById('root')!).render(<App />);
`],
    ['src/app.tsx', 'M', `
@ 38 38 export function App() {
-  const ref = useRef();
+  const ref = useRef<HTMLDivElement>(null);
   return <Shell ref={ref} />;
 }
`],
  ],
  'feat/upload-limit': [
    ['src/routes/uploads.ts', 'M', `
@ 6 6
 import { limit } from '../middleware/limit';

-router.post('/uploads', handle);
+router.post('/uploads', limit({ max: 10, window: '1m' }), handle);
`],
    ['tests/uploads.test.ts', 'A', `
@ 0 1
+import { api } from './helpers';
+
+test('returns 429 over the limit', async () => {
+  for (let i = 0; i < 10; i++) await api.post('/uploads');
+  expect((await api.post('/uploads')).status).toBe(429);
+});
`],
  ],
  'perf/invoice-cache': [
    ['src/invoices/list.ts', 'M', `
@ 14 14 export async function listInvoices(user: User) {
-  return db.invoices.where({ user: user.id });
+  return cache.wrap(\`invoices:\${user.id}\`, () => db.invoices.where({ user: user.id }));
 }
`],
  ],
  'fix/resume-crash': [
    ['src/downloads/queue.ts', 'M', `
@ 52 52 resume(id: string) {
   const task = this.tasks.get(id);
-  task.start();
+  if (!task || task.cancelled) return;
+  task.start();
 }
`],
  ],
};

const COMMITS = [
  { sha: 'c7d21e0', subject: 'Bump design tokens to 2.3', author: 'Mina Park', base: true },
  { sha: '8be4f19', subject: 'Add settings route', author: 'Jon Reyes', base: true },
  { sha: '3a09c5d', subject: 'Move sheets onto the shared portal', author: 'Ada Kim', base: true },
  { sha: '51f7a2c', subject: 'Initial commit', author: 'Dev User', base: true },
];

const states = new Map();
let panel = 'changes';
let lastRail = null;
let split = false;
let draft = false;
let menu = null;
let resetScroll = false;
let toast = null;
let toastId = 0;

const hex = () => Math.random().toString(16).slice(2, 9);
const plural = (n, w) => `${n} ${w}${n === 1 ? '' : 's'}`;
const ext = (p) => p.split('.').pop();

function mkFiles(label) {
  return (SETS[label] ?? []).map(([path, status, src]) => {
    const lines = parseDiff(src);
    return { path, status, lines, add: lines.filter((l) => l.k === 'a').length, del: lines.filter((l) => l.k === 'd').length };
  });
}

function mkPr(r) {
  if (r.pr !== 'open') return null;
  return {
    number: 214, title: r.title, base: 'main', state: 'open', folded: false, method: 'Squash and merge', fixing: 0, deleteAfter: false, kept: false,
    checks: [['e2e / checkout', 'failed', true], ['unit', 'passed'], ['lint', 'passed'], ['build', 'passed']],
    review: 'required', requested: ['mina'], approvedBy: [],
    threads: [{ id: 1, path: 'src/app.tsx', line: 42, author: 'mina', body: 'createRoot should be wrapped in StrictMode here.', state: 'open' }],
  };
}

const stateOf = (r) => {
  if (!states.has(r.id)) {
    const files = mkFiles(r.label);
    states.set(r.id, {
      files, all: files.map((f) => ({ ...f })), staged: new Set(), msg: '', busy: null, scope: 'unc', against: 'main', folded: new Set(),
      tree: false, shown: null, commits: r.pr ? [{ sha: hex(), subject: 'Migrate render calls to createRoot', author: 'You' }, { sha: hex(), subject: 'Bump react to 19', author: 'You' }] : [], pushed: r.pr ? 2 : 0, pr: mkPr(r), composing: null, createMenu: false,
    });
  }
  return states.get(r.id);
};

const graphOf = (r, s) => {
  const mine = s.commits.map((c, i) => ({ ...c, tips: i === 0 ? [['head', r.label]] : [] }));
  const rest = COMMITS.map((c, i) => ({ ...c, tips: i === 0 ? (mine.length ? [['base', 'main']] : [['head', r.label], ['base', 'main']]) : [] }));
  const list = [...mine, ...rest];
  if (s.pushed) list[s.commits.length - s.pushed].tips.push(['up', `origin/${r.label}`]);
  return list;
};

// ---- Pieces ------------------------------------------------------------------------------------------------

const fileIco = (name) => (FILES[ext(name)] ? AW.fileIcon(name) : `<span class="rv-gen">${ic('file', 14)}</span>`);
const dirIco = (name, open) => {
  try { return AW.sized(AW.folder(name, open), 16); } catch { return `<span class="rv-gen">${ic('folder', 15)}</span>`; }
};
const letterOf = (s) => `<b class="rv-st ${s}">${s}</b>`;
const stat = (add, del) => `<span class="rv-stat"><i class="add">+${add}</i><i class="del">−${del}</i></span>`;
const pop = (id, body, cls = '') => `<div class="rv-pop ${cls}" data-pop="${id}">${body}</div>`;
const mrow = (act, v, icon, label, extra = '') => `<button class="rv-mrow" data-act="${act}" data-v="${v ?? ''}">${ic(icon, 14)}<span>${label}</span>${extra}</button>`;
const checkbox = (on) => `<span class="rv-cb${on ? ' on' : ''}">${on ? ic('check', 11) : ''}</span>`;

function treeItems(files, key, folded) {
  const out = [];
  const walk = (node, depth, prefix) => {
    for (const [name, child] of Object.entries(node.dirs ?? {}).sort()) {
      const id = `${key}:${prefix}${name}`;
      out.push({ dir: name, id, depth, open: !folded.has(id) });
      if (!folded.has(id)) walk(child, depth + 1, `${prefix}${name}/`);
    }
    for (const f of node.files ?? []) out.push({ f, depth });
  };
  const root = {};
  for (const f of files) {
    let n = root;
    const parts = f.path.split('/');
    for (const d of parts.slice(0, -1)) n = (n.dirs ??= {})[d] ??= {};
    (n.files ??= []).push(f);
  }
  walk(root, 0, '');
  return out;
}

function fileRow(s, f, section, depth, compared) {
  const on = s.shown?.path === f.path && s.shown.cmp === compared;
  const name = f.path.split('/').pop();
  const dir = f.path.includes('/') ? f.path.slice(0, f.path.lastIndexOf('/')) : '';
  const acts = compared ? '' : section === 'staged'
    ? `<span class="rv-acts"><button class="rv-act" data-act="stage" data-v="${f.path}" aria-label="Unstage">${ic('minus', 14)}</button></span>`
    : `<span class="rv-acts"><button class="rv-act" data-act="discard" data-v="${f.path}" aria-label="Discard">${ic('discard', 14)}</button><button class="rv-act" data-act="stage" data-v="${f.path}" aria-label="Stage">${ic('plus', 14)}</button></span>`;
  return `<div class="rv-row${on ? ' on' : ''}" style="--d:${depth}" data-act="open" data-v="${f.path}" data-cmp="${compared ? 1 : 0}">
    ${s.tree ? '<i class="rv-cv"></i>' : ''}${fileIco(name)}
    <span class="rv-name"><b>${esc(name)}</b>${!s.tree && dir ? `<small>${esc(dir)}</small>` : ''}</span>${acts}${compared ? stat(f.add, f.del) : ''}${letterOf(f.status)}</div>`;
}

function section(s, key, label, files, compared) {
  const open = !s.folded.has(key);
  const acts = key === 'staged'
    ? `<button class="rv-act" data-act="unstage-all" aria-label="Unstage all">${ic('minus', 14)}</button>`
    : key === 'changes' ? `<button class="rv-act" data-act="discard-all" aria-label="Discard all">${ic('discard', 14)}</button><button class="rv-act" data-act="stage-all" aria-label="Stage all">${ic('plus', 14)}</button>` : '';
  const head = `<div class="rv-sec" data-act="fold" data-v="${key}">${ic(open ? 'chevron-down' : 'chevron-right', 12)}<b>${label}</b><span class="rv-acts">${acts}</span><i class="rv-pill">${files.length}</i></div>`;
  if (!open) return head;
  if (!s.tree) return head + files.map((f) => fileRow(s, f, key, 0, compared)).join('');
  return head + treeItems(files, key, s.folded).map((it) => it.f ? fileRow(s, it.f, key, it.depth, compared)
    : `<div class="rv-row dir" style="--d:${it.depth}" data-act="fold" data-v="${it.id}">${ic(it.open ? 'chevron-down' : 'chevron-right', 12)}${dirIco(it.dir, it.open)}<span class="rv-name"><b>${esc(it.dir)}</b></span></div>`).join('');
}

function list(s) {
  if (s.scope === 'cmp') return s.all.length ? section(s, 'compared', 'Files', s.all, true) : '<div class="rv-empty">No differences with main.</div>';
  if (!s.files.length) return '<div class="rv-empty">No changes.</div>';
  const staged = s.files.filter((f) => s.staged.has(f.path));
  const rest = s.files.filter((f) => !s.staged.has(f.path));
  return (staged.length ? section(s, 'staged', 'Staged Changes', staged) : '') + (rest.length ? section(s, 'changes', 'Changes', rest) : '');
}

const split_ = (id, ready, lead, label, menuHtml) => `<div class="rv-split${ready ? '' : ' off'}" data-id="${id}">
  <button class="main" data-act="${id}">${lead}<span>${label}</span></button>
  <button class="chev" data-menu data-act="menu" data-v="${id}" aria-label="More">${ic('chevron-down', 12)}</button>${menu === id ? pop(id, menuHtml, 'right') : ''}</div>`;

const spin = (n = 13) => D.api.spinner(n);

function commitBox(r, s) {
  const unpushed = s.commits.length - s.pushed;
  const ahead = s.commits.length;
  const noPr = !s.pr;
  const ship = s.files.length ? 'commit' : unpushed > 0 ? 'push' : noPr && ahead > 0 ? 'create' : 'commit';
  const menuRow = (act, icon, label) => mrow(act, '', icon, label);
  const drafts = `<button class="rv-mrow" data-act="draft">${ic('pull-request', 14)}<span>Create PRs as drafts</span>${checkbox(draft)}</button>`;
  if (ship !== 'commit') {
    const line = ship === 'push'
      ? `<div class="rv-ship"><span>${plural(unpushed, 'commit')} to push</span><i>·</i><em>${esc(s.commits[0].subject)}</em></div>`
      : `<div class="rv-ship">${ic('check', 13)}<span>Up to date with origin/${esc(r.label)}</span></div>`;
    const lead = s.busy ? spin(13) : ic(ship === 'push' ? 'arrow-up' : 'pull-request', 14);
    const label = s.busy ?? (ship === 'push' ? `Push ${plural(unpushed, 'commit')}` : 'Create Pull Request');
    const body = (ship === 'push' ? menuRow('push', 'arrow-up', 'Push') + (noPr ? menuRow('push-create', 'pull-request', 'Push and create PR') : '') : menuRow('create', 'pull-request', 'Create pull request')) + '<hr>' + drafts;
    return `<div class="rv-commit">${line}${split_(ship === 'push' ? 'push' : 'create', !s.busy, lead, label, body)}</div>`;
  }
  const none = !s.staged.size;
  const label = s.busy ?? (none && s.files.length ? 'Commit All' : 'Commit');
  const ready = s.files.length && s.msg.trim() && !s.busy;
  const body = menuRow('commit-push', 'arrow-up', 'Commit &amp; Push') + (noPr ? menuRow('commit-ship', 'pull-request', 'Commit, push and create PR') : '') + menuRow('amend', 'compose', 'Amend Last Commit') + '<hr>' + drafts;
  return `<div class="rv-commit">
    <div class="rv-msg"><textarea rows="1" placeholder="Message (⌘↩ to commit)" aria-label="Commit message" data-msg>${esc(s.msg)}</textarea>
      <button class="rv-sparkle${s.files.length ? '' : ' off'}" data-act="write" aria-label="Write a message">${ic('sparkle', 14)}</button></div>
    ${split_('commit', ready, s.busy ? spin(13) : ic('check', 14), label, body)}</div>`;
}

function graph(r, s) {
  const rows = graphOf(r, s);
  const head = `<div class="rv-ghead">${ic('chevron-down', 12)}<b>Graph</b></div>`;
  const body = rows.map((c, i) => {
    const tone = c.tips.some((t) => t[0] === 'head') || !c.base ? 'head' : 'base';
    const pills = c.tips.map(([kind, name], k) => `<span class="rv-gp k-${kind}">${ic(kind === 'up' ? 'cloud' : 'branch', kind === 'up' ? 14 : 12)}${k === 0 ? `<em>${esc(name)}</em>` : ''}</span>`).join('');
    return `<div class="rv-gr${i === 0 ? ' tip' : ''}"><span class="rv-gl ${tone}${i === 0 ? ' first' : ''}${i === rows.length - 1 ? ' last' : ''}"><i></i></span>
      <span class="rv-gt"><b>${esc(c.subject)}</b><small>${esc(c.author ?? 'You')}</small></span>${pills ? `<span class="rv-gps">${pills}</span>` : ''}</div>`;
  }).join('');
  return `<div class="rv-graph">${head}<div class="rv-gbody">${body}</div></div>`;
}

function changesPanel(api, r, s) {
  const pr = s.pr;
  const head = `<div class="rv-chead"><b>Changes</b><span class="br">${ic('branch', 12)}<span>${esc(r.local ? 'main' : r.label)}</span></span>
    ${pr ? `<button class="rv-prchip" data-act="panel" data-v="prs" style="color:${verdict(pr).c}">#${pr.number}</button>` : ''}
    <button class="rv-ib" data-act="tree" aria-label="View">${ic(s.tree ? 'list-flat' : 'list-tree', 14)}</button>
    <span class="rv-rel"><button class="rv-ib${menu === 'more' ? ' on' : ''}" data-menu data-act="menu" data-v="more" aria-label="More">${ic('more', 14)}</button>${menu === 'more' ? pop('more', moreMenu(r, s)) : ''}</span></div>`;
  const cmp = s.scope === 'cmp';
  const total = cmp && s.all.length ? stat(s.all.reduce((a, f) => a + f.add, 0), s.all.reduce((a, f) => a + f.del, 0)) : '';
  const bar = `<div class="rv-scope"><span class="rv-rel"><button class="rv-sw${menu === 'scope' ? ' on' : ''}" data-menu data-act="menu" data-v="scope"><span>${cmp ? `vs ${s.against}` : 'Uncommitted'}</span>${ic('chevron-down', 10)}</button>${menu === 'scope' ? scopeMenu(r, s) : ''}</span>${total}</div>`;
  return `<div class="panel rv-panel"><div class="head"><span class="t">${esc(r.p.name)}</span><button class="ib" aria-label="New file">${ic('plus', 15)}</button></div>
    ${head}${bar}${cmp ? '' : commitBox(r, s)}
    <div class="rv-split2"><div class="rv-list">${list(s)}</div>${graph(r, s)}</div></div>`;
}

function moreMenu(r, s) {
  const unpushed = s.commits.length - s.pushed;
  return `${mrow('push', '', 'arrow-up', 'Push')}${s.pr ? mrow('panel', 'prs', 'external', 'Open pull request') : s.commits.length ? mrow('create', '', 'plus', 'Create pull request') : ''}
    <hr><div class="rv-info">${esc(r.label)} → main</div><div class="rv-info">${s.commits.length} ahead · 0 behind${unpushed ? '' : ''}</div>`;
}

function scopeMenu(r, s) {
  const others = D.api.all().filter((x) => !x.local && x.p === r.p && x !== r).map((x) => x.label);
  const row = (name, meta = '') => `<button class="rv-pick" data-act="compare" data-v="${name}"><span class="rv-bi">${ic('branch', 13)}</span><b>${name}</b>${meta ? `<i>${meta}</i>` : ''}<span class="ck">${s.scope === 'cmp' && s.against === name ? ic('check', 14) : ''}</span></button>`;
  return pop('scope', `<div class="rv-p5"><button class="rv-mrow pick" data-act="uncommitted"><span class="rv-bi">${ic('list-flat', 13)}</span><span>Uncommitted</span><span class="ck">${s.scope === 'unc' ? ic('check', 14) : ''}</span></button></div>
    <div class="rv-search">${ic('search', 15)}<input placeholder="Search" data-branch aria-label="Search branches"></div>
    <div class="rv-p5 refs" data-refs><div class="rv-ph">Branches</div>${row('main', 'base')}${others.map((o) => row(o)).join('')}</div>`, 'wide');
}

// ---- Pull request ------------------------------------------------------------------------------------------

function gate(pr) {
  const failed = pr.checks.filter((c) => c[1] === 'failed').length;
  const pending = pr.checks.filter((c) => c[1] === 'pending').length;
  if (pr.state === 'merged') return ['done', ''];
  if (failed) return ['failed', "Can't merge until checks pass"];
  if (pending) return ['running', 'Checks in progress'];
  if (pr.review === 'required') return ['review', 'Needs an approving review'];
  return ['ready', `No conflicts with ${pr.base}`];
}

function verdict(pr) {
  const failed = pr.checks.filter((c) => c[1] === 'failed').length;
  const pending = pr.checks.filter((c) => c[1] === 'pending').length;
  if (pr.state === 'merged') return { c: 'var(--merged)', t: `Merged into ${pr.base}` };
  if (pr.fixing) return { c: 'var(--waiting-text)', t: `Claude Code is fixing ${plural(pr.fixing, 'item')}` };
  const [g, text] = gate(pr);
  if (g === 'ready') return { c: 'var(--success-text)', t: 'Ready to merge' };
  if (g === 'failed') return { c: 'var(--failed-text)', t: `${plural(failed, 'check')} failed` };
  if (g === 'running') return { c: 'var(--waiting-text)', t: `${plural(pending, 'check')} in progress` };
  return { c: 'var(--t3)', t: text };
}

const small = (act, v, label, cls = '') => `<button class="rv-small ${cls}" data-act="${act}" data-v="${v}">${label}</button>`;
const slot = (html) => `<span class="rv-slot">${html}</span>`;
const section2 = (title, status) => `<div class="rv-s2"><b>${title}</b>${status ? `<span style="color:${status[0]}">${status[1]}</span>` : ''}</div>`;
const group = (rows) => `<div class="rv-group">${rows.map((r) => `<div class="rv-grow">${r}</div>`).join('')}</div>`;

function prCard(r, s) {
  const pr = s.pr;
  const v = verdict(pr);
  const open = pr.state === 'open';
  const merged = pr.state === 'merged';
  const unfolds = open || (merged && !pr.kept);
  const folded = !unfolds || pr.folded;
  const head = `<div class="rv-phead${unfolds ? ' click' : ''}" ${unfolds ? 'data-act="pr-fold"' : ''}>
    <span class="ico" style="color:${v.c}">${ic(merged ? 'merge' : 'pull-request', 16)}</span>
    <div class="tt"><b class="${folded ? 'one' : ''}">${esc(pr.title)}</b><span><code style="color:${v.c}">#${pr.number}</code> · ${pr.fixing ? spin(11) : ''}${esc(v.t)}</span></div>
    <span class="end"><button class="rv-ib sm" data-act="noop" aria-label="Open on GitHub">${ic('external', 14)}</button>${unfolds ? `<span class="rv-slot lg">${ic(folded ? 'chevron-right' : 'chevron-down', 13)}</span>` : ''}</span></div>`;
  if (folded) return `<div class="rv-pwrap"><div class="rv-pcard">${head}</div></div>`;
  if (merged) {
    return `<div class="rv-pwrap"><div class="rv-pcard">${head}<div class="rv-pmerged"><p>Delete the worktree ${esc(r.label)}? Its terminals close; the branch stays.</p>
      <div class="two"><button class="rv-small sec big" data-act="keep">Keep</button><button class="rv-small pri big" data-act="delete-tree">${ic('trash', 14)}Delete worktree</button></div></div></div></div>`;
  }
  const failed = pr.checks.filter((c) => c[1] === 'failed');
  const unresolved = pr.threads.filter((t) => t.state !== 'resolved');
  const toFixChecks = pr.fixing ? [] : failed;
  const toFixThreads = pr.fixing ? [] : unresolved.filter((t) => t.state === 'open');
  const n = toFixChecks.length + toFixThreads.length;
  let banner = '';
  if (pr.fixing) banner = `<div class="rv-banner wait">${spin(13)}<div><b>Claude Code is fixing ${plural(pr.fixing, 'item')}</b><span>The checks run again once it pushes</span></div></div>`;
  else if (n) banner = `<div class="rv-banner bad">${ic('warning', 15)}<div><b>${plural(n, 'thing')} to fix</b><span>${[toFixChecks.length && plural(toFixChecks.length, 'failed check'), toFixThreads.length && plural(toFixThreads.length, 'review comment')].filter(Boolean).join(' · ')}</span></div>${small('send-all', '', 'Send all', 'pri')}</div>`;
  const cs = pr.checks;
  const nPass = cs.filter((c) => c[1] === 'passed').length;
  const nFail = failed.length;
  const nPend = cs.filter((c) => c[1] === 'pending').length;
  const cstat = nFail ? ['var(--failed-text)', [nFail && `${nFail} failed`, nPass && `${nPass} passed`].filter(Boolean).join(' · ')] : nPend ? ['var(--waiting-text)', `${plural(nPend, 'check')} in progress`] : ['var(--success-text)', 'All passed'];
  const mark = '<span class="rv-fixing"><i></i>Fixing</span>';
  const checkRows = [
    ...cs.filter((c) => c[1] === 'failed').map((c, i) => `${slot(ic('x', 13))}<span class="nm bad">${esc(c[0])}</span><span class="trail">${pr.fixing && c[2] ? mark : pr.fixing ? '' : small('log', i, 'Log', 'ghost') + small('send-check', c[0], 'Send', 'sec')}</span>`),
    ...cs.filter((c) => c[1] === 'pending').map((c) => `${slot(spin(11))}<span class="nm">${esc(c[0])}</span><code>in progress</code>`),
    ...(nPass ? [`${slot(`<span class="ok">${ic('check', 13)}</span>`)}<span class="nm">${nPass} passed</span>`] : []),
  ];
  const reviewRows = [];
  for (const who of pr.approvedBy) reviewRows.push(`${slot(`<span class="ok">${ic('check', 13)}</span>`)}<span class="nm">${who} approved</span>`);
  if (pr.review === 'required' && pr.requested.length) reviewRows.push(`${slot(ic('clock', 13))}<span class="nm">Review requested from ${pr.requested.join(', ')}</span>`);
  if (!reviewRows.length) reviewRows.push(`${slot('<i class="rv-dot"></i>')}<span class="nm">No review yet</span>`);
  for (const t of unresolved) {
    const chip = t.state === 'sent' ? mark : '';
    reviewRows.push(`<div class="rv-cm"><span class="av">${t.author.slice(0, 2).toUpperCase()}</span><div><span class="at"><span>${esc(t.path.split('/').pop())}:${t.line}</span>${chip}</span><span class="body">${esc(t.body)}</span></div>${t.state === 'open' && !pr.fixing ? small('send-thread', t.id, 'Send', 'sec') : ''}</div>`);
  }
  if (pr.threads.length) reviewRows.push(`${slot(ic('comment', 13))}<span class="nm all">All comments</span>${unresolved.length ? `<code>${unresolved.length} unresolved</code>` : ''}${ic('chevron-right', 12)}`);
  const [g, gtext] = gate(pr);
  const gl = g === 'ready' ? `${slot(`<span class="ok">${ic('check', 13)}</span>`)}` : g === 'running' ? slot(spin(12)) : slot('<i class="rv-dot"></i>');
  const methods = ['Squash and merge', 'Create a merge commit', 'Rebase and merge'];
  const mm = methods.map((m) => `<button class="rv-mrow" data-act="method" data-v="${m}">${ic('merge', 14)}<span>${m}</span>${pr.method === m ? ic('check', 13) : ''}</button>`).join('');
  const merge = `<div class="rv-mg"><div class="rv-gate ${g}">${gl}<span>${esc(pr.fixing ? 'Claude Code is resolving the checks' : gtext)}</span></div>
    ${split_('merge', g === 'ready' && !s.busy, s.busy ? spin(13) : ic('merge', 14), s.busy ?? pr.method, mm)}
    <button class="rv-del" data-act="delete-after">${checkbox(pr.deleteAfter)}<span>Delete the worktree after merging<small>Keeps the branch</small></span></button></div>`;
  return `<div class="rv-pwrap"><div class="rv-pcard">${head}${banner}</div>
    ${section2('CHECKS', cstat)}${group(checkRows)}${section2('REVIEW', pr.approvedBy.length ? ['var(--success-text)', 'Approved'] : pr.review === 'required' ? ['var(--t3)', 'Review required'] : null)}${group(reviewRows)}${section2('MERGE')}${group([merge])}</div>`;
}

function composer(r, s) {
  const c = s.composing;
  const busy = s.busy;
  const ready = c.title.trim() && !busy;
  const lead = busy ? spin(13) : ic('pull-request', 14);
  const label = busy ?? (draft ? 'Create draft PR' : 'Create PR');
  const body = [[false, 'Create pull request'], [true, 'Create draft pull request']].map(([d, name]) => `<button class="rv-mrow" data-act="draft-kind" data-v="${d}">${ic('pull-request', 14)}<span>${name}</span>${d === draft ? ic('check', 13) : ''}</button>`).join('');
  return `<div class="rv-comp">
    <div class="top">${ic('pull-request', 13)}<b>${esc(r.label)}</b>${ic('arrow-right', 11)}<span>main</span></div>
    <label><i>Title</i><input data-pr="title" value="${esc(c.title)}" aria-label="Title"></label>
    <label><i>Description</i><textarea rows="4" data-pr="body" aria-label="Description">${esc(c.body)}</textarea></label>
    <div class="note">${ic('sparkle', 12)}<span>Written by Claude Code from ${plural(s.commits.length, 'commit')} ·</span><button class="lnk" data-act="regen">Regenerate</button></div>
    <div class="btns"><button class="rv-small sec big" data-act="cancel-pr">Cancel</button>${split_('create-pr', ready, lead, label, body)}</div></div>`;
}

function prPanel(api, r, s) {
  const head = `<div class="head"><span class="t">${esc(r.p.name)}</span><button class="ib" aria-label="New file">${ic('plus', 15)}</button></div><div class="rv-ptitle">Pull request</div>`;
  let body;
  if (s.composing) body = `<div class="rv-pscroll">${composer(r, s)}</div>`;
  else if (s.pr) body = `<div class="rv-pscroll">${prCard(r, s)}</div>`;
  else if (r.local) body = '<div class="rv-empty gray">Run gh auth login to see PRs</div>';
  else if (s.commits.length) {
    const unpushed = s.commits.length - s.pushed;
    body = `<div class="rv-nopr"><span class="bd">${ic('pull-request', 18)}</span><b>No pull request yet</b><span class="mn">${esc(r.label)}${ic('arrow-right', 10)}main</span><span class="sm">${plural(s.commits.length, 'commit')} ahead</span>
      ${unpushed ? '<span class="sm">Push the branch to open one.</span>' : `<button class="rv-small pri big" data-act="create">${ic('pull-request', 14)}Create pull request</button>`}</div>`;
  } else body = '<div class="rv-empty">No pull request.</div>';
  return `<div class="panel rv-panel">${head}${body}</div>`;
}

// ---- Files panel -------------------------------------------------------------------------------------------

const TREE = [
  { name: 'components', kids: [['Button.tsx'], ['Sheet.tsx', 'M']] },
  { name: 'hooks', kids: [['useTheme.ts', 'A']] },
  { name: 'src', kids: [['app.tsx'], ['settings.ts', 'M'], ['store.ts']] },
  ['README.md'],
];
const opened = new Set();

function explorer(api, r, s) {
  const changed = new Map(s.files.map((f) => [f.path, f.status]));
  const file = ([name, git], depth, dir) => {
    const status = changed.get(dir ? `${dir}/${name}` : name) ?? (!s.files.length ? undefined : git && !r.local ? undefined : undefined);
    return `<div class="rv-row ex${status ? ` ${status}` : ''}" style="--d:${depth}" ${status ? `data-act="open" data-v="${dir ? `${dir}/` : ''}${name}" data-cmp="0"` : ''}><i class="rv-cv"></i>${fileIco(name)}<span class="rv-name"><b>${name}</b></span>${status ? letterOf(status) : ''}</div>`;
  };
  const tree = TREE.map((n) => n.kids
    ? `<div class="rv-row dir" data-act="dir" data-v="${n.name}">${ic(opened.has(n.name) ? 'chevron-down' : 'chevron-right', 12)}${dirIco(n.name, opened.has(n.name))}<span class="rv-name"><b>${n.name}</b></span></div>${opened.has(n.name) ? n.kids.map((k) => file(k, 1, n.name)).join('') : ''}`
    : file(n, 0, '')).join('');
  return `<div class="panel rv-panel"><div class="head"><span class="t">${esc(r.p.name)}</span><button class="ib" aria-label="New file">${ic('plus', 15)}</button></div>
    <div class="rv-goto">${ic('search', 15)}<span>Go to file…</span><kbd>⌘P</kbd></div><div class="rv-list ex">${tree}</div></div>`;
}

// ---- Main pane ---------------------------------------------------------------------------------------------

function diffView(r, s) {
  const f = (s.shown.cmp ? s.all : s.files).find((x) => x.path === s.shown.path) ?? s.all.find((x) => x.path === s.shown.path);
  const dir = f.path.includes('/') ? f.path.slice(0, f.path.lastIndexOf('/')).split('/') : [];
  const crumbs = [s.shown.cmp ? 'Compare' : 'Changes', ...dir, f.path.split('/').pop()];
  const meta = [
    s.shown.cmp ? `against <b>${esc(s.against)}</b>` : '',
    `<span class="mono">${stat(f.add, f.del)}</span>`,
    !s.shown.cmp && s.files.includes(f) ? `${ic('claude', 13)}by <b>Claude Code · 3 min ago</b>` : '',
  ].filter(Boolean);
  const bar = `<div class="rv-docbar"><div class="rv-crumbs">${crumbs.map((c, i) => `${i ? '<i>/</i>' : ''}<span class="${i === crumbs.length - 1 ? 'last' : ''}">${esc(c)}</span>`).join('')}</div>
    <div class="rv-meta">${meta.map((m, i) => `${i ? '<i class="sep"></i>' : ''}<span>${m}</span>`).join('')}</div>
    <div class="rv-seg"><button class="${split ? '' : 'on'}" data-act="mode" data-v="0">Unified</button><button class="${split ? 'on' : ''}" data-act="mode" data-v="1">Split</button></div>
    <div class="rv-grp"><button class="rv-ib" aria-label="More">${ic('more', 16)}</button></div></div>`;
  const e = ext(f.path);
  const num = (n) => `<i class="nm">${n ?? ''}</i>`;
  const fold = (l) => `<div class="dh">${ic('unfold', 11)}${l.hidden ? `<span class="hn">${l.hidden} unchanged lines</span>` : ''}<span class="hc">${esc(l.ctx)}</span></div>`;
  const cell = (l, both) => `<div class="dc ${l.k}">${both ? num(l.o) + num(l.n) : num(l.k === 'd' ? l.o : l.n)}<i class="sg">${l.k === 'a' ? '+' : l.k === 'd' ? '-' : ''}</i><span class="tx">${code(l.text, l.w)}</span></div>`;
  let rows = '';
  if (!split) rows = f.lines.map((l) => (l.k === 'h' ? fold(l) : `<div class="dl">${cell(l, true)}</div>`)).join('');
  else {
    for (let i = 0; i < f.lines.length; i++) {
      const l = f.lines[i];
      if (l.k === 'h') rows += fold(l);
      else if (l.k === 'c') rows += `<div class="dl">${cell(l, false)}${cell(l, false)}</div>`;
      else {
        const ds = [];
        const as = [];
        while (f.lines[i]?.k === 'd') ds.push(f.lines[i++]);
        while (f.lines[i]?.k === 'a') as.push(f.lines[i++]);
        i--;
        for (let j = 0; j < Math.max(ds.length, as.length); j++) rows += `<div class="dl">${ds[j] ? cell(ds[j], false) : '<div class="dc void"></div>'}${as[j] ? cell(as[j], false) : '<div class="dc void"></div>'}</div>`;
      }
    }
  }
  return `${bar}<div class="scroll rv-diff${split ? ' split' : ''}" data-ext="${e}">${rows}<div class="dend"></div></div>`;
}

function chat(api, r) {
  if (!r.chat) return `<div class="scroll chat"><div class="col"></div></div>`;
  const out = [];
  let reply = null;
  for (const [kind, a, b] of r.log) {
    if (kind === 'you') { reply = null; out.push(`<div class="you"><div class="bubble">${esc(a)}</div></div>`); continue; }
    if (!reply) { reply = []; out.push(reply); }
    if (kind === 'say') reply.push(`<div class="md">${api.richText(a)}</div>`);
    else if (kind === 'tool') reply.push(`<div class="tool">${ic('tool', 14)}<b>${a}</b><code>${esc(b)}</code></div>`);
    else reply.push(`<div class="tool ${kind === 'err' ? 'err' : 'out'}">${ic(kind === 'err' ? 'circle-x' : 'corner-down-right', 14)}<span>${esc(a)}</span></div>`);
  }
  const html = out.map((t) => (typeof t === 'string' ? t : `<div class="reply">${t.join('')}</div>`)).join('');
  return `<div class="scroll chat"><div class="col">${html}</div></div>
    <div class="dock"><div class="composer empty"><textarea rows="2" placeholder="Ask, build, / for commands, @ for files" aria-label="Message"></textarea>
    <div class="foot"><button class="chip"><span class="ag claude">${ic('claude', 15)}</span><b>Opus</b><span class="soft">High</span>${ic('chevron-down', 12)}</button><span class="sp"></span><button class="ib">${ic('plus', 17)}</button><button class="send">${ic('arrow-up', 16)}</button></div></div>
    <div class="where">${ic(r.local ? 'laptop' : 'worktree', 13)}<span>${esc(r.label)}</span></div></div>`;
}

function newSession(api, r) {
  return `<div class="pane dots rv-new"><div class="bar bare"><div class="crumbs">${esc(r.p.name)}<i>/</i><b>New session</b></div></div>
    <div class="home"><h2>What should we build next?</h2>
      <div class="composer empty"><textarea rows="2" placeholder="Ask, build, / for commands, @ for files" aria-label="Message"></textarea>
        <div class="foot"><button class="chip"><span class="ag claude">${ic('claude', 15)}</span><b>Claude Code</b>${ic('chevron-down', 12)}</button><button class="chip">${ic('shield', 15)}Agent's settings${ic('chevron-down', 12)}</button><span class="sp"></span><button class="ib">${ic('plus', 17)}</button><button class="send">${ic('arrow-up', 16)}</button></div></div>
      <div class="where-row"><button class="chip ghost"><span class="mini">${api.letter(r.p.name)}</span>${esc(r.p.name)}${ic('chevron-down', 12)}</button><button class="chip ghost">${ic('worktree', 14)}Worktree${ic('chevron-down', 12)}</button><button class="chip ghost mono">${ic('branch', 14)}main${ic('chevron-down', 12)}</button></div></div></div>`;
}

function render(api) {
  const r = api.sel;
  const s = stateOf(r);
  const [icon, name] = AW.AGENTS[r.agent ?? 'claude'];
  const shown = !!s.shown;
  const docTab = shown ? `<div class="tab doc"><span class="ico">${fileIco(s.shown.path.split('/').pop())}</span><b>${esc(s.shown.path.split('/').pop())}</b></div>` : '';
  const sessionTab = r.local ? '' : api.tab(r).replace('<div class="tab">', `<div class="tab${shown ? ' off' : ''}" data-act="close-doc">`);
  const main = shown ? diffView(r, s) : chat(api, r);
  const side = panel === 'changes' ? changesPanel(api, r, s) : panel === 'prs' ? prPanel(api, r, s) : panel === 'explorer' ? explorer(api, r, s) : '';
  const counts = { changes: s.files.length, prs: 0 };
  if (r.local) return `${newSession(api, r)}${side}${api.rail(panel, counts)}`;
  return `<div class="pane rv-main"><div class="bar">${sessionTab}${docTab}<button class="ib" aria-label="New tab">${ic('plus', 16)}</button></div>${main}${toast && Date.now() - toast.at < 3200 ? `<div class="rv-toast" style="animation-delay:${toast.at - Date.now()}ms">${toast.html}</div>` : ''}</div>${side}${api.rail(panel, counts)}`;
}

// ---- Actions -----------------------------------------------------------------------------------------------

const live = (api) => api.state.screen === 'review';
const later = (api, ms, fn) => setTimeout(() => { fn(); if (live(api)) api.draw(); }, ms);
const say = (api, html) => { toast = { id: ++toastId, html, at: Date.now() }; };

function open(api, s, path, cmp) {
  s.shown = { path, cmp: !!cmp };
  resetScroll = true;
  api.draw();
}

function doCommit(api, r, s, then) {
  if (s.busy || !s.files.length) return;
  if (!s.msg.trim()) return api.$('[data-msg]')?.focus();
  const take = s.staged.size ? s.files.filter((f) => s.staged.has(f.path)) : s.files.slice();
  const msg = s.msg.trim().split('\n')[0];
  s.busy = 'Committing…';
  api.draw();
  later(api, 700, () => {
    const gone = new Set(take.map((f) => f.path));
    s.files = s.files.filter((f) => !gone.has(f.path));
    gone.forEach((p) => s.staged.delete(p));
    if (s.shown && !s.shown.cmp && gone.has(s.shown.path)) s.shown = null;
    const sha = hex();
    s.commits.unshift({ sha, subject: msg, author: 'You' });
    s.msg = '';
    s.busy = null;
    say(api, `${ic('check', 13)}<span>Committed <code>${sha}</code></span>`);
    if (then) then();
  });
}

function doPush(api, r, s, then) {
  if (s.busy) return;
  s.busy = 'Pushing…';
  api.draw();
  later(api, 900, () => {
    s.pushed = s.commits.length;
    s.busy = null;
    say(api, `${ic('arrow-up', 13)}<span>Pushed to <code>origin/${esc(r.label)}</code></span>`);
    if (then) then();
  });
}

function openComposer(api, r, s) {
  panel = 'prs';
  menu = null;
  const subs = s.commits.map((c) => c.subject);
  s.composing = { title: subs[subs.length - 1] ?? r.title, body: subs.length > 1 ? subs.slice().reverse().map((x) => `- ${x}`).join('\n') : '' };
}

function startChecks(api, s) {
  later(api, 4200, () => {
    if (!s.pr) return;
    s.pr.checks = s.pr.checks.map((c) => [c[0], 'passed']);
  });
}

function createPr(api, r, s) {
  if (s.busy || !s.composing.title.trim()) return;
  s.busy = 'Creating pull request…';
  api.draw();
  later(api, 1000, () => {
    s.pr = { number: 218, title: s.composing.title.trim(), base: 'main', state: 'open', folded: false, method: 'Squash and merge', fixing: 0, deleteAfter: false, kept: false, checks: [['build', 'pending'], ['unit', 'pending'], ['lint', 'pending']], review: 'none', requested: [], approvedBy: [], threads: [] };
    r.pr = 'open';
    s.composing = null;
    s.busy = null;
    startChecks(api, s);
  });
}

function sendFix(api, s, items, finish) {
  const pr = s.pr;
  pr.fixing = items;
  pr.sent = true;
  api.draw();
  later(api, 3200, () => {
    pr.fixing = 0;
    pr.checks = pr.checks.map((c) => [c[0], 'passed']);
    pr.threads.forEach((t) => { t.state = 'resolved'; });
    pr.review = 'approved';
    pr.approvedBy = pr.requested;
    pr.requested = [];
    if (finish) finish();
  });
}

function act(a, v, el, api, ev) {
  const r = api.sel;
  const s = stateOf(r);
  const toggleMenu = a === 'menu';
  if (!toggleMenu && !el.matches('[data-menu]')) menu = null;
  switch (a) {
    case 'rail':
      lastRail = null;
      if (v === 'sessions') return api.go('inbox');
      panel = panel === v ? null : v;
      menu = null;
      return api.draw();
    case 'panel': panel = v; menu = null; return api.draw();
    case 'menu': menu = menu === v ? null : v; return api.draw();
    case 'noop': return;
    case 'tree': s.tree = !s.tree; return api.draw();
    case 'fold': s.folded.has(v) ? s.folded.delete(v) : s.folded.add(v); return api.draw();
    case 'dir': opened.has(v) ? opened.delete(v) : opened.add(v); return api.draw();
    case 'open': return open(api, s, v, el.dataset.cmp === '1');
    case 'close-doc': if (s.shown) { s.shown = null; return api.draw(); } return;
    case 'mode': split = v === '1'; return api.draw();
    case 'stage': s.staged.has(v) ? s.staged.delete(v) : s.staged.add(v); return api.draw();
    case 'stage-all': s.files.forEach((f) => s.staged.add(f.path)); return api.draw();
    case 'unstage-all': s.staged.clear(); return api.draw();
    case 'discard': {
      s.files = s.files.filter((f) => f.path !== v);
      s.staged.delete(v);
      if (s.shown?.path === v && !s.shown.cmp) s.shown = null;
      return api.draw();
    }
    case 'discard-all': s.files = s.files.filter((f) => s.staged.has(f.path)); s.shown = null; return api.draw();
    case 'uncommitted': s.scope = 'unc'; menu = null; return api.draw();
    case 'compare': s.scope = 'cmp'; s.against = v; menu = null; return api.draw();
    case 'write': {
      if (!s.files.length) return;
      s.msg = s.files.some((f) => f.path === 'hooks/useTheme.ts') ? 'Add a dark mode setting that follows the system' : `Update ${plural(s.files.length, 'file')}`;
      return api.draw();
    }
    case 'commit': return doCommit(api, r, s);
    case 'commit-push': menu = null; return doCommit(api, r, s, () => doPush(api, r, s, () => { api.draw(); }));
    case 'commit-ship': menu = null; return doCommit(api, r, s, () => doPush(api, r, s, () => { openComposer(api, r, s); api.draw(); }));
    case 'amend': {
      menu = null;
      if (s.msg.trim() && s.commits.length) { s.commits[0].subject = s.msg.trim().split('\n')[0]; s.msg = ''; say(api, `${ic('check', 13)}<span>Amended the last commit</span>`); }
      return api.draw();
    }
    case 'push': return doPush(api, r, s);
    case 'push-create': menu = null; return doPush(api, r, s, () => { openComposer(api, r, s); api.draw(); });
    case 'create': menu = null; openComposer(api, r, s); return api.draw();
    case 'draft': draft = !draft; return api.draw();
    case 'draft-kind': draft = v === 'true'; menu = null; return api.draw();
    case 'regen': {
      const subs = s.commits.map((c) => c.subject);
      s.composing.title = subs[0] ?? r.title;
      return api.draw();
    }
    case 'cancel-pr': s.composing = null; return api.draw();
    case 'create-pr': return createPr(api, r, s);
    case 'pr-fold': s.pr.folded = !s.pr.folded; return api.draw();
    case 'method': s.pr.method = v; menu = null; return api.draw();
    case 'delete-after': s.pr.deleteAfter = !s.pr.deleteAfter; return api.draw();
    case 'send-all': return sendFix(api, s, s.pr.checks.filter((c) => c[1] === 'failed').length + s.pr.threads.filter((t) => t.state === 'open').length, () => api.draw());
    case 'send-check': s.pr.checks.forEach((c) => { if (c[0] === v) c[2] = true; }); return sendFix(api, s, 1, null);
    case 'send-thread': s.pr.threads.forEach((t) => { if (t.id === +v) t.state = 'sent'; }); return sendFix(api, s, 1, null);
    case 'merge': {
      if (s.busy || gate(s.pr)[0] !== 'ready') return;
      s.busy = 'Merging…';
      api.draw();
      return later(api, 1100, () => { s.pr.state = 'merged'; s.busy = null; s.pr.folded = false; });
    }
    case 'keep': s.pr.kept = true; return api.draw();
    case 'delete-tree': {
      const p = r.p;
      p.rows.splice(p.rows.indexOf(r), 1);
      api.state.sel = p.rows[0] ?? api.PROJECTS[0].rows[0];
      return api.go('sessions');
    }
    case 'log': return;
  }
}

D.register('review', {
  sessionScreen: true,
  tour: 'review',
  keepScroll: true,
  enter(api) {
    // Chosen from the feature list, not from a rail button: the dark-mode session starts over, so the tour can play
    // again. Found by branch, since Board adds sessions ahead of it.
    if (!lastRail) {
      const first = api.all().find((r) => r.label === 'feat/dark-mode');
      states.delete(first.id);
      delete first.pr;
      api.state.sel = first;
      panel = 'changes';
      split = false;
      draft = false;
      toast = null;
    }
    if (lastRail === 'prs') panel = 'prs';
    else if (lastRail === 'changes' || !panel) panel = 'changes';
    lastRail = null;
    menu = null;
    const r = api.sel;
    const s = stateOf(r);
    if (panel === 'changes' && !s.shown && s.files.length) { s.shown = { path: s.files[0].path, cmp: false }; resetScroll = true; }
  },
  render,
  mount(root) {
    if (resetScroll) {
      const d = root.querySelector('.rv-diff');
      if (d) d.scrollTop = 0;
      resetScroll = false;
    }
    const chatEl = root.querySelector('.chat');
    if (chatEl) chatEl.scrollTop = chatEl.scrollHeight;
    if (!root._rv) {
      root._rv = true;
      root.addEventListener('click', (e) => {
        if (!live(D.api) || !menu) return;
        if (e.target.closest('.rv-pop, [data-menu]')) return;
        menu = null;
        D.api.draw();
      });
    }
  },
  act,
  input(e, api) {
    const s = stateOf(api.sel);
    const t = e.target;
    if (t.matches('[data-msg]')) {
      s.msg = t.value;
      const ready = s.files.length && s.msg.trim() && !s.busy;
      api.$('.rv-split[data-id="commit"]')?.classList.toggle('off', !ready);
    } else if (t.matches('[data-pr]')) {
      s.composing[t.dataset.pr] = t.value;
      api.$('.rv-split[data-id="create-pr"]')?.classList.toggle('off', !s.composing.title.trim());
    } else if (t.matches('[data-branch]')) {
      const q = t.value.trim().toLowerCase();
      api.$('[data-refs]')?.querySelectorAll('.rv-pick').forEach((b) => { b.hidden = q && !b.dataset.v.toLowerCase().includes(q); });
    }
  },
  key(e, api) {
    if (e.key === 'Escape' && menu) { menu = null; return api.draw(); }
    if (e.target.matches('[data-msg]') && e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); act('commit', '', e.target, api, e); }
  },
});

AnywhereTour.define('review', [
  ['wait', 500],
  ['click', '[data-act="close-doc"]'],
  ['wait', 600],
  ['click', '.rv-row[data-v="hooks/useTheme.ts"]'],
  ['wait', 1500],
  ['click', '[data-act="stage-all"]'],
  ['wait', 600],
  ['click', '[data-act="write"]'],
  ['wait', 1000],
  ['click', '.rv-split[data-id="commit"] .chev'],
  ['wait', 700],
  ['click', '[data-act="commit-ship"]'],
  ['wait', 2300],
  ['click', '[data-act="create-pr"]'],
  ['wait', 4400],
]);

// A rail click on the sessions screen only says "review"; the last rail button pressed says which panel.
document.addEventListener('click', (e) => {
  const b = e.target.closest?.('.app .rail [data-act="rail"]');
  if (b) lastRail = b.dataset.v;
}, true);
// The rail's Changes count everywhere a session shows, so it agrees with this panel.
window.AnywhereDesktop.api.changeCount = (r) => stateOf(r).files.length;
})();
