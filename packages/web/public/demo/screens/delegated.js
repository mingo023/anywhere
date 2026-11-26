// Delegated: a chat whose agent handed work to three other sessions. The Delegated panel lists them, they
// progress on their own, one asks a question, and the chat wakes with the result. A click opens a child with
// the app's push; Back (or Esc) returns.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS } = AW;

const md = (s) => esc(s).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>').replace(/`(.+?)`/g, '<code>$1</code>');
const TOOL_VERB = { Read: 'Read', Edit: 'Edited', Write: 'Wrote', Bash: 'Ran' };
const toolIcon = { Read: 'file', Edit: 'pencil', Write: 'file-plus', Bash: 'terminal' };
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

const PARENT_LABEL = 'chore/react-19';
const BRIEF = 'Now split the rest: write the tests, review the diff and update the docs.';

const FULL_W = 312, CAPSULE_H = 34, HEAD_H = 36, ROW_H = 48, QUESTION_LINE = 17;
const COLLAPSE_AFTER = 3500;

const LABEL = { starting: 'Starting', working: 'Working', asking: 'Needs you', done: 'Done' };
const clock = (ms) => { const s = Math.max(0, ms) / 1000 | 0; return `${s / 60 | 0}:${String(s % 60).padStart(2, '0')}`; };

let S = null;
let timers = [];
let parentRow = null;
let panelEl = null;
let snapshot = null;

const child = (id, title, agent, brief) => ({ id, title, agent, brief, phase: 'starting', t0: Date.now(), t1: 0, steps: [], question: '', options: [] });

function fresh() {
  const kids = [
    { ...child('c1', 'Write the tests', 'codex', 'Write the tests for the React 19 migration in orbit-web.'), question: 'Which test runner?', options: ['Vitest', 'Jest'] },
    child('c2', 'Review the diff', 'claude', 'Review the diff of the React 19 migration and fix what you find.'),
    child('c3', 'Update the docs', 'opencode', 'Update the README for the React 19 migration.'),
  ];
  return {
    view: 'parent', kids, open: true, tail: [], collapse: 0, woke: false, seq: 0,
    watch: { total: 0, asking: 0, settled: false },
    items: [
      ['you', 'Upgrade us to React 19'],
      ['reply', '1m 4s', [['tool', 'Bash', 'pnpm up react@19 react-dom@19'], ['tool', 'Edit', 'src/app.tsx'], ['tool', 'Bash', 'gh pr create --fill'], ['say', 'Opened **PR #214**: Migrate to React 19. Checks are running.']]],
      ['you', BRIEF],
      ['reply', '1s', [['say', "I'll split this three ways: tests on Codex, a review of the diff on Claude, and the docs on OpenCode. Each runs as a session of its own, and what it reports comes back here."]]],
    ],
  };
}

const live = () => D.api.state.screen === 'delegated';
function refresh() { if (live()) D.api.draw(); }
function later(ms, fn) { timers.push(setTimeout(() => { if (S) fn(); }, ms)); }
const find = (id) => S.kids.find((k) => k.id === id);
const counts = () => {
  const c = { total: S.kids.length, asking: 0, working: 0, settled: false };
  S.kids.forEach((k) => { if (k.phase === 'asking') c.asking++; else if (k.phase !== 'done') c.working++; });
  c.settled = c.total > 0 && c.working + c.asking === 0;
  return c;
};

// The panel opens for a new delegation or a new question and shuts soon after the last session ends.
function watch() {
  const c = counts(), w = S.watch;
  const grew = c.total > w.total || c.asking > w.asking;
  const justSettled = c.settled && !w.settled;
  Object.assign(w, { total: c.total, asking: c.asking, settled: c.settled });
  if (grew && !c.settled) {
    S.open = true;
    S.hidden = false;
    clearTimeout(S.collapse);
  } else if (justSettled) {
    S.collapse = setTimeout(() => { if (S) { S.open = false; fillPanel(); } }, COLLAPSE_AFTER);
    later(1400, wake);
  }
}

