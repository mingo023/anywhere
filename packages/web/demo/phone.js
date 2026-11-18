// A working copy of the iPhone app, drawn at its 393×852pt screen size and scaled to fit its frame.
// Four screens (sessions, chat, diff, pairing) share one store, so approving a command in the chat also moves the
// session in the list. Layout, type sizes and colours follow the app's own renders (DesignSystem/Palette.swift).

(() => {
const figure = document.querySelector('figure.phone');
if (!figure) return;
const { ic, esc, fileIcon } = AW;
const still = matchMedia('(prefers-reduced-motion: reduce)').matches;

// Demo data only: never the names of real projects. It mirrors the desktop demo's projects and sessions.
const MAC = "Ada's MacBook Pro";
const SESSIONS = [
  { id: 'dark', title: 'Add dark mode to settings', project: 'orbit-web', tree: 'dark-mode', status: 'working', model: 'Sonnet 5', ago: '1m', detail: 'Reading src/theme/tokens.ts',
    log: [['you', 'Add a dark mode toggle to the settings screen'], ['say', "I'll add a theme setting and wire it to the existing tokens."], ['tool', 'Read', 'src/settings.ts'], ['tool', 'Read', 'src/theme/tokens.ts']] },
  { id: 'flaky', title: 'Fix flaky checkout test', project: 'orbit-web', tree: 'flaky-checkout', status: 'needs', model: 'Opus 5.5', ago: '2m', inline: true,
    log: [['you', 'The checkout test fails about one run in five. Find out why.'], ['tool', 'Read', 'tests/checkout.spec.ts'], ['say', 'It waits a fixed 500ms for the cart total. I want to run it 20 times to confirm.']],
    ask: { cmd: 'pnpm test checkout --repeat 20' },
    after: [['tool', 'Bash', 'pnpm test checkout --repeat 20'], ['err', '4 failed, 16 passed'], ['tool', 'Update', 'tests/checkout.spec.ts'], ['out', 'Wait for the total to render instead of sleeping'], ['tool', 'Bash', 'pnpm test checkout --repeat 20'], ['out', '20 passed'], ['say', 'Fixed. The test now waits for the cart total instead of a timer.']] },
  { id: 'react', title: 'Migrate to React 19', project: 'orbit-web', tree: 'react-19', status: 'idle', model: 'Opus 5.5', ago: '14m', pr: ['#214', 'open'], worked: '1m 4s',
    log: [['you', 'Upgrade us to React 19'], ['tool', 'Bash', 'pnpm up react@19 react-dom@19'], ['tool', 'Update', 'src/app.tsx'], ['out', 'Replaced ReactDOM.render with createRoot'], ['tool', 'Bash', 'gh pr create --fill'], ['say', 'Opened PR #214: Migrate to React 19.']] },
  { id: 'upload', title: 'Rate-limit the upload route', project: 'fox-api', tree: 'upload-limit', status: 'done', model: 'Opus 5.5', ago: '5m', worked: '23s',
    log: [['you', 'Rate-limit POST /uploads to 10 a minute per user'], ['tool', 'Update', 'src/routes/uploads.ts'], ['out', 'Updated with 21 additions'], ['tool', 'Bash', 'pnpm test uploads'], ['out', '9 passed'], ['say', 'Done. Over the limit now returns 429 with Retry-After.']] },
  { id: 'invoice', title: 'Cache invoice queries', project: 'fox-api', tree: 'invoice-cache', status: 'failed', agent: 'codex', model: 'GPT-5', ago: '9m', worked: '15s',
    log: [['you', 'Cache the invoice list queries'], ['tool', 'Update', 'src/invoices/list.ts'], ['tool', 'Bash', 'pnpm test invoices'], ['err', '2 failed: cached totals are stale after a refund']] },
  { id: 'pg', title: 'Bump the Postgres driver', project: 'fox-api', tree: 'pg-bump', status: 'idle', model: 'Sonnet 5', ago: '3h', pr: ['#88', 'merged'], worked: '41s',
    log: [['you', 'Bump pg to the latest minor'], ['tool', 'Bash', 'pnpm up pg'], ['tool', 'Bash', 'pnpm test'], ['out', '131 passed'], ['say', 'PR #88 is merged.']] },
  { id: 'crash', title: 'Crash when resuming a download', project: 'garden-mobile', tree: 'resume-crash', status: 'needs', model: 'Opus 5.5', ago: '6m',
    log: [['you', 'The app crashes when a paused download resumes'], ['tool', 'Read', 'src/downloads/queue.ts'], ['say', 'The resume handler reads a task that was already freed. I want to reproduce it with the queue tests.']],
    ask: { cmd: 'pnpm test queue --runInBand' },
    after: [['tool', 'Bash', 'pnpm test queue --runInBand'], ['err', '1 failed: resume after cancel'], ['tool', 'Update', 'src/downloads/queue.ts'], ['out', 'Drop the task from the queue before freeing it'], ['tool', 'Bash', 'pnpm test queue --runInBand'], ['out', '17 passed'], ['say', 'Fixed. Resuming a cancelled download no longer touches the freed task.']] },
  { id: 'haptics', title: 'Haptics on long press', project: 'garden-mobile', tree: 'haptics', status: 'idle', model: 'Sonnet 5', ago: '1d', worked: '12s',
    log: [['you', 'Add a light haptic when you long-press a plant'], ['tool', 'Update', 'components/PlantCard.tsx'], ['say', 'Added. Want the same on the watering button?']] },
];
const BY = Object.fromEntries(SESSIONS.map((s) => [s.id, s]));
const PROJECTS = [
  { name: 'orbit-web', open: true, trees: [['local', 'Local', 'main', []], ['wt', 'dark-mode', null, ['dark']], ['wt', 'flaky-checkout', null, ['flaky']], ['wt', 'react-19', null, ['react']]] },
  { name: 'fox-api', open: true, trees: [['local', 'Local', 'main', []], ['wt', 'upload-limit', null, ['upload']], ['wt', 'invoice-cache', null, ['invoice']], ['wt', 'pg-bump', null, ['pg']]] },
  { name: 'garden-mobile', open: false, trees: [['local', 'Local', 'main', []], ['wt', 'resume-crash', null, ['crash']], ['wt', 'haptics', null, ['haptics']]] },
];
const AGENT = { claude: ['Claude', 'claude', '#d97757'], codex: ['Codex', 'openai', '#ebebeb'] };

// What the Changes screen lists for the checkout fix. Hunks: ' ' context, '-' removed, '+' added, ⟦…⟧ a changed word.
const BRANCH = { name: 'flaky-checkout', base: 'main', ahead: 2, behind: 1 };
const FILES = [
  { path: 'tests/checkout.spec.ts', st: 'M', staged: true, diff: `@@ -12,9 +12,11 @@ describe('checkout', () => {
   it('shows the cart total', async () => {
     await page.goto('/cart');
-    await page.waitForTimeout(500);
-    const total = ⟦await page.textContent('[data-total]')⟧;
+    const total = ⟦page.locator('[data-total]')⟧;
+    await expect(total).toHaveText('$42.00');
     expect(await page.title()).toContain('Cart');
   });
@@ -31,5 +33,6 @@ describe('checkout', () => {
   it('applies the promo code', async () => {
-    await page.waitForTimeout(500);
+    await page.getByLabel('Promo').fill('SAVE10');
+    await expect(page.locator('[data-total]')).toHaveText('$37.80');
     // The total repaints after the code is accepted.
   });` },
  { path: 'src/cart/useCartTotal.ts', st: 'M', staged: true, diff: `@@ -8,7 +8,9 @@ export function useCartTotal(cart: Cart) {
   const [total, setTotal] = useState(0);
   useEffect(() => {
-    setTotal(sum(cart.items));
+    const next = sum(cart.items);
+    setTotal(next);
+    cart.onSettled?.(next);
   }, [cart.items]);
   return total;
 }` },
  { path: 'src/cart/CartTotal.tsx', st: 'M', staged: false, diff: `@@ -3,6 +3,7 @@ import { useCartTotal } from './useCartTotal';
 export function CartTotal({ cart }: Props) {
   const total = useCartTotal(cart);
-  return <b>{total}</b>;
+  return <b data-total>{format(total)}</b>;
 }` },
  { path: 'tests/helpers/waitForTotal.ts', st: 'A', staged: false, diff: `@@ -0,0 +1,6 @@
+import { expect, Page } from '@playwright/test';
+
+export async function waitForTotal(page: Page, text: string) {
+  await expect(page.locator('[data-total]')).toHaveText(text);
+}` },
  { path: 'package.json', st: 'M', staged: false, diff: `@@ -9,7 +9,7 @@
   "scripts": {
-    "test": "vitest run",
+    "test": "vitest run --retry 0",
     "build": "tsc -b"
   }` },
  { path: 'docs/flaky-tests.md', st: 'D', staged: false, diff: `@@ -1,4 +0,0 @@
-# Flaky tests
-
-The checkout spec fails about one run in five. Re-run it.` },
];

const state = { stack: [], pair: { addr: '', code: '' }, pairing: false, nav: 0 };
const later = (fn, ms) => setTimeout(fn, ms);
const fmt = (sec) => (sec < 60 ? `${sec}s` : `${Math.floor(sec / 60)}m ${sec % 60}s`);
const agentOf = (s) => AGENT[s.agent ?? 'claude'];
const place = (s) => `${s.project} · ${s.tree}`;

SESSIONS.forEach((s) => Object.assign(s, { base: s.status, baseAgo: s.ago, baseDetail: s.detail }));
function init(s) {
  Object.assign(s, {
    status: s.base, ago: s.baseAgo, detail: s.baseDetail, fold: false, dockFresh: false, finishedWorked: null, gen: (s.gen ?? 0) + 1,
    since: s.base === 'working' ? Date.now() - 72000 : 0,
    items: s.log.map((l) => ({ k: l[0], v: l[1], a: l[2] })),
  });
}
SESSIONS.forEach(init);

// Icons the shared set lacks.
const chevL = (n = 20) => `<svg width="${n}" height="${n}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>`;
const chevR = (n = 10) => `<svg width="${n}" height="${n}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>`;
const chevD = (n = 10, w = 3) => `<svg class="ch" width="${n}" height="${n}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>`;
const chevU = (n = 18) => `<svg width="${n}" height="${n}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6"/></svg>`;
const lap = (n) => `<svg width="${n}" height="${n * 0.66}" viewBox="0 0 74 48" fill="none"><rect x="12" y="3" width="50" height="34" rx="5" fill="#3a3a3a" stroke="currentColor" stroke-width="4"/><path d="M2 42h70" stroke="currentColor" stroke-width="5" stroke-linecap="round"/></svg>`;
const qr = (n = 18) => `<svg width="${n}" height="${n}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M3 8V5a2 2 0 0 1 2-2h3M16 3h3a2 2 0 0 1 2 2v3M21 16v3a2 2 0 0 1-2 2h-3M8 21H5a2 2 0 0 1-2-2v-3"/><path d="M9 9h2v2H9zM13 9h2v2h-2zM9 13h2v2H9zM14 14h1v1h-1z" stroke-width="1.4"/></svg>`;
const devices = `<svg width="46" height="34" viewBox="0 0 46 34" fill="currentColor"><path d="M4 3.5A2.5 2.5 0 0 1 6.5 1h22A2.5 2.5 0 0 1 31 3.5V22H4z" opacity=".9"/><rect x="1" y="23" width="32" height="3.5" rx="1.75"/><rect x="29" y="8" width="15" height="25" rx="3.2" stroke="#e9e9e9" stroke-width="2.2" fill="#1b1b1c"/><rect x="33" y="11.5" width="7" height="2" rx="1" fill="#e9e9e9"/></svg>`;
const shieldX = `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9.5 9.5 5 5m0-5-5 5"/></svg>`;
const shieldLock = `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><rect x="9" y="10.5" width="6" height="4.5" rx="1" fill="currentColor"/><path d="M10.5 10.5V9a1.5 1.5 0 0 1 3 0v1.5"/></svg>`;
const eye = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3" fill="currentColor"/></svg>`;
const tapHand = `<svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M9 11V5.5a1.5 1.5 0 0 1 3 0V11m0-1.5a1.5 1.5 0 0 1 3 0V12m0-1a1.5 1.5 0 0 1 3 0v4c0 3.5-2.2 6-5.5 6-2.4 0-3.6-1-5-2.8L5 14.5a1.5 1.5 0 0 1 2.3-1.9L9 14.5"/></svg>`;
const arrowDown = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14m7-7-7 7-7-7"/></svg>`;
const copyIc = (n = 15) => ic('copy', n);

const fi = (name) => { try { return fileIcon(name); } catch { return ic('file', 16); } };
const markOf = (s, n = 14) => { const [, icon, color] = agentOf(s); return `<span class="ph-mark" style="color:${color};width:${n}px;height:${n}px">${ic(icon, n)}</span>`; };

// Status glyphs, as StatusGlyph.swift draws them: ink halo, green halo, bold cross, 3×3 gradient spinner, ring.
const DIAG = ['#8fb8ff', '#a9aafa', '#c39cf5', '#e19db7', '#ff9f7a'];
const spinner = (z) => `<span class="ph-gspin" style="--z:${z}px">${[0, 1, 2].map((r) => [0, 1, 2].map((c) => `<i style="background:${DIAG[r + c]};animation-delay:${-((8 - r - c) / 8) * 800}ms"></i>`).join('')).join('')}</span>`;
function glyph(status, size = 14) {
  const box = +(size * 16 / 14).toFixed(1);
  const inner = +(size * 9 / 14).toFixed(1);
  const body = status === 'working' ? spinner(size)
    : status === 'failed' ? `<span class="ph-x">${ic('x-bold', +(size * 0.86).toFixed(1))}</span>`
    : status === 'idle' ? `<span class="ph-ring" style="width:${size}px;height:${size}px"></span>`
    : `<span class="ph-halo${status === 'done' ? ' done' : ''}" style="width:${box}px;height:${box}px"><i style="width:${inner}px;height:${inner}px"></i></span>`;
  return `<span class="ph-gl" style="width:${box}px;height:${box}px">${body}</span>`;
}

const upNext = () => {
  const rank = { needs: 0, failed: 1, done: 2 };
  return SESSIONS.filter((s) => s.status in rank).sort((a, b) => rank[a.status] - rank[b.status]);
};
const needing = () => SESSIONS.filter((s) => s.status === 'needs');
const roll = (list) => ['needs', 'failed', 'working', 'done'].find((st) => list.some((s) => s.status === st));
const elapsed = (s) => fmt(Math.max(0, Math.round((Date.now() - s.since) / 1000)));

// Sessions
function homeHTML() {
  const asks = needing().length;
  const up = upNext();
  const working = SESSIONS.filter((s) => s.status === 'working');
  const rows = up.map((s) => {
    const headline = s.status === 'needs' ? `Wants to run ${esc(s.ask.cmd)}` : esc(s.title);
    const outcome = s.status === 'failed' ? '<em class="bad">Failed</em><span class="sep">·</span>' : s.status === 'done' ? '<em class="good">Finished</em><span class="sep">·</span>' : '';
    const inline = s.status === 'needs' && s.inline;
    return `<button class="ph-row${inline ? ' nosep' : ''}" data-act="open" data-id="${s.id}">${glyph(s.status)}<span class="ph-rt"><b class="ph-t">${headline}</b><span class="ph-s">${outcome}<span class="ph-mono">${esc(place(s))}</span></span></span><span class="ph-ago">${s.ago}</span></button>`
      + (inline ? `<div class="ph-answer"><button class="ph-pill" data-act="deny" data-id="${s.id}">Deny</button><button class="ph-pill primary" data-act="approve" data-id="${s.id}">Approve</button></div>` : '');
  }).join('');
  const work = working.map((s) => `<button class="ph-row" data-act="open" data-id="${s.id}">${glyph('working')}<span class="ph-rt"><span class="ph-s" style="font:500 11.2px/14px var(--sf);color:var(--t1)"><b style="font-weight:500">${s.project}</b><span class="sep">·</span><span class="ph-mono">${s.tree}</span></span><span class="ph-s ph-mono">${esc(s.detail ?? 'Working')}</span></span><span class="ph-ago" data-since="${s.since}">${elapsed(s)}</span></button>`).join('');
  const projects = PROJECTS.map((p) => {
    const list = p.trees.flatMap((t) => t[3].map((id) => BY[id]));
    const alert = ['needs', 'failed', 'done'].find((st) => list.some((s) => s.status === st));
    const wts = p.trees.filter((t) => t[0] === 'wt').length, locals = p.trees.length - wts;
    const line = `${wts} worktree${wts === 1 ? '' : 's'} · ${locals} Local${locals === 1 ? '' : 's'}`;
    const badge = alert ? `<span class="ph-badge"><i style="background:${alert === 'failed' ? 'var(--failed)' : alert === 'done' ? 'var(--ok)' : 'var(--t1)'}"></i></span>` : '';
    const sp = list.some((s) => s.status === 'working') ? `<span class="ph-sp">${spinner(10)}</span>` : '';
    const trees = p.open ? p.trees.map(([kind, label, where, ids]) => {
      const ss = ids.map((id) => BY[id]);
      const count = ss.length === 0 ? 'No sessions' : ss.length === 1 ? '1 session' : `${ss.length} sessions`;
      const st = roll(ss);
      const pr = ss.find((s) => s.pr)?.pr;
      const icon = kind === 'local' ? (ss.length ? 'laptop' : 'folder') : 'worktree';
      // A worktree goes by its session's name, as on the Mac; its branch moves to the line below.
      if (kind === 'wt' && ss[0]) [label, where] = [ss[0].title, label];
      return `<button class="ph-tree" data-act="tree" data-id="${ids[0] ?? ''}"><span class="ic">${ic(icon, 17)}</span><span class="ph-rt"><b class="ph-t" style="font-size:11.2px;line-height:15px;font-weight:600">${esc(label)}</b><span class="ph-s ph-mono">${where ? `${where}<span class="sep">·</span>` : ''}${count}</span></span>${pr ? `<span class="ph-pr${pr[1] === 'merged' ? ' merged' : ''}"><i></i>${pr[0]}</span>` : ''}${ss.length ? `<span class="ph-ago">${ss[0].ago}</span>` : ''}${st ? `<span class="gl">${glyph(st, 12)}</span>` : ''}</button>`;
    }).join('') : '';
    return `<button class="ph-proj" data-act="fold" data-id="${p.name}"><span class="ph-tile">${p.name.slice(0, 2).toUpperCase()}${badge}${sp}</span><span class="ph-rt"><b class="ph-t">${p.name}</b><span class="ph-s">${line}</span></span><span class="ph-ago">${list.length ? list[0].ago : ''}</span><span class="chev${p.open ? ' open' : ''}">${chevR(11)}</span></button>${trees}`;
  }).join('');
  return `<div class="ph-bar-fade"></div>
    <div class="ph-nav"><div class="ph-end"><span class="ph-circle ph-glass" aria-hidden="true">${ic('filter', 18)}</span><span class="ph-circle ph-glass" aria-hidden="true">${ic('plus', 20)}</span></div></div>
    <div class="ph-scroll" style="padding-top:103px">
      <header class="ph-home-head"><h1>${esc(MAC)}</h1><p class="ph-conn"><i></i>Connected over Tailscale · ${SESSIONS.length} sessions</p></header>
      <div class="ph-search">${ic('search', 17)}<span>Search sessions, branches</span></div>
      ${up.length ? `<h2 class="ph-sec">Up next${asks ? `<b>${asks} need${asks === 1 ? 's' : ''} you</b>` : ''}</h2><div class="ph-list ph-card">${rows}</div>` : ''}
      ${working.length ? `<h2 class="ph-sec">Working</h2><div class="ph-bare"><div class="ph-list">${work}</div></div>` : ''}
      <h2 class="ph-sec">Projects<button tabindex="-1" aria-hidden="true">As on the Mac ${chevD(9, 3.4)}</button></h2>
      <div class="ph-list">${projects}</div>
      <div class="ph-foot"></div>
    </div>`;
}

// Chat
const toolLine = (it, bad) => {
  if (it.denied) return `<div class="ph-tool denied${it.fresh ? ' ph-fresh' : ''}">${ic('x-bold', 10)}<span>Denied</span><code class="raw">${esc(it.a)}</code></div>`;
  const raw = it.v === 'Bash';
  const verb = { Read: 'Read', Update: 'Edited', Write: 'Wrote' }[it.v];
  const arg = raw ? `<code class="raw">${esc(it.a)}</code>` : `<span class="ph-chip">${fi(it.a.split('/').pop())}${esc(it.a.split('/').pop())}</span>`;
  return `<div class="ph-tool${bad ? ' bad' : ''}${it.fresh ? ' ph-fresh' : ''}">${bad ? ic('x-bold', 10) : ''}${verb ? `<span>${verb}</span>` : ''}${arg}</div>`;
};
function turnHTML(items, s, last) {
  const live = last && (s.status === 'working' || s.status === 'needs');
  const says = items.filter((i) => i.k === 'say');
  const final = !live && says.length ? says[says.length - 1] : null;
  const body = items.filter((i) => i !== final).map((it, n, arr) => {
    const fresh = it.fresh ? ' ph-fresh' : '';
    if (it.k === 'say') return `<p class="ph-say${fresh}">${esc(it.v)}</p>`;
    if (it.k === 'tool') return toolLine(it, arr[n + 1]?.k === 'err');
    if (it.k === 'out') return `<div class="ph-out${fresh}">${esc(it.v)}</div>`;
    if (it.k === 'err') return `<div class="ph-err${fresh}">${esc(it.v)}</div>`;
    return '';
  });
  const hasWork = items.some((i) => i.k === 'tool');
  let out = '';
  if (live || !hasWork) out += body.join('');
  else {
    const dur = s.worked ?? s.finishedWorked ?? '1m 4s';
    out += `<button class="ph-fold" data-act="fold-turn" aria-expanded="${s.fold}">${s.fold ? chevD(11, 2.8) : markOf(s)}<span>${agentOf(s)[0]} worked for ${dur}</span>${s.fold ? '' : chevR(10)}</button>`;
    if (s.fold) out += `<div class="ph-rail">${body.join('')}</div>`;
  }
  if (final) out += `<p class="ph-say last${final.fresh ? ' ph-fresh' : ''}">${esc(final.v)}</p>`;
  if (!live && last && final) out += `<div class="ph-acts"><span>${copyIc(15)}Copy answer</span><span>${ic('reload', 15)}Retry this turn</span><em>${s.worked ?? s.finishedWorked ?? '1m 4s'} · 9:41</em></div>`;
  if (live && s.status === 'working') out += `<div class="ph-live">${markOf(s)}<span>${agentOf(s)[0] === 'Claude' ? s.model : agentOf(s)[0]} working for <b data-since="${s.since}">${elapsed(s)}</b></span></div>`;
  if (live && s.status === 'needs') out += `<div class="ph-live">${markOf(s)}<span><b style="color:var(--t1)">Needs</b> your approval</span></div>`;
  return out;
}
function transcriptHTML(s) {
  const parts = [];
  let turn = null;
  for (const it of s.items) {
    if (it.k === 'you') { turn = { you: it, items: [] }; parts.push(turn); } else (turn ?? (turn = parts[parts.push({ items: [] }) - 1])).items.push(it);
  }
  return parts.map((t, n) => (t.you ? `<div class="ph-you"><div class="ph-bub">${esc(t.you.v)}</div><div class="ph-meta">${copyIc(16)}<span>9:3${n + 6}</span></div></div>` : '') + turnHTML(t.items, s, n === parts.length - 1)).join('');
}
function dockHTML(s) {
  const fresh = s.dockFresh ? ' ph-fresh' : '';
  const [agentName, , color] = agentOf(s);
  const branch = `<span class="l">${ic('worktree', 13)}${esc(s.tree)}</span>`;
  if (s.status === 'needs') {
    return `<div class="ph-perm${fresh}"><div class="head"><span class="dot"><i></i></span><div><b>Allow Bash?</b><span>${agentName} is paused until you answer</span></div></div>
      <div class="ph-cmd">${ic('prompt', 14)}<div><span>Run</span> ${esc(s.ask.cmd)}</div></div>
      <div class="ph-btns"><button class="ph-pill danger" data-act="deny" data-id="${s.id}">Deny</button><button class="ph-pill" data-act="reply">Reply</button><span class="grow"></span><button class="ph-pill primary" data-act="approve" data-id="${s.id}">Allow once</button></div></div>
      <div class="ph-foot-line">${branch}<span>Answer the request above to continue</span></div>`;
  }
  const busy = s.status === 'working';
  const ring = `<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><circle cx="10" cy="10" r="8" stroke="#ebebeb26" stroke-width="2.4"/><path d="M10 2a8 8 0 0 1 3 .6" stroke="#ebebebb3" stroke-width="2.4" stroke-linecap="round"/></svg>`;
  return `<div class="ph-box${fresh}"><div class="ph-input">${busy ? 'Send a follow-up into this turn…' : 'Ask, build, / for commands, @ for files'}</div>
    <div class="ph-bar"><span class="model">${markOf(s, 15)}${esc(s.model)}<span>High</span>${chevD(11, 2.4)}</span><span class="edits">${ic('shield', 15)}Edits</span><span class="grow"></span>${ring}<span class="ph-send${busy ? ' go' : ''}">${busy ? '<i class="sq"></i>' : ic('arrow-up', 20)}</span></div></div>
    <div class="ph-foot-line">${branch}<span>${busy ? 'Working · esc to stop' : ''}</span></div>`;
}
function chatHTML(s) {
  return `<div class="ph-bar-fade solid"></div>
    <div class="ph-nav"><button class="ph-circle ph-glass" data-act="back" aria-label="Back to sessions">${chevL(20)}</button>
      <div class="ph-title-pill ph-glass"><b>${esc(s.title)}</b><span>${esc(s.tree)}</span></div>
      <button class="ph-circle ph-glass" data-act="changes" aria-label="Changes">${ic('diff-multiple', 19)}</button></div>
    <div class="ph-scroll" data-tx>${transcriptHTML(s)}</div>
    <div class="ph-dock" data-dock>${dockHTML(s)}</div>`;
}

// Changes and diff
function parseDiff(text) {
  let a = 0, b = 0, add = 0, del = 0;
  const lines = text.split('\n').map((l) => {
    if (l.startsWith('@@')) { [, a, b] = l.match(/-(\d+)(?:,\d+)? \+(\d+)/).map(Number); return { t: 'hunk', x: l }; }
    const t = l[0], x = l.slice(1);
    if (t === '+') return add++, { t: 'add', n: b++, x };
    if (t === '-') return del++, { t: 'del', o: a++, x };
    return { t: 'ctx', o: a++, n: b++, x };
  });
  return { lines, add, del };
}
FILES.forEach((f) => { Object.assign(f, parseDiff(f.diff)); f.name = f.path.split('/').pop(); f.dir = f.path.split('/').slice(0, -1).join('/'); });
const stat = (f) => `<span class="ph-ds add">+${f.add}</span> <span class="ph-ds del">−${f.del}</span>`;

const KW = new Set(['await', 'async', 'const', 'let', 'var', 'return', 'new', 'import', 'from', 'export', 'function', 'if', 'else', 'type', 'true', 'false', 'null']);
function hl(src) {
  let out = '', last = 0, m;
  const re = /(\/\/.*$)|('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*"|`(?:[^`\\]|\\.)*`)|([A-Za-z_$][\w$]*)(?=\()|([A-Za-z_$][\w$]*)/g;
  while ((m = re.exec(src))) {
    out += esc(src.slice(last, m.index));
    last = re.lastIndex;
    const cls = m[1] ? 'c' : m[2] ? 's' : m[3] ? (KW.has(m[3]) ? 'k' : 'f') : KW.has(m[4]) ? 'k' : '';
    out += cls ? `<i class="${cls}">${esc(m[0])}</i>` : esc(m[0]);
  }
  return out + esc(src.slice(last));
}
const codeHTML = (x) => x.split(/[⟦⟧]/).map((seg, n) => (n % 2 ? `<mark>${hl(seg)}</mark>` : hl(seg))).join('');

function changesHTML() {
  const row = (f, i) => `<button class="ph-file" data-act="file" data-id="${i}"><span class="st ${f.st}">${f.st}</span><span class="fi">${fi(f.name)}</span><span class="nm"><b${f.st === 'D' ? ' class="gone"' : ''}>${esc(f.name)}</b>${f.dir ? `<span>${esc(f.dir)}</span>` : ''}</span><span class="ds">${stat(f)}</span></button>`;
  const staged = FILES.map((f, i) => [f, i]).filter(([f]) => f.staged), unstaged = FILES.map((f, i) => [f, i]).filter(([f]) => !f.staged);
  const add = FILES.reduce((n, f) => n + f.add, 0), del = FILES.reduce((n, f) => n + f.del, 0);
  const group = (name, list, all) => (list.length ? `<div class="ph-gh"><div>${name}<span>${list.length}</span></div><button data-act="all" data-id="${name}">${all}</button></div><div class="ph-files">${list.map(([f, i]) => row(f, i)).join('')}</div>` : '');
  return `<div class="ph-bar-fade solid"></div>
    <div class="ph-nav"><button class="ph-circle ph-glass" data-act="back" aria-label="Back">${chevL(20)}</button><div class="ph-seg-pill ph-glass"><span class="on">Uncommitted</span><span>vs ${BRANCH.base}</span></div></div>
    <div class="ph-scroll" style="padding-top:111px">
      <div class="ph-search">${ic('search', 17)}<span>Filter ${FILES.length} files</span></div>
      <div class="ph-branch"><div class="top">${ic('branch', 17)}<span>${BRANCH.name}</span>→<span>${BRANCH.base}</span></div>
        <div class="sub">${BRANCH.ahead} ahead · ${BRANCH.behind} behind ${BRANCH.base}</div>
        <div class="ph-bacts"><span class="ph-mini">${ic('arrow-up', 14)}Push ${BRANCH.ahead}</span><span class="ph-mini">${arrowDown}Pull</span><span class="tot">${FILES.length} files · <span class="add">+${add}</span> <span class="del">−${del}</span></span></div>
        <div class="commits">${chevR(9)}${BRANCH.ahead + 2} commits since ${BRANCH.base}</div></div>
      ${group('Staged', staged, 'Unstage All')}${group('Unstaged', unstaged, 'Stage All')}
    </div>
    <div class="ph-commit ph-glass"><span>Commit message</span><span class="ph-pill">Commit ${chevD(9, 3)}</span></div>`;
}
function diffHTML(i) {
  const f = FILES[i];
  const lines = f.lines.map((l) => (l.t === 'hunk' ? `<div class="ph-ln hunk"><span class="tx">${esc(l.x)}</span></div>`
    : `<div class="ph-ln ${l.t}"><span class="no">${l.o ?? ''}</span><span class="no">${l.n ?? ''}</span><span class="mk">${l.t === 'add' ? '+' : l.t === 'del' ? '−' : ''}</span><span class="tx">${codeHTML(l.x)}</span></div>`)).join('');
  return `<div class="ph-bar-fade solid"></div>
    <div class="ph-nav"><button class="ph-circle ph-glass" data-act="back" aria-label="Back to changes">${chevL(20)}</button></div>
    <div class="ph-diffhead"><span class="fi">${fi(f.name)}</span><span>${esc(f.path)}</span><em>${stat(f).replace(/ph-ds /g, '')}</em></div>
    <div class="ph-scroll"><div class="ph-code">${lines}</div></div>
    <div class="ph-diffbar"><div class="pair ph-glass"><button aria-label="Previous change">${chevU(18)}</button><button aria-label="Next change">${chevD(18, 2.6)}</button></div><button class="ph-stage ph-glass" data-act="stage" data-id="${i}">${f.staged ? 'Unstage' : 'Stage'}</button></div>`;
}

// Pairing
const ADDR = '100.101.7.24', CODE = 'K7M2QX9TLP4WZ8RN3YB6HD';
function welcomeHTML() {
  return `<div class="ph-scroll"><div class="ph-app">${devices}</div><h1>Anywhere</h1>
      <p class="ph-lede">Watch and drive the coding agents on your Mac, from your iPhone</p>
      <div class="ph-feat"><span class="tl">${glyph('needs', 14)}</span><div><b>Know when a session needs you</b><p>Approvals, questions, failures and finished runs, as they happen.</p></div></div>
      <div class="ph-feat"><span class="tl">${shieldLock}</span><div><b>Private link over Tailscale</b><p>Your iPhone reaches your Mac across your own tailnet.</p></div></div>
      <p class="ph-hint">On your Mac, open Anywhere → Settings → Phone → Pair a phone.</p></div>
    <div class="ph-cta"><button class="ph-big primary" data-act="scan">${qr(18)}Scan pairing code</button><button class="ph-big" data-act="enter">Enter code instead</button><p class="ph-small">No Tailscale yet? <b>Get Tailscale</b></p></div>`;
}
function enterHTML() {
  const { addr, code } = state.pair;
  const caret = '<i class="ph-caret"></i>';
  return `<div class="ph-bar-fade solid"></div><div class="ph-nav"><button class="ph-circle ph-glass" data-act="back" aria-label="Back">${chevL(20)}</button><div class="ph-title-pill ph-glass"><b>Enter code</b></div></div>
    <div class="ph-scroll"><div class="ph-lbl">Mac address</div>
      <div class="ph-fld" data-f="addr">${addr ? esc(addr) : '<span class="ph">100.101.102.103</span>'}${addr.length < ADDR.length && code === '' ? caret : ''}</div>
      <p class="ph-help">The Mac's Tailscale address or name. Port 4517 is added if you leave it out.</p>
      <div class="ph-lbl">Code</div>
      <div class="ph-fld mono" data-f="code">${code ? esc(code) : '<span class="ph">22 letters and digits</span>'}${addr.length === ADDR.length && code.length < CODE.length ? caret : ''}</div>
      <p class="ph-help">Your Mac shows it under the QR code.</p>
      <div class="ph-pastebox"><button class="ph-paste" data-act="paste">${copyIc(15)}Paste</button></div>
      <p class="ph-help" style="color:var(--t3)">Paste the pairing link or the line pocketd pair prints.</p></div>
    <div class="ph-cta" style="background:#282828"><button class="ph-big primary" data-act="next"${addr === ADDR && code === CODE ? '' : ' disabled'}>Continue</button></div>`;
}
function confirmHTML() {
  return `<div class="ph-bar-fade solid"></div><div class="ph-nav"><button class="ph-circle ph-glass" data-act="back" aria-label="Back">${chevL(20)}</button></div>
    <div class="ph-scroll"><div class="ph-lap">${lap(72)}</div><h1>Pair with ${esc(MAC)}?</h1><div class="addr">${ADDR}:4517</div>
      <div class="ph-perms">
        <div class="ph-feat"><span class="tl">${eye}</span><div><b>See sessions</b><p>Chats, changes and files on this Mac.</p></div></div>
        <div class="ph-feat"><span class="tl">${tapHand}</span><div><b>Drive them</b><p>Send prompts, answer permissions and start new sessions.</p></div></div>
        <div class="ph-feat"><span class="tl">${shieldX}</span><div><b>Remove it any time</b><p>On the Mac, in Anywhere → Settings → Phone.</p></div></div></div></div>
    <div class="ph-cta"><button class="ph-big primary${state.pairing ? ' busy' : ''}" data-act="pair">${state.pairing ? '<i class="ph-spin"></i>' : 'Pair'}</button></div>`;
}

const PAGES = {
  home: { html: homeHTML, root: true, tab: 'sessions' },
  chat: { html: (arg) => chatHTML(BY[arg]), cls: 'ph-chat', tab: 'chat' },
  changes: { html: changesHTML, cls: 'ph-git', tab: 'diff' },
  diff: { html: (arg) => diffHTML(+arg), cls: 'ph-diff', tab: 'diff' },
  welcome: { html: welcomeHTML, cls: 'ph-pair', root: true, tab: 'pair' },
  enter: { html: enterHTML, cls: 'ph-pair ph-form', tab: 'pair' },
  confirm: { html: confirmHTML, cls: 'ph-confirm', tab: 'pair' },
};
const parts = (key) => key.split(':');
const pageOf = (key) => PAGES[parts(key)[0]];

// The device
const STATUS_ICONS = `<svg width="19" height="12" viewBox="0 0 19 12" fill="#fff"><rect x="0" y="7.5" width="3.2" height="4.5" rx="1"/><rect x="5" y="5" width="3.2" height="7" rx="1"/><rect x="10" y="2.5" width="3.2" height="9.5" rx="1"/><rect x="15" y="0" width="3.2" height="12" rx="1"/></svg>
  <svg width="17" height="12" viewBox="0 0 17 12" fill="#fff"><path d="M8.5 2.3c2.3 0 4.4.9 6 2.4.1.1.3.1.4 0l1.1-1.1c.1-.1.1-.3 0-.4A11.2 11.2 0 0 0 8.5 0C5.6 0 2.9 1.1.9 3.2c-.1.1-.1.3 0 .4l1.1 1.1c.1.1.3.1.4 0a8.7 8.7 0 0 1 6.1-2.4zm0 3.8c1.2 0 2.3.5 3.2 1.2.1.1.3.1.4 0l1.1-1.1c.1-.1.1-.3 0-.4a7 7 0 0 0-9.4 0c-.1.1-.1.3 0 .4l1.1 1.1c.1.1.3.1.4 0 .9-.7 2-1.2 3.2-1.2zm0 3.7c.6 0 1.2.2 1.6.6.1.1.1.3 0 .4L8.7 11.9c-.1.1-.3.1-.4 0L6.9 10.8c-.1-.1-.1-.3 0-.4.4-.4 1-.6 1.6-.6z"/></svg>
  <svg width="27" height="13" viewBox="0 0 27 13" fill="none"><rect x=".5" y=".5" width="23" height="12" rx="3.8" stroke="#fff" opacity=".4"/><rect x="2" y="2" width="20" height="9" rx="2.5" fill="#fff"/><path d="M25 4.5v4c.8-.3 1.5-1.1 1.5-2s-.7-1.7-1.5-2z" fill="#fff" opacity=".45"/></svg>`;

figure.innerHTML = `<div class="ph-dev"><i class="ph-btn action"></i><i class="ph-btn up"></i><i class="ph-btn down"></i><i class="ph-btn power"></i>
  <div class="ph-bezel"><div class="ph-screen">
    <div class="ph-stack"></div>
    <button class="ph-banner" data-act="banner" tabindex="-1" aria-hidden="true"><img src="assets/icon.png" alt=""><span class="bt"><b>Anywhere</b><span></span></span><time>now</time></button>
    <div class="ph-status" aria-hidden="true"><time>9:41</time><div class="ph-icons">${STATUS_ICONS}</div></div>
    <div class="ph-island"></div><div class="ph-home"></div>
  </div></div></div>`;
const dev = figure.querySelector('.ph-dev');
const stackEl = figure.querySelector('.ph-stack');
const banner = figure.querySelector('.ph-banner');

new ResizeObserver(([e]) => figure.style.setProperty('--s', (e.contentRect.width / 413).toFixed(5))).observe(figure);

// Navigation: pages slide in and out like a UINavigationController, or cross-fade between roots.
const pages = new Map();
const top = () => state.stack[state.stack.length - 1];
function build(key) {
  const [name, arg] = parts(key);
  const def = PAGES[name];
  const el = document.createElement('section');
  el.className = `ph-page ${def.cls ?? ''}${def.root ? ' ph-root' : ''}`;
  el.dataset.key = key;
  el.innerHTML = def.html(arg);
  stackEl.appendChild(el);
  pages.set(key, el);
  return el;
}
function refresh(key, keepBottom) {
  const el = pages.get(key);
  if (!el) return;
  const [name, arg] = parts(key);
  const sc = el.querySelector('.ph-scroll');
  const y = sc?.scrollTop ?? 0;
  el.innerHTML = PAGES[name].html(arg);
  const next = el.querySelector('.ph-scroll');
  if (next) next.scrollTop = keepBottom ? next.scrollHeight : y;
}
function refreshChat(s) {
  const el = pages.get(`chat:${s.id}`);
  if (!el) return;
  const tx = el.querySelector('[data-tx]');
  const stick = tx.scrollHeight - tx.scrollTop - tx.clientHeight < 40;
  const y = tx.scrollTop;
  tx.innerHTML = transcriptHTML(s);
  el.querySelector('[data-dock]').innerHTML = dockHTML(s);
  s.items.forEach((i) => { i.fresh = false; });
  s.dockFresh = false;
  tx.scrollTop = stick ? tx.scrollHeight : y;
}
const touch = () => { refresh('home'); if (top()?.startsWith('chat:')) refreshChat(BY[parts(top())[1]]); };

function setStates() {
  const n = state.stack.length;
  pages.forEach((el, key) => {
    const i = state.stack.indexOf(key);
    el.dataset.state = i < 0 ? el.dataset.leaving ?? 'gone' : i === n - 1 ? 'current' : i === n - 2 ? 'behind' : 'hidden';
    el.inert = i !== n - 1;
  });
}
function show(next, how) {
  const prev = state.stack;
  const token = ++state.nav;
  const n = next.length;
  const leaving = [...pages.keys()].filter((k) => !next.includes(k));
  const finalState = (k) => { const i = next.indexOf(k); return i === n - 1 ? 'current' : i === n - 2 ? 'behind' : 'hidden'; };
  next.filter((k) => !pages.has(k)).forEach((k) => {
    const el = build(k);
    const isTop = k === next[n - 1];
    el.dataset.state = how === 'push' && isTop ? 'ahead' : finalState(k);
    if (how === 'fade' && isTop) el.style.opacity = '0';
  });
  void stackEl.offsetWidth;
  leaving.forEach((k) => {
    const el = pages.get(k);
    el.dataset.leaving = how === 'push' ? 'hidden' : how === 'fade' ? 'current' : 'ahead';
    if (how === 'fade') el.style.opacity = '0';
  });
  state.stack = next;
  setStates();
  if (how === 'fade') requestAnimationFrame(() => pages.get(next[n - 1])?.style.removeProperty('opacity'));
  const done = () => {
    if (token !== state.nav) return;
    leaving.forEach((k) => { if (!state.stack.includes(k)) { pages.get(k)?.remove(); pages.delete(k); } });
  };
  if (still || how === 'none') done(); else later(done, 520);
  if (prev.at(-1) !== next.at(-1)) emitScreen();
}
function navigate(next) {
  const prev = state.stack;
  if (prev.length === next.length && prev.every((k, i) => k === next[i])) return;
  let common = 0;
  while (common < prev.length && common < next.length && prev[common] === next[common]) common++;
  const how = !prev.length ? 'none' : common === 0 ? 'fade' : next.length > common ? 'push' : 'pop';
  show(next, still && how !== 'none' ? 'none' : how);
}
const push = (key) => navigate([...state.stack, key]);
const pop = () => navigate(state.stack.slice(0, -1));
function emitScreen() {
  const t = top();
  if (t) document.dispatchEvent(new CustomEvent('phone:screen', { detail: pageOf(t).tab }));
  if (t !== 'home') hideBanner();
  if (t === 'enter' && !still && state.pair.code === '') typeForm();
}

// Approving moves a session through working to done; each step lands in the chat and the list.
function stream(s, steps) {
  const gen = s.gen;
  const run = (n) => {
    if (gen !== s.gen) return;
    if (n >= steps.length) { s.status = 'done'; s.ago = 'now'; s.worked = null; s.finishedWorked = still ? '14s' : fmt(Math.max(1, Math.round((Date.now() - s.since) / 1000))); s.dockFresh = true; touch(); return; }
    const [k, v, a] = steps[n];
    s.items.push({ k, v, a, fresh: true });
    touch();
    later(() => run(n + 1), k === 'tool' ? 900 : k === 'say' ? 500 : 650);
  };
  run(0);
}
function approve(s) {
  if (s.status !== 'needs') return;
  s.status = 'working'; s.since = Date.now(); s.detail = `Running ${s.ask.cmd}`; s.ago = 'now'; s.dockFresh = true;
  const steps = s.after;
  if (still) {
    s.status = 'done'; s.finishedWorked = '14s'; s.items.push(...steps.map(([k, v, a]) => ({ k, v, a })));
    touch();
  } else {
    touch();
    later(() => stream(s, steps), 450);
  }
  hideBanner();
}
function deny(s) {
  if (s.status !== 'needs') return;
  const gen = ++s.gen;
  s.status = 'working'; s.since = Date.now(); s.detail = 'Reading your answer'; s.ago = 'now'; s.dockFresh = true;
  s.items.push({ k: 'tool', v: 'Bash', a: s.ask.cmd, denied: true, fresh: true });
  const reply = { k: 'say', v: "Understood, I won't run it. Tell me how you'd like to go on.", fresh: true };
  const finish = () => { if (gen !== s.gen) return; s.items.push(reply); s.status = 'done'; s.ago = 'now'; s.finishedWorked = '3s'; s.dockFresh = true; touch(); };
  touch();
  if (still) finish(); else later(finish, 900);
  hideBanner();
}

// The push notification, dropped in now and then while the sessions list is showing.
let visible = false, bannerTimer = 0, bannerOff = 0, shownFor = null;
function hideBanner() { clearTimeout(bannerOff); banner.classList.remove('on'); banner.tabIndex = -1; banner.setAttribute('aria-hidden', 'true'); }
function scheduleBanner(ms) {
  clearTimeout(bannerTimer);
  if (still || !visible) return;
  bannerTimer = setTimeout(() => {
    const s = needing()[0];
    if (!s || top() !== 'home') return scheduleBanner(4000);
    shownFor = s.id;
    banner.querySelector('.bt span').textContent = `${s.title} needs you`;
    banner.classList.add('on'); banner.tabIndex = 0; banner.setAttribute('aria-hidden', 'false');
    bannerOff = setTimeout(() => { hideBanner(); scheduleBanner(14000); }, 4800);
  }, ms);
}
new IntersectionObserver(([e]) => {
  visible = e.isIntersecting;
  if (visible) scheduleBanner(state.stack.length ? 2600 : 0); else { clearTimeout(bannerTimer); hideBanner(); }
}, { threshold: 0.4 }).observe(figure);

// Taps
figure.addEventListener('click', (e) => {
  const t = e.target.closest('[data-act]');
  if (!t || !figure.contains(t)) return;
  const { act, id } = t.dataset;
  switch (act) {
    case 'open': return push(`chat:${id}`);
    case 'tree': return id && push(`chat:${id}`);
    case 'banner': hideBanner(); return top() === 'home' && shownFor && push(`chat:${shownFor}`);
    case 'approve': return approve(BY[id]);
    case 'deny': return deny(BY[id]);
    case 'reply': return;
    case 'back': return pop();
    case 'fold': { const p = PROJECTS.find((x) => x.name === id); p.open = !p.open; return refresh('home'); }
    case 'fold-turn': { const s = BY[parts(top())[1]]; s.fold = !s.fold; return refreshChat(s); }
    case 'changes': return push('changes');
    case 'file': return push(`diff:${id}`);
    case 'stage': { FILES[+id].staged = !FILES[+id].staged; refresh(top()); return refresh('changes'); }
    case 'all': FILES.forEach((f) => { f.staged = id === 'Unstaged'; }); return refresh('changes');
    case 'scan': case 'enter': return push('enter');
    case 'paste': state.pair.code = CODE; state.pair.addr ||= ADDR; return refresh('enter');
    case 'next': return push('confirm');
    case 'pair': return pairNow();
  }
});
function pairNow() {
  if (state.pairing) return;
  state.pairing = true;
  refresh('confirm');
  const gen = state.nav;
  later(() => { state.pairing = false; if (state.nav === gen) navigate(['home']); }, still ? 0 : 1100);
}

// The address and code type themselves into the form, as if pasted from the Mac's pairing link.
function typeForm() {
  const p = state.pair;
  const step = () => {
    if (top() !== 'enter') return;
    if (p.addr.length < ADDR.length) p.addr = ADDR.slice(0, p.addr.length + 1);
    else if (p.code.length < CODE.length) p.code = CODE.slice(0, p.code.length + 1);
    else return refresh('enter');
    refresh('enter');
    later(step, p.addr.length === ADDR.length && !p.code ? 380 : 55);
  };
  later(step, 650);
}

const TARGETS = { sessions: ['home'], chat: ['home', 'chat:flaky'], diff: ['home', 'changes'], pair: ['welcome'] };
window.AnywherePhone = {
  go(id) {
    const target = TARGETS[id];
    if (!target) return;
    if (id === 'chat') { init(BY.flaky); touch(); }
    if (id === 'pair') { state.pair = { addr: '', code: '' }; state.pairing = false; }
    navigate(target);
  },
};

// Elapsed times tick while the phone is on screen.
if (!still) setInterval(() => {
  if (!visible) return;
  figure.querySelectorAll('[data-since]').forEach((el) => { el.textContent = fmt(Math.max(0, Math.round((Date.now() - +el.dataset.since) / 1000))); });
}, 1000);

show(['home'], 'none');
})();
