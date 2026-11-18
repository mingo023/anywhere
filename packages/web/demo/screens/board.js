// Board: tickets across projects in columns by state. A ticket is written in the dock, started to give it a worktree and
// a session (a row in the sidebar), and its card moves column as that session works, asks, and finishes.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS } = AW;

const LIMIT = 3;
const LABELS = ['Bug', 'Feature', 'Docs', 'Infra'];
const LABEL_COLOR = { Bug: 'var(--failed)', Docs: 'var(--waiting)', Infra: 'var(--goal)', Feature: 'var(--merged)' };
const PRIORITIES = [1, 2, 3, 4, 0];
const PRIORITY_NAME = ['No priority', 'Urgent', 'High', 'Medium', 'Low'];
const PRIORITY_ICON = ['priority-none', 'priority-urgent', 'priority-high', 'priority-medium', 'priority-low'];
const COLUMNS = [['todo', 'To do'], ['queued', 'Queued'], ['working', 'Working'], ['needs', 'Needs you'], ['done', 'Done'], ['review', 'In review']];
const COL_GLYPH = { todo: ['status-todo', 'var(--t4)'], queued: ['status-queued', 'var(--t3)'], working: ['status-half', 'var(--working)'], needs: ['status-needs', 'var(--waiting)'], done: ['status-three-quarters', 'var(--success)'], review: ['status-full', 'var(--goal)'] };
const COL_HINT = { todo: 'Write a ticket below, or press N.', queued: 'Tickets wait here until a slot frees up.', working: 'Tickets you start show up here.', needs: 'Questions from your agents land here.', done: 'New results land here.', review: "Results you've opened wait here." };
const FILTERS = [['status', 'Status', 'status-half'], ['agent', 'Agent', 'sparkle'], ['project', 'Project', 'folder'], ['priority', 'Priority', 'priority-medium'], ['label', 'Labels', 'pin']];
const PROPS = [['id', 'ID'], ['priority', 'Priority'], ['labels', 'Labels'], ['project', 'Project'], ['agent', 'Agent']];
const TABS = [['all', 'All tickets'], ['active', 'Active'], ['todo', 'To do']];

const ticket = (o) => ({ prio: 0, labels: [], agent: 'claude', prompt: '', ...o });
let serial = 10;
const tickets = [
  ticket({ n: 1, title: 'Virtualize the file tree', project: 'orbit-web', labels: ['Feature'], prio: 3, state: 'todo', prompt: 'The Files panel builds every row, which stalls on big repos. Virtualize it so only the visible rows render.' }),
  ticket({ n: 2, title: 'Retry the upload with backoff', project: 'fox-api', labels: ['Infra'], prio: 2, agent: 'codex', state: 'todo', prompt: 'Retry failed uploads with exponential backoff and jitter, up to five times.' }),
  ticket({ n: 3, title: 'Fix the scroll jump on resize in chat', project: 'orbit-web', labels: ['Bug'], prio: 1, state: 'queued', prompt: 'Resizing the window while a chat is scrolled back jumps the view to the top.' }),
  ticket({ n: 4, title: 'Version the settings store', project: 'garden-mobile', agent: 'codex', state: 'queued', prompt: 'Add a version field to the stored settings and migrate older shapes on load.' }),
  ticket({ n: 5, title: 'Add dark mode to settings', project: 'orbit-web', labels: ['Feature'], prio: 3, state: 'live', row: 'feat/dark-mode' }),
  ticket({ n: 6, title: 'Rate-limit the upload route', project: 'fox-api', labels: ['Infra'], prio: 2, state: 'live', row: 'feat/upload-limit' }),
  ticket({ n: 7, title: 'Fix flaky checkout test', project: 'orbit-web', labels: ['Bug'], prio: 1, agent: 'codex', state: 'live', row: 'fix/flaky-checkout' }),
  ticket({ n: 8, title: 'Crash when resuming a download', project: 'garden-mobile', labels: ['Bug'], prio: 2, state: 'live', row: 'fix/resume-crash' }),
  ticket({ n: 9, title: 'Cache invoice queries', project: 'fox-api', prio: 3, agent: 'codex', state: 'live', row: 'perf/invoice-cache' }),
  ticket({ n: 10, title: 'Migrate to React 19', project: 'orbit-web', labels: ['Feature'], state: 'live', row: 'chore/react-19' }),
];

