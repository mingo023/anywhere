(() => {
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const clamp = (x, a, b) => Math.min(b, Math.max(a, x));
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const plural = (n, w) => `${n} ${w}${n === 1 ? '' : 's'}`;

const STATIONS = [
  { id: 'todo', name: 'To do' }, { id: 'spec', name: 'Spec' }, { id: 'plan', name: 'Plan', you: true },
  { id: 'implement', name: 'Implement' }, { id: 'checks', name: 'Checks' }, { id: 'pr', name: 'PR review', you: true },
  { id: 'merged', name: 'Merged' },
];
const ST = Object.fromEntries(STATIONS.map(s => [s.id, s]));
const ST_ID = Object.fromEntries(STATIONS.map(s => [s.name, s.id]));
const PRI = ['', 'Low', 'Medium', 'High', 'Urgent'];
const AGENT = { C: 'Claude', X: 'Codex' };
const LABELS = ['Bug', 'Feature', 'Docs', 'Infra'];
const MODELS = ['Claude · Opus', 'Claude · Sonnet', 'Codex'];
const PROFILES = () => [
  ...MODELS.map(m => ({ name: m, model: m, builtin: true, soul: '', instructions: '', skills: [] })),
  {
    name: 'Go backend', model: 'Claude · Sonnet',
    soul: 'You are a careful backend engineer. You prefer small, boring changes, and you say so when a task is bigger than it looks.',
    instructions: 'Write a failing test first. Keep handlers thin and put logic in pure functions. Run go vet before you hand back.',
    skills: [{ name: 'table-tests', files: [['SKILL.md', 1.8]] }, { name: 'go-errors', files: [['SKILL.md', 1.2]] }],
  },
  {
    name: 'Swift UI', model: 'Claude · Opus',
    soul: 'You care about how a screen feels in the hand. You match the platform before you invent anything.',
    instructions: 'Reuse the existing SwiftUI components. Record a snapshot test for every new view.',
    skills: [{ name: 'snapshot-tests', files: [['SKILL.md', 2.0], ['scripts/record.sh', 0.9]] }],
  },
];
const INSTALLED = [
  { name: 'write-docs', files: [['SKILL.md', 2.4], ['templates/page.md', 1.1]] },
  { name: 'release-notes', files: [['SKILL.md', 1.3], ['scripts/collect.sh', 0.7]] },
  { name: 'table-tests', files: [['SKILL.md', 1.8]] },
];
const IMPORT = {
  from: 'team-defaults.zip',
  soul: 'You are a calm senior engineer. You explain trade-offs in one sentence and never guess at a file you have not read.',
  instructions: 'Keep each commit to one change. Name tests as sentences. Ask before adding a dependency.',
  extra: [['SOUL.md', 1.2], ['AGENTS.md', 3.4]],
  skills: [{ name: 'sql-migrations', files: [['SKILL.md', 2.1], ['check.sh', 0.6]] }],
};
const MERGE = ['Squash and merge', 'Create a merge commit', 'Rebase and merge'];
const RAIL = 56, MINW = 212, MAXW = 320, RAIL_W = 44, RAIL_OPEN = 232;

const win = $('#window'), sidebar = $('#sidebar'), main = $('#main'), view = $('#view'), detail = $('#detail'), layer = $('#layer');

const S = {
  route: 'board', layout: 'factory',
  theme: matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark',
  sbOpen: true, sbW: 272, open: null, inboxSel: null, merge: 0,
  display: { labels: true, branch: true },
  cfg: {
    enabled: true, spec: true, planGate: true, reviewer: true, slots: 4, fix: 2,
    lead: 'Claude · Opus', workers: 'Claude · Sonnet', reviewerAgent: 'Codex',
    checks: ['cargo fmt --check', 'cargo clippy --workspace --all-targets', 'cargo test --workspace', 'go test ./...'],
  },
};
function seed() {
  Object.assign(S, SEED(), {
    tab: {}, folded: {}, nextId: 52, profiles: PROFILES(), rails: { todo: false, merged: false },
    filters: { att: null, project: null, label: null, agent: null, priority: null },
  });
}

/* ---------- model ---------- */

const T = id => S.tickets.find(t => t.id === id);
function D(id) {
  if (!S.details[id]) {
    const t = T(id);
    const activity = [{ s: 'To do', who: 'You', av: 'you', text: 'Created the ticket', time: age(t.mins) }];
    if (t.station === 'merged') activity.push({ s: 'Merged', who: 'You', av: 'you', text: `Merged #${t.pr}`, time: age(t.mins) });
    else if (t.station !== 'todo') activity.push({ s: ST[t.station].name, who: `Lead · ${AGENT[t.agent]}`, av: t.agent, text: t.work || 'Working', time: 'now' });
    S.details[id] = { created: 'Today', activity };
  }
  return S.details[id];
}
const age = m => m < 1 ? 'now' : m < 60 ? `${m}m` : m < 1440 ? `${Math.round(m / 60)}h` : `${Math.round(m / 1440)}d`;
const rank = t => t.gate ? 0 : t.failed ? 1 : 2;
const upNext = list => [...list].sort((a, b) => rank(a) - rank(b) || b.mins - a.mins);
const inboxItems = () => upNext(S.tickets.filter(t => t.gate || t.failed));
function visible(t) {
  const f = S.filters;
  if (f.att === 'you' && !t.gate) return false;
  if (f.att === 'ask' && t.gate !== 'ask') return false;
  if (f.att === 'failed' && !t.failed) return false;
  return (!f.project || t.project === f.project) && (!f.label || t.label === f.label)
    && (!f.agent || t.agent === f.agent) && (!f.priority || t.pri === f.priority);
}
function statusOf(t) {
  if (t.station === 'merged') return `#${t.pr}`;
  if (t.gate === 'ask') return 'needs you';
  if (t.gate) return 'waiting for you';
  if (t.failed) return 'failed';
  if (t.station === 'todo') return 'not started';
  return t.station === 'implement' ? 'working' : (t.work || 'working').toLowerCase();
}

function log(id, s, who, av, text, extra) {
  D(id).activity.push({ s, who, av, text, time: 'now', fresh: true, ...extra });
}

function decide(id, act, text) {
  const t = T(id), d = D(id);
  delete S.tab[id];
  switch (act) {
    case 'approve': {
      const n = d.tasks.length, br = 'team-c08d4e';
      d.tasks.forEach(k => Object.assign(k, { branch: `${br}-${k.id}`, diff: [0, 0], state: k.after ? `Waiting for ${k.after}` : 'Working' }));
      Object.assign(t, { station: 'implement', gate: null, work: `0/${n}`, branch: br, slot: true });
      log(id, 'Plan', 'You', 'you', `Approved the plan (${plural(n, 'task')})`);
      log(id, 'Implement', 'Lead · Claude', 'C', `Started ${d.tasks.filter(k => !k.after).map(k => k.id).join(', ')}`);
      break;
    }
    case 'changes':
      if (t.gate === 'plan') {
        Object.assign(t, { gate: null, work: 'Planning', slot: true });
        log(id, 'Plan', 'You', 'you', 'requested changes', { quote: text });
      } else {
        Object.assign(t, { station: 'implement', gate: null, work: 'Changes requested', slot: true });
        log(id, 'PR review', 'You', 'you', 'requested changes', { quote: text });
      }
      break;
    case 'allow': case 'deny': {
      const k = d.tasks.find(k => k.ask), cmd = k.ask;
      log(id, 'Implement', 'You', 'you', act === 'deny' ? `Denied ${cmd}` : `Allowed ${cmd} once`);
      Object.assign(k, { ask: null, state: 'Working' });
      Object.assign(t, { gate: null, ask: null, slot: true });
      break;
    }
    case 'hint':
      Object.assign(t, { failed: false, work: 'Fixing · your hint', slot: true });
      d.failure.attempts = 'One attempt granted by you';
      log(id, 'Checks', 'You', 'you', 'sent back with a hint', { quote: text });
      break;
    case 'merge':
      Object.assign(t, { station: 'merged', gate: null, mins: 0 });
      d.gateRows[3][1] = 'Merged';
      log(id, 'Merged', 'You', 'you', `Merged #${t.pr}, ${MERGE[S.merge].toLowerCase().replace(' and merge', '')}`);
      break;
    case 'close':
      S.tickets = S.tickets.filter(x => x !== t);
      break;
  }
  changed();
}

/* ---------- atoms ---------- */

const icon = (n, s = 14, cls = 'ic', color) => `<svg class="${cls}" width="${s}" height="${s}"${color ? ` style="color:var(--${color})"` : ''} ${ICONS[n][0]}>${ICONS[n][1]}</svg>`;

/* Marks and colours are parts::status_glyph's. Plan is the one station with no column in the app, so its mark is drawn here on the same ring. */
ICONS['status-quarter'] = [ICONS['status-half'][0], '<circle cx="7" cy="7" r="6" stroke="currentColor" stroke-width="1.5" fill="none"/><path d="M7 7V3.5A3.5 3.5 0 0 1 10.5 7Z"/>'];
const GLYPH = {
  todo: ['todo', 'text-4'], spec: ['queued', 'working'], plan: ['quarter', 'working'], implement: ['half', 'working'],
  checks: ['three-quarters', 'success'], done: ['three-quarters', 'success'], pr: ['full', 'goal'], merged: ['merged', 'indigo'],
  needs: ['needs', 'waiting-dot'], failed: ['closed', 'failed'],
};
const glyph = (k, s = 14) => icon(`status-${GLYPH[k][0]}`, s, 'gl', GLYPH[k][1]);
const ticketGlyph = t => glyph(t.gate ? 'needs' : t.failed ? 'failed' : t.station);
const pri = p => icon(`priority-${PRI[p].toLowerCase()}`, 14, 'pri', p === 4 ? 'working' : 'text-3');
const LABEL_DOT = { Bug: 'failed', Docs: 'waiting-dot', Infra: 'goal' };
const labelDot = l => `<i class="ldot" style="background:var(--${LABEL_DOT[l] || 'merged'})"></i>`;
const labelChip = l => `<span class="pchip">${labelDot(l)}${l}</span>`;
function av(k) {
  if (k === 'you') return '<span class="av me">Y</span>';
  if (k === 'sys') return '<span class="av sys"><i class="dot" style="background:currentColor;width:4px;height:4px"></i></span>';
  return `<span class="av">${icon(k === 'X' ? 'openai' : 'claude', 11)}</span>`;
}
const prof = name => S.profiles.find(p => p.name === name);
const modelOf = name => prof(name)?.model || name;
const avOf = name => modelOf(name).startsWith('Codex') ? 'X' : 'C';
const diffStat = d => d ? `<span class="mono"><span class="plus">+${d[0]}</span> <span class="minus">−${d[1]}</span></span>` : '';
const seg = (name, opts, active, act) => `<div class="seg" data-seg="${name}"><i class="thumb"></i>${opts.map(([v, l]) => `<button data-act="${act}" data-arg="${v}" class="${v === active ? 'on' : ''}">${l}</button>`).join('')}</div>`;

/* ---------- board ---------- */

function note(t) {
  if (t.gate === 'plan') return ['asking', `Plan ready · ${plural(D(t.id).tasks.length, 'task')}`];
  if (t.gate === 'ask') return ['asking', `Ask · <span class="mono">${esc(t.ask)}</span>`];
  if (t.gate === 'pr') return ['asking', 'Ready to merge'];
  if (t.failed) return ['failed', 'Checks failed'];
  return t.work && t.station !== 'merged' ? ['dim', t.work] : null;
}
const noteHTML = t => { const n = note(t); return n ? `<span class="note ${n[0]}">${n[1]}</span>` : ''; };
function pills(t) {
  const merged = t.station === 'merged', h = [];
  if (S.display.branch && t.pr) h.push(`<span class="pill">${icon(merged ? 'merge' : 'pull-request', 12, 'ic', merged ? 'merged' : 'success-text')}#${t.pr}</span>`);
  else if (S.display.branch && t.branch) h.push(`<span class="pill">${icon('branch', 11)}<span class="mono">${t.branch}</span></span>`);
  if (t.diff) h.push(`<span class="pill">${diffStat(t.diff)}</span>`);
  return h.join('');
}
const cardHTML = t => `<article class="card ${t.station === 'merged' ? 'done' : ''}" data-ticket="${t.id}" data-flip="${t.id}" data-act="open" tabindex="0">
  <div class="top"><span>${t.id}</span><span class="grow"></span><span class="age">${age(t.mins)}</span>${av(t.agent)}</div>
  <div class="head">${ticketGlyph(t)}<div class="title">${esc(t.title)}</div></div>
  ${noteHTML(t)}
  <div class="meta">${pri(t.pri)}${S.display.labels ? labelChip(t.label) : ''}${pills(t)}</div>
</article>`;

const railG = {};
function railHTML(st, list) {
  const w = railG[st.id] ? railG[st.id].v.w : S.rails[st.id] ? RAIL_OPEN : RAIL_W, p = (w - RAIL_W) / (RAIL_OPEN - RAIL_W);
  return `<section class="col rail ${w > RAIL_W + .5 ? 'open' : ''}" data-rail="${st.id}" style="width:${w}px">
    <button class="rail-tab" data-act="rail" data-arg="${st.id}" style="opacity:${1 - p};${p > .5 ? 'pointer-events:none' : ''}" aria-label="Show ${st.name}">${glyph(st.id)}<b>${st.name}</b><span class="n">${list.length}</span></button>
    <div class="rail-body" style="opacity:${p}">
      <button class="col-h" data-act="rail" data-arg="${st.id}" aria-label="Hide ${st.name}">${glyph(st.id)}<b>${st.name}</b><span class="n">${list.length}</span></button>
      <div class="stack">${list.map(cardHTML).join('')}</div>
    </div>
  </section>`;
}
function setRail(name, open) {
  S.rails[name] = open;
  const find = () => $(`.rail[data-rail="${name}"]`);
  const el = find();
  if (!el) return;
  el.classList.add('open');
  railG[name] ||= springs({ w: el.offsetWidth }, { response: .3, precision: .3 }, v => {
    const r = find();
    if (!r) return;
    const p = (v.w - RAIL_W) / (RAIL_OPEN - RAIL_W), tab = r.firstElementChild;
    r.style.width = `${v.w}px`;
    tab.style.opacity = 1 - p;
    tab.style.pointerEvents = p > .5 ? 'none' : '';
    r.lastElementChild.style.opacity = p;
  });
  railG[name].to({ w: open ? RAIL_OPEN : RAIL_W }, {
    onRest: () => {
      if (S.rails[name]) find()?.scrollIntoView({ inline: 'nearest', block: 'nearest', behavior: 'smooth' });
      else find()?.classList.remove('open');
    },
  });
}

function factoryHTML(list) {
  return `<div class="board">${STATIONS.map(st => {
    const cards = upNext(list.filter(t => t.station === st.id));
    if (st.id === 'todo' || st.id === 'merged') return railHTML(st, cards);
    return `<section class="col"><div class="col-h">${glyph(st.id)}<b>${st.name}</b><span class="n">${cards.length}</span>${st.you ? '<span class="who">You</span>' : ''}</div><div class="stack">${cards.map(cardHTML).join('')}</div></section>`;
  }).join('')}</div>`;
}
function statusHTML(list) {
  const groups = [
    ['To do', t => t.station === 'todo'], ['Working', t => !t.gate && !t.failed && !['todo', 'merged'].includes(t.station)],
    ['Needs you', t => t.gate], ['Failed', t => t.failed], ['Merged', t => t.station === 'merged'],
  ];
  return `<div class="board">${groups.map(([name, fn]) => {
    const cards = upNext(list.filter(fn));
    return `<section class="col"><div class="col-h"><b>${name}</b><span class="n">${cards.length}</span></div><div class="stack">${cards.map(cardHTML).join('')}</div></section>`;
  }).join('')}</div>`;
}
function listHTML(list) {
  return `<div class="list">${STATIONS.map(st => {
    const rows = upNext(list.filter(t => t.station === st.id));
    if (!rows.length) return '';
    const shut = S.folded[st.id];
    return `<button class="lgroup-h" data-act="fold" data-arg="${st.id}" aria-expanded="${!shut}">${icon(shut ? 'chevron-right' : 'chevron-down', 12, 'ic', 'text-4')}${glyph(st.id)}<b>${st.name}</b><span>${rows.length}</span></button>${shut ? '' : rows.map(t => `<div class="lrow ${t.station === 'merged' ? 'done' : ''}" data-ticket="${t.id}" data-flip="${t.id}" data-act="open" tabindex="0">
      ${pri(t.pri)}<span class="id">${t.id}</span>${ticketGlyph(t)}<span class="title">${esc(t.title)}</span>${noteHTML(t)}${pills(t)}${S.display.labels ? labelChip(t.label) : ''}${av(t.agent)}<span class="age">${age(t.mins)}</span></div>`).join('')}`;
  }).join('')}</div>`;
}

function boardView() {
  const f = S.filters, n = fn => S.tickets.filter(fn).length;
  const att = (kind, count, label) => `<button class="chip ${kind} ${f.att === kind ? 'on' : ''}" data-act="att" data-arg="${kind}"><span class="n">${count}</span>${label}</button>`;
  const pick = (key, label, value) => `<button class="chip token ${f[key] ? 'on' : ''}" data-act="filter" data-arg="${key}">${value || label}${icon('chevron-down', 12)}</button>`;
  const list = S.tickets.filter(visible);
  const body = { factory: factoryHTML, status: statusHTML, list: listHTML }[S.layout](list);
  return `<div class="page">
    <div class="tb flat">
      <div class="crumb">${icon('blocks')}<b>Board</b></div>
      <span class="grow"></span>
      <button class="field" data-act="search">${icon('search', 13)}<span>Search</span><kbd>⌘K</kbd></button>
      <button class="btn" data-act="newTicket">${icon('plus', 13)}New ticket</button>
    </div>
    <div class="fb">
      ${seg('layout', [['factory', 'Factory'], ['status', 'Status'], ['list', 'List']], S.layout, 'layout')}<i class="sep"></i>
      ${att('you', n(t => t.gate), 'waiting on you')}${att('ask', n(t => t.gate === 'ask'), 'ask')}${att('failed', n(t => t.failed), 'failed')}
      <span class="slots"><b class="mono">${n(t => t.slot)}/${S.cfg.slots}</b> slots</span><i class="sep"></i>
      ${pick('project', 'Project', f.project)}${pick('label', 'Label', f.label)}${pick('agent', 'Agent', AGENT[f.agent])}${pick('priority', 'Priority', PRI[f.priority])}
      <span class="grow"></span>
      <button class="icon-btn round" data-act="display" aria-label="Display" title="Display">${icon('sliders', 13)}</button>
    </div>
    <div class="scroll" data-keep><div class="content">${body}</div></div>
  </div>`;
}

/* ---------- ticket ---------- */

function tabsFor(t) {
  const i = STATIONS.indexOf(ST[t.station]), tabs = [['spec', 'Spec & plan']];
  if (i >= 3) tabs.push(['workers', 'Workers']);
  if (i >= 4) tabs.push(['checks', 'Checks']);
  if (i >= 5) tabs.push(['pr', 'PR']);
  return tabs;
}
function tabOf(t) {
  const want = S.tab[t.id];
  if (want && tabsFor(t).some(x => x[0] === want)) return want;
  return { implement: 'workers', checks: 'checks', pr: 'pr', merged: 'pr' }[t.station] || 'spec';
}
const quiet = s => `<p class="quiet">${s}</p>`;
const markOf = state => `<span class="mark">${glyph(state === 'Done' ? 'done' : state === 'Needs you' ? 'needs' : state ? 'implement' : 'todo')}</span>`;

function specTab(t, d) {
  let h = '<h2>Spec</h2>';
  if (d.spec) {
    h += `<div class="byline">${d.spec.by}</div><p>${esc(d.spec.brief)}</p>`;
    if (d.spec.criteria) h += `<h3>Acceptance criteria</h3><ul class="crit">${d.spec.criteria.map(c => `<li>${esc(c)}</li>`).join('')}</ul>`;
  } else h += quiet(t.station === 'todo' ? 'Written when the ticket starts' : t.station === 'spec' ? 'The lead is drafting the spec' : 'The lead is still planning');
  if (!d.tasks) return h;
  const edit = t.gate === 'plan';
  h += `<h2>Plan<small>${plural(d.tasks.length, 'task')}</small>${edit ? '<small class="r">Up to 4 workers at once</small>' : ''}</h2><div class="rows">`;
  for (const k of d.tasks) {
    const needed = d.tasks.some(o => o.after === k.id);
    h += `<div class="sub" data-task="${k.id}">${markOf(edit ? '' : k.state)}
      <div class="head"><span class="mono">${k.id}</span><b>${esc(k.title)}</b></div>
      ${edit ? `<button class="inline-btn" data-act="rmTask" data-arg="${k.id}" ${needed ? 'disabled' : ''} title="${needed ? 'Another task starts after this one' : 'Remove from the plan'}" aria-label="Remove ${k.id}">${icon('x', 12)}</button>` : `<span class="state">${k.state || ''}</span>`}
      <div class="more">${k.body ? `<p>${esc(k.body)}</p>` : ''}<div class="meta">
        ${k.files ? `<span class="mono">${k.files.join('  ')}</span>` : ''}
        ${edit ? `<button class="inline-btn" data-act="taskAgent" data-arg="${k.id}">${av(avOf(k.agent))}${k.agent}${icon('chevron-down', 11)}</button>` : `<span>${k.agent}</span>`}
        ${edit ? `<span>${k.after ? `after ${k.after}` : 'starts first'}</span>` : ''}
        ${k.allow ? `<span>allow <span class="mono">${esc(k.allow)}</span></span>` : ''}
      </div></div></div>`;
  }
  return `${h}</div>`;
}
function workersTab(t, d) {
  if (!d.tasks) return quiet('Workers start after the plan is approved');
  const done = d.tasks.filter(k => k.state === 'Done').length;
  return `<h2>Sub-issues<small>${done}/${d.tasks.length}</small></h2><div class="rows">${d.tasks.map(k => `<div class="sub">${markOf(k.state)}
    <div class="head"><span class="mono">${k.id}</span><b>${esc(k.title)}</b></div>
    <span class="state ${k.state === 'Needs you' ? 'you-t' : ''}">${k.state}</span>
    <div class="more"><div class="meta">${av(avOf(k.agent))}<span>${k.agent}</span><span class="mono">${k.branch}</span>${diffStat(k.diff)}</div>
    ${k.ask ? `<div class="meta">Ask · <span class="mono">${esc(k.ask)}</span></div>` : k.note && k.state === 'Working' ? `<div>${esc(k.note)}</div>` : ''}</div>
  </div>`).join('')}</div>`;
}
function checksTab(t, d) {
  if (!d.checks) return quiet(t.work ? `Running, ${t.work}` : 'Checks have not run');
  const fixing = !t.failed && d.failure && t.station === 'checks';
  return `<h2>Checks<small>${d.checks.filter(c => c.ok).length}/${d.checks.length} passed</small></h2>
  <div class="rows">${d.checks.map(c => `<div class="check"><span class="mark">${c.ok ? icon('check', 13, 'ic', 'success') : glyph(fixing ? 'implement' : 'failed')}</span><b>${c.name}</b><span class="mono">${c.cmd}</span><span class="t">${c.ok ? c.time : fixing ? 'Fixing' : 'Failed'}</span></div>`).join('')}</div>
  ${d.failure ? `<div class="callout ${fixing ? 'calm' : ''}"><div class="ch"><b>${d.failure.title}</b><span>${d.failure.attempts}</span></div><pre>${esc(d.failure.output)}</pre></div>` : ''}`;
}
function prTab(t, d) {
  if (!d.prInfo) return quiet(t.pr ? `#${t.pr} · ${t.station === 'merged' ? 'merged' : 'the reviewer is reading the diff'}` : 'Opens after Checks');
  const p = d.prInfo;
  return `<div class="prh"><span class="mono">#${t.pr}</span><b>${esc(t.title)}</b></div>
  <div class="prm"><span class="mono">${p.from}</span> into <span class="mono">${p.into}</span> · ${p.files} files · ${diffStat(t.diff)}</div>
  <h3>Merge gate</h3><div class="rows">${d.gateRows.map(([a, b]) => `<div class="grow-row"><span>${a}</span><span>${b}</span></div>`).join('')}</div>
  <h3>Reviewer · Codex</h3><div class="rows">${d.findings.map(f => `<div class="find"><span class="sev">${f.sev}</span><span>${esc(f.text)}</span><span class="mono">${f.at}</span></div>`).join('')}</div>`;
}
const tabBody = (t, still) => `<div class="tabbody ${still ? 'still' : ''}">${{ spec: specTab, workers: workersTab, checks: checksTab, pr: prTab }[tabOf(t)](t, D(t.id))}</div>`;

function activityHTML(d) {
  let h = '', cur = null;
  for (const a of d.activity) {
    if (a.s !== cur) { cur = a.s; h += `<div class="act-h">${glyph(ST_ID[a.s], 12)}${a.s}</div>`; }
    h += `<div class="act ${a.fresh ? 'new' : ''}">${av(a.av)}<div><b>${a.who}</b>${esc(a.text)}</div><time>${a.time}</time>${a.diff ? `<pre>${esc(a.diff)}</pre>` : ''}${a.quote ? `<div class="quote">${esc(a.quote)}</div>` : ''}</div>`;
    a.fresh = false;
  }
  return h;
}

function docHTML(t, inbox) {
  const i = STATIONS.indexOf(ST[t.station]), needs = !!t.gate;
  return `${inbox ? `<div class="pane-id"><span>${t.id}</span><button class="btn ghost" data-act="open">Open ticket${icon('chevron-right', 12)}</button></div>` : ''}
  <h1>${esc(t.title)}</h1>
  <div class="status">${ticketGlyph(t)}<b>${ST[t.station].name}</b><span class="${needs ? 'you-t' : ''}">${statusOf(t)}</span><span class="track">${STATIONS.map((_, j) => `<i class="${j < i ? 'done' : j === i ? 'at' : ''}"></i>`).join('')}</span></div>
  ${tabsFor(t).length > 1 ? seg(inbox ? 'tab-i' : 'tab-d', tabsFor(t), tabOf(t), 'tab') : ''}
  ${tabBody(t)}
  <h2>Activity</h2><div class="activity">${activityHTML(D(t.id))}</div>
  <div class="composer">${av('you')}<textarea placeholder="Leave a comment" aria-label="Leave a comment"></textarea></div>`;
}

function barHTML(t, still) {
  const d = D(t.id), cls = `bar ${still ? 'still' : ''}`, k = '<span class="key">⌘↵</span>';
  if (t.gate === 'plan') {
    const n = d.tasks.length;
    return `<div class="${cls}"><div class="what"><i class="dot you"></i><b>Plan approval</b><span>${plural(n, 'task')}, ${plural(n, 'worker')}</span></div>
      <button class="btn" data-act="changes">Request changes</button>
      <button class="btn primary" data-act="decide" data-arg="approve">Approve and start ${plural(n, 'worker')}${k}</button></div>`;
  }
  if (t.gate === 'ask') {
    const task = d.tasks.find(x => x.ask);
    return `<div class="${cls}"><div class="what"><i class="dot you"></i><b>${AGENT[avOf(task.agent)]} wants to run</b><span class="mono">${esc(t.ask)}</span></div>
      <button class="btn ghost" data-act="decide" data-arg="deny">Deny</button>
      <button class="btn primary" data-act="decide" data-arg="allow">Allow once${k}</button></div>`;
  }
  if (t.gate === 'pr') {
    return `<div class="${cls}"><div class="what"><i class="dot you"></i><b>PR review</b><span>#${t.pr} · +${t.diff[0]} −${t.diff[1]} · ${d.prInfo.files} files</span></div>
      <button class="btn" data-act="changes">Request changes</button>
      <span style="display:inline-flex"><button class="btn primary split-l" data-act="decide" data-arg="merge">${MERGE[S.merge]}${k}</button><button class="btn primary split-r" data-act="mergeMenu" aria-label="Merge method">${icon('chevron-down', 12)}</button></span></div>`;
  }
  if (t.failed) {
    return `<div class="${cls}"><div class="what" style="flex:none"><i class="dot red"></i><b>Checks failed</b></div>
      <input type="text" placeholder="Hint for the lead" aria-label="Hint for the lead">
      <button class="btn ghost" data-act="decide" data-arg="close">Close ticket</button>
      <button class="btn primary" data-act="decide" data-arg="hint" disabled>Send back with a hint${k}</button></div>`;
  }
  return '';
}

function inspHTML(t) {
  const d = D(t.id), verb = s => `<span class="verb">${s}</span>`, chip = s => `<span class="pchip">${s}</span>`;
  const rows = [
    ['Status', chip(`${ticketGlyph(t)}${ST[t.station].name}`)],
    ['Priority', `<button class="pchip" data-act="setPri">${pri(t.pri)}${PRI[t.pri]}</button>`],
    ['Labels', `<button class="pchip" data-act="setLabel">${labelDot(t.label)}${t.label}</button>`],
    null,
    ['Agent', chip(`${icon(t.agent === 'X' ? 'openai' : 'claude', 13)}${AGENT[t.agent]}`)],
    ['Project', chip(`${icon('folder', 13)}${t.project}`)],
    ['Branch', t.branch ? chip(`${icon('branch', 12)}<span class="mono">${t.branch}</span>`) : verb('Created at Implement')],
    ['PR', t.pr ? chip(`#${t.pr}${diffStat(t.diff)}`) : verb('Opens after Checks')],
    ['Sub-issues', d.tasks ? chip(`${d.tasks.filter(k => k.state === 'Done').length}/${d.tasks.length}`) : verb('Planned at Plan')],
    ['Updated', chip(`${age(t.mins)} ago`.replace('now ago', 'Just now'))],
  ];
  return `<aside class="insp">${rows.map(r => r ? `<div class="prop"><span>${r[0]}</span>${r[1]}</div>` : '<hr>').join('')}</aside>`;
}

function renderDetail() {
  const t = T(S.open), keep = detail.dataset.id === S.open ? $('.scroll', detail)?.scrollTop : 0;
  detail.dataset.id = S.open;
  detail.innerHTML = `<div class="detail-inner">
    <div class="tb"><button class="icon-btn" data-act="back" aria-label="Back to Board">${icon('back', 15)}</button><div class="crumb"><button class="crumb-btn" data-act="back">${icon('blocks', 13)}Board</button>${icon('chevron-right', 11)}<span class="key">${t.id}</span><span class="title">${esc(t.title)}</span></div></div>
    <div class="cols"><div class="pane"><div class="scroll"><div class="doc">${docHTML(t)}</div></div>${barHTML(t)}</div>${inspHTML(t)}</div>
  </div>`;
  $('.scroll', detail).scrollTop = keep || 0;
  syncSegs();
}

/* The surfaces a ticket is on screen in: the open page, the inbox pane, or both. */
function roots(id) {
  const r = [], pane = $('#inpane');
  if (S.open === id) r.push(detail);
  if (pane && S.inboxSel === id) r.push(pane);
  return r;
}
function refresh(id, { bar } = {}) {
  const t = T(id);
  for (const r of roots(id)) {
    $('.tabbody', r).outerHTML = tabBody(t, true);
    if (bar) $('.bar', r).outerHTML = barHTML(t, true);
    if (r === detail) $('.insp', r).outerHTML = inspHTML(t);
  }
}

/* ---------- inbox, settings ---------- */

function inboxView() {
  const items = inboxItems();
  if (!items.some(t => t.id === S.inboxSel)) S.inboxSel = items[Math.min(S.inboxAt || 0, items.length - 1)]?.id || null;
  S.inboxAt = items.findIndex(t => t.id === S.inboxSel);
  const TYPE = { plan: 'Plan approval', ask: 'Command ask', pr: 'PR review' };
  const row = t => `<button class="inrow" data-ticket="${t.id}" data-flip="in-${t.id}" data-act="inboxSel">${av(t.agent)}<b>${esc(t.title)}</b>
    <span class="end">${age(t.mins)}<i class="dot ${t.failed ? 'red' : 'you'}"></i></span>
    <div class="meta"><span>${t.id}</span><span class="ty">${TYPE[t.gate] || 'Checks failed'}</span><span class="${t.gate === 'ask' ? 'mono' : ''}">${t.gate === 'plan' ? plural(D(t.id).tasks.length, 'task') : t.gate === 'ask' ? esc(t.ask) : t.gate === 'pr' ? `#${t.pr} · checks green` : D(t.id).failure.attempts}</span></div></button>`;
  const group = (name, list) => list.length ? `<h6>${name} · ${list.length}</h6>${list.map(row).join('')}` : '';
  const sel = T(S.inboxSel);
  return `<div class="inbox">
    <div class="tb"><div class="crumb">${icon('inbox')}<b>Inbox</b><span class="hint">Needs you first, then failed</span></div><span class="grow"></span>
      <div class="hints"><kbd>J</kbd><kbd>K</kbd><span>move</span><kbd>⌘↵</kbd><span>decide</span></div></div>
    ${sel ? `<div class="split"><div class="inbox-list" data-keep><i class="selhl"></i>${group('Needs you', items.filter(t => t.gate))}${group('Failed', items.filter(t => !t.gate))}</div>
      <div class="pane" id="inpane">${paneHTML(sel)}</div></div>` : '<div class="empty"><b>Nothing waiting</b>The line is running</div>'}
  </div>`;
}
const paneHTML = t => `<div class="scroll"><div class="doc">${docHTML(t, true)}</div></div>${barHTML(t)}`;

let selG;
function syncInboxSel(animate) {
  const row = $(`.inrow[data-ticket="${S.inboxSel}"]`, view);
  if (!row) return;
  const to = { y: row.offsetTop, h: row.offsetHeight };
  const apply = v => {
    const hl = $('.selhl', view);
    if (hl) Object.assign(hl.style, { transform: `translateY(${v.y}px)`, height: `${v.h}px`, opacity: 1 });
  };
  if (!selG || !animate) selG = springs(to, { response: .22, precision: .2 }, apply);
  apply(selG.v);
  selG.to(to);
}
function selectInbox(id) {
  S.inboxSel = id;
  S.inboxAt = inboxItems().findIndex(t => t.id === id);
  delete segs['tab-i'];
  $('#inpane').innerHTML = paneHTML(T(id));
  syncSegs();
  syncInboxSel(true);
  $(`.inrow[data-ticket="${id}"]`, view).scrollIntoView({ block: 'nearest' });
}

const METRICS = [
  ['2.4', 'Merged per day'],
  ['3h 10m', 'Start → PR ready'],
  ['12m · 1h 5m · 4m', 'Wait at Plan · PR · Ask'],
  ['78% · 1.3', 'Auto-fix success · tries'],
];

function cfgHead() {
  const c = S.cfg, gates = 1 + c.planGate;
  const steps = ['To do', c.spec && 'Spec', c.planGate ? ['Plan approval'] : 'Plan', 'Implement', 'Checks', ['PR review'], 'Merged'].filter(Boolean);
  return `<p class="sum">${plural(gates, 'gate')} · ${c.fix ? `auto-fix ${c.fix}×` : 'auto-fix off'} · ${plural(c.slots, 'slot')}${c.enabled ? '' : ' · off for this project'}</p>
  <div class="stepper">${steps.map(s => Array.isArray(s) ? `<span class="gate"><i class="dot you" style="padding:0"></i>${s[0]}</span>` : `<span>${s}</span>`).join('<i>·</i>')}</div>`;
}
const cmdRows = () => S.cfg.checks.map((c, i) => `<div class="set cmd"><span class="mono">${esc(c)}</span><button class="icon-btn" data-act="rmCheck" data-arg="${i}" aria-label="Remove ${esc(c)}">${icon('x', 12)}</button></div>`).join('')
  + '<div class="set cmd"><input type="text" id="addcmd" placeholder="Add a command" aria-label="Add a check command"></div>';
function configView() {
  const c = S.cfg;
  const set = (title, sub, ctl) => `<div class="set"><div class="txt"><b>${title}</b><span>${sub}</span></div>${ctl}</div>`;
  const sw = k => `<button class="switch" role="switch" aria-checked="${c[k]}" data-act="cfgToggle" data-arg="${k}" aria-label="${k}"></button>`;
  const step = (k, min, max) => `<div class="step" data-step="${k}"><button data-act="cfgStep" data-arg="${k}:-1" ${c[k] <= min ? 'disabled' : ''} aria-label="Less">${icon('minus', 12)}</button><b>${c[k]}</b><button data-act="cfgStep" data-arg="${k}:1" ${c[k] >= max ? 'disabled' : ''} aria-label="More">${icon('plus', 12)}</button></div>`;
  const agent = k => `<button class="btn" data-act="cfgAgent" data-arg="${k}">${c[k]}${icon('chevron-down', 12)}</button>`;
  return `<div class="page">
    <div class="tb"><div class="crumb">${icon('settings')}<span>Settings</span>${icon('chevron-right', 11)}<b>Factory</b><span class="hint">anywhere</span></div><span class="grow"></span><span class="saved" id="saved">Saved</span></div>
    <div class="scroll" data-keep><div class="cfg">
      <div class="metrics" aria-label="Last 14 days, computed on this Mac">${METRICS.map(([v, l]) => `<div><b>${v}</b><span>${l}</span></div>`).join('')}</div>
      <p class="metrics-note">Last 14 days · computed on this Mac</p>
      <div id="cfg-head">${cfgHead()}</div>
      <h3>Pipeline</h3><div class="rows">
        ${set('Factory', 'Off: this project’s tickets behave as they do today', sw('enabled'))}
        ${set('Spec station', 'The lead writes a brief and acceptance criteria before it plans', sw('spec'))}
        ${set('Plan gate', 'No worker starts until you approve the lead’s plan', sw('planGate'))}
        ${set('PR gate', 'You review the PR and merge it. Agents never merge', `<span class="fixed">${icon('shield', 12)}Always on</span>`)}
        ${set('Reviewer', 'A separate read-only agent comments before the PR gate', sw('reviewer'))}
      </div>
      <h3>Profile per role</h3><div class="rows">
        ${set('Lead', 'Writes the spec and the plan, fixes failed checks', agent('lead'))}
        ${set('Workers', 'Default for each task; change it per task at the Plan gate', agent('workers'))}
        ${set('Reviewer', 'Read-only. A profile never lifts that', agent('reviewerAgent'))}
      </div>
      <h3>Profiles</h3><p class="lead-p">Shared by all projects. Factory rules always win over a profile.</p><div class="rows" id="profiles">${profileRows()}</div>
      <h3>Limits</h3><div class="rows">
        ${set('Slot limit', 'Tickets with a running agent at once. A ticket waiting on you holds no slot', step('slots', 1, 8))}
        ${set('Auto-fix attempts', 'Tries by the lead before failed checks come to you', step('fix', 0, 2))}
      </div>
      <h3>Check commands</h3><div class="rows" id="cmds">${cmdRows()}</div>
    </div></div>
  </div>`;
}
const kb = n => `${n.toFixed(1)} KB`;
const runs = f => !f.endsWith('.md');
function profileRows() {
  const meta = p => p.builtin ? 'No soul, instructions or skills'
    : [p.model, plural(p.skills.length, 'skill'), p.soul && 'soul', p.instructions && 'instructions'].filter(Boolean).join(' · ');
  return S.profiles.map(p => `<button class="set link" data-act="profile" data-arg="${esc(p.name)}">${av(avOf(p.name))}<div class="txt"><b>${esc(p.name)}</b><span>${meta(p)}</span></div>${p.builtin ? '<span class="fixed">Built-in</span>' : ''}${icon('chevron-right', 12, 'ic', 'text-4')}</button>`).join('')
    + `<button class="set link add" data-act="profile" data-arg="">${icon('plus', 13)}New profile</button>`;
}
/* Who would lose a profile if it went: the project's roles, and tasks of plans at the gate or running. */
function usersOf(name) {
  const roles = [['lead', 'Lead'], ['workers', 'Workers'], ['reviewerAgent', 'Reviewer']].filter(([k]) => S.cfg[k] === name).map(r => `${r[1]} role`);
  const tasks = S.tickets.filter(t => ['plan', 'implement'].includes(t.station) && S.details[t.id]?.tasks)
    .flatMap(t => S.details[t.id].tasks.filter(k => k.agent === name).map(k => `${t.id} ${k.id}`));
  return [...roles, ...tasks];
}
function rename(from, to) {
  for (const k of ['lead', 'workers', 'reviewerAgent']) if (S.cfg[k] === from) S.cfg[k] = to;
  for (const d of Object.values(S.details)) for (const k of d.tasks || []) if (k.agent === from) k.agent = to;
}
function profileSheet(anchor, p) {
  if (pop?.anchor === anchor) return hidePop();
  const ro = !!p?.builtin;
  const draft = p ? structuredClone(p) : { name: '', model: 'Claude · Sonnet', soul: '', instructions: '', skills: [] };
  let preview = null, picking = false, note = '';
  const el = document.createElement('div');
  el.className = 'pop dialog profile';
  const fileRow = f => `<div class="pf"><span class="mono">${esc(f[0])}</span><span>${kb(f[1])}</span></div>`;
  const skillRow = (s, i) => `<div class="srow">${icon('folder', 13)}<b>${esc(s.name)}</b><span>${plural(s.files.length, 'file')}${s.files.some(f => runs(f[0])) ? ' · has files the agent can run' : ''}</span>${ro ? '' : `<button class="icon-btn" data-rm="${i}" aria-label="Remove ${esc(s.name)}">${icon('x', 12)}</button>`}</div>`;
  const previewHTML = () => {
    const all = [...preview.extra, ...preview.skills.flatMap(s => s.files.map(f => [`skills/${s.name}/${f[0]}`, f[1]]))];
    const code = all.filter(f => runs(f[0]));
    return `<div class="preview"><div class="ph">${icon('folder', 13)}<b>${preview.from}</b><span>${kb(all.reduce((a, f) => a + f[1], 0))} of 2 MB</span></div>
      ${all.filter(f => !runs(f[0])).map(fileRow).join('')}
      ${code.length ? `<h6>Files the agent can run</h6>${code.map(fileRow).join('')}` : ''}
      <p class="hint">AGENTS.md is read as the instructions. Nothing is saved until you save the profile.</p>
      <div class="pa"><button class="btn ghost" data-do="cancelImport">Cancel</button><button class="btn" data-do="useImport">Use these files</button></div></div>`;
  };
  const sync = () => {
    const save = $('[data-do="save"]', el);
    if (!save) return;
    const name = draft.name.trim(), taken = S.profiles.some(x => x !== p && x.name === name);
    save.disabled = !name || taken || !!preview;
    $('.taken', el).hidden = !taken;
  };
  const draw = () => {
    const off = ro ? 'disabled' : '';
    el.innerHTML = `<h5>${ro ? esc(p.name) : p ? 'Edit profile' : 'New profile'}</h5>
      ${ro ? '<p class="ro">A built-in profile has no soul, instructions or skills, and can’t be changed.</p>' : ''}
      <label>Name<input type="text" data-f="name" value="${esc(draft.name)}" placeholder="Go backend" ${off}><em class="taken" hidden>Another profile has this name</em></label>
      <div class="lbl">Model</div><div class="pseg">${MODELS.map(m => `<button data-do="model" data-v="${m}" class="${draft.model === m ? 'on' : ''}" ${off}>${m}</button>`).join('')}</div>
      <label>Soul<span>Who the agent is: tone, habits, limits</span><textarea data-f="soul" placeholder="You are a careful engineer who…" ${off}>${esc(draft.soul)}</textarea></label>
      <label>Instructions<span>How it works on a task</span><textarea data-f="instructions" placeholder="Write a failing test first…" ${off}>${esc(draft.instructions)}</textarea></label>
      <div class="lbl">Skills<span>Copied into the agent’s checkout, never into a commit</span></div>
      <div class="rows">${draft.skills.length ? draft.skills.map(skillRow).join('') : '<div class="srow none">No skills</div>'}</div>
      ${ro ? '' : `<div class="adds"><button class="btn" data-do="import">${icon('folder', 13)}Import folder or zip</button><button class="btn" data-do="pick">${icon('plus', 13)}Add an installed skill</button></div>`}
      ${picking ? `<div class="rows picks">${INSTALLED.map((s, i) => `<button class="srow pick" data-add="${i}" ${draft.skills.some(x => x.name === s.name) ? 'disabled' : ''}>${icon('folder', 13)}<b>${s.name}</b><span>${plural(s.files.length, 'file')}${s.files.some(f => runs(f[0])) ? ' · has files the agent can run' : ''}</span>${draft.skills.some(x => x.name === s.name) ? 'Added' : ''}</button>`).join('')}</div>` : ''}
      ${preview ? previewHTML() : ''}
      ${note ? `<p class="refuse">${note}</p>` : ''}
      <div class="foot">${p && !ro ? '<button class="btn ghost danger" data-do="delete">Delete</button><span class="grow"></span>' : ''}<button class="btn ghost" data-do="close">${ro ? 'Close' : 'Cancel'}</button>${ro ? '' : `<button class="btn primary" data-do="save">Save profile</button>`}</div>`;
    sync();
  };
  el.addEventListener('input', e => { const f = e.target.dataset.f; if (f) { draft[f] = e.target.value; sync(); } });
  el.addEventListener('click', e => {
    const b = e.target.closest('[data-do], [data-rm], [data-add]');
    if (!b || b.disabled) return;
    note = '';
    if (b.dataset.rm) draft.skills.splice(+b.dataset.rm, 1);
    else if (b.dataset.add) { draft.skills.push(structuredClone(INSTALLED[+b.dataset.add])); }
    else switch (b.dataset.do) {
      case 'model': draft.model = b.dataset.v; break;
      case 'pick': picking = !picking; preview = null; break;
      case 'import': preview = IMPORT; picking = false; break;
      case 'cancelImport': preview = null; break;
      case 'useImport':
        draft.soul ||= IMPORT.soul;
        draft.instructions ||= IMPORT.instructions;
        for (const sk of IMPORT.skills) if (!draft.skills.some(x => x.name === sk.name)) draft.skills.push(structuredClone(sk));
        preview = null;
        break;
      case 'delete': {
        const users = usersOf(p.name);
        if (users.length) { note = `Can’t delete. Used by ${users.join(', ')}. Pick another profile there first.`; break; }
        S.profiles = S.profiles.filter(x => x !== p);
        hidePop();
        return profilesChanged();
      }
      case 'save': {
        draft.name = draft.name.trim();
        if (p) { if (p.name !== draft.name) rename(p.name, draft.name); Object.assign(p, draft); } else S.profiles.push(draft);
        hidePop();
        return profilesChanged();
      }
      case 'close': return hidePop();
    }
    draw();
  });
  draw();
  showPop(el, anchor, { modal: true });
  if (!ro) $('input', el).focus();
}
function profilesChanged() {
  if (S.route === 'config') { renderView(); saved(); }
  if (S.open) refresh(S.open);
}
const CFG_RANGE = { slots: [1, 8], fix: [0, 2] };
let savedTimer;
function saved() {
  $('#cfg-head').innerHTML = cfgHead();
  const el = $('#saved');
  el.classList.add('on');
  clearTimeout(savedTimer);
  savedTimer = setTimeout(() => el.classList.remove('on'), 1400);
}

/* ---------- shell ---------- */

function renderSidebar() {
  const live = S.tickets.filter(t => t.station !== 'merged'), f = S.filters, board = S.route === 'board';
  const row = (act, arg, ic, label, n, on, cls = '') => `<button class="nav-row ${cls} ${on ? 'on' : ''}" data-act="${act}" data-arg="${arg}" title="${label}">${ic}<span>${label}</span>${n == null ? '' : `<em>${n}</em>`}</button>`;
  const ic = n => icon(n, 17);
  sidebar.innerHTML = `<nav class="nav">
    ${row('sidebar', '', ic(S.sbOpen ? 'sidebar-collapse' : 'sidebar-expand'), 'Anywhere', null, false, 'head')}
    ${row('nav', 'inbox', ic('inbox'), 'Inbox', inboxItems().length ? `<i class="badge">${inboxItems().length}</i>` : null, S.route === 'inbox')}
    ${row('nav', 'you', ic('status-needs'), 'Waiting on you', S.tickets.filter(t => t.gate).length, board && f.att === 'you')}
    ${row('nav', 'board', ic('blocks'), 'Board', null, board && f.att !== 'you')}
    ${row('nav', 'automations', ic('bolt'), 'Automations', null, S.route === 'automations')}
  </nav>
  <div class="sb-label">Projects</div><nav class="nav">
    ${['anywhere', 'pocket-site'].map(p => row('project', p, ic('folder'), p, live.filter(t => t.project === p).length, false)).join('')}
  </nav>
  <div class="sb-label">Agents</div><nav class="nav">
    ${['C', 'X'].map(a => row('agent', a, av(a), AGENT[a], `${live.filter(t => t.agent === a && !t.gate && !t.failed && t.station !== 'todo').length} running`, false)).join('')}
  </nav>
  <nav class="nav sb-foot">
    ${row('nav', 'config', ic('settings'), 'Settings', null, S.route === 'config')}
    ${row('theme', '', ic('appearance'), S.theme === 'dark' ? 'Light mode' : 'Dark mode')}
    ${row('reset', '', ic('reload'), 'Reset demo')}
  </nav>`;
}

function renderView() {
  const key = `${S.route}:${S.layout}`;
  const keep = view.dataset.key === key ? $$('[data-keep]', view).map(e => [e.scrollTop, e.scrollLeft]) : [];
  view.dataset.key = key;
  view.innerHTML = {
    board: boardView, inbox: inboxView, config: configView,
    automations: () => `<div class="inbox"><div class="tb"><div class="crumb">${icon('bolt')}<b>Automations</b></div></div><div class="empty"><b>Unchanged by Light Factory</b>Not part of this prototype</div></div>`,
  }[S.route]();
  $$('[data-keep]', view).forEach((e, i) => { if (keep[i]) [e.scrollTop, e.scrollLeft] = keep[i]; });
  syncSegs();
  syncInboxSel(false);
}

const flips = new Map();
/** Re-render, then let every surviving row travel from where it was on screen to where it now belongs. */
function flip(mutate) {
  const before = new Map($$('[data-flip]', view).map(el => [el.dataset.flip, el.getBoundingClientRect()]));
  mutate();
  for (const el of $$('[data-flip]', view)) {
    const k = el.dataset.flip, b = before.get(k), old = flips.get(k);
    if (!b) { el.classList.add('enter'); continue; }
    const a = el.getBoundingClientRect(), x = b.left - a.left, y = b.top - a.top;
    old?.stop();
    flips.delete(k);
    if (!b.width || (Math.abs(x) < .5 && Math.abs(y) < .5)) continue;
    const g = springs({ x, y }, { response: .36, precision: .3 }, v => { el.style.transform = `translate(${v.x}px,${v.y}px)`; });
    el.style.transform = `translate(${x}px,${y}px)`;
    el.style.zIndex = 2;
    flips.set(k, g);
    g.to({ x: 0, y: 0 }, { velocity: old?.vel, onRest: () => { el.style.transform = el.style.zIndex = ''; flips.delete(k); } });
  }
}

function changed() {
  renderSidebar();
  flip(renderView);
  if (!S.open) return;
  if (T(S.open)) renderDetail();
  else closeTicket();
}

const segs = {};
function syncSegs() {
  for (const el of $$('.seg')) {
    const name = el.dataset.seg, on = $('.on', el);
    const to = { x: on.offsetLeft, w: on.offsetWidth };
    const apply = v => {
      const th = $(`.seg[data-seg="${name}"] .thumb`);
      if (th) Object.assign(th.style, { transform: `translateX(${v.x}px)`, width: `${v.w}px` });
    };
    segs[name] ||= springs(to, { response: .24, precision: .2 }, apply);
    apply(segs[name].v);
    segs[name].to(to);
  }
}

/* ---------- ticket zoom: the row you pressed becomes the page, and the page returns to it ---------- */

function sourceRect(id) {
  let el = $(`[data-ticket="${id}"]`, view);
  if (!el) return null;
  const rail = el.closest('.rail:not(.open)');
  if (rail) el = rail.firstElementChild;
  return el.getBoundingClientRect();
}
function zoomFrom(id) {
  const m = main.getBoundingClientRect(), r = sourceRect(id);
  if (!r || !r.width) return { x: 0, y: 24, s: .97, h: m.height };
  const s = r.width / m.width;
  return { x: r.left - m.left, y: r.top - m.top, s, h: Math.min(r.height / s, m.height) };
}
let zfrom = null;
const zoom = springs({ p: 0 }, { response: .36, precision: .001 }, v => {
  const H = main.clientHeight, q = 1 - v.p, p = clamp(v.p, 0, 1), inner = detail.firstElementChild;
  const s = 1 - (1 - zfrom.s) * q, h = H - (H - zfrom.h) * q;
  detail.style.transform = `translate(${zfrom.x * q}px,${zfrom.y * q}px) scale(${s})`;
  detail.style.clipPath = `inset(0 0 ${Math.max(0, H - h)}px 0 round ${9 * (1 - p) / s}px)`;
  detail.style.opacity = clamp(p * 4, 0, 1);
  if (inner) inner.style.opacity = clamp((p - .2) / .6, 0, 1);
  view.style.opacity = 1 - .5 * p;
  view.style.scale = 1 - .02 * p;
});
function openTicket(id) {
  const fresh = detail.hidden;
  S.open = id;
  delete segs['tab-d'];
  detail.hidden = false;
  renderDetail();
  view.inert = true;
  if (fresh) { zfrom = zoomFrom(id); zoom.set({ p: 0 }); }
  zoom.to({ p: 1 }, {
    onRest: () => { detail.style.transform = detail.style.clipPath = ''; },
  });
}
function closeTicket() {
  if (detail.hidden) return;
  view.inert = false;
  if (zoom.v.p > .98) zfrom = zoomFrom(S.open);
  zoom.to({ p: 0 }, {
    onRest: () => {
      detail.hidden = true;
      S.open = null;
      view.style.opacity = view.style.scale = '';
    },
  });
}

/* ---------- sidebar: toggles, drags 1:1, and a flick throws it shut ---------- */

const sb = springs({ w: S.sbW }, { response: .28, precision: .2 }, v => {
  win.style.setProperty('--sbw', `${v.w}px`);
  win.style.setProperty('--sbl', clamp((v.w - 96) / 80, 0, 1));
});
function setSidebar(open, opts) {
  S.sbOpen = open;
  renderSidebar();
  sb.to({ w: open ? S.sbW : RAIL }, opts);
}
/* B3: a working station is never hidden, so the sidebar gives way first. */
function fitSidebar() {
  if (S.sbOpen && S.route === 'board' && S.layout === 'factory' && win.clientWidth - S.sbW < 1164) setSidebar(false);
}
const handle = $('#resize');
handle.addEventListener('pointerdown', e => {
  handle.setPointerCapture(e.pointerId);
  const start = sb.v.w, x0 = e.clientX, tr = tracker();
  sb.stop();
  win.classList.add('dragging');
  const move = ev => {
    tr.push(ev.clientX, ev.timeStamp);
    let w = start + ev.clientX - x0;
    if (w > MAXW) w = MAXW + rubberband(w - MAXW, 240);
    else if (w < RAIL) w = RAIL - rubberband(RAIL - w, 120);
    sb.set({ w });
  };
  const up = ev => {
    handle.removeEventListener('pointermove', move);
    handle.removeEventListener('pointerup', up);
    handle.removeEventListener('pointercancel', up);
    win.classList.remove('dragging');
    const v = tr.velocity(ev.timeStamp), landing = sb.v.w + project(v);
    const open = landing > 150;
    if (open) S.sbW = clamp(landing, MINW, MAXW);
    setSidebar(open, { velocity: { w: v }, damping: Math.abs(v) > 300 ? .8 : 1, response: .28 });
  };
  handle.addEventListener('pointermove', move);
  handle.addEventListener('pointerup', up);
  handle.addEventListener('pointercancel', up);
});

/* ---------- layer: menus drop from their control; dialogs and the palette are modal ---------- */

let pop = null;
function showPop(el, anchor, { above, end, modal } = {}) {
  hidePop();
  let sc = null;
  if (modal) { sc = document.createElement('div'); sc.className = 'scrim'; layer.append(sc); }
  layer.append(el);
  if (!modal) {
    const r = anchor.getBoundingClientRect(), w = el.offsetWidth, h = el.offsetHeight;
    el.style.left = `${clamp(end ? r.right - w : r.left, 8, innerWidth - w - 8)}px`;
    const below = r.bottom + 6 + h <= innerHeight - 8;
    el.style.top = `${above || !below ? Math.max(8, r.top - h - 8) : r.bottom + 6}px`;
  }
  pop = { el, anchor, sc };
  el.offsetWidth;
  el.classList.add('in');
  sc?.classList.add('in');
}
function hidePop() {
  if (!pop) return;
  const { el, sc } = pop;
  pop = null;
  el.classList.remove('in');
  sc?.classList.remove('in');
  setTimeout(() => { el.remove(); sc?.remove(); }, 160);
}
function menu(anchor, items, onPick, opts) {
  if (pop?.anchor === anchor) return hidePop();
  const el = document.createElement('div');
  el.className = 'pop';
  el.innerHTML = items.map((it, i) => `<button class="mi" data-i="${i}"><span class="ck">${it.on ? icon('check', 12) : ''}</span>${esc(it.label)}${it.hint ? `<em>${esc(it.hint)}</em>` : ''}</button>`).join('');
  el.addEventListener('click', e => {
    const b = e.target.closest('.mi');
    if (!b) return;
    hidePop();
    onPick(items[+b.dataset.i].value);
  });
  showPop(el, anchor, opts);
}
function dialog(anchor, html, onSubmit) {
  if (pop?.anchor === anchor) return hidePop();
  const el = document.createElement('div');
  el.className = 'pop dialog';
  el.innerHTML = html;
  const input = $('textarea, input', el), submit = $('[data-submit]', el);
  const sync = () => { submit.disabled = !input.value.trim(); };
  const go = () => { if (!submit.disabled) { hidePop(); onSubmit(input.value.trim(), el); } };
  input.addEventListener('input', sync);
  input.addEventListener('keydown', e => { if (e.key === 'Enter' && (input.tagName === 'INPUT' || e.metaKey || e.ctrlKey)) { e.preventDefault(); go(); } });
  submit.addEventListener('click', go);
  $('[data-cancel]', el).addEventListener('click', hidePop);
  sync();
  showPop(el, anchor, { modal: true });
  input.focus();
  return el;
}

function palette(anchor) {
  const el = document.createElement('div');
  el.className = 'pop palette';
  el.innerHTML = '<input type="text" placeholder="Search" aria-label="Search"><div class="res"></div>';
  const input = $('input', el), res = $('.res', el);
  let rows = [], at = 0;
  const actions = [
    ['Go to Board', () => go('board')], ['Go to Inbox', () => go('inbox')], ['Go to Factory settings', () => go('config')],
    [S.theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode', () => actions_.theme()],
  ];
  const draw = () => {
    const q = input.value.trim().toLowerCase(), hit = s => s.toLowerCase().includes(q);
    const a = actions.filter(x => hit(x[0])), t = S.tickets.filter(x => hit(x.id) || hit(x.title));
    rows = [...a.map(x => x[1]), ...t.map(x => () => openTicket(x.id))];
    at = clamp(at, 0, Math.max(0, rows.length - 1));
    let i = 0;
    res.innerHTML = rows.length ? `${a.length ? '<div class="sec">Actions</div>' : ''}${a.map(x => `<button class="mi ${i === at ? 'cur' : ''}" data-i="${i++}"><span class="lead">${icon('chevron-right', 13)}</span><span>${x[0]}</span></button>`).join('')}
      ${t.length ? '<div class="sec">Tickets</div>' : ''}${t.map(x => `<button class="mi ${i === at ? 'cur' : ''}" data-i="${i++}"><span class="lead">${ticketGlyph(x)}</span><span class="id">${x.id}</span><span>${esc(x.title)}</span><em>${ST[x.station].name}</em></button>`).join('')}` : '<div class="none">No results</div>';
    $('.cur', res)?.scrollIntoView({ block: 'nearest' });
  };
  const run = i => { hidePop(); rows[i]?.(); };
  input.addEventListener('input', () => { at = 0; draw(); });
  input.addEventListener('keydown', e => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); at = clamp(at + (e.key === 'ArrowDown' ? 1 : -1), 0, rows.length - 1); draw(); }
    if (e.key === 'Enter') run(at);
  });
  res.addEventListener('click', e => { const b = e.target.closest('.mi'); if (b) run(+b.dataset.i); });
  draw();
  showPop(el, anchor, { modal: true });
  input.focus();
}