function report(kind, kid) { S.items.push(['event', kind, kid.id]); }

function setPhase(kid, phase) {
  kid.phase = phase;
  if (phase === 'done') kid.t1 = Date.now();
  watch();
}

function wake() {
  if (S.woke) return;
  S.woke = true;
  S.items.push(['reply', '3s', [['say', 'All three are back. **24 tests pass** on Vitest, the review found one unused import and removed it, and the README now covers the React 19 migration.']]]);
  refresh();
}

function scenario() {
  later(400, () => { find('c1').phase = 'working'; find('c3').phase = 'working'; find('c2').phase = 'working'; refresh(); });
  later(900, () => { const k = find('c1'); k.steps.push(['tool', 'Read', 'tests/app.spec.tsx'], ['say', 'Which test runner should I use?']); setPhase(k, 'asking'); report('asks', k); refresh(); });
  later(1100, () => { const k = find('c2'); k.steps.push(['tool', 'Read', 'src/app.tsx'], ['tool', 'Bash', 'git diff main'], ['tool', 'Edit', 'src/app.tsx'], ['say', 'The diff is clean. One unused import in `src/app.tsx`, which I removed.']); setPhase(k, 'done'); report('done', k); refresh(); });
  later(3000, () => { find('c3').steps.push(['tool', 'Read', 'README.md']); refresh(); });
  later(5200, () => { find('c3').steps.push(['tool', 'Edit', 'README.md']); refresh(); });
  later(6800, () => { const k = find('c3'); k.steps.push(['say', 'The README now covers the React 19 migration.']); setPhase(k, 'done'); report('done', k); refresh(); });
}

function answerAsk(v) {
  const k = find('c1');
  if (k.phase !== 'asking') return;
  k.steps.push(['you', v]);
  setPhase(k, 'working');
  refresh();
  later(1100, () => { k.steps.push(['tool', 'Write', 'tests/useTheme.test.ts']); refresh(); });
  later(2200, () => { k.steps.push(['tool', 'Bash', 'pnpm test']); refresh(); });
  later(3400, () => { k.steps.push(['say', `Done. **24 tests pass** on ${v}.`]); setPhase(k, 'done'); report('done', k); refresh(); });
}

function start() {
  timers.forEach(clearTimeout);
  if (S) clearTimeout(S.collapse);
  timers = [];
  S = fresh();
  S.watch = { total: 3, asking: 0, settled: false };
  scenario();
}

/* ── Chat pieces ─────────────────────────────────────────────────────── */

const agentIcon = (a) => AGENTS[a][0];
const parts = (list) => list.map(([k, a, b]) =>
  k === 'say' ? `<p>${md(a)}</p>`
  : k === 'you' ? `<div class="you"><div class="bubble">${esc(a)}</div></div>`
  : `<div class="tool">${ic(toolIcon[a] ?? 'tool', 14)}<b>${TOOL_VERB[a] ?? a}</b><code>${esc(b)}</code></div>`).join('');

function reply(r, worked, list, time = '14:21') {
  const [icon, name] = AGENTS[r.agent];
  return `<div class="reply"><div class="worked"><span class="ag ${r.agent}">${ic(icon, 15)}</span>${name.split(' ')[0]} worked for ${worked}${ic('chevron-right', 13)}</div>${parts(list)}<div class="acts">${ic('copy', 15)}${ic('reload', 15)}<span>· ${worked} · ${time}</span></div></div>`;
}

const you = (text) => `<div class="you"><div class="bubble">${esc(text)}</div><div class="meta">${ic('copy', 14)}<span>14:21</span></div></div>`;

function eventLine(kind, id) {
  const k = find(id);
  const said = kind === 'asks' ? `asks “${esc(k.question)}”` : 'done';
  return `<button class="dg-line" data-act="child" data-v="${k.id}">${ic(agentIcon(k.agent), 13)}<span class="who">${esc(k.title)}</span><span class="vb${kind === 'asks' ? ' asks' : ''}">${said}</span><span class="chev">${ic('chevron-right', 12)}</span></button>`;
}