let tab = 'all';
let layout = 'board';
let searching = false;
let query = '';
let menu = null;
let page = null;
let wantFocus = false;
let colsLeft = 0;
const filters = { status: new Set(), agent: new Set(), project: new Set(), priority: new Set(), label: new Set() };
const props = { id: true, priority: true, labels: true, project: true, agent: true };
const tucked = new Set();
const folded = new Set();
const draft = { text: '', project: 'orbit-web', agent: 'claude', priority: 0, labels: [] };

const api = () => D.api;
const prefix = (name) => name.replace(/[^a-z]/gi, '').slice(0, 3).toUpperCase();
const keyOf = (t) => `${prefix(t.project)}-${t.n}`;
const projectOf = (name) => api().PROJECTS.find((p) => p.name === name);
const rowOf = (t) => (typeof t.row === 'string' ? (t.row = projectOf(t.project).rows.find((r) => r.label === t.row || r.title === t.row)) : t.row);

function column(t) {
  const r = t.state === 'live' ? rowOf(t) : null;
  if (!r) return t.state === 'queued' ? 'queued' : 'todo';
  return r.status === 'working' ? 'working' : r.status === 'needs' || r.status === 'failed' ? 'needs' : r.status === 'done' ? 'done' : 'review';
}
const busy = () => tickets.filter((t) => ['working', 'needs'].includes(column(t)) && rowOf(t)?.status !== 'failed').length;
const finished = (c) => c === 'done' || c === 'review';
const askOf = (t) => { const r = rowOf(t); return r?.status === 'needs' ? r.ask : null; };

function shown(t) {
  const c = column(t);
  const f = filters;
  if (f.status.size && !f.status.has(c)) return false;
  if (f.agent.size && !f.agent.has(t.agent)) return false;
  if (f.project.size && !f.project.has(t.project)) return false;
  if (f.priority.size && !f.priority.has(t.prio)) return false;
  if (f.label.size && !t.labels.some((l) => f.label.has(l))) return false;
  const q = query.trim().toLowerCase();
  return !q || t.title.toLowerCase().includes(q) || keyOf(t).toLowerCase().includes(q);
}
const visible = () => tickets.filter(shown);
const inTab = (c) => (tab === 'all' ? true : tab === 'todo' ? c === 'todo' : c !== 'todo' && !finished(c));
const filtered = () => Object.values(filters).some((s) => s.size);

// Sessions

const FILES = ['src/app.tsx', 'src/settings.ts', 'src/store.ts', 'src/theme/tokens.ts'];
const slugOf = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 22);

function later(ms, fn) { setTimeout(() => { fn(); api().changed(); }, ms); }

function startTicket(t) {
  const a = api();
  const p = projectOf(t.project);
  const row = {
    id: Date.now() + t.n, p, label: `${t.labels[0] === 'Bug' ? 'fix' : 'feat'}/${slugOf(t.title)}`, title: t.title, agent: t.agent, chat: true, add: 0, del: 0, status: 'working',
    log: [['you', t.prompt || t.title]],
    ask: { cmd: 'pnpm test', rule: 'pnpm test', after: [['tool', 'Bash', 'pnpm test'], ['out', 'All tests passed'], ['tool', 'Edit', FILES[t.n % FILES.length]], ['say', 'Done. The change is in and the tests still pass.']] },
  };
  p.rows.unshift(row);
  p.open = true;
  Object.assign(t, { state: 'live', row });
  later(1200, () => row.log.push(['say', "I'll look around the repo first."], ['tool', 'Search', 'theme tokens']));
  later(2600, () => row.log.push(['tool', 'Read', FILES[t.n % FILES.length]]));
  later(4000, () => { row.log.push(['say', 'I want to run the tests before I change anything.']); row.status = 'needs'; });
}

function answer(t, allow) {
  const r = rowOf(t);
  if (!r?.ask) return;
  if (!allow) {
    r.status = undefined;
    r.log.push(['say', "Okay, I won't run it. What should I do instead?"]);
    return;
  }
  r.status = 'working';
  r.ask.after.forEach((line, i, all) => later(1000 * (i + 1), () => {
    r.log.push(line);
    if (i === all.length - 1) { r.status = 'done'; r.add ||= 24; r.del ||= 5; }
  }));
}

