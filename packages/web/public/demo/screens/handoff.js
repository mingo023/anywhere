// Handoff: a Claude Code session handed to a connected host. The tab's menu opens on "Hand off to a host…", the
// sheet checks devbox and lists what moves, its steps tick through, the mascot cheers, and "Open on devbox" shows
// the same chat there, taking another prompt. The sheet, menu and pointer live in one layer on the window, not in
// the screen's draw, so a redraw from elsewhere never restarts their motion. Leaving the screen stops it all.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS } = AW;
const win = document.querySelector('.window.desktop');

const md = (s) => esc(s).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>').replace(/`(.+?)`/g, '<code>$1</code>');
const TOOL_VERB = { Read: 'Read', Edit: 'Edited', Write: 'Wrote', Bash: 'Ran' };
const toolIcon = { Read: 'file', Edit: 'pencil', Write: 'file-plus', Bash: 'terminal' };
const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;
const megabytes = (b) => (b < 100_000 ? `${Math.max(1, (b / 1000) | 0)} KB` : `${(b / 1e6).toFixed(1)} MB`);

const HOST = { name: 'devbox', address: 'dev@devbox.local' };
const FACTS = { commits: 2, files: 3, messages: 4, bundle: 86_000, conversation: 1_240_000 };
const LOCK = 'Prompts wait until the handoff finishes';
const ASK = 'Now add a test for the 429 path';
const ANSWER = [['tool', 'Write', 'tests/uploads.limit.test.ts'], ['tool', 'Bash', 'pnpm test uploads'], ['say', 'Added the test. **13 tests pass** on devbox.']];
// The steps handoff::steps lists for a session that isn't mid-turn and whose conversation moves, with how long each takes here.
const STEPS = [['pack', 600], ['send', 700], ['worktree', 600], ['conversation', 1500], ['start', 600], ['check', 600]];
const CHEER = ['#f5b83d', '#8ee6cc', '#7a68bf', '#f27a8b', '#5aa9f5', '#9bd16a'];

const BASE = [
  ['you', 'Rate-limit POST /uploads to 10 a minute per user'],
  ['reply', '21s', [['tool', 'Read', 'src/routes/uploads.ts'], ['tool', 'Edit', 'src/routes/uploads.ts'], ['say', 'Added a limiter: 10 requests a minute per user. Over it, the route returns `429`.']]],
  ['you', 'Send Retry-After too, and run the tests'],
  ['reply', '38s', [['tool', 'Edit', 'src/routes/uploads.ts'], ['tool', 'Bash', 'pnpm test uploads'], ['say', 'Done. Over the limit, the route now returns `429` with `Retry-After`.']]],
];

let S = null;
let row = null;
let timers = [];

const layer = document.createElement('div');
layer.className = 'app ho-layer';
layer.innerHTML = '<div class="ho-scrim" data-act="stop" hidden></div><div class="ho-menu" hidden></div><div class="ho-sheet" hidden></div>'
  + '<div class="ho-cursor"><svg width="20" height="20" viewBox="0 0 20 20"><path d="M3 2l12 7.2-5.2 1.3L7.4 16z" fill="#fff" stroke="#000" stroke-width="1.2" stroke-linejoin="round"/></svg><i></i></div>';
const [scrim, menu, sheet, cursor] = layer.children;

const fresh = () => ({ stage: 'chat', probe: 'checking', at: -1, sent: null, cheers: 0, hot: '', draft: '', over: false, items: BASE.map((i) => [...i]) });
const live = () => D.api.state.screen === 'handoff';
const refresh = () => { if (live()) D.api.draw(); };
function later(ms, fn) {
  const mine = S;
  timers.push(setTimeout(() => { if (S === mine) fn(); }, ms));
}
function clear() {
  timers.forEach(clearTimeout);
  timers = [];
}

/* ── Chat pieces, as delegated.js draws them ─────────────────────────── */

const parts = (list) => list.map(([k, a, b]) =>
  k === 'say' ? `<p>${md(a)}</p>`
  : `<div class="tool">${ic(toolIcon[a] ?? 'tool', 14)}<b>${TOOL_VERB[a] ?? a}</b><code>${esc(b)}</code></div>`).join('');

const you = (text) => `<div class="you"><div class="bubble">${esc(text)}</div><div class="meta">${ic('copy', 14)}<span>14:21</span></div></div>`;

