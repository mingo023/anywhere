// Inbox (the sidebar bell) and Activity (the page of every session by status). Both read the demo's live rows,
// so a session answered in one moves in the other and in the sidebar.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS } = AW;

const md = (s) => esc(s).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>').replace(/`(.+?)`/g, '<code>$1</code>');
const plain = (s) => s.replace(/[*`]/g, '');
const basename = (s) => s.split('/').pop();
const agentName = (a) => AGENTS[a][1];

// When each row last changed status, so the lists can say "4m" and order oldest first. Rows that open the demo already
// in a status get an age.
const SEED = { 'fix/flaky-checkout': 240, 'fix/resume-crash': 120, 'perf/invoice-cache': 10800, 'feat/upload-limit': 3600, 'feat/dark-mode': 90 };
const marks = new Map();
// A failed or done session stops alerting once seen, until it changes status again.
const seen = new Map();

function stamp(api) {
  for (const r of api.all()) {
    if (!r.status) { marks.delete(r.id); continue; }
    const m = marks.get(r.id);
    if (!m || m.status !== r.status) marks.set(r.id, { status: r.status, at: Date.now() - (m ? 0 : SEED[r.label] ?? 0) * 1000 });
  }
}
const at = (r) => marks.get(r.id)?.at ?? Date.now();
const unseen = (r) => seen.get(r.id) !== at(r);
const markSeen = (r) => seen.set(r.id, at(r));

const ago = (t) => {
  const m = Math.max(0, Date.now() - t) / 60000 | 0;
  return m < 1 ? 'now' : m < 60 ? `${m}m` : m < 1440 ? `${m / 60 | 0}h` : m < 2880 ? 'yesterday' : `${m / 1440 | 0}d`;
};
const clock = (t) => { const s = Math.max(0, Date.now() - t) / 1000 | 0; return `${s / 60 | 0}:${String(s % 60).padStart(2, '0')}`; };

const lastOf = (r, kind) => [...r.log].reverse().find((l) => l[0] === kind);

// Plays lines into a row's log one by one, as the sessions screen does for an answered ask.
function play(api, r, lines, end, ms) {
  r.status = 'working';
  lines.forEach((l, i) => setTimeout(() => {
    r.log.push(l);
    if (i === lines.length - 1) { r.status = end; r.worked = `${lines.length * 2}s`; }
    api.changed();
  }, ms * (i + 1)));
  api.changed();
}

function answer(api, r, v) {
  if (v === 'no') {
    r.status = undefined;
    r.log.push(['say', "Okay, I won't run it. What should I do instead?"]);
    return api.changed();
  }
  play(api, r, r.ask.after, 'done', 900);
}

/* ── Inbox ───────────────────────────────────────────────────────────── */

const ORDER = { needs: 0, failed: 1, done: 2 };
const FILTERS = [['all', 'All'], ['needs', 'Needs you'], ['failed', 'Failed'], ['done', 'Done']];
const HEADING = { needs: 'Needs you', failed: 'Failed', done: 'Done' };
const inbox = { filter: 'all', open: false, id: null, idx: 0 };

function notes(api) {
  stamp(api);
  return api.all()
    .filter((r) => r.status === 'needs' || ((r.status === 'failed' || r.status === 'done') && unseen(r)))
    .map((r) => {
      const [title, sub] = r.status === 'needs' ? [r.ask ? `Wants to run ${r.ask.cmd}` : (r.question ?? 'Needs your answer'), r.title]
        : r.status === 'failed' ? [`${r.title} failed`, plain(lastOf(r, 'err')?.[1] ?? '')]
        : [r.title, plain(lastOf(r, 'say')?.[1] ?? '')];
      return { row: r, status: r.status, title, sub, at: at(r) };
    })
    .sort((a, b) => ORDER[a.status] - ORDER[b.status] || a.at - b.at);
}

function shown(api) {
  const list = notes(api).filter((n) => inbox.filter === 'all' || n.status === inbox.filter);
  let i = list.findIndex((n) => n.row.id === inbox.id);
  if (i < 0) i = Math.min(inbox.idx, list.length - 1);
  inbox.idx = Math.max(i, 0);
  inbox.id = list[inbox.idx]?.row.id ?? null;
  return list;
}

const noteRow = (n, i, on) => `<button class="ibx-note${on ? ' on' : ''}" data-act="note" data-v="${i}">
  <span class="ibx-mark">${n.row.status === 'failed' ? `<span class="fail">${ic('x-bold', 11)}</span>` : n.status === 'done' ? '<span class="halo done"><i></i></span>' : '<span class="halo"><i></i></span>'}</span>
  <span class="ibx-text">
    <span class="ibx-top"><b>${esc(n.row.p.name)}</b><i>·</i><span class="prov">${agentName(n.row.agent)}</span><span class="ago">${ago(n.at)}</span></span>
    <span class="ibx-title">${esc(n.title)}</span>
    <span class="ibx-sub">${esc(n.sub)}</span>
  </span></button>`;

const TOOL_VERB = { Read: 'Read', Edit: 'Edited', Write: 'Wrote', Bash: 'Ran', Search: 'Searched' };
const toolIcon = { Read: 'file', Edit: 'pencil', Write: 'file-plus', Bash: 'terminal', Search: 'search' };

function chatTurns(r) {
  const out = [];
  let reply = null;
  for (const [kind, a, b] of r.log) {
    if (kind === 'you') { reply = null; out.push(`<div class="you"><div class="bubble">${esc(a)}</div><div class="meta">${ic('copy', 14)}<span>14:20</span></div></div>`); continue; }
    if (!reply) { reply = []; out.push(reply); }
    if (kind === 'say') reply.push(`<p>${md(a)}</p>`);
    else if (kind === 'tool') reply.push(`<div class="tool">${ic(toolIcon[a] ?? 'tool', 14)}<b>${TOOL_VERB[a] ?? a}</b><code>${esc(b)}</code></div>`);
    else if (kind === 'err') reply.push(`<div class="tool err">${ic('circle-x', 14)}<span>${esc(a)}</span></div>`);
    else reply.push(`<div class="tool out">${ic('corner-down-right', 14)}<span>${esc(a)}</span></div>`);
  }
  const [icon, name] = AGENTS[r.agent];
  return out.map((t, i) => {
    if (typeof t === 'string') return t;
    const last = i === out.length - 1;
    const head = last && r.status === 'working'
      ? `<div class="worked live"><span class="ag ${r.agent}">${ic(icon, 15)}</span><span class="shimmer">Working…</span></div>`
      : `<div class="worked"><span class="ag ${r.agent}">${ic(icon, 15)}</span>${name.split(' ')[0]} worked for ${r.worked ?? '48s'}${ic('chevron-right', 13)}</div>`;
    return `<div class="reply">${head}${t.join('')}</div>`;
  }).join('');
}

const permission = (r) => `<div class="perm">
  <div class="perm-h">${ic('shield', 15)}<b>Allow Bash?</b><span>${esc(r.title)}</span></div>
  <pre>$ ${esc(r.ask.cmd)}</pre>
  <div class="perm-b">
    <button class="pbtn" data-act="ans" data-v="no">Deny</button>
    <button class="pbtn" data-act="ans" data-v="always">Always allow <code>${esc(r.ask.rule)}</code></button>
    <button class="pbtn primary" data-act="ans" data-v="yes">Allow <kbd>↵</kbd></button>
  </div>
</div>`;

const dock = (r) => {
  const [icon] = AGENTS[r.agent];
  const model = r.agent === 'claude' ? '<b>Opus</b><span class="soft">High</span>' : r.agent === 'codex' ? '<b>GPT-5.5</b><span class="soft">Medium</span>' : `<b>${agentName(r.agent)}</b>`;
  return `<div class="dock"><div class="composer empty"><textarea rows="2" placeholder="Ask, build, / for commands, @ for files" aria-label="Message"></textarea>
    <div class="foot"><button class="chip"><span class="ag ${r.agent}">${ic(icon, 15)}</span>${model}${ic('chevron-down', 12)}</button><span class="sp"></span><button class="ib">${ic('plus', 17)}</button><button class="send">${ic('arrow-up', 16)}</button></div></div>
    <div class="where">${ic('worktree', 13)}<span>${esc(r.label)}</span></div></div>`;
};

const tline = ([kind, a, b]) =>
  kind === 'you' ? `<div class="l you">› ${esc(a)}</div>`
  : kind === 'tool' ? `<div class="l"><i class="b">•</i><span><b>${a === 'Bash' ? 'Ran' : TOOL_VERB[a] ?? a}</b> ${esc(b)}</span></div>`
  : kind === 'out' ? `<div class="l o">  └ ${esc(a)}</div>`
  : kind === 'err' ? `<div class="l o e">  └ ${esc(a)}</div>`
  : `<div class="l p">${md(a)}</div>`;

const termView = (r) => `<div class="scroll term"><div class="tcol">${r.log.map(tline).join('')}${
  r.status === 'needs' ? `<div class="allow"><b>Allow command?</b><span class="c">$ ${esc(r.ask.cmd)}</span><br>
    <button data-act="ans" data-v="yes"><span class="sel">› 1. Yes</span></button>
    <button data-act="ans" data-v="always">  2. Yes, always allow <span class="c">${esc(r.ask.rule)}</span></button>
    <button data-act="ans" data-v="no">  3. No, tell ${agentName(r.agent)} what to do differently</button></div><div class="l o">⏎ confirm · esc cancel</div>`
  : r.status === 'working' ? '<div class="l w"><i class="spin"></i> Working <span class="o">(esc to interrupt)</span></div>'
  : '<div class="l prompt">› <i class="caret"></i></div>'}</div></div>`;

const kbd = (...k) => k.map((x) => `<kbd class="ibx-k">${x}</kbd>`).join('');

function detail(api, n) {
  if (!n) return '<div class="ibx-detail"><div class="ibx-none">You\'re all caught up.</div></div>';
  const r = n.row;
  const pane = r.chat
    ? `<div class="scroll chat"><div class="col">${chatTurns(r)}${r.status === 'needs' ? permission(r) : ''}</div></div>${dock(r)}`
    : termView(r);
  return `<div class="ibx-detail">
    <div class="ibx-head">
      <span class="tile">${api.letter(r.p.name)}</span><span class="pj">${esc(r.p.name)}</span><i>/</i><b>${esc(r.title)}</b>
      ${n.status === 'needs' ? `<span class="ibx-pill"><span class="halo"><i></i></span>Needs you</span><span class="ibx-timer" data-at="${n.at}">${clock(n.at)}</span>` : ''}
      <span class="sp"></span>
      <button class="ibx-open" data-act="open">Open session${ic('forward', 14)}</button>
    </div>
    <div class="ibx-body">${pane}</div>
    <div class="ibx-hints">${r.chat ? 'Answer in the chat ·' : 'Answer in the terminal ·'}${kbd('J', 'K')}next / previous ·${kbd('⌘', '↵')}open session</div>
  </div>`;
}

function renderInbox(api) {
  const list = shown(api);
  const rows = [];
  list.forEach((n, i) => {
    if (inbox.filter === 'all' && (i === 0 || list[i - 1].status !== n.status)) {
      rows.push(`<div class="ibx-sec"><span>${HEADING[n.status]}</span><b>${list.filter((x) => x.status === n.status).length}</b></div>`);
    }
    rows.push(noteRow(n, i, i === inbox.idx));
  });
  if (inbox.filter !== 'all' && list.length) rows.unshift('');
  const filtered = inbox.filter !== 'all';
  return `<div class="ibx-list">
    <div class="ibx-lh"><b>Inbox</b>
      <span class="ibx-fw"><button class="ib${filtered ? ' on' : ''}" data-act="filter" aria-label="Filter">${ic('filter', 17)}</button>
        ${inbox.open ? `<div class="ibx-menu">${FILTERS.map(([f, label]) => `<button data-act="pick" data-v="${f}"><span class="ck">${inbox.filter === f ? ic('check', 13) : ''}</span>${label}</button>`).join('')}</div>` : ''}</span>
      <button class="ibx-ghost" data-act="seen">Mark all seen</button></div>
    <div class="ibx-notes" tabindex="-1">${rows.join('') || '<div class="ibx-empty">Nothing needs you.</div>'}</div>
  </div>${detail(api, list[inbox.idx])}`;
}

function openNote(api) {
  const n = shown(api)[inbox.idx];
  if (n) api.select(n.row);
}

D.register('inbox', {
  tour: 'inbox',
  enter() { inbox.open = false; },
  render: renderInbox,
  mount(root) {
    const sc = root.querySelector('.ibx-body .chat, .ibx-body .term');
    if (sc) sc.scrollTop = sc.scrollHeight;
    const a = document.activeElement;
    if (!a || a === document.body || root.contains(a)) root.querySelector('.ibx-notes')?.focus({ preventScroll: true });
  },
  act(act, v, t, api) {
    switch (act) {
      case 'note': {
        const n = shown(api)[+v];
        if (!n) return;
        if (n.status !== 'needs') markSeen(n.row);
        return api.select(n.row);
      }
      case 'open': return openNote(api);
      case 'filter': inbox.open = !inbox.open; return api.draw();
      case 'pick': Object.assign(inbox, { filter: v, open: false, id: null, idx: 0 }); return api.draw();
      case 'seen':
        notes(api).forEach((n) => { if (n.status !== 'needs') markSeen(n.row); });
        Object.assign(inbox, { id: null, idx: 0 });
        return api.changed();
      case 'ans': {
        const r = shown(api)[inbox.idx]?.row;
        return r && answer(api, r, v);
      }
    }
  },
  key(e, api) {
    if (e.target.matches('textarea, input')) return;
    const step = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[e.key];
    if (step) {
      e.preventDefault();
      const list = shown(api);
      inbox.id = list[Math.max(0, Math.min(list.length - 1, inbox.idx + step))]?.row.id;
      return api.draw();
    }
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); openNote(api); }
  },
});

// The elapsed time on a note that needs you counts up without redrawing the screen.
setInterval(() => {
  const el = D.api.$('.ibx-timer');
  if (el) el.textContent = clock(+el.dataset.at);
}, 1000);

/* ── Activity ────────────────────────────────────────────────────────── */

const COLS = [['idle', 'Idle'], ['working', 'Working'], ['needs', 'Needs you'], ['done', 'Done'], ['review', 'In review']];
const WHY = { done: 'unseen', review: 'PR open' };
const HINT = {
  idle: 'At the prompt, nothing unseen.', working: 'Agents mid-turn show up here.', needs: 'Questions and permissions land here.',
  done: 'Ended turns you haven’t seen yet.', review: 'Idle sessions with an open or draft PR.',
};

// Sessions the sidebar's rows don't cover: they sit at the prompt.
const EXTRA = [
  { key: 'x-loader', project: 'garden-mobile', agent: 'claude', branch: 'feat/image-loader', title: 'Lazy-load the gallery images', age: 1500, add: 41, del: 8,
    asked: 'Lazy-load the gallery images', read: 3, edited: 2, said: 'Added a lazy image list. It loads rows as they scroll into view.', files: [['src/gallery/List.tsx', 29, 5], ['src/gallery/useVisible.ts', 12, 3]] },
  { key: 'x-webhooks', project: 'docs-site', agent: 'codex', branch: 'docs/webhooks', title: 'Document the webhook retries', age: 7200, add: 0, del: 0,
    asked: 'Document the webhook retries', read: 4, edited: 0, said: 'The retry schedule is in the guide already. Nothing to change.' },
];

const act = { q: '', project: new Set(), agent: new Set(), menu: null, prev: null, enter: false, cols: new Map(), made: new Map() };
const prNumber = (r) => +(r.log.map((l) => l[1]).join(' ').match(/#(\d+)/)?.[1] ?? 214);

function items(api) {
  stamp(api);
  const rows = api.all().filter((r) => !r.local).map((r) => ({
    key: `r${r.id}`, row: r, project: r.p.name, agent: r.agent, branch: r.label, title: r.title, status: r.status,
    at: at(r), add: r.add, del: r.del, pr: r.pr ? prNumber(r) : 0,
    alerting: r.status === 'needs' || ((r.status === 'failed' || r.status === 'done') && unseen(r)),
  }));
  const extra = EXTRA.map((x) => ({ ...x, at: Date.now() - x.age * 1000, pr: act.made.get(x.key) ?? 0, alerting: false }));
  return rows.concat(extra);
}

const colOf = (i) => !i.alerting && i.status !== 'working' ? (i.pr ? 'review' : 'idle')
  : i.status === 'needs' || i.status === 'failed' ? 'needs' : i.status === 'working' ? 'working' : 'done';

const verbing = { Read: 'Reading', Edit: 'Editing', Write: 'Writing', Bash: 'Running', Search: 'Searching' };
function recap(i) {
  if (!i.row) return { asked: i.asked, read: i.read, edited: i.edited, said: i.said, files: i.files ?? [] };
  const log = i.row.log;
  const from = log.map((l) => l[0]).lastIndexOf('you');
  const turn = log.slice(Math.max(from, 0));
  const paths = (v) => [...new Set(turn.filter((l) => l[0] === 'tool' && v.includes(l[1])).map((l) => l[2]))];
  const edited = paths(['Edit', 'Write']);
  const per = (i.add || 0) / (edited.length || 1) | 0;
  return {
    asked: turn.find((l) => l[0] === 'you')?.[1] ?? '', read: paths(['Read']).length, edited: edited.length,
    said: plain([...turn].reverse().find((l) => l[0] === 'say')?.[1] ?? ''),
    files: edited.map((p) => [p, per, i.del / edited.length | 0]),
  };
}

function note(i) {
  if (i.row?.status === 'working') {
    const t = lastOf(i.row, 'tool');
    return t ? `${verbing[t[1]] ?? t[1]} ${basename(t[2])}` : 'Working…';
  }
  if (i.row?.status === 'failed') return plain(lastOf(i.row, 'err')?.[1] ?? '');
  return recap(i).said;
}

const pill = (inner, cls = '') => `<span class="act-pill${cls}">${inner}</span>`;
const diff = (a, d, cls = '') => `<span class="act-diff ${cls}"><b class="add">+${a}</b><b class="del">−${d}</b></span>`;
const badge = (name) => `<span class="act-badge">${esc(name[0].toUpperCase())}</span>`;

function colGlyph(c, busy) {
  if (c === 'idle') return ic('status-todo', 14);
  if (c === 'working') return `<span class="act-ring${busy ? ' rot' : ''}">${ic('spinner-ring', 12)}</span>`;
  if (c === 'needs') return '<span class="act-halo"><i></i></span>';
  if (c === 'done') return '<span class="act-dot"><i></i></span>';
  return ic('pull-request', 14);
}
const itemGlyph = (i, c) => i.status === 'failed' && c === 'needs' ? '<span class="act-halo bad"><i></i></span>' : colGlyph(c, true);

function askBox(i, cls = '') {
  const r = i.row;
  if (r?.status === 'failed') {
    const err = lastOf(r, 'err')?.[1];
    return `<div class="act-ask bad ${cls}"><div class="line"><b>Turn failed.</b> ${esc(err ?? '')}</div><div class="btns"><button class="sm primary" data-act="retry" data-v="${i.key}">Retry</button><button class="sm" data-act="dismiss" data-v="${i.key}">Dismiss</button></div></div>`;
  }
  if (r?.status !== 'needs') return '';
  return `<div class="act-ask ${cls}"><div class="line"><b>Bash</b> wants to run</div><code>${esc(r.ask.cmd)}</code>
    <div class="btns"><button class="sm" data-act="deny" data-v="${i.key}">Deny</button><button class="sm primary" data-act="allow" data-v="${i.key}">Allow once</button></div></div>`;
}

function card(i, c, first, landed) {
  const quick = c === 'done' ? `<button class="sm" data-act="clear" data-v="${i.key}">${ic('check', 11)}Seen</button>`
    : c === 'idle' && i.add && !i.pr && i.branch !== 'main' ? `<button class="sm" data-act="pr" data-v="${i.key}">${ic('pull-request', 12)}Create PR</button>` : '';
  const pills = (i.pr ? pill(`${ic('pull-request', 12)}#${i.pr}`) : '') + (i.add || i.del ? pill(diff(i.add, i.del), ' mono') : '');
  const ask = askBox(i);
  const text = ask ? '' : `<div class="act-note">${esc(note(i))}</div>`;
  return `<div class="act-cardw${first ? ' first' : ''}"><div class="act-card${landed ? ' land' : ''}${act.prev === i.key ? ' prev' : ''}${c === 'done' ? ' done' : ''}" data-act="prev" data-v="${i.key}">
    <div class="act-ctop">${badge(i.project)}<span class="pn">${esc(i.project)}</span><span class="ag">${ic(AGENTS[i.agent][0], 11)}${i.agent}</span><span class="when">${ago(i.at)}</span></div>
    <div class="act-ctitle">${esc(i.title)}</div>
    ${i.branch ? `<div class="act-branch">${ic('branch', 11)}<span>${esc(i.branch)}</span></div>` : ''}
    ${ask}${text}
    ${pills || quick ? `<div class="act-meta">${pills}${quick ? `<span class="q">${quick}</span>` : ''}</div>` : ''}
  </div></div>`;
}

const visible = (i) => (!act.project.size || act.project.has(i.project)) && (!act.agent.size || act.agent.has(i.agent))
  && (!act.q.trim() || `${i.title} ${i.project} ${i.branch}`.toLowerCase().includes(act.q.trim().toLowerCase()));

function columns(api) {
  const all = items(api);
  const groups = Object.fromEntries(COLS.map(([c]) => [c, []]));
  for (const i of all.filter(visible)) groups[colOf(i)].push(i);
  for (const g of Object.values(groups)) g.sort((a, b) => b.at - a.at);
  const moved = new Set();
  for (const [c, g] of Object.entries(groups)) g.forEach((i) => { if (act.cols.has(i.key) && act.cols.get(i.key) !== c) moved.add(i.key); });
  act.cols = new Map(Object.entries(groups).flatMap(([c, g]) => g.map((i) => [i.key, c])));
  return { all, groups, html: COLS.map(([c, name]) => {
    const g = groups[c];
    return `<div class="act-col"><div class="act-ch"><span class="gl">${colGlyph(c, g.length > 0)}</span><b>${name}</b><span class="n">${g.length}</span>${WHY[c] ? `<span class="why">${WHY[c]}</span>` : ''}</div>
      ${g.length ? '' : `<div class="act-hint">${HINT[c]}</div>`}
      <div class="act-cards">${g.map((i, n) => card(i, c, n === 0, moved.has(i.key))).join('')}</div></div>`;
  }).join('') };
}

const values = (all, menu) => {
  const m = new Map();
  all.forEach((i) => m.set(i[menu], (m.get(i[menu]) ?? 0) + 1));
  return [...m].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
};

function chip(menu, all) {
  const n = act[menu].size;
  const rows = values(all, menu).map(([v, count]) => `<button class="act-vr" data-act="tog" data-v="${menu}:${v}">
    <span class="tick${act[menu].has(v) ? ' on' : ''}">${act[menu].has(v) ? ic('check', 9) : ''}</span>
    <span class="lead">${menu === 'project' ? ic('folder', 13) : ic(AGENTS[v][0], 12)}</span><span class="nm">${esc(menu === 'project' ? v : agentName(v))}</span><span class="ct">${count}</span></button>`).join('');
  return `<span class="act-cw"><button class="act-chip${act.menu === menu || n ? ' on' : ''}" data-act="menu" data-v="${menu}">${menu === 'project' ? 'Project' : 'Agent'}${n ? ` · ${n}` : ''}${ic('chevron-down', 11)}</button>
    ${act.menu === menu ? `<div class="act-menu">${rows}</div>` : ''}</span>`;
}

function files(i, rc) {
  if (!rc.files.length) return '';
  return `<div class="act-box"><div class="bh">Changes<span class="sp"></span>${pill(diff(i.add, i.del), ' mono')}</div>
    ${rc.files.slice(0, 5).map(([p, a, d]) => `<div class="bf"><span>${esc(p)}</span>${diff(a, d)}</div>`).join('')}</div>`;
}

const CHECKS = [['build', 'ok'], ['test', 'ok'], ['lint', 'run']];
function prBox(i) {
  if (!i.pr) return '';
  return `<div class="act-box"><div class="bh">${ic('pull-request', 13)}<span>Pull request #${i.pr}</span><span class="sp"></span><span class="st">Checks running</span></div>
    ${[...CHECKS].sort((a, b) => (a[1] === 'run' ? 0 : 1) - (b[1] === 'run' ? 0 : 1)).map(([n, s]) =>
      `<div class="bc"><span class="gl">${s === 'ok' ? `<span class="ok">${ic('check', 13)}</span>` : '<span class="rot">' + ic('spinner-ring', 12) + '</span>'}</span><span class="nm">${n}</span><span class="dt">Details</span></div>`).join('')}</div>`;
}

function drawer(api, all) {
  const i = all.find((x) => x.key === act.prev);
  if (!i) return '';
  const c = colOf(i);
  const rc = recap(i);
  const label = i.status === 'failed' && c === 'needs' ? 'Failed' : COLS.find(([k]) => k === c)[1];
  const working = c === 'working';
  return `<div class="act-drawer${act.enter ? ' enter' : ''}">
    <div class="dh"><div class="what"><span class="chip2">${itemGlyph(i, c)}${label}</span><div class="t">${esc(i.title)}</div>
      <div class="m">${badge(i.project)}${esc(i.project)}<span class="ag">${ic(AGENTS[i.agent][0], 11)}${i.agent}</span>${ago(i.at)}</div>
      ${i.branch ? `<div class="br">${ic('branch', 11)}<span>${esc(i.branch)}</span></div>` : ''}</div>
      <button class="open" data-act="launch" data-v="${i.key}">Open session<i>↵</i></button>
      <button class="ib x" data-act="close" aria-label="Close">${ic('x', 15)}</button></div>
    <div class="db">
      ${askBox(i, 'big')}
      <div class="act-log">
        ${rc.asked ? `<div class="u"><span>${esc(rc.asked)}</span></div>` : ''}
        ${rc.read ? `<div class="tl">${ic('file', 13)}<b>Read</b>${rc.read} file${rc.read === 1 ? '' : 's'}</div>` : ''}
        ${rc.edited ? `<div class="tl">${ic('pencil', 13)}<b>Edited</b>${rc.edited} file${rc.edited === 1 ? '' : 's'}${i.add || i.del ? diff(i.add, i.del) : ''}</div>` : ''}
        ${working ? `<div class="tl busy">${ic('spinner', 13)}${esc(note(i))}</div>` : rc.said ? `<div class="s">${esc(rc.said)}</div>` : ''}
      </div>
      ${files(i, rc)}${prBox(i)}
    </div>
    <div class="dr"><input class="act-reply" placeholder="Reply to ${agentName(i.agent)}…" aria-label="Reply"><span>↵</span></div>
  </div>`;
}

function renderActivity(api) {
  const { all, html } = columns(api);
  const out = `<div class="pane act-page" data-act="bar">
    <div class="act-bar"><span class="t">${ic('activity', 14)}Activity</span>
      <span class="act-search">${ic('search', 12)}<input class="act-q" placeholder="Search sessions" value="${esc(act.q)}" aria-label="Search sessions"></span>
      ${chip('project', all)}${chip('agent', all)}</div>
    <div class="act-cols" data-act="bg">${html}</div>
    ${drawer(api, all)}
  </div>`;
  act.enter = false;
  return out;
}

const byKey = (api, k) => items(api).find((i) => i.key === k);
const reload = (api) => { const c = api.$('.act-cols'); if (c) c.innerHTML = columns(api).html; };

D.register('activity', {
  page: 'activity',
  tour: 'inbox',
  enter() { Object.assign(act, { menu: null, prev: null, enter: false, cols: new Map() }); },
  render: renderActivity,
  act(a, v, t, api) {
    const i = ['prev', 'allow', 'deny', 'retry', 'dismiss', 'clear', 'pr', 'launch'].includes(a) ? byKey(api, v) : null;
    switch (a) {
      case 'prev':
        act.enter = !act.prev;
        act.prev = v;
        act.menu = null;
        if (i?.row && i.alerting && i.status !== 'needs') markSeen(i.row);
        return api.draw();
      case 'bg': case 'close': act.prev = null; act.menu = null; return api.draw();
      case 'bar': if (!act.menu) return; act.menu = null; return api.draw();
      case 'menu': act.menu = act.menu === v ? null : v; return api.draw();
      case 'tog': {
        const [m, val] = [v.slice(0, v.indexOf(':')), v.slice(v.indexOf(':') + 1)];
        act[m].has(val) ? act[m].delete(val) : act[m].add(val);
        return api.draw();
      }
      case 'allow': return i && answer(api, i.row, 'yes');
      case 'deny': return i && answer(api, i.row, 'no');
      case 'retry':
        return i && play(api, i.row, [['tool', 'Edit', 'src/invoices/list.ts'], ['tool', 'Bash', 'pnpm test invoices'], ['say', 'Fixed. The cache now clears when a refund lands, so totals stay current.']], 'done', 900);
      case 'dismiss': case 'clear': if (i?.row) markSeen(i.row); return api.changed();
      case 'pr': act.made.set(v, 217); return api.draw();
      case 'launch': return api.select(i.row ?? api.PROJECTS.find((p) => p.name === i.project).rows[0]);
    }
  },
  input(e, api) {
    if (!e.target.matches('.act-q')) return;
    act.q = e.target.value;
    reload(api);
  },
  key(e, api) {
    if (e.key === 'Escape') { act.prev = act.menu = null; return api.draw(); }
    if (e.key === 'Enter' && e.target.matches('.act-reply')) {
      const text = e.target.value.trim();
      const i = text && byKey(api, act.prev);
      if (!i?.row) return;
      e.target.value = '';
      i.row.log.push(['you', text]);
      play(api, i.row, [['tool', 'Read', 'src/app.tsx'], ['say', 'Done. I made that change and the tests still pass.']], 'done', 1000);
    }
  },
});

// Statuses move on their own (a working session finishes, an answered ask runs): keep the lists current.
D.api.onChange(() => stamp(D.api));
D.api.alerts = () => notes(D.api).length;
})();