const dock = (r, label) => {
  const [icon, name] = AGENTS[r];
  const model = r === 'claude' ? '<b>Opus</b><span class="soft">High</span>' : r === 'codex' ? '<b>GPT-5.5</b><span class="soft">Medium</span>' : `<b>${name}</b>`;
  return `<div class="dock"><div class="composer empty"><textarea rows="2" placeholder="Ask, build, / for commands, @ for files" aria-label="Message"></textarea>
    <div class="foot"><button class="chip"><span class="ag ${r}">${ic(icon, 15)}</span>${model}${ic('chevron-down', 12)}</button><span class="sp"></span><button class="ib">${ic('plus', 17)}</button><button class="send">${ic('arrow-up', 16)}</button></div></div>
    <div class="where">${ic('worktree', 13)}<span>${esc(label)}</span></div></div>`;
};

function parentView(api) {
  const r = parentRow;
  const lastYou = [...S.items].reverse().find((i) => i[0] === 'you')[1];
  const body = S.items.map(([k, a, b], i) => {
    if (k === 'you') return you(a);
    if (k === 'reply') return reply(r, a, b);
    const first = S.items[i - 1]?.[0] !== 'event';
    const last = S.items[i + 1]?.[0] !== 'event';
    return `${first ? '<div class="dg-lines">' : ''}${eventLine(a, b)}${last ? '</div>' : ''}`;
  }).join('');
  return `<div class="pane dg-parent">
    <div class="bar">${api.tab(r)}<button class="ib" aria-label="New tab">${ic('plus', 16)}</button><span class="sp"></span><span class="dg-app"><i></i>${ic('chevron-down', 12)}</span></div>
    <div class="dg-jump">${ic('arrow-up', 14)}<span class="p">${esc(lastYou)}</span><span class="t">· 14:21</span></div>
    <div class="scroll chat"><div class="col">${body}</div></div>
    ${dock(r.agent, r.label)}
    <div class="dg-slot"></div>
  </div>`;
}

function childView(api) {
  const k = find(S.view);
  const [icon, name] = AGENTS[k.agent];
  const steps = k.steps.map(([kind, a, b]) => [kind, a, b]);
  const blocks = [];
  let cur = null;
  for (const st of steps) {
    if (st[0] === 'you') { cur = null; blocks.push(you(st[1])); continue; }
    if (!cur) { cur = []; blocks.push(cur); }
    cur.push(st);
  }
  const worked = k.t1 ? `${Math.max(1, Math.round((k.t1 - k.t0) / 1000))}s` : '';
  const out = blocks.map((b, i) => {
    if (typeof b === 'string') return b;
    const last = i === blocks.length - 1;
    const head = last && k.phase === 'working' ? `<div class="worked live"><span class="ag ${k.agent}">${ic(icon, 15)}</span><span class="shimmer">Working…</span></div>`
      : `<div class="worked"><span class="ag ${k.agent}">${ic(icon, 15)}</span>${name.split(' ')[0]} worked${worked && last ? ` for ${worked}` : ''}${ic('chevron-right', 13)}</div>`;
    return `<div class="reply">${head}${parts(b)}</div>`;
  }).join('');
  const ask = k.phase === 'asking' ? `<div class="dg-ask"><div class="h"><span class="halo"><i></i></span>Needs you</div><p>${esc(k.question)}</p><div class="opts">${k.options.map((o) => `<button data-act="pick" data-v="${esc(o)}">${esc(o)}</button>`).join('')}</div></div>` : '';
  const starting = k.phase === 'starting' ? '<div class="reply"><div class="worked live"><span class="shimmer">Starting…</span></div></div>' : '';
  return `<div class="pane dg-child" tabindex="-1">
    <div class="bar">${api.tab(parentRow)}<button class="ib" aria-label="New tab">${ic('plus', 16)}</button></div>
    <div class="scroll chat"><div class="col dg-ccol">
      <div class="dg-from">${ic(agentIcon(parentRow.agent), 11)}Delegated by ${AGENTS[parentRow.agent][1]} · 14:21</div>
      ${you(k.brief)}${starting}${out}${ask}</div></div>
    ${dock(k.agent, `${parentRow.label}-${k.id}`)}
    <div class="dg-crumb"><button class="back" data-act="back">${ic('back', 14)}<span>${esc(parentRow.title)}</span></button><i>/</i>
      <span class="here">${ic(icon, 13)}<span>${esc(k.title)}</span></span><span class="sp"></span><kbd class="dg-k">esc</kbd></div>
  </div>`;
}