function reply(worked, list, busy) {
  const [icon, name] = AGENTS[row.agent];
  const head = busy
    ? `<div class="worked live"><span class="ag ${row.agent}">${ic(icon, 15)}</span><span class="shimmer">Working…</span></div>`
    : `<div class="worked"><span class="ag ${row.agent}">${ic(icon, 15)}</span>${name.split(' ')[0]} worked for ${worked}${ic('chevron-right', 13)}</div>`;
  return `<div class="reply">${head}${parts(list)}${busy ? '' : `<div class="acts">${ic('copy', 15)}${ic('reload', 15)}<span>· ${worked} · 14:21</span></div>`}</div>`;
}

// What the tab says under the title while the session moves: Mark::tab.
function tab() {
  const [icon, name] = AGENTS[row.agent];
  const sub = S.stage === 'moving' ? `${name} · moving to ${HOST.name}` : name;
  return `<div class="tab"><span class="ag ${row.agent}">${ic(icon, 15)}</span><span class="tt"><b>${esc(row.title)}</b><small>${esc(sub)}</small></span></div>`;
}

// Stands for the space switcher at the sidebar's foot: Open on devbox moves the app to devbox's space.
const space = (host) => host
  ? `<span class="ho-space"><i class="ho-tile">${ic('server', 11)}</i>${HOST.name}<b class="ho-dot"></b></span>`
  : `<span class="ho-space">${ic('laptop', 13)}This Mac</span>`;

function composer(locked) {
  const [icon] = AGENTS[row.agent];
  return `<div class="composer${locked ? ' locked' : S.draft ? '' : ' empty'}"><textarea rows="2" placeholder="${locked ? LOCK : 'Ask, build, / for commands, @ for files'}" aria-label="Message"${locked ? ' disabled' : ''}>${esc(S.draft)}</textarea>
    <div class="foot"><button class="chip"><span class="ag ${row.agent}">${ic(icon, 15)}</span><b>Opus</b><span class="soft">High</span>${ic('chevron-down', 12)}</button><span class="sp"></span><button class="ib">${ic('plus', 17)}</button><button class="send">${ic('arrow-up', 16)}</button></div></div>`;
}

function pane() {
  const host = S.stage === 'host';
  const body = S.items.map(([k, a, b, busy]) => (k === 'you' ? you(a) : reply(a, b, busy))).join('');
  return `<div class="pane ho-pane">
    <div class="bar">${tab()}<button class="ib" aria-label="New tab">${ic('plus', 16)}</button><span class="sp"></span>${S.over ? `<button class="ho-replay" data-act="replay">${ic('reload', 13)}Replay</button>` : ''}${space(host)}</div>
    <div class="scroll chat"><div class="col">${body}</div></div>
    <div class="dock">${composer(S.stage === 'moving')}<div class="where">${ic('worktree', 13)}<span>${esc(row.label)}</span></div></div>
  </div>`;
}

/* ── The sheet: view.rs's, step by step ──────────────────────────────── */

const x = (act = 'stop') => `<button class="ho-x" data-act="${act}" aria-label="Close">${ic('x', 15)}</button>`;
const head = (title) => `<div class="ho-h"><h2>${esc(title)}</h2>${x()}</div>`;
const hot = (id) => (S.hot === id ? ' hov' : '');
const agentName = () => AGENTS[row.agent][1];

function pickSheet() {
  const ready = S.probe === 'ready';
  const tag = ready ? `<span class="ho-tag ok">${ic('check', 12)}Ready</span>` : '<span class="ho-tag mute"><i class="ho-ring sm"></i>Checking…</span>';
  return `${head('Hand off to a host')}
    <p class="ho-sub">“${esc(row.title)}” · ${agentName()}</p>
    <div class="ho-lab">Host</div>
    <div class="ho-box"><div class="ho-host on"><span class="ho-radio on"></span><span class="ho-htile">${ic('server', 14)}<b class="ho-dot"></b></span>
      <span class="ho-hx"><span class="n">${HOST.name}</span><span class="a">${HOST.address}</span></span>${tag}</div></div>
    <div class="ho-lab">What moves</div>
    <div class="ho-box">
      <div class="ho-mv">${ic('branch', 13)}<span class="tx mono">${esc(row.label)}</span><span class="mt">${plural(FACTS.commits, 'commit', 'commits')}</span></div>
      <div class="ho-mv">${ic('file', 13)}<span class="tx">${plural(FACTS.files, 'changed file', 'changed files')}</span><span class="mt stat"><b class="add">+${row.add}</b><b class="del">−${row.del}</b></span></div>
      <div class="ho-mv">${ic('comment', 13)}<span class="tx">Conversation</span><span class="mt">${plural(FACTS.messages, 'message', 'messages')}</span></div>
    </div>
    <div class="ho-foot"><button class="ho-btn ghost" data-act="stop">Cancel</button><button class="ho-btn primary${ready ? hot('go') : ' wait'}">Hand off</button></div>`;
}