function again(t) {
  const r = rowOf(t);
  r.status = 'working';
  r.log.push(['you', 'Try again.']);
  later(2600, () => { r.log.push(['tool', 'Edit', 'src/invoices/list.ts'], ['say', 'Fixed. The cache is cleared after a refund.']); r.status = 'done'; });
}

// A freed slot starts the first ticket waiting in Queued.
function pump() {
  while (busy() < LIMIT) {
    const next = tickets.find((t) => t.state === 'queued');
    if (!next) return;
    startTicket(next);
  }
}

function create(start) {
  const [title, ...rest] = draft.text.trim().split('\n');
  if (!title) return;
  const t = ticket({ n: serial++, title: title.trim(), prompt: rest.join('\n').trim(), project: draft.project, agent: draft.agent, prio: draft.priority, labels: [...draft.labels], state: 'todo' });
  tickets.push(t);
  if (start) { t.state = 'queued'; pump(); }
  Object.assign(draft, { text: '', priority: 0, labels: [] });
  Object.keys(filters).forEach((k) => filters[k].clear());
  query = '';
  if (!start && tab === 'active') tab = 'all';
  wantFocus = true;
  api().changed();
}

// Rendering

const glyphOf = (t, c) => {
  const r = rowOf(t);
  if (c === 'needs' && r?.status === 'failed') return ic('status-closed', 14).replace('<svg', '<svg style="color:var(--failed)"');
  if (c === 'needs') return `<span style="color:var(--waiting)">${ic('status-needs', 14)}</span>`;
  const [name, color] = COL_GLYPH[c];
  return `<span style="color:${color}">${ic(name, 14)}</span>`;
};
const prioGlyph = (n) => `<span class="${n === 1 ? 'urgent' : n ? 'p3' : 'p0'}">${ic(PRIORITY_ICON[n], 14)}</span>`;
const dot = (l) => `<i class="ldot" style="background:${LABEL_COLOR[l] ?? 'var(--merged)'}"></i>`;
const avatar = (a) => `<span class="avatar ag ${a}">${ic(AGENTS[a][0], 11)}</span>`;
const tile = (name) => `<span class="mini">${api().letter(name)}</span>`;

function note(t, c) {
  const r = rowOf(t);
  if (!r) return c === 'queued' ? [inLine(t), ''] : null;
  if (c === 'working') {
    const last = [...r.log].reverse().find((l) => l[0] === 'tool');
    return last ? [`${last[1]} ${last[2]}`, ''] : ['Starting…', ''];
  }
  if (c === 'needs' && r.status === 'needs') return [`Wants to run ${r.ask.cmd}`, 'ask'];
  if (c === 'needs') { const e = [...r.log].reverse().find((l) => l[0] === 'err'); return e ? [e[1], 'bad'] : null; }
  return null;
}
function inLine(t) {
  const n = tickets.filter((x) => column(x) === 'queued').indexOf(t) + 1;
  return n === 1 ? 'Next' : n === 2 ? '2nd in line' : n === 3 ? '3rd in line' : `${n}th in line`;
}

function quick(t, c) {
  const r = rowOf(t);
  if (c === 'todo') return `<button class="qb" data-act="start" data-v="${t.n}">${ic('play', 11)}Start</button>`;
  if (c === 'needs' && r?.status === 'needs') return `<button class="qb ghost" data-act="deny" data-v="${t.n}">Deny</button><button class="qb solid" data-act="allow" data-v="${t.n}">Allow once</button>`;
  if (c === 'needs') return `<button class="qb" data-act="again" data-v="${t.n}">${ic('reload', 11)}Start again</button>`;
  if (c === 'done' && !r?.pr) return `<button class="qb" data-act="pr" data-v="${t.n}">${ic('pull-request', 11)}Create PR</button>`;
  return '';
}

function pills(t) {
  const r = rowOf(t);
  if (!r) return '';
  const pr = r.pr ? `<span class="pill">${ic('pull-request', 12).replace('<svg', '<svg style="color:var(--merged)"')}#${r.n ?? 214}</span>` : '';
  const diff = r.add || r.del ? `<span class="pill mono"><b class="add">+${r.add}</b><b class="del">−${r.del}</b></span>` : '';
  return pr + diff;
}