/* ── The Delegated panel ─────────────────────────────────────────────── */

const glyph = (k) => {
  if (k.phase === 'starting') return ic('agent-starting', 14);
  if (k.phase === 'working') return `<span class="dg-run">${ic('agent-ring', 14)}<span class="dg-arc">${ic('agent-arc', 14)}</span></span>`;
  if (k.phase === 'asking') return '<span class="dg-dot"><i></i></span>';
  return ic('agent-done', 14);
};
const elapsed = (k) => `<span class="dg-el" data-t0="${k.t0}" data-t1="${k.t1}">${clock((k.t1 || Date.now()) - k.t0)}</span>`;

function headline(c) {
  const done = S.kids.filter((k) => k.phase === 'done').length;
  return c.asking ? `${c.asking} needs you` : c.working ? `${c.working} working` : `${done} done`;
}

function fillPanel() {
  if (!panelEl) return;
  if (!live() || S.view !== 'parent' || S.hidden) { panelEl.remove(); return; }
  const c = counts();
  const text = headline(c);
  const logos = [...new Set(S.kids.map((k) => k.agent))].slice(0, 3);
  const capW = 7 + 20 + 15 * (logos.length - 1) + 8 + 19 + text.length * 6.9 + 12;
  const rowsH = S.kids.reduce((h, k) => h + ROW_H + (k.phase === 'asking' ? 6 + QUESTION_LINE * (k.question.length > 38 ? 2 : 1) : 0), 0);
  const fullH = HEAD_H + rowsH + 6;
  const shownKid = S.kids.find((k) => k.phase === 'asking') ?? S.kids.find((k) => k.phase !== 'done') ?? S.kids[S.kids.length - 1];
  panelEl.querySelector('.dg-cap').innerHTML = `<span class="logos">${logos.map((a) => `<span class="lg">${ic(agentIcon(a), 10)}</span>`).join('')}</span><span class="sum${c.asking ? ' ask' : ''}">${glyph(shownKid)}${text}</span>`;
  panelEl.querySelector('.dg-cap').dataset.act = S.open ? '' : 'expand';
  panelEl.querySelector('.dg-full').innerHTML = `<div class="hd"><span class="t">Delegated</span><span class="n">${c.total}</span><span class="sp"></span>
      ${c.settled ? '<button class="txtb" data-act="clear">Clear</button>' : ''}<button class="ib" data-act="collapse" aria-label="Collapse">${ic('chevron-up', 12)}</button></div>
    <div class="rows">${S.kids.map((k) => `<button class="dg-row" data-act="child" data-v="${k.id}">
      <span class="lg">${ic(agentIcon(k.agent), 14)}</span>
      <span class="tx"><span class="tt">${esc(k.title)}</span><span class="meta"><span class="${k.phase === 'asking' ? 'needs' : ''}">${LABEL[k.phase]}</span>·${elapsed(k)}</span></span>
      <span class="end"><span class="gl">${glyph(k)}</span><span class="go">${ic('chevron-right', 12)}</span></span>
      ${k.phase === 'asking' ? `<span class="q">${esc(k.question)}</span>` : ''}</button>`).join('')}</div>`;
  panelEl.classList.toggle('open', S.open);
  panelEl.classList.add('shown');
  Object.assign(panelEl.style, { width: `${S.open ? FULL_W : capW}px`, height: `${S.open ? fullH : CAPSULE_H}px`, borderRadius: `${S.open ? 15 : CAPSULE_H / 2}px` });
}

