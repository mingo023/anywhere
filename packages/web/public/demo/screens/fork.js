// Fork: a Claude Code chat hands its work to Codex by forking itself onto it with anywhere_thread_transfer. The new
// session opens from a context block built from the chat, works on its own in the same checkout, and sends a summary
// back with merge_back. Plays once on entry; Replay (or the tour tab) runs it again.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS } = AW;

const md = (s) => esc(s).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>').replace(/`(.+?)`/g, '<code>$1</code>')
  .replace(/\[(.+?)\]\(anywhere:\/\/session\/[\w-]+\)/g, '<button class="fk-link" data-act="open">$1</button>');
const TOOL_VERB = { Read: 'Read', Edit: 'Edited', Write: 'Wrote', Bash: 'Ran' };
const toolIcon = { Read: 'file', Edit: 'pencil', Write: 'file-plus', Bash: 'terminal' };
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

const PARENT_LABEL = 'feat/upload-limit';
const ASK = 'Hand the tests to Codex.';
const FORK_ID = 'e6b2f0a1-5c3d-4a79-8d14-9b7c2f3a6e58';
const FORK_TITLE = 'Upload limit tests';
const FORK_PROMPT = 'Write tests for the upload rate limit in src/routes/uploads.ts: 10 requests a minute per user, then 429 with Retry-After. Run them.';
const MCP_CLAUDE = 'anywhere · anywhere_thread_transfer';
const MCP_CODEX = 'anywhere.anywhere_thread_transfer';
const CLIP_H = 110;

let S = null;
let timers = [];
let snapshot = null;
let quiet = false;
let uid = 0;

const live = () => D.api.state.screen === 'fork';
let drew = false;
function refresh() {
  if (!live() || quiet) return;
  drew = true;
  D.api.draw();
}

const you = (text, extra = {}) => ({ k: 'you', id: uid++, text, t: Date.now(), ...extra });
const reply = (parts = [], extra = {}) => ({ k: 'reply', live: true, worked: '', parts, t: Date.now(), ...extra });
const part = (p) => ({ ...p, t: Date.now() });

function fromLog(log) {
  const turns = [];
  let cur = null;
  for (const [kind, a, b] of log) {
    if (kind === 'you') { cur = null; turns.push({ ...you(a), t: 0 }); continue; }
    if (kind !== 'say' && kind !== 'tool') continue;
    if (!cur) { cur = { ...reply([], { live: false, worked: '31s' }), t: 0 }; turns.push(cur); }
    cur.parts.push(kind === 'say' ? { k: 'say', text: a, t: 0 } : { k: 'tool', name: a, arg: b, t: 0 });
  }
  return turns;
}

// What pocketd builds from a session's items (tools_b.go, ContextBlock): one paragraph per message, a tool call as one line.
function contextBlock(title, turns) {
  const lines = [];
  for (const t of turns) {
    if (t.k === 'you') lines.push(`User: ${t.text}`);
    else for (const p of t.parts) {
      if (p.k === 'say') lines.push(`Assistant: ${p.text}`);
      else if (p.k === 'tool') lines.push(`Tool ${p.name}: ${p.arg}`);
    }
  }
  return `<context source="${title}">\n${lines.join('\n\n')}\n</context>`;
}

const MERGED = `Context merged back from session ${FORK_ID} (${FORK_TITLE}), since it split from this one:\n\n<context source="${FORK_TITLE}">\n`
  + 'Tool Bash: sed -n \'1,80p\' src/routes/uploads.ts\n\nTool Edit: tests/uploads.limit.test.ts\n\nTool Bash: pnpm test uploads\n\n'
  + 'Assistant: Done. 6 tests cover the limit and the Retry-After header, and they all pass. I sent a summary back to the Claude Code session.\n</context>';

function fresh(api) {
  const parent = api.all().find((r) => r.label === PARENT_LABEL) ?? api.all()[0];
  return { parent, fork: null, rowAt: 0, saved: parent.status, turns: { parent: fromLog(parent.log), fork: [] }, finished: false };
}

const last = (turns) => turns[turns.length - 1];

function insertFork(api) {
  const { parent } = S;
  S.fork = { id: 9001, p: parent.p, label: parent.label, title: FORK_TITLE, agent: 'codex', status: 'working', chat: true, add: 0, del: 0, log: [] };
  parent.p.rows.splice(parent.p.rows.indexOf(parent) + 1, 0, S.fork);
  parent.p.open = true;
  S.rowAt = Date.now();
}

function view(api, row, dir) {
  if (api.state.sel === row) return;
  const now = api.$('.area .pane');
  snapshot = now && !reduced && !quiet ? { node: now.cloneNode(true), dir } : null;
  api.state.sel = row;
  refresh();
}

function setCtx(api, t, open) {
  t.open = open;
  const el = api.$(`[data-ctx="${t.id}"]`);
  if (!el || quiet) return;
  const body = el.querySelector('.fk-body');
  body.style.height = `${body.offsetHeight}px`;
  void body.offsetHeight;
  el.classList.toggle('open', open);
  el.querySelector('.fk-more span').textContent = open ? 'Show less' : 'Show more';
  body.style.height = `${open ? body.firstElementChild.offsetHeight : CLIP_H}px`;
}