function card(t, c) {
  const r = rowOf(t);
  const n = note(t, c);
  const labels = props.labels ? t.labels.map((l) => `<span class="chipx">${dot(l)}${l}</span>`).join('') : '';
  const project = props.project ? `<span class="pj">${esc(t.project)}</span>` : '';
  const prio = props.priority ? prioGlyph(t.prio) : '';
  const top = props.id || props.agent ? `<div class="top">${props.id ? `<span>${keyOf(t)}</span>` : ''}<i></i>${props.agent ? avatar(t.agent) : ''}</div>` : '';
  return `<div class="bcard${finished(c) ? ' fin' : ''}${r || c !== 'queued' ? ' go' : ''}" data-act="open" data-v="${t.n}">
    ${top}
    <div class="head"><span class="st">${glyphOf(t, c)}</span><span class="ti">${esc(t.title)}</span></div>
    ${n ? `<div class="note ${n[1]}">${esc(n[0])}</div>` : ''}
    <div class="meta">${prio}${labels}${project}${pills(t)}<i></i>${quick(t, c)}</div>
  </div>`;
}

const colHead = (id, name, count, extra = '') => `<div class="ch"><span class="cg" style="color:${COL_GLYPH[id][1]}">${ic(COL_GLYPH[id][0], 14)}</span><b>${name}</b><span class="n">${count}</span><i></i><button class="ib" data-act="tuck" data-v="${id}" aria-label="Hide column">${ic('minus', 13)}</button>${extra}</div>`;

function columnsView(list) {
  const cols = COLUMNS.filter(([id]) => inTab(id) && !tucked.has(id));
  const hidden = COLUMNS.filter(([id]) => inTab(id) && tucked.has(id));
  const html = cols.map(([id, name]) => {
    const items = list.filter((t) => column(t) === id);
    const count = id === 'working' ? `${busy()}/${LIMIT}` : items.length;
    const add = id === 'todo' ? `<button class="ib" data-act="compose" aria-label="Add ticket">${ic('plus', 13)}</button>` : '';
    return `<div class="col" data-col="${id}">${colHead(id, name, count, add)}<div class="bcards">${items.length ? items.map((t) => card(t, id)).join('') : `<div class="empty">${COL_HINT[id]}</div>`}</div></div>`;
  }).join('');
  const strip = hidden.length ? `<div class="tucked">${hidden.map(([id, name]) => `<button data-act="untuck" data-v="${id}"><span style="color:${COL_GLYPH[id][1]}">${ic(COL_GLYPH[id][0], 14)}</span>${name}<span class="n">${list.filter((t) => column(t) === id).length}</span></button>`).join('')}</div>` : '';
  return `<div class="cols">${html}${strip}</div>`;
}

function listView(list) {
  const out = COLUMNS.filter(([id]) => inTab(id)).map(([id, name]) => {
    const items = list.filter((t) => column(t) === id);
    if (!items.length) return '';
    const open = !folded.has(id);
    const rows = open ? items.map((t) => {
      const c = id;
      return `<div class="lrow${finished(c) ? ' fin' : ''}" data-act="open" data-v="${t.n}"><span class="st">${glyphOf(t, c)}</span>${props.id ? `<span class="key">${keyOf(t)}</span>` : ''}<span class="ti">${esc(t.title)}</span>${props.labels ? t.labels.map((l) => `<span class="chipx">${dot(l)}${l}</span>`).join('') : ''}${props.project ? `<span class="pj">${esc(t.project)}</span>` : ''}${pills(t)}${quick(t, c)}${props.priority ? prioGlyph(t.prio) : ''}${props.agent ? avatar(t.agent) : ''}</div>`;
    }).join('') : '';
    return `<button class="lhead" data-act="fold" data-v="${id}">${ic(open ? 'chevron-down' : 'chevron-right', 12)}<span style="color:${COL_GLYPH[id][1]}">${ic(COL_GLYPH[id][0], 14)}</span><b>${name}</b><span class="n">${items.length}</span></button>${rows}`;
  }).join('');
  return `<div class="list">${out}</div>`;
}

const counts = () => {
  const l = visible();
  return { all: l.length, active: l.filter((t) => column(t) !== 'todo' && !finished(column(t))).length, todo: l.filter((t) => column(t) === 'todo').length };
};