function stepRow([id], i) {
  const now = i === S.at, todo = i > S.at;
  const sending = id === 'conversation' && now && S.sent != null;
  const label = { pack: 'Pack the code', send: `Send to ${HOST.name}`, worktree: 'Make the worktree', conversation: 'Bring the conversation', start: `Start ${agentName()}`, check: 'Check the history' }[id];
  const detail = id === 'pack' ? `${plural(FACTS.files, 'file', 'files')} · ${plural(FACTS.commits, 'commit', 'commits')}`
    : id === 'send' ? (S.at > 0 ? megabytes(FACTS.bundle) : '')
    : id === 'conversation' ? (sending ? `<span data-sent>${megabytes(S.sent)} of ${megabytes(FACTS.conversation)}</span>` : plural(FACTS.messages, 'message', 'messages'))
    : '';
  const mark = now ? '<i class="ho-ring"></i>' : todo ? '<i class="ho-todo"></i>' : ic('check', 13);
  return `<div class="ho-st${now ? ' now' : todo ? ' todo' : ''}"><span class="mk${now || todo ? '' : ' ok'}">${mark}</span><span class="tx"><b>${label}</b>${detail ? `<span>${detail}</span>` : ''}${sending ? `<span class="ho-bar"><i style="width:${(S.sent / FACTS.conversation) * 100}%"></i></span>` : ''}</span></div>`;
}

function movingSheet() {
  const cancellable = !['start', 'check'].includes(STEPS[S.at]?.[0]);
  return `${head(`Handing off to ${HOST.name}`)}
    <div class="ho-box"><div class="ho-steps">${STEPS.map(stepRow).join('')}</div></div>
    <div class="ho-safe">${ic('laptop', 13)}<span>Keep this Mac awake until it’s done.</span></div>
    <div class="ho-foot">${cancellable ? '<button class="ho-btn ghost" data-act="stop">Cancel handoff</button>' : ''}<button class="ho-btn sec" data-act="stop">Hide</button></div>`;
}

// The mascot drops in on the first cheer and hops on each click after; both throw confetti.
function mascot() {
  const still = reduced();
  const hop = S.cheers > 0;
  const pieces = still ? '' : Array.from({ length: 18 }, (_, i) => {
    const angle = -Math.PI * (0.08 + (0.84 * i) / 17);
    const reach = 70 + 26 * ((i * 7) % 5);
    return `<i style="--x:${(Math.cos(angle) * reach).toFixed(1)}px;--y:${(Math.sin(angle) * reach + 60).toFixed(1)}px;background:${CHEER[i % 6]}"></i>`;
  }).join('');
  return `<button class="ho-stage${still ? ' still' : ''}" data-act="cheer" aria-label="Cheer"><span class="glow"></span><span class="shadow"></span>
    <span class="ho-mas ${still ? '' : hop ? `hop-${S.cheers % 2 ? 'a' : 'b'}` : 'drop'}"><svg viewBox="0 0 200 200" width="140" height="140">
      <circle cx="131" cy="19" r="17" fill="#8ee6cc" opacity=".55"/>
      <g fill="#1a1a1e"><path d="M110 48C112 36 118 28 127 23" stroke="#1a1a1e" stroke-width="7" fill="none" stroke-linecap="round"/><circle cx="131" cy="19" r="11"/><path d="M26 130C26 78 60 46 104 46C150 46 178 80 178 128C178 152 166 166 150 162C138 159 134 148 120 150C106 152 104 168 88 168C72 168 70 152 56 154C42 156 26 156 26 130Z"/></g>
      <path d="M40 112C42 80 66 58 98 56C78 66 60 86 54 114C52 124 40 124 40 112Z" fill="#fff" opacity=".08"/>
      <circle cx="131" cy="19" r="6.5" fill="#8ee6cc"/>
      <g fill="none" stroke="#f4f2ee" stroke-width="10" stroke-linecap="round"><path d="M75 104q11-17 22 0" transform="rotate(8 86 98)"/><path d="M111 106q11-17 22 0" transform="rotate(8 122 100)"/></g></svg></span>
    <span class="confetti">${pieces}</span></button>`;
}