const steps = (api) => {
  const { parent } = S;
  const P = S.turns.parent, F = S.turns.fork;
  const forkReply = () => last(F);
  const claude = () => last(P);
  return [
    [400, () => { P.push(you(ASK), reply()); parent.status = 'working'; }],
    [1300, () => claude().parts.push(part({ k: 'say', text: "I'll fork this session onto Codex, in the same checkout. It starts from our conversation as context." }))],
    [2400, () => claude().parts.push(part({ k: 'mcp', label: MCP_CLAUDE, running: true }))],
    [3500, () => {
      const call = claude().parts.find((p) => p.k === 'mcp');
      call.running = false;
      const block = contextBlock(parent.title, P);
      F.push(you(`${block}\n\n${FORK_PROMPT}`, { ctx: true, open: false }), reply());
      insertFork(api);
    }],
    [3900, () => {
      claude().parts.push(part({ k: 'say', text: `Forked to Codex: [${FORK_TITLE}](anywhere://session/${FORK_ID}). It carries this conversation and works in the same checkout.` }));
      Object.assign(claude(), { live: false, worked: '3s' });
      parent.status = undefined;
    }],
    [5200, () => { api.$('.fk-link')?.classList.add('pressed'); }],
    [5400, () => view(api, S.fork, 'into')],
    [6400, () => setCtx(api, F[0], true)],
    [7500, () => forkReply().parts.push(part({ k: 'tool', name: 'Bash', arg: "sed -n '1,80p' src/routes/uploads.ts" }))],
    [8200, () => forkReply().parts.push(part({ k: 'tool', name: 'Edit', arg: 'tests/uploads.limit.test.ts' }))],
    [8900, () => forkReply().parts.push(part({ k: 'tool', name: 'Bash', arg: 'pnpm test uploads' }))],
    [9600, () => forkReply().parts.push(part({ k: 'mcp', label: MCP_CODEX, running: true }))],
    [10300, () => {
      forkReply().parts.find((p) => p.k === 'mcp').running = false;
      forkReply().parts.push(part({ k: 'say', text: 'Done. **6 tests** cover the limit and the `Retry-After` header, and they all pass. I sent a summary back to the Claude Code session.' }));
      Object.assign(forkReply(), { live: false, worked: '9s' });
      S.fork.status = 'done';
    }],
    [11800, () => {
      P.push(you(MERGED, { ctx: true, open: false }), reply());
      parent.status = 'working';
      view(api, parent, 'back');
    }],
    [12900, () => {
      claude().parts.push(part({ k: 'say', text: "Codex's tests are in `tests/uploads.limit.test.ts`: **6 tests pass**, covering the 10-a-minute limit and the `Retry-After` header." }));
      Object.assign(claude(), { live: false, worked: '2s' });
      parent.status = undefined;
      S.finished = true;
    }],
  ];
};

function teardown(api) {
  timers.forEach(clearTimeout);
  timers = [];
  if (!S) return;
  const { parent, fork } = S;
  if (fork) {
    fork.p.rows.splice(fork.p.rows.indexOf(fork), 1);
    if (api.state.sel === fork) api.state.sel = parent;
  }
  parent.status = S.saved;
  S = null;
  snapshot = null;
}

function start(api) {
  teardown(api);
  S = fresh(api);
  api.state.sel = S.parent;
  S.parent.status = undefined;
  const mine = S;
  const list = steps(api);
  if (reduced) {
    quiet = true;
    list.forEach(([, run]) => run());
    quiet = false;
    S.rowAt = 0;
    return;
  }
  list.forEach(([ms, run]) => timers.push(setTimeout(() => {
    if (S !== mine) return;
    drew = false;
    run();
    if (!drew) refresh();
  }, ms)));
}

/* ── Chat pieces ─────────────────────────────────────────────────────── */

const fresher = (t) => (t.t && Date.now() - t.t < 450 ? ' fresh' : '');
const meta = `<div class="meta">${ic('copy', 14)}<span>14:21</span></div>`;

function youView(t) {
  if (!t.ctx) return `<div class="you${fresher(t)}"><div class="bubble">${esc(t.text)}</div>${meta}</div>`;
  return `<div class="you${fresher(t)}"><div class="bubble fk-ctx${t.open ? ' open' : ''}" data-ctx="${t.id}">
    <div class="fk-body"${t.open ? ' style="height:auto"' : ''}><div class="fk-text">${esc(t.text)}</div></div>
    <div class="fk-dots">...</div>
    <button class="fk-more" data-act="ctx" data-v="${t.id}"><span>${t.open ? 'Show less' : 'Show more'}</span>${ic('chevron-down', 12)}</button></div>${meta}</div>`;
}