function tokenBar() {
  const toks = FILTERS.filter(([k]) => filters[k].size).map(([k, name]) => {
    const vals = [...filters[k]];
    const one = vals.length === 1 ? valueFace(k, vals[0]) : `${vals.length} ${name.toLowerCase()}`;
    return `<span class="tok"><span class="tk">${name}</span><span class="tv">${one}</span><button data-act="untoken" data-v="${k}" aria-label="Remove filter">${ic('x', 11)}</button></span>`;
  }).join('');
  return `<div class="tokens">${toks}<button class="ib" data-act="menu" data-v="filter" aria-label="Add filter">${ic('plus', 13)}</button><i></i><button class="bb" data-act="clear">Clear</button></div>`;
}

function valueFace(k, v) {
  if (k === 'status') { const [name] = COLUMNS.find(([id]) => id === v); return `<span style="color:${COL_GLYPH[v][1]};display:flex">${ic(COL_GLYPH[v][0], 13)}</span>${name}`; }
  if (k === 'agent') return `${avatarIcon(v)}${AGENTS[v][1]}`;
  if (k === 'project') return `${tile(v)}${esc(v)}`;
  if (k === 'priority') return `${prioGlyph(v)}${PRIORITY_NAME[v]}`;
  return `${dot(v)}${v}`;
}
const avatarIcon = (a) => `<span class="ag ${a}" style="display:flex">${ic(AGENTS[a][0], 13)}</span>`;

function filterValues(k) {
  const all = tickets;
  const get = { status: (t) => column(t), agent: (t) => t.agent, project: (t) => t.project, priority: (t) => t.prio, label: null }[k];
  const vals = { status: COLUMNS.map(([id]) => id), agent: Object.keys(AGENTS).filter((a) => ['claude', 'codex', 'cursor', 'grok'].includes(a)), project: api().PROJECTS.map((p) => p.name), priority: PRIORITIES, label: LABELS }[k];
  return vals.map((v) => {
    const count = get ? all.filter((t) => get(t) === v).length : all.filter((t) => t.labels.includes(v)).length;
    const on = filters[k].has(v);
    return `<button class="mrow" data-act="fval" data-v="${k}:${v}"><span class="tick${on ? ' on' : ''}">${on ? ic('check', 9) : ''}</span><span class="lead">${valueFace(k, v)}</span><b></b><span class="cnt">${count}</span></button>`;
  }).join('');
}

function filterMenu() {
  const open = menu.in;
  const cats = FILTERS.map(([k, name, icon]) => `<button class="mrow${open === k ? ' on' : ''}" data-act="fcat" data-v="${k}">${ic(icon, 14)}<span class="mname">${name}</span>${ic('chevron-right', 11)}</button>`).join('');
  return `<div class="menu right filter">${open ? `<div class="pop values">${filterValues(open)}</div>` : ''}<div class="pop cats">${cats}</div></div>`;
}

function displayMenu() {
  const tiles = [['list', 'list', 'List'], ['board', 'blocks', 'Board']].map(([id, icon, name]) => `<button class="lay${layout === id ? ' on' : ''}" data-act="layout" data-v="${id}">${ic(icon, 14)}${name}</button>`).join('');
  const chips = PROPS.map(([id, name]) => `<button class="opt${props[id] ? ' on' : ''}" data-act="prop" data-v="${id}">${name}</button>`).join('');
  return `<div class="menu right display"><div class="pop"><div class="lays">${tiles}</div><hr><div class="sec">Display properties</div><div class="opts">${chips}</div></div></div>`;
}

function pickMenu(kind, surface) {
  const rows = {
    project: () => api().PROJECTS.map((p) => `<button class="mrow" data-act="pick" data-v="project:${p.name}">${tile(p.name)}<span class="mname">${esc(p.name)}</span>${(surface === 'page' ? tickets.find((t) => t.n === page).project : draft.project) === p.name ? ic('check', 13) : ''}</button>`),
    agent: () => ['claude', 'codex', 'cursor', 'grok'].map((a) => `<button class="mrow" data-act="pick" data-v="agent:${a}">${avatarIcon(a)}<span class="mname">${AGENTS[a][1]}</span>${(surface === 'page' ? tickets.find((t) => t.n === page).agent : draft.agent) === a ? ic('check', 13) : ''}</button>`),
    priority: () => PRIORITIES.map((n) => `<button class="mrow" data-act="pick" data-v="priority:${n}">${prioGlyph(n)}<span class="mname">${PRIORITY_NAME[n]}</span>${(surface === 'page' ? tickets.find((t) => t.n === page).prio : draft.priority) === n ? ic('check', 13) : ''}</button>`),
    label: () => LABELS.map((l) => `<button class="mrow" data-act="pick" data-v="label:${l}">${dot(l)}<span class="mname">${l}</span>${(surface === 'page' ? tickets.find((t) => t.n === page).labels : draft.labels).includes(l) ? ic('check', 13) : ''}</button>`),
  }[kind]();
  return `<div class="menu ${surface === 'page' ? 'down' : 'up'}"><div class="pop pick">${rows.join('')}</div></div>`;
}