function mountPanel(root) {
  const slot = root.querySelector('.dg-slot');
  if (!slot) return;
  if (S.hidden) return slot.remove();
  if (!panelEl) {
    panelEl = document.createElement('div');
    panelEl.className = 'dg-panel';
    panelEl.innerHTML = '<div class="dg-cap"></div><div class="dg-full"></div>';
  }
  slot.replaceWith(panelEl);
  // Laid out once with its last look, so the change fillPanel makes next animates.
  void panelEl.offsetWidth;
  fillPanel();
}

/* ── Push between the chat and a child ───────────────────────────────── */

function open(api, view, dir) {
  const now = api.$('.area .pane');
  snapshot = now && !reduced ? { node: now.cloneNode(true), dir } : null;
  S.view = view;
  api.draw();
}

function playPush(root) {
  if (!snapshot) return;
  const { node, dir } = snapshot;
  snapshot = null;
  const area = root.querySelector('.area');
  const arriving = area.querySelector('.pane');
  const side = dir === 'into' ? 1 : -1;
  node.classList.add('dg-leaving');
  area.append(node);
  const ease = { duration: 420, easing: 'cubic-bezier(0.22, 1, 0.36, 1)', fill: 'both' };
  arriving.animate([{ transform: `translateX(${28 * side}px)`, opacity: 0 }, { transform: 'none', opacity: 1 }], ease);
  node.animate([{ transform: 'none', opacity: 1 }, { transform: `translateX(${-28 * side}px)`, opacity: 0 }], ease).finished.then(() => node.remove(), () => node.remove());
}

// The tour answers the child's question, so the parent hears back from all three.
AnywhereTour.define('delegated', [
  ['wait', 2200],
  ['click', '.dg-row[data-v="c1"]'],
  ['wait', 900],
  ['click', '.dg-ask [data-act="pick"][data-v="Vitest"]'],
  ['wait', 1400],
  ['click', '.dg-crumb [data-act="back"]'],
]);

D.register('delegated', {
  tour: 'delegated',
  enter(api) {
    parentRow = api.all().find((r) => r.label === PARENT_LABEL) ?? api.all()[0];
    api.state.sel = parentRow;
    start();
  },
  render(api) {
    if (!S) start();
    return (S.view === 'parent' ? parentView(api) : childView(api)) + api.rail(null, {});
  },
  mount(root) {
    const sc = root.querySelector('.chat');
    if (sc) sc.scrollTop = sc.scrollHeight;
    mountPanel(root);
    playPush(root);
    if (S.view !== 'parent') root.querySelector('.dg-child')?.focus({ preventScroll: true });
  },
  act(act, v, t, api) {
    switch (act) {
      case 'child': return open(api, v, 'into');
      case 'back': return open(api, 'parent', 'back');
      case 'pick': return answerAsk(v);
      case 'expand': S.open = true; clearTimeout(S.collapse); return fillPanel();
      case 'collapse': S.open = false; return fillPanel();
      case 'clear': S.hidden = true; return fillPanel();
      case 'rail':
        if (v === 'changes' || v === 'prs') return api.go('review');
        if (v === 'sessions') return api.go('inbox');
    }
  },
  key(e, api) {
    if (e.key === 'Escape' && S.view !== 'parent' && !e.target.matches('textarea')) open(api, 'parent', 'back');
  },
});

// A press outside an open panel shuts it to its capsule.
document.addEventListener('mousedown', (e) => {
  if (!S || !live() || !S.open || !panelEl?.isConnected || panelEl.contains(e.target)) return;
  S.open = false;
  fillPanel();
});

// Elapsed times count up in place, without redrawing the screen.
setInterval(() => {
  if (!panelEl?.isConnected) return;
  panelEl.querySelectorAll('.dg-el').forEach((el) => {
    el.textContent = clock((+el.dataset.t1 || Date.now()) - +el.dataset.t0);
  });
}, 1000);
})();