function partView(p) {
  const cls = fresher(p);
  if (p.k === 'say') return `<p class="${cls}">${md(p.text)}</p>`;
  if (p.k === 'mcp') return `<div class="tool${cls}">${ic('blocks', 14)}<code>${p.running ? `<span class="shimmer">${esc(p.label)}</span>` : esc(p.label)}</code></div>`;
  return `<div class="tool${cls}">${ic(toolIcon[p.name] ?? 'tool', 14)}<b>${TOOL_VERB[p.name] ?? p.name}</b><code>${esc(p.arg)}</code></div>`;
}

function replyView(t, agent, closing) {
  const [icon, name] = AGENTS[agent];
  const head = t.live
    ? `<div class="worked live"><span class="ag ${agent}">${ic(icon, 15)}</span><span class="shimmer">Working…</span></div>`
    : `<div class="worked"><span class="ag ${agent}">${ic(icon, 15)}</span>${name.split(' ')[0]} worked for ${t.worked}${ic('chevron-right', 13)}</div>`;
  const acts = !t.live && closing ? `<div class="acts">${ic('copy', 15)}${ic('reload', 15)}<span>· ${t.worked} · 14:21</span></div>` : '';
  return `<div class="reply${fresher(t)}">${head}${t.parts.map(partView).join('')}${acts}</div>`;
}

const dock = (agent, label) => {
  const [icon, name] = AGENTS[agent];
  const model = agent === 'claude' ? '<b>Opus</b><span class="soft">High</span>' : '<b>GPT-5.5</b><span class="soft">Medium</span>';
  return `<div class="dock"><div class="composer empty"><textarea rows="2" placeholder="Ask, build, / for commands, @ for files" aria-label="Message"></textarea>
    <div class="foot"><button class="chip"><span class="ag ${agent}">${ic(icon, 15)}</span>${model}${ic('chevron-down', 12)}</button><span class="sp"></span><button class="ib">${ic('plus', 17)}</button><button class="send">${ic('arrow-up', 16)}</button></div></div>
    <div class="where">${ic('worktree', 13)}<span>${esc(label)}</span></div></div>`;
};

function chat(api, row, turns) {
  const body = turns.map((t, i) => (t.k === 'you' ? youView(t) : replyView(t, row.agent, i === turns.length - 1))).join('');
  const replay = S.finished && !reduced ? `<button class="fk-replay" data-act="replay">${ic('reload', 13)}Replay</button>` : '';
  return `<div class="pane fk-pane">
    <div class="bar">${api.tab(row)}<button class="ib" aria-label="New tab">${ic('plus', 16)}</button><span class="fk-sp"></span>${replay}</div>
    <div class="scroll chat"><div class="col">${body}</div></div>
    ${dock(row.agent, row.label)}
  </div>`;
}

/* ── Push between the two chats ──────────────────────────────────────── */

function playPush(root) {
  if (!snapshot) return;
  const { node, dir } = snapshot;
  snapshot = null;
  const area = root.querySelector('.area');
  const arriving = area.querySelector('.pane');
  const side = dir === 'into' ? 1 : -1;
  node.classList.add('fk-leaving');
  area.append(node);
  const ease = { duration: 420, easing: 'cubic-bezier(0.22, 1, 0.36, 1)', fill: 'both' };
  arriving.animate([{ transform: `translateX(${28 * side}px)`, opacity: 0 }, { transform: 'none', opacity: 1 }], ease);
  node.animate([{ transform: 'none', opacity: 1 }, { transform: `translateX(${-28 * side}px)`, opacity: 0 }], ease).finished.then(() => node.remove(), () => node.remove());
}

D.register('fork', {
  sessionScreen: true,
  tour: 'fork',
  enter(api) { start(api); },
  render(api) {
    if (!S) start(api);
    const { sel } = api.state;
    if (sel !== S.parent && sel !== S.fork) queueMicrotask(() => D.go('sessions'));
    const onFork = sel === S.fork;
    return chat(api, onFork ? S.fork : S.parent, onFork ? S.turns.fork : S.turns.parent) + api.rail(null, {});
  },
  mount(root) {
    const sc = root.querySelector('.chat');
    if (sc) sc.scrollTop = sc.scrollHeight;
    const row = S?.rowAt && Date.now() - S.rowAt < 700 ? root.querySelector(`.row.wt[data-v="${S.fork.id}"]`) : null;
    row?.classList.add('fk-new');
    playPush(root);
  },
  act(act, v, t, api) {
    switch (act) {
      case 'open': return S.fork && view(api, S.fork, 'into');
      case 'replay':
        start(api);
        return api.draw();
      case 'ctx': {
        const turn = [...S.turns.parent, ...S.turns.fork].find((x) => x.id === +v);
        return turn && setCtx(api, turn, !turn.open);
      }
      case 'rail':
        if (v === 'changes' || v === 'prs') return api.go('review');
        if (v === 'sessions') return api.go('inbox');
    }
  },
});

// Leaving the screen stops the scene and takes the forked session out of the sidebar.
document.addEventListener('desktop:screen', (e) => {
  if (e.detail === 'fork' || !S) return;
  teardown(D.api);
  D.api.draw();
});
})();