const chipFace = {
  project: (v) => `${tile(v.project)}<span>${esc(v.project)}</span>`,
  agent: (v) => `${avatarIcon(v.agent)}<span>${AGENTS[v.agent][1]}</span>`,
  priority: (v, page) => `${prioGlyph(v.prio ?? v.priority)}<span>${(v.prio ?? v.priority) || !page ? ((v.prio ?? v.priority) ? PRIORITY_NAME[v.prio ?? v.priority] : 'Priority') : 'No priority'}</span>`,
  label: (v, page) => `<span class="dots">${(v.labels.length ? v.labels : ['Feature']).map(dot).join('')}</span><span>${v.labels.length ? v.labels.join(', ') : page ? 'Add label' : 'Label'}</span>`,
};
const chip = (kind, v, surface) => `<span class="rel"><button class="pc ${surface}${menu?.kind === kind && menu.surface === surface ? ' on' : ''}" data-act="menu" data-v="${kind}:${surface}">${chipFace[kind](v, surface === 'page')}</button>${menu?.kind === kind && menu.surface === surface ? pickMenu(kind, surface) : ''}</span>`;

function dock() {
  const ready = !!draft.text.trim();
  const hint = busy() >= LIMIT ? `${LIMIT} of ${LIMIT} working, so it will wait in Queued` : '';
  return `<div class="bdock"><div class="compose">
    <div class="cfield"><textarea rows="1" data-act="focus" spellcheck="false" placeholder="Fix the flaky login test">${esc(draft.text)}</textarea></div>
    <div class="cfoot">${chip('project', draft, 'cmp')}${chip('agent', draft, 'cmp')}${chip('priority', draft, 'cmp')}${chip('label', draft, 'cmp')}<span class="pc static">${ic('branch', 12)}<span>main</span></span>
      <span class="bhint">${hint}</span>
      <button class="bb${ready ? '' : ' dim'}" data-act="add">Add to To do</button>
      <button class="bb prim${ready ? '' : ' dim'}" data-act="startnew">Start<kbd>⌘↵</kbd></button></div>
  </div></div>`;
}

function bar() {
  const field = searching || query ? `<div class="sfield">${ic('search', 12)}<input data-act="focus" data-f="query" placeholder="Search tickets" value="${esc(query)}"></div>` : '';
  const fOpen = menu?.kind === 'filter';
  const dOpen = menu?.kind === 'display';
  const mark = filtered() ? '<i class="mark"></i>' : '';
  return `<div class="bar bd-bar"><span class="bt">${ic('blocks', 14)}Board</span><i></i>${field}<button class="ib" data-act="search" aria-label="Search">${ic('search', 14)}</button></div>
  <div class="tabs">
    <div class="trow">${TABS.map(([id, name]) => `<button class="tab-chip${tab === id ? ' on' : ''}" data-act="tab" data-v="${id}">${name}<span class="n">${counts()[id]}</span></button>`).join('')}</div>
    <span class="rel"><button class="rb${fOpen ? ' on' : ''}" data-act="menu" data-v="filter" aria-label="Filter">${ic('filter', 13)}</button>${mark}${fOpen ? filterMenu() : ''}</span>
    <span class="rel"><button class="rb${dOpen ? ' on' : ''}" data-act="menu" data-v="display" aria-label="Display">${ic('sliders', 13)}</button>${dOpen ? displayMenu() : ''}</span>
  </div>${filtered() ? tokenBar() : ''}`;
}

// A ticket that hasn't started opens as a page: title, prompt and properties.