/* ---------- actions ---------- */

function go(route) {
  if (S.open) closeTicket();
  S.filters.att = route === 'you' ? 'you' : S.filters.att === 'you' ? null : S.filters.att;
  S.route = route === 'you' ? 'board' : route;
  renderSidebar();
  renderView();
  fitSidebar();
}
const ticketOf = el => el.closest('[data-ticket]')?.dataset.ticket || (detail.contains(el) ? S.open : S.inboxSel);

const actions_ = {
  nav: (el, arg) => go(arg),
  project(el, arg) { S.filters.project = S.filters.project === arg ? null : arg; go('board'); },
  agent(el, arg) { S.filters.agent = S.filters.agent === arg ? null : arg; go('board'); },
  layout(el, arg) {
    S.layout = arg;
    renderView();
    $('.content', view).style.animation = 'fade .22s ease both';
    fitSidebar();
  },
  att(el, arg) { S.filters.att = S.filters.att === arg ? null : arg; changed(); },
  filter(el, key) {
    const values = { project: ['anywhere', 'pocket-site'], label: LABELS, agent: ['C', 'X'], priority: [4, 3, 2, 1] }[key];
    const name = v => key === 'agent' ? AGENT[v] : key === 'priority' ? PRI[v] : v;
    menu(el, [{ label: 'All', value: null, on: !S.filters[key] }, ...values.map(v => ({ label: name(v), value: v, on: S.filters[key] === v }))], v => { S.filters[key] = v; changed(); });
  },
  display(el) {
    menu(el, [{ label: 'Labels on cards', value: 'labels', on: S.display.labels }, { label: 'Branch and PR on cards', value: 'branch', on: S.display.branch }], k => { S.display[k] = !S.display[k]; renderView(); }, { end: true });
  },
  rail: (el, arg) => setRail(arg, !S.rails[arg]),
  fold(el, arg) { S.folded[arg] = !S.folded[arg]; renderView(); },
  open: el => openTicket(ticketOf(el)),
  back: closeTicket,
  inboxSel: el => selectInbox(el.dataset.ticket),
  tab(el, arg) {
    const id = ticketOf(el);
    S.tab[id] = arg;
    for (const r of roots(id)) {
      $('.tabbody', r).outerHTML = tabBody(T(id));
      $$('.seg[data-seg^="tab"] button', r).forEach(b => b.classList.toggle('on', b.dataset.arg === arg));
    }
    syncSegs();
  },
  decide(el, arg) { decide(ticketOf(el), arg, $('input', el.closest('.bar'))?.value.trim()); },
  changes(el) {
    const id = ticketOf(el);
    dialog(el, `<h5>Request changes</h5><textarea placeholder="What should the lead change?" aria-label="What should the lead change?"></textarea>
      <div class="foot"><button class="btn ghost" data-cancel>Cancel</button><button class="btn primary" data-submit>Send to the lead<span class="key">⌘↵</span></button></div>`, text => decide(id, 'changes', text));
  },
  mergeMenu(el) {
    const id = ticketOf(el);
    menu(el, MERGE.map((label, i) => ({ label, value: i, on: S.merge === i })), i => { S.merge = i; refresh(id, { bar: true }); }, { above: true, end: true });
  },
  rmTask(el, arg) {
    const id = ticketOf(el), d = D(id), row = el.closest('.sub');
    const g = springs({ h: row.offsetHeight, o: 1 }, { response: .3, precision: .3 }, v => Object.assign(row.style, { height: `${v.h}px`, opacity: v.o, paddingBlock: `${10 * v.o}px` }));
    g.to({ h: 0, o: 0 }, {
      onRest: () => {
        d.tasks = d.tasks.filter(k => k.id !== arg);
        refresh(id, { bar: true });
        flip(renderView);
      },
    });
  },
  taskAgent(el, arg) {
    const id = ticketOf(el), k = D(id).tasks.find(x => x.id === arg);
    menu(el, S.profiles.map(p => ({ label: p.name, hint: p.builtin ? '' : p.model, value: p.name, on: k.agent === p.name })), m => { k.agent = m; refresh(id); });
  },
  setLabel(el) {
    const t = T(ticketOf(el));
    menu(el, LABELS.map(l => ({ label: l, value: l, on: t.label === l })), l => { t.label = l; refresh(t.id); flip(renderView); });
  },
  setPri(el) {
    const t = T(ticketOf(el));
    menu(el, [4, 3, 2, 1].map(p => ({ label: PRI[p], value: p, on: t.pri === p })), p => { t.pri = p; refresh(t.id); flip(renderView); });
  },
  newTicket(el) {
    const s = dialog(el, `<h5>New ticket</h5><input type="text" placeholder="Ticket title" aria-label="Ticket title">
      <div class="labels">${LABELS.map(l => `<button class="chip ${l === 'Feature' ? 'on' : ''}" data-label="${l}">${l}</button>`).join('')}</div>
      <div class="foot"><button class="btn ghost" data-cancel>Cancel</button><button class="btn primary" data-submit>Create ticket<span class="key">↵</span></button></div>`, (title, box) => {
      S.tickets.push({ id: `ANY-${S.nextId++}`, station: 'todo', title, label: $('.chip.on', box).dataset.label, agent: 'C', mins: 0, pri: 2, project: 'anywhere' });
      S.route = 'board';
      changed();
      if (S.layout === 'factory') setRail('todo', true);
    });
    s?.addEventListener('click', e => {
      const c = e.target.closest('[data-label]');
      if (c) $$('[data-label]', s).forEach(b => b.classList.toggle('on', b === c));
    });
  },
  search: el => palette(el),
  sidebar: () => setSidebar(!S.sbOpen),
  theme() {
    S.theme = S.theme === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = S.theme;
    renderSidebar();
  },
  reset() {
    seed();
    if (S.open) closeTicket();
    Object.keys(railG).forEach(k => { railG[k].stop(); delete railG[k]; });
    changed();
  },
  cfgToggle(el, k) {
    S.cfg[k] = !S.cfg[k];
    el.setAttribute('aria-checked', S.cfg[k]);
    saved();
  },
  cfgStep(el, arg) {
    const [k, dir] = arg.split(':'), [min, max] = CFG_RANGE[k], box = el.closest('.step');
    S.cfg[k] = clamp(S.cfg[k] + +dir, min, max);
    $('b', box).textContent = S.cfg[k];
    box.firstElementChild.disabled = S.cfg[k] <= min;
    box.lastElementChild.disabled = S.cfg[k] >= max;
    saved();
  },
  cfgAgent(el, k) {
    menu(el, S.profiles.map(p => ({ label: p.name, hint: p.builtin ? '' : p.model, value: p.name, on: S.cfg[k] === p.name })), m => { S.cfg[k] = m; el.firstChild.textContent = m; saved(); }, { end: true });
  },
  profile(el, arg) {
    profileSheet(el, arg ? prof(arg) : null);
  },
  rmCheck(el, i) {
    S.cfg.checks.splice(+i, 1);
    $('#cmds').innerHTML = cmdRows();
    saved();
  },
};