function doneSheet() {
  return `<div class="ho-top">${x()}</div>
    <div class="ho-cheer">${mascot()}<h2>${agentName()} made it to ${HOST.name}!</h2><p>It picks up where you left off. You can close this Mac.</p></div>
    <div class="ho-foot"><button class="ho-btn ghost" data-act="stop">Done</button><button class="ho-btn primary${hot('open')}" data-act="open">Open on ${HOST.name}</button></div>`;
}

const sheetHTML = () => ({ pick: pickSheet, moving: movingSheet, done: doneSheet })[S.stage]?.() ?? null;

const menuHTML = () => `<button class="ho-mi">${ic('terminal', 15)}<span>Open in Terminal chat</span></button><button class="ho-mi${hot('item')}">${ic('server', 15)}<span>Hand off to a host…</span></button>`;

/* ── The layer: sheet, menu and pointer ──────────────────────────────── */

function put(node, html, from) {
  if (html == null) { node.hidden = true; node._h = null; return false; }
  const entering = node.hidden;
  node.hidden = false;
  if (node._h !== html) { node.innerHTML = html; node._h = html; }
  if (entering && !reduced()) node.animate(from, { duration: 200, easing: 'cubic-bezier(0.22, 1, 0.36, 1)' });
  return true;
}

function paint() {
  const modal = ['pick', 'moving', 'done'].includes(S.stage);
  put(scrim, modal ? '' : null, [{ opacity: 0 }, { opacity: 1 }]);
  put(sheet, modal ? sheetHTML() : null, [{ opacity: 0, transform: 'translateX(-50%) scale(0.97)' }, { opacity: 1, transform: 'translateX(-50%)' }]);
  const showMenu = put(menu, S.stage === 'menu' ? menuHTML() : null, [{ opacity: 0, marginTop: '-4px' }, { opacity: 1, marginTop: 0 }]);
  if (showMenu) {
    const t = box(D.api.$('.ho-pane .tab'));
    Object.assign(menu.style, { left: `${t.x + t.w * 0.55}px`, top: `${t.y + t.h * 0.6}px` });
  }
  if (S.stage === 'host') cursor.style.opacity = 0;
}

// An element's place in the window's own points, whatever the scale it is drawn at.
function box(node) {
  const l = layer.getBoundingClientRect(), k = l.width / layer.offsetWidth || 1, r = node.getBoundingClientRect();
  return { x: (r.left - l.left) / k, y: (r.top - l.top) / k, w: r.width / k, h: r.height / k };
}

function aim(node, fx, fy, ms) {
  const b = box(node);
  cursor.style.transitionDuration = `${ms}ms`;
  cursor.style.transform = `translate(${b.x + b.w * fx - 3}px, ${b.y + b.h * fy - 2}px)`;
  cursor.style.opacity = 1;
}

function tap() {
  if (reduced()) return;
  cursor.firstElementChild.animate([{ transform: 'scale(1)' }, { transform: 'scale(0.8)' }, { transform: 'scale(1)' }], { duration: 240 });
  cursor.lastElementChild.animate([{ opacity: 0.5, transform: 'translate(-50%, -50%) scale(0.3)' }, { opacity: 0, transform: 'translate(-50%, -50%) scale(1.6)' }], { duration: 420, easing: 'ease-out' });
}

/* ── The scene ───────────────────────────────────────────────────────── */

function type(text, ms, done) {
  const step = ms / text.length;
  for (let i = 1; i <= text.length; i++) {
    later(step * i, () => {
      S.draft = text.slice(0, i);
      const field = D.api.$('.ho-pane textarea');
      if (field) field.value = S.draft;
      D.api.$('.ho-pane .composer')?.classList.toggle('empty', !S.draft);
    });
  }
  later(ms, done);
}

function converse() {
  const asked = () => { S.draft = ''; S.items.push(['you', ASK], ['reply', '', [], true]); refresh(); };
  const grow = (part, ms) => later(ms, () => { S.items.at(-1)[2].push(part); refresh(); });
  later(0, () => type(ASK, 700, () => later(250, () => {
    asked();
    grow(ANSWER[0], 450);
    grow(ANSWER[1], 1050);
    grow(ANSWER[2], 1650);
    later(2000, () => { Object.assign(S.items.at(-1), { 1: '6s', 3: false }); S.over = true; refresh(); });
  })));
}