const pageIds = () => tickets.filter((t) => ['todo', 'queued'].includes(column(t))).map((t) => t.n);

function ticketPage(t) {
  const c = column(t);
  const ids = pageIds();
  const at = ids.indexOf(t.n) + 1;
  const queued = c === 'queued';
  const row = (name, value) => `<div class="prow"><span>${name}</span>${value}</div>`;
  const action = queued
    ? '<button class="bb scnd" data-act="back-todo">Move back to To do</button>'
    : `<button class="bb prim" data-act="start-page">${ic('play', 12)}Start<kbd>⌘↵</kbd></button>`;
  return `<div class="pane bd page">
    <div class="bar bd-bar"><button class="ib" data-act="leave" aria-label="Back">${ic('back', 14)}</button><button class="crumb" data-act="leave">${ic('blocks', 13)}Board</button>${ic('chevron-right', 11)}<span class="pname">${keyOf(t)}  ${esc(t.title)}</span><i></i>
      ${queued ? `<span class="pill">${inLine(t)}</span>` : ''}${action}<span class="vsep"></span><span class="pcount">${at} / ${ids.length}</span>
      <button class="ib" data-act="step" data-v="-1" aria-label="Previous">${ic('chevron-up', 14)}</button><button class="ib" data-act="step" data-v="1" aria-label="Next">${ic('chevron-down', 14)}</button></div>
    <div class="pbody">
      <div class="pmain"><input class="ptitle" data-f="title" value="${esc(t.title)}" placeholder="Ticket title" spellcheck="false"><textarea class="pprompt" data-f="prompt" placeholder="Tell the agent what you want done…" spellcheck="false">${esc(t.prompt)}</textarea></div>
      <div class="pprops">
        ${row('Status', `<span class="pc static">${glyphOf(t, c)}<span>${COLUMNS.find(([id]) => id === c)[1]}</span></span>`)}
        ${row('Priority', chip('priority', t, 'page'))}${row('Labels', chip('label', t, 'page'))}<hr>
        ${row('Agent', chip('agent', t, 'page'))}${row('Project', chip('project', t, 'page'))}${row('Base', '<span class="pc static page">' + ic('branch', 12) + '<span>main</span></span>')}
        ${row('Updated', '<span class="pc static page"><span>Just now</span></span>')}
      </div>
    </div>${menu ? '<div class="scrim" data-act="close"></div>' : ''}
  </div>`;
}

function render() {
  const left = document.querySelector('.app[data-screen="board"] .cols')?.scrollLeft;
  if (left != null) colsLeft = left;
  if (page != null) {
    const t = tickets.find((x) => x.n === page);
    if (t && ['todo', 'queued'].includes(column(t))) return ticketPage(t);
    page = null;
  }
  const list = visible();
  const body = layout === 'list' ? listView(list) : columnsView(list);
  const empty = !list.length ? `<div class="none">${tickets.length ? 'No tickets match' : 'No tickets yet'}</div>` : '';
  return `<div class="pane bd">${bar()}<div class="area-b">${empty || body}</div>${dock()}${menu ? '<div class="scrim" data-act="close"></div>' : ''}</div>`;
}

function mount(root) {
  const cols = root.querySelector('.cols');
  if (cols) cols.scrollLeft = colsLeft;
  const ta = root.querySelector('.compose textarea');
  if (ta) grow(ta);
  const q = root.querySelector('[data-f="query"]');
  const target = searching && q && !query ? q : wantFocus ? ta : null;
  if (target) { target.focus(); target.setSelectionRange?.(target.value.length, target.value.length); }
  const pt = root.querySelector('.pprompt');
  if (pt) { if (wantFocus) { wantFocus = false; root.querySelector('.ptitle')?.focus(); } }
}

function grow(el, max = 150) {
  el.style.height = 'auto';
  el.style.height = `${Math.min(el.scrollHeight, max)}px`;
}