document.addEventListener('click', e => {
  const el = e.target.closest('[data-act]');
  if (el && !el.disabled) actions_[el.dataset.act](el, el.dataset.arg);
});
document.addEventListener('pointerdown', e => {
  if (pop && !pop.el.contains(e.target) && !pop.anchor.contains(e.target)) hidePop();
}, true);
document.addEventListener('input', e => {
  const bar = e.target.closest('.bar');
  if (bar) $('.primary', bar).disabled = !e.target.value.trim();
});

let gKey = 0;
document.addEventListener('keydown', e => {
  const typing = e.target.matches('input, textarea'), cmd = e.metaKey || e.ctrlKey;
  if (cmd && e.key.toLowerCase() === 'k') { e.preventDefault(); return pop?.el.classList.contains('palette') ? hidePop() : palette($('[data-act="search"]') || main); }
  if (e.key === 'Escape') return pop ? hidePop() : typing ? e.target.blur() : S.open && closeTicket();
  if (cmd && e.key === 'Enter') {
    if (pop) return;
    const b = $('.bar .primary:not([disabled]):not(.split-r)', S.open ? detail : view);
    if (!b) return;
    e.preventDefault();
    b.classList.add('pressed');
    return setTimeout(() => b.click(), 90);
  }
  if (e.key === 'Enter' && !typing && e.target.matches('[data-act="open"]')) return openTicket(e.target.dataset.ticket);
  if (e.key === 'Enter' && !e.shiftKey && e.target.matches('.composer textarea') && e.target.value.trim()) {
    e.preventDefault();
    const id = ticketOf(e.target), text = e.target.value.trim();
    log(id, ST[T(id).station].name, 'You', 'you', 'commented', { quote: text });
    for (const r of roots(id)) {
      $('.activity', r).innerHTML = activityHTML(D(id));
      $('.composer textarea', r).value = '';
      $('.act:last-child', r).scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
    return;
  }
  if (e.key === 'Enter' && e.target.id === 'addcmd' && e.target.value.trim()) {
    S.cfg.checks.push(e.target.value.trim());
    $('#cmds').innerHTML = cmdRows();
    $('#addcmd').focus();
    return saved();
  }
  if (typing || cmd || pop) return;
  const key = e.key.toLowerCase();
  if (key === ']') return setSidebar(!S.sbOpen);
  if (Date.now() - gKey < 800 && { b: 'board', i: 'inbox', f: 'config' }[key]) { gKey = 0; return go({ b: 'board', i: 'inbox', f: 'config' }[key]); }
  if (key === 'g') { gKey = Date.now(); return; }
  if ((key === 'j' || key === 'k') && S.route === 'inbox' && !S.open) {
    const items = inboxItems(), next = items[clamp(items.findIndex(t => t.id === S.inboxSel) + (key === 'j' ? 1 : -1), 0, items.length - 1)];
    if (next && next.id !== S.inboxSel) selectInbox(next.id);
  }
});
addEventListener('resize', fitSidebar);

seed();
document.documentElement.dataset.theme = S.theme;
sb.set({ w: S.sbW });
renderSidebar();
renderView();
fitSidebar();
})();