function openHost() {
  clear();
  S.stage = 'host';
  S.hot = '';
  paint();
  refresh();
}

function finish() {
  S.stage = 'done';
  S.at = STEPS.length;
  S.hot = '';
  paint();
  refresh();
  later(650, () => aim(sheet.querySelector('[data-act="open"]'), 0.5, 0.5, 500));
  later(1150, () => { S.hot = 'open'; paint(); });
  later(1350, () => {
    tap();
    openHost();
    converse();
  });
}

function move() {
  S.stage = 'moving';
  S.hot = '';
  S.at = 0;
  paint();
  refresh();
  let t = 0;
  STEPS.forEach(([id, ms], i) => {
    if (i) later(t, () => { S.at = i; S.sent = null; paint(); });
    if (id === 'conversation') {
      for (let k = 0; k <= 5; k++) {
        later(t + (ms * k) / 6, () => {
          S.sent = (FACTS.conversation * k) / 5;
          const fill = sheet.querySelector('.ho-bar i');
          const say = sheet.querySelector('[data-sent]');
          if (fill && say) {
            fill.style.width = `${(S.sent / FACTS.conversation) * 100}%`;
            say.textContent = `${megabytes(S.sent)} of ${megabytes(FACTS.conversation)}`;
          } else paint();
        });
      }
    }
    t += ms;
  });
  later(t, finish);
}

function script() {
  const t = D.api.$('.ho-pane .tab');
  cursor.style.transitionDuration = '0ms';
  cursor.style.opacity = 0;
  cursor.style.transform = 'translate(1010px, 640px)';
  void cursor.offsetWidth;
  later(300, () => aim(t, 0.55, 0.6, 750));
  later(1250, () => { tap(); S.stage = 'menu'; paint(); });
  later(1750, () => aim(menu.lastElementChild, 0.4, 0.5, 450));
  later(2250, () => { S.hot = 'item'; paint(); });
  later(2500, () => { tap(); S.stage = 'pick'; S.hot = ''; paint(); });
  later(3150, () => { S.probe = 'ready'; paint(); });
  later(3400, () => aim(sheet.querySelector('.ho-btn.primary'), 0.5, 0.5, 500));
  later(3950, () => { S.hot = 'go'; paint(); });
  later(4150, () => { tap(); move(); });
}

function start() {
  clear();
  S = fresh();
  cursor.style.opacity = 0;
  if (reduced()) {
    Object.assign(S, { stage: 'done', probe: 'ready', at: STEPS.length });
    paint();
    return;
  }
  paint();
  // The first draw hasn't mounted when enter runs; the scene starts once it has.
  later(0, script);
}

function stop() {
  clear();
  S.stage = 'chat';
  S.hot = '';
  S.over = true;
  paint();
  cursor.style.opacity = 0;
  refresh();
}

function leave() {
  clear();
  S = null;
  layer.remove();
}

document.addEventListener('desktop:screen', (e) => { if (e.detail !== 'handoff') leave(); });

function act(a) {
  if (!S) return;
  switch (a) {
    case 'stop': return stop();
    case 'open': clear(); S.items = [...BASE.map((i) => [...i]), ['you', ASK], ['reply', '6s', ANSWER, false]]; S.over = true; cursor.style.opacity = 0; return openHost();
    case 'replay': start(); return refresh();
    case 'cheer': S.cheers++; return paint();
  }
}

layer.addEventListener('click', (e) => {
  const t = e.target.closest('[data-act]');
  if (t) act(t.dataset.act);
});

D.register('handoff', {
  tour: 'handoff',
  enter(api) {
    row = api.all().find((r) => r.p.name === 'fox-api' && r.label === 'feat/upload-limit') ?? api.all()[0];
    api.state.sel = row;
    if (layer.parentNode !== win) win.append(layer);
    start();
  },
  render() {
    return pane() + D.api.rail(null, { changes: FACTS.files });
  },
  mount(root) {
    const sc = root.querySelector('.chat');
    if (sc) sc.scrollTop = sc.scrollHeight;
    if (S.stage === 'moving') {
      const glyph = root.querySelector('.side .row.wt.on .glyph');
      if (glyph) glyph.innerHTML = ic('server', 13);
    }
  },
  act(a) { act(a); },
});
})();