function act(a, v, el) {
  const t = tickets.find((x) => x.n === +v);
  const keepMenu = ['pick', 'fval', 'fcat', 'prop', 'layout'].includes(a);
  if (!keepMenu && !['menu', 'focus', 'step'].includes(a)) menu = null;
  if (!['focus', 'menu', 'pick', 'add', 'startnew', 'compose'].includes(a)) wantFocus = false;
  switch (a) {
    case 'focus': wantFocus = el.matches('textarea'); return;
    case 'search': searching = !searching; if (!searching) query = ''; return api().draw();
    case 'tab': tab = v; return api().draw();
    case 'menu': {
      const [kind, surface] = v.split(':');
      menu = menu?.kind === kind && menu.surface === (surface ?? 'bar') ? null : { kind, surface: surface ?? 'bar' };
      if (kind === 'filter') menu.in = null;
      wantFocus = surface === 'cmp' && !!draft.text;
      return api().draw();
    }
    case 'close': return api().draw();
    case 'fcat': menu.in = menu.in === v ? null : v; return api().draw();
    case 'fval': {
      const [k, raw] = v.split(':');
      const val = ['priority'].includes(k) ? +raw : raw;
      filters[k].has(val) ? filters[k].delete(val) : filters[k].add(val);
      return api().draw();
    }
    case 'untoken': filters[v].clear(); return api().draw();
    case 'clear': Object.values(filters).forEach((s) => s.clear()); query = ''; searching = false; return api().draw();
    case 'layout': layout = v; return api().draw();
    case 'prop': props[v] = !props[v]; return api().draw();
    case 'tuck': tucked.add(v); return api().draw();
    case 'untuck': tucked.delete(v); return api().draw();
    case 'fold': folded.has(v) ? folded.delete(v) : folded.add(v); return api().draw();
    case 'compose': wantFocus = true; return api().draw();
    case 'pick': {
      const [k, raw] = v.split(':');
      const target = page != null ? tickets.find((x) => x.n === page) : draft;
      if (k === 'label') { const l = target.labels; l.includes(raw) ? l.splice(l.indexOf(raw), 1) : l.push(raw); menu = { kind: 'label', surface: page != null ? 'page' : 'cmp' }; }
      else if (k === 'priority') target[page != null ? 'prio' : 'priority'] = +raw;
      else target[k] = raw;
      if (k !== 'label') menu = null;
      wantFocus = page == null && !!draft.text;
      return api().draw();
    }
    case 'add': create(false); return;
    case 'startnew': create(true); return;
    case 'open': {
      if (!t) return;
      const c = column(t);
      if (c === 'todo' || c === 'queued') { page = t.n; wantFocus = false; return api().draw(); }
      const r = rowOf(t);
      return r && api().select(r);
    }
    case 'start': startTicket(t); return api().changed();
    case 'allow': answer(t, true); return api().changed();
    case 'deny': answer(t, false); return api().changed();
    case 'again': again(t); return api().changed();
    case 'pr': { const r = rowOf(t); r.pr = 'open'; r.n = 215 + t.n; return api().changed(); }
    case 'leave': page = null; return api().draw();
    case 'start-page': { const p = tickets.find((x) => x.n === page); page = null; startTicket(p); return api().changed(); }
    case 'back-todo': { const p = tickets.find((x) => x.n === page); p.state = 'todo'; return api().draw(); }
    case 'step': {
      const ids = pageIds();
      const next = ids[ids.indexOf(page) + +v];
      if (next != null) page = next;
      return api().draw();
    }
  }
}

D.register('board', {
  page: 'board',
  tour: 'board',
  render,
  mount,
  act,
  input(e) {
    const f = e.target.dataset.f;
    if (e.target.matches('.compose textarea')) {
      draft.text = e.target.value;
      grow(e.target);
      const ready = !!draft.text.trim();
      e.target.closest('.bdock').querySelectorAll('.bb').forEach((b) => b.classList.toggle('dim', !ready));
      wantFocus = true;
    } else if (f === 'query') {
      query = e.target.value;
      api().draw();
    } else if (f) {
      tickets.find((x) => x.n === page)[f] = e.target.value;
      if (f === 'title') e.target.closest('.bd').querySelector('.pname').textContent = `${keyOf(tickets.find((x) => x.n === page))}  ${e.target.value}`;
    }
  },
  key(e) {
    if (e.target.matches('.compose textarea') && e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); create(e.metaKey || e.ctrlKey); }
    else if (e.key === 'Escape' && (menu || page != null)) { menu = null; if (!menu) page = page; api().draw(); }
    else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && page != null) { act('start-page'); }
  },
  enter() { menu = null; page = null; },
});

addEventListener('DOMContentLoaded', () => api().onChange(pump));
})();
