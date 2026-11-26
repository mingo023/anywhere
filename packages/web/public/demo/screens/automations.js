// Automations: agents that run on a schedule. A list of automations (or of their runs) on the left, the selected one on the
// right, and an editor to write a new one. A run that asks for a decision is a session that needs you.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS } = AW;
const api = () => D.api;

// The demo's clock: Wednesday 14:23, minutes from midnight today, moving with the page.
const BASE = 14 * 60 + 23;
const START = Date.now();
const now = () => BASE + Math.floor((Date.now() - START) / 60000);
const WD = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const WD_PLURAL = ['Sundays', 'Mondays', 'Tuesdays', 'Wednesdays', 'Thursdays', 'Fridays', 'Saturdays'];
const TODAY = 3;
const WEEK = [1, 2, 3, 4, 5, 6, 0];
const LETTERS = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];
const pad = (n) => String(n).padStart(2, '0');
const clock = (m) => `${pad(Math.floor(((m % 1440) + 1440) % 1440 / 60))}:${pad(((m % 60) + 60) % 60)}`;
const dayOf = (m) => Math.floor(m / 1440);
const stamp = (m) => {
  const d = dayOf(m);
  return d === 0 ? `Today ${clock(m)}` : d === 1 ? `Tomorrow ${clock(m)}` : d === -1 ? `Yesterday ${clock(m)}` : `${WD[(((TODAY + d) % 7) + 7) % 7]} ${clock(m)}`;
};
const ago = (m) => { const n = Math.max(0, now() - m); return n < 1 ? 'just now' : n < 60 ? `${n}m ago` : `${Math.floor(n / 60)}h ago`; };
const short = (m) => (dayOf(m) === 0 ? clock(m) : stamp(m));
const dur = (min) => (min < 1 ? '' : min < 60 ? `${min}m` : `${Math.floor(min / 60)}h ${min % 60}m`);

const daysText = (days) => {
  const s = [...days].sort((a, b) => ((a + 6) % 7) - ((b + 6) % 7));
  const key = s.join();
  return key === '1,2,3,4,5,6,0' ? 'Every day' : key === '1,2,3,4,5' ? 'Weekdays' : key === '6,0' ? 'Weekends' : s.length === 1 ? WD_PLURAL[s[0]] : s.map((d) => WD[d]).join(', ');
};
const every = (n, unit) => (n === 1 ? `Every ${unit}` : `Every ${n} ${unit}s`);
const whenLabel = (s) => (s.kind === 'webhook' ? 'Webhook' : s.kind === 'days' ? `${daysText(s.days)} at ${s.time}` : s.min % 1440 === 0 ? every(s.min / 1440, 'day') : s.min % 60 === 0 ? every(s.min / 60, 'hour') : every(s.min, 'minute'));

function nextOf(a) {
  if (!a.enabled) return null;
  const s = a.schedule;
  if (s.kind === 'webhook') return null;
  if (s.kind === 'interval') return Math.ceil((now() + 1) / s.min) * s.min;
  const [h, m] = s.time.split(':').map(Number);
  for (let ahead = 0; ahead <= 7; ahead++) {
    const at = ahead * 1440 + h * 60 + m;
    if (s.days.includes((TODAY + ahead) % 7) && at > now()) return at;
  }
  return null;
}

const auto = (o) => ({ enabled: true, runs: 0, prompt: '', ...o });
const autos = [
  auto({ id: 'a1', name: 'Morning PR review', agent: 'claude', project: 'orbit-web', schedule: { kind: 'days', days: [1, 2, 3, 4, 5], time: '09:00' }, prompt: 'Review the open pull requests and leave comments on anything risky.', result: 'Reviewed 3 pull requests; 1 needs a second look.' }),
  auto({ id: 'a2', name: 'Dependency audit', agent: 'codex', project: 'fox-api', schedule: { kind: 'days', days: [5], time: '17:00' }, prompt: 'Audit the dependencies for known vulnerabilities and open a PR with safe upgrades.', result: 'Found 2 outdated packages and opened no PR.' }),
  auto({ id: 'a3', name: 'Flaky test triage', agent: 'codex', project: 'garden-mobile', schedule: { kind: 'interval', min: 120 }, prompt: 'Rerun the tests that failed on the last CI run and report the flaky ones.', result: 'Reran 14 tests; 1 is flaky.' }),
  auto({ id: 'a4', name: 'Re-run tests on push', agent: 'codex', project: 'orbit-web', schedule: { kind: 'webhook' }, prompt: 'When called, run the test suite on the pushed branch and report failures.', result: 'All tests passed.' }),
  auto({ id: 'a5', name: 'Nightly lint sweep', agent: 'claude', project: 'docs-site', schedule: { kind: 'days', days: [1, 2, 3, 4, 5], time: '23:00' }, prompt: 'Run the linter across the repo and fix what can be fixed safely.', result: 'Fixed 6 lint errors.' }),
  auto({ id: 'a6', name: 'Weekly changelog', agent: 'claude', project: 'orbit-web', schedule: { kind: 'days', days: [1], time: '08:00' }, enabled: false, prompt: 'Draft the changelog from last week’s merged pull requests.', result: 'Drafted the changelog.' }),
];
const autoOf = (id) => autos.find((a) => a.id === id);

const mkRun = (o) => ({ trigger: 'scheduled', why: '', summary: '', end: 0, ...o });
let runSerial = 10;
const runs = [
  mkRun({ id: 'r1', auto: 'a1', status: 'waiting', start: BASE - 8, why: 'Asked to run tests', summary: 'Wants to run pnpm test --filter orbit-web before commenting.', cmd: 'pnpm test --filter orbit-web' }),
  mkRun({ id: 'r3', auto: 'a3', status: 'running', start: BASE - 3, summary: '' }),
  mkRun({ id: 'r2', auto: 'a1', status: 'succeeded', start: -1440 + 9 * 60, end: -1440 + 9 * 60 + 12, summary: 'Reviewed 3 pull requests; 1 needs a second look.' }),
  mkRun({ id: 'r4', auto: 'a2', status: 'skipped', start: -5 * 1440 + 17 * 60, why: 'Skipped: the Mac was asleep' }),
];

const SEEDED = { autos: autos.length, runs: runs.length, waiting: { ...runs.find((r) => r.id === 'r1') } };

let tab = 'automations';
let sel = 'a1';
let selRun = 'r1';
let only = null;
let editor = null;
let menu = null;
const groups = { active: true, paused: false };

const lastRun = (a) => runs.find((r) => r.auto === a.id);
const waiting = () => runs.filter((r) => r.status === 'waiting');

// Sessions behind runs

function later(ms, fn) { setTimeout(() => { fn(); api().changed(); }, ms); }
const slug = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 22);

function makeRow(a, run, status, log, ask) {
  const p = api().PROJECTS.find((x) => x.name === a.project);
  const row = { id: Date.now() + runSerial++, p, label: `auto/${slug(a.name)}`, title: a.name, agent: a.agent, chat: true, add: 0, del: 0, status, log, ask };
  p.rows.unshift(row);
  p.open = true;
  run.row = row;
  return row;
}

// Back to the first frame, so the tour can play again: what it made goes, and the run that needed you needs you again.
function reset() {
  const drop = (row) => { if (row) row.p.rows = row.p.rows.filter((r) => r !== row); };
  autos.splice(0, autos.length - SEEDED.autos);
  runs.splice(0, runs.length - SEEDED.runs).forEach((r) => drop(r.row));
  const waitingRun = runs.find((r) => r.id === SEEDED.waiting.id);
  drop(waitingRun.row);
  delete waitingRun.row;
  Object.assign(waitingRun, SEEDED.waiting);
  runSerial = 10;
  tab = 'automations';
  sel = 'a1';
  selRun = 'r1';
  only = null;
  editor = null;
  menu = null;
  focusField = null;
}

function ensureRow(run) {
  if (run.row) return run.row;
  const a = autoOf(run.auto);
  return makeRow(a, run, 'needs', [['you', a.prompt], ['tool', 'Read', 'package.json'], ['say', 'I want to run the tests before I comment.']], {
    cmd: run.cmd, rule: 'pnpm test',
    after: [['tool', 'Bash', run.cmd], ['out', '3 test files, all passed'], ['tool', 'Edit', 'reviews/summary.md'], ['say', `Done. ${a.result}`]],
  });
}

function answer(run, allow) {
  const row = ensureRow(run);
  if (!allow) {
    row.status = undefined;
    row.log.push(['say', "Okay, I won't run it."]);
    Object.assign(run, { status: 'cancelled', end: now() });
    return;
  }
  row.status = 'working';
  row.ask.after.forEach((line, i, all) => later(900 * (i + 1), () => {
    row.log.push(line);
    if (i === all.length - 1) row.status = 'done';
  }));
}

function runNow(a) {
  const run = mkRun({ id: `r${runSerial++}`, auto: a.id, status: 'running', start: now(), trigger: 'manual' });
  runs.unshift(run);
  selRun = run.id;
  makeRow(a, run, 'working', [['you', a.prompt]]);
  later(1200, () => run.row.log.push(['say', "I'll start by looking at what changed."], ['tool', 'Search', 'recent changes']));
  later(2600, () => run.row.log.push(['tool', 'Read', 'src/app.tsx']));
  later(4000, () => { run.row.log.push(['say', `Done. ${a.result}`]); run.row.status = 'done'; });
}

// A run follows the session it started.
function sync() {
  for (const r of runs) {
    const s = r.row?.status;
    if (s === 'done' && r.status !== 'succeeded') Object.assign(r, { status: 'succeeded', end: Math.max(now(), r.start + 1), summary: autoOf(r.auto).result, why: '' });
    else if (s === 'working' && r.status !== 'running') r.status = 'running';
    else if (s === 'needs' && r.status !== 'waiting') r.status = 'waiting';
    else if (s === 'failed') r.status = 'failed';
  }
}

// Pieces

const glyph = (status, size = 14) => ({
  waiting: `<span class="halo"><i></i></span>`,
  running: api().spinner(size - 2),
  succeeded: `<span class="ok">${ic('check', size)}</span>`,
  failed: `<span class="bad">${ic('x-bold', size)}</span>`,
  skipped: `<span class="mute">${ic('run-skipped', size)}</span>`,
  cancelled: `<span class="mute">${ic('run-cancelled', size)}</span>`,
}[status] ?? '');
const LABEL = { waiting: 'Needs you', running: 'Working', succeeded: 'Done', failed: 'Failed', skipped: 'Skipped', cancelled: 'Stopped' };
const TONE = { waiting: 'wait', running: 'run', failed: 'bad' };
const trigText = (r) => (r.trigger === 'manual' ? 'Run now' : `Scheduled ${clock(r.start)}`);
const reason = (r) => (r.why && r.why !== trigText(r) ? `${trigText(r)} · ${r.why}` : trigText(r));
const runWhen = (r) => (['waiting', 'running'].includes(r.status) ? ago(r.start) : short(r.start));
const ageLong = (r) => { const n = Math.max(0, now() - r.start); return n < 1 ? 'just now' : n < 60 ? `${n}m ago` : `${Math.floor(n / 60)}h ago`; };
const runAge = (r) => (r.status === 'waiting' ? ageLong(r) : r.status === 'running' ? `Started ${clock(r.start)} · ${ago(r.start)}` : [stamp(r.start), r.end > r.start ? dur(r.end - r.start) : ''].filter(Boolean).join(' · '));
const toggle = (on) => `<span class="tg${on ? ' on' : ''}"><i></i></span>`;
const agentIcon = (a, s = 12) => `<span class="ag ${a}" style="display:flex">${ic(AGENTS[a][0], s)}</span>`;
const initials = (name) => name.split('-').map((w) => w[0]).join('').slice(0, 2).toUpperCase();
const folderOf = (a) => `~/code/${a.project}`;

function lastLabel(r) {
  if (!r) return '<span class="none">Never run</span>';
  if (r.status === 'succeeded') return `<span class="last">${esc(short(r.start))}</span>`;
  return `<span class="last ${TONE[r.status] ?? ''}">${['waiting', 'running'].includes(r.status) ? glyph(r.status, 11) : r.status === 'failed' ? glyph('failed', 11) : glyph(r.status, 12)}${LABEL[r.status]}</span>`;
}

function autoRow(a) {
  const next = nextOf(a);
  const line = whenLabel(a.schedule) + (a.enabled && next != null ? ` · next ${stamp(next)}` : '');
  const status = a.enabled ? lastLabel(lastRun(a)) : '<span class="paused">Paused</span>';
  const open = sel === a.id && !(editor && !editor.id);
  return `<div class="arow${open ? ' on' : ''}${a.enabled ? '' : ' off'}" data-act="pick" data-v="${a.id}">
    <button class="tgb" data-act="flip" data-v="${a.id}" aria-label="${a.enabled ? 'Pause' : 'Resume'}">${toggle(a.enabled)}</button>
    <div class="t1">${agentIcon(a.agent)}<span>${AGENTS[a.agent][1]} · ${esc(a.project)}</span></div>
    <div class="t2">${esc(a.name)}</div>
    <div class="t3">${ic('clock', 13)}<span class="tl">${esc(line)}</span>${status}</div>
  </div>`;
}

function banner() {
  const r = waiting()[0];
  if (!r || editor) return '';
  const a = autoOf(r.auto);
  return `<div class="banner"><span class="bdot"><i></i></span><div class="bt"><b>${esc(a.name)} needs you</b><span>${esc(r.summary)}</span></div><button class="pbtn prim sm" data-act="answer" data-v="${r.id}">Answer</button></div>`;
}

function draftRow() {
  if (!editor || editor.id) return '';
  const title = editor.title.trim() || 'New automation';
  const when = scheduleLabel() ?? 'No trigger yet';
  return `<div class="draft"><b>Draft</b><span class="dt">${esc(title)}</span><span class="dw">${ic('clock', 13)}<span>${esc(when)}</span></span></div>`;
}

function column() {
  const w = waiting().length;
  const head = `<div class="ahead"><b>Automations</b><button class="ib" data-act="new" aria-label="New automation">${ic('plus', 15)}</button></div>`;
  const tabs = `<div class="atabs">${[['automations', 'Automations'], ['runs', 'Runs']].map(([id, name]) => `<button class="${tab === id ? 'on' : ''}" data-act="tab" data-v="${id}">${name}${id === 'runs' && w ? `<span class="badge">${w}</span>` : ''}</button>`).join('')}</div>`;
  return `<div class="acol">${head}${tabs}${tab === 'automations' ? banner() : ''}${draftRow()}${tab === 'automations' ? autoList() : runsList()}</div>`;
}

function autoList() {
  const active = autos.filter((a) => a.enabled);
  const paused = autos.filter((a) => !a.enabled);
  const grp = (key, name, list) => !list.length ? '' : `<button class="ghd" data-act="group" data-v="${key}">${ic(groups[key] ? 'chevron-down' : 'chevron-right', 12)}${name}<span>${list.length}</span></button>${groups[key] ? list.map(autoRow).join('') : ''}`;
  return `<div class="alist">${autos.length ? grp('active', 'Active', active) + grp('paused', 'Paused', paused) : '<div class="blank">No automations yet.</div>'}</div>`;
}

function section(r) {
  if (r.status === 'waiting') return 'Needs you';
  if (r.status === 'running') return 'Working';
  const d = -dayOf(r.start);
  return d === 0 ? 'Today' : d === 1 ? 'Yesterday' : d < 7 ? 'Earlier this week' : 'Older';
}

function runsList() {
  const list = runs.filter((r) => !only || r.auto === only);
  const bar = only ? `<div class="only">Runs of ${esc(autoOf(only).name)}<i>·</i><button data-act="allruns2">Show all</button></div>` : '';
  const soon = autos.map((a) => [a, nextOf(a)]).filter(([, n]) => n != null).sort((x, y) => x[1] - y[1]).slice(0, 3);
  const up = !only && soon.length ? `<div class="rhead">Up next</div>${soon.map(([a, n]) => `<div class="upnext" data-act="pick-auto" data-v="${a.id}">${ic('clock', 13)}<span>${esc(a.name)}</span><small>${esc(stamp(n))}</small></div>`).join('')}` : '';
  const order = ['Needs you', 'Working', 'Today', 'Yesterday', 'Earlier this week', 'Older'];
  const body = order.map((name) => {
    const rs = list.filter((r) => section(r) === name);
    if (!rs.length) return '';
    return `<div class="rhead${name === 'Needs you' ? ' wait' : ''}">${name.toUpperCase()}<span>${rs.length}</span></div>${rs.map((r) => {
      const a = autoOf(r.auto);
      return `<div class="rrow${selRun === r.id ? ' on' : ''}" data-act="pick-run" data-v="${r.id}"><span class="rg">${glyph(r.status)}</span><div class="rt"><b>${esc(a.name)}</b><span>${esc(reason(r))}</span></div><small>${esc(runWhen(r))}</small></div>`;
    }).join('')}`;
  }).join('');
  return `<div class="alist runs">${bar}${list.length ? up + body : '<div class="blank">No runs yet.</div>'}</div>`;
}

// The right-hand page

const crumbs = (parent, title) => `<div class="crumbs2"><span>${parent}</span>${ic('chevron-right', 12)}<b>${esc(title)}</b></div>`;
const details = (rows) => `<div class="details">${rows.map(([k, v]) => `<div><span>${k}</span><span>${v}</span></div>`).join('')}</div>`;
const sec = (label, body) => `<div class="sec"><h4>${label}</h4>${body}</div>`;

function page() {
  if (editor) return editorPage();
  if (tab === 'runs') {
    const r = runs.find((x) => x.id === selRun && (!only || x.auto === only)) ?? runs.find((x) => !only || x.auto === only);
    return r ? runPage(r) : '<div class="apage"><div class="note">No runs yet.</div></div>';
  }
  const a = autoOf(sel) ?? autos[0];
  return a ? autoPage(a) : emptyPage();
}

const emptyPage = () => `<div class="apage"><div class="empty">${ic('bolt', 22)}<span>No automations yet.</span><button class="pbtn prim" data-act="new">${ic('plus', 14)}New automation</button></div></div>`;

function autoPage(a) {
  const rs = runs.filter((r) => r.auto === a.id);
  const w = rs.find((r) => r.status === 'waiting');
  const next = nextOf(a);
  const rows = [
    ['Trigger', esc(whenLabel(a.schedule))],
    ['Folder', esc(folderOf(a))],
    ['Agent', AGENTS[a.agent][1]],
    ['Next run', a.enabled ? (next != null ? stamp(next) : 'When its webhook is called') : 'Paused'],
  ];
  if (a.schedule.kind === 'webhook') rows.push(['Webhook', `<span class="mono">https://hooks.anywhere.dev/a/${a.id}x7f2</span>`]);
  else rows.push(['If missed', 'Skipped if more than 12 hours late']);
  const bars = rs.slice(0, 6).reverse().map((r) => `<i class="s-${r.status}"></i>`).join('');
  const recent = rs.length
    ? `<div class="acard">${rs.slice(0, 5).map((r, i) => recentRow(r, i)).join('')}</div>`
    : `<div class="acard none">No runs yet. It runs ${esc(whenLabel(a.schedule).replace(/^./, (c) => c.toLowerCase()))}, or press Run now.</div>`;
  return `<div class="apage">
    <div class="abar">${crumbs('Automations', a.name)}
      <button class="swt" data-act="flip" data-v="${a.id}">${toggle(a.enabled)}<span>${a.enabled ? 'Active' : 'Paused'}</span></button>
      <button class="pbtn scnd" data-act="edit" data-v="${a.id}">${ic('pencil', 14)}Edit</button>
      <button class="pbtn prim" data-act="run" data-v="${a.id}">${ic('play', 14)}Run now</button>
      <span class="rel"><button class="ib" data-act="more" aria-label="More">${ic('more', 15)}</button>${menu === 'more' ? `<div class="pop more"><button class="dang" data-act="delete" data-v="${a.id}">${ic('trash', 14)}Delete</button></div>` : ''}</span></div>
    <div class="abody"><div class="acenter">
      <div class="title"><span class="tile40">${initials(a.project)}</span><div><h2>${esc(a.name)}</h2><p>${agentIcon(a.agent, 14)}${AGENTS[a.agent][1]} · ${esc(a.project)}</p></div></div>
      ${w ? `<div class="wcard"><span class="wi">${ic('bolt', 17)}</span><div><b>Needs you</b><span>${esc(w.summary || `${a.name} paused until you answer`)}</span></div><button class="pbtn wait" data-act="answer" data-v="${w.id}">Answer</button></div>` : ''}
      ${sec('Details', details(rows))}
      <div class="sec"><h4>Recent runs<span class="rstrip">${bars}</span><button class="lnk" data-act="allruns" data-v="${a.id}">All runs</button></h4>${recent}</div>
    </div></div></div>`;
}

function recentRow(r, i) {
  const tone = TONE[r.status] ?? '';
  const out = r.why ? `${LABEL[r.status]} · ${r.why}` : LABEL[r.status];
  return `<div class="recent${i ? ' sepline' : ''}"><span class="rg">${glyph(r.status)}</span><div class="rm"><div><b>${esc(trigText(r))}</b><span class="oc ${tone}">${esc(out)}</span></div>${r.summary ? `<p>${esc(r.summary)}</p>` : ''}</div>
    <div class="rr"><span>${esc(runAge(r))}</span>${r.row ? `<button class="lnk ul" data-act="open-run" data-v="${r.id}">Open${ic('external', 12)}</button>` : ''}</div></div>`;
}

function runPage(r) {
  const a = autoOf(r.auto);
  const pill = `<span class="spill ${r.status}">${r.status === 'waiting' ? '<i class="d"></i>' : r.status === 'running' ? glyph('running', 11) : glyph(r.status, 11)}${LABEL[r.status]}</span>`;
  const approval = r.status === 'waiting' ? `<div class="approval"><div class="ah"><span class="bdot"><i></i></span><b>Allow command?</b><span>${AGENTS[a.agent][1]} paused until you answer</span></div>
      <pre>$ ${esc(r.cmd)}</pre>
      <div class="ab"><button class="pbtn prim lg" data-act="allow" data-v="${r.id}">Allow once</button><button class="pbtn ghost lg" data-act="deny" data-v="${r.id}">Deny</button><button class="reply" data-act="open-run" data-v="${r.id}">Reply in session${ic('external', 11)}</button></div></div>` : '';
  const failure = r.status === 'failed' ? `<div class="rfail">Run failed${r.why ? ` · ${esc(r.why)}` : ''}</div>` : '';
  const summary = r.summary && r.status !== 'waiting' ? sec('Summary', `<div class="acard summary">${esc(r.summary)}</div>`) : '';
  const duration = ['running', 'waiting'].includes(r.status) ? 'In progress' : r.end > r.start ? dur(r.end - r.start) : '—';
  const rows = [['Trigger', esc(whenLabel(a.schedule))], ['Started', esc(stamp(r.start))], ['Duration', duration], ['Permission mode', "Agent's settings"], ['Folder', esc(folderOf(a))]];
  if (r.row) rows.push(['Worktree', `${esc(folderOf(a))}/.worktrees/${slug(a.name)}`]);
  return `<div class="apage"><div class="abar">${crumbs('Runs', reason(r))}${r.row ? `<button class="pbtn scnd" data-act="open-run" data-v="${r.id}">Open session${ic('external', 12)}</button>` : ''}</div>
    <div class="abody"><div class="acenter">
      <div class="rh">${pill}<span>${esc(runAge(r))}</span></div><h2 class="rtitle">${esc(reason(r))}</h2>
      <p class="ran">${agentIcon(a.agent, 14)}${AGENTS[a.agent][1]} ran <button class="lnk blue" data-act="pick-auto" data-v="${a.id}">${esc(a.name)}</button> in ${esc(a.project)}</p>
      ${approval}${failure}${summary}${sec('Details', details(rows))}
    </div></div></div>`;
}

// The editor

const TEMPLATES = [
  ['merge', 'Review draft PRs', 'List the open draft pull requests, read each diff, and leave review comments on anything risky. Do not push commits.', { days: [1, 2, 3, 4, 5], time: '10:00' }],
  ['shield', 'Watch failing checks', 'Check the latest CI runs on the default branch. For each failure, find the cause and summarise it. Fix flaky tests only if the change is small.', { every: 60 }],
  ['settings', 'Environment doctor', 'Verify the toolchain builds from a clean checkout: install dependencies, build, run the test suite. Report what broke and why.', { days: [1], time: '08:00' }],
  ['flow', 'Repo health check', 'Look for stale branches, unused dependencies, and TODOs older than 90 days. Summarise what to clean up first.', { days: [5], time: '16:00' }],
];
const EVERIES = [['15 min', 15], ['30 min', 30], ['1 hour', 60], ['6 hours', 360], ['1 day', 1440]];
const PRESETS = [['Weekdays', [1, 2, 3, 4, 5]], ['Every day', [1, 2, 3, 4, 5, 6, 0]], ['Weekends', [6, 0]]];

function blank(a) {
  return a
    ? { id: a.id, title: a.name, prompt: a.prompt, scheduled: true, repeat: a.schedule.kind === 'interval', webhook: a.schedule.kind === 'webhook', days: new Set(a.schedule.days ?? [1, 2, 3, 4, 5]), time: a.schedule.time ?? '09:00', every: String(a.schedule.min ?? 60), project: a.project, agent: a.agent, worktree: true, enabled: a.enabled }
    : { id: null, title: '', prompt: '', scheduled: false, repeat: false, webhook: false, days: new Set([1, 2, 3, 4, 5]), time: '09:00', every: '60', project: 'orbit-web', agent: 'claude', worktree: true, enabled: true };
}

const validTime = (t) => /^([01]\d|2[0-3]):[0-5]\d$/.test(t.trim());
function scheduleOf() {
  const e = editor;
  if (!e.scheduled) return null;
  if (e.webhook) return { kind: 'webhook' };
  if (e.repeat) { const n = Number(e.every); return Number.isInteger(n) && n >= 5 && n <= 10080 ? { kind: 'interval', min: n } : null; }
  return e.days.size && validTime(e.time) ? { kind: 'days', days: [...e.days], time: e.time.trim() } : null;
}
const scheduleLabel = () => { const s = scheduleOf(); return s ? whenLabel(s) : null; };
function whyNot() {
  const e = editor;
  if (!e.prompt.trim()) return 'Write instructions';
  if (!e.scheduled) return 'Add a trigger';
  if (!e.webhook && e.repeat && !scheduleOf()) return 'Repeat every 5 minutes to 7 days';
  if (!e.webhook && !e.repeat && !e.days.size) return 'Pick at least one day';
  if (!scheduleOf()) return 'Set a time as HH:MM';
  return null;
}

function triggerCard() {
  const e = editor;
  if (!e.scheduled) {
    return `<div class="acard addtrig rel"><button class="addbtn" data-act="addtrig">${ic('plus', 15)}Add trigger</button>${menu === 'trigger' ? `<div class="pop trig"><button class="mr" data-act="schedule">${ic('clock', 14)}<span>Schedule</span></button><hr><div class="sh">Events</div>${['Pull request opened', 'Issue created', 'Check failed'].map((n) => `<div class="mr off">${ic('bolt', 15)}<span>${n}</span><small>Soon</small></div>`).join('')}</div>` : ''}</div>`;
  }
  const at = scheduleOf() && !e.webhook ? nextOf({ enabled: true, schedule: scheduleOf() }) : null;
  const head = `<div class="th">${ic('clock', 16)}<b>${esc(scheduleLabel() ?? 'Schedule')}</b><span>${at != null ? `· next ${esc(stamp(at))}` : ''}</span><button class="ib" data-act="rmtrig" aria-label="Remove trigger">${ic('x', 14)}</button></div>`;
  const mode = `<div class="seg">${['Days and time', 'Repeat'].map((n, i) => `<button class="${(e.repeat ? 1 : 0) === i ? 'on' : ''}" data-act="mode" data-v="${i}">${n}</button>`).join('')}</div>`;
  const chipx = (label, on, act, v) => `<button class="pchip${on ? ' on' : ''}" data-act="${act}" data-v="${v}">${label}</button>`;
  const body = e.repeat
    ? `<div class="lbl">Repeat every</div><div class="every"><input class="num" data-f="every" value="${esc(e.every)}" spellcheck="false"><span>minutes</span></div><div class="chips">${EVERIES.map(([n, m]) => chipx(n, Number(e.every) === m, 'every', m)).join('')}</div>`
    : `<div class="days">${WEEK.map((d) => `<button class="day${e.days.has(d) ? ' on' : ''}" data-act="day" data-v="${d}">${LETTERS[d]}</button>`).join('')}</div>
       <div class="chips">${PRESETS.map(([n, ds], i) => chipx(n, ds.length === e.days.size && ds.every((d) => e.days.has(d)), 'preset', i)).join('')}</div>
       <div class="lbl">Time</div><input class="num time" data-f="time" value="${esc(e.time)}" spellcheck="false">`;
  const err = e.repeat ? (scheduleOf() ? '' : 'Between 5 minutes and 7 days') : (validTime(e.time) ? '' : 'Use 24-hour HH:MM, like 09:00');
  return `<div class="acard trigcard">${head}${mode}${body}${err ? `<div class="err">${err}</div>` : ''}</div>`;
}

function pickMenu(kind) {
  const rows = kind === 'project'
    ? api().PROJECTS.map((p) => `<button class="mr" data-act="set" data-v="project:${p.name}"><span class="mini">${api().letter(p.name)}</span><span>${esc(p.name)}</span>${editor.project === p.name ? ic('check', 13) : ''}</button>`)
    : kind === 'agent'
      ? ['claude', 'codex', 'cursor', 'grok'].map((a) => `<button class="mr" data-act="set" data-v="agent:${a}">${agentIcon(a, 14)}<span>${AGENTS[a][1]}</span>${editor.agent === a ? ic('check', 13) : ''}</button>`)
      : [[true, 'worktree', 'Worktree', 'Each run gets its own branch and copy of the files'], [false, 'laptop', 'Local', "Runs in the project's checkout: files and branch are shared"]]
        .map(([v, icon, name, hint]) => `<button class="mr two" data-act="set" data-v="worktree:${v}">${ic(icon, 15)}<span><b>${name}</b><small>${hint}</small></span>${editor.worktree === v ? ic('check', 13) : ''}</button>`);
  return `<div class="pop ${kind}">${rows.join('')}</div>`;
}

function editorPage() {
  const e = editor;
  const why = whyNot();
  const editing = !!e.id;
  const crumb = editing ? `Edit · ${e.title.trim() || 'Untitled'}` : 'New automation';
  const templates = !editing && !e.prompt.trim() ? `<div class="tpls">${TEMPLATES.map((t, i) => `<button data-act="tpl" data-v="${i}">${ic(t[0], 14)}${t[1]}</button>`).join('')}</div>` : '';
  const chipM = (kind, inner) => `<span class="rel"><button class="mchip${menu === kind ? ' on' : ''}" data-act="emenu" data-v="${kind}">${inner}${ic('chevron-down', 12)}</button>${menu === kind ? pickMenu(kind) : ''}</span>`;
  return `<div class="apage"><div class="abar">${crumbs('Automations', crumb)}
      ${why ? `<span class="why">${why}</span>` : ''}<button class="pbtn scnd" data-act="cancel">Cancel</button><button class="pbtn prim${why ? ' dis' : ''}" data-act="save">${editing ? 'Save' : 'Create'}</button></div>
    <div class="abody"><div class="acenter ed">
      <input class="etitle" data-f="title" value="${esc(e.title)}" placeholder="Untitled automation" spellcheck="false">
      <button class="swt" data-act="eenable">${toggle(e.enabled)}<span>${e.enabled ? 'Active' : 'Paused'}</span></button>
      ${templates}
      ${sec('Trigger', triggerCard())}
      ${sec('Instructions', `<div class="instr"><div class="iframe${e.prompt.trim() ? ' has' : ''}"><textarea data-f="prompt" rows="4" placeholder="Tell the agent what to do when this automation runs…" spellcheck="false">${esc(e.prompt)}</textarea>
        <div class="ibar"><button class="mchip plain">${ic('shield', 14)}Agent's settings${ic('chevron-down', 12)}</button><i></i>${chipM('agent', `${agentIcon(e.agent, 14)}${AGENTS[e.agent][1]}`)}</div></div>
        <div class="runsin">${chipM('project', `<span class="mini">${api().letter(e.project)}</span>${esc(e.project)}`)}<span class="dotsep">·</span>${chipM('worktree', `${ic(e.worktree ? 'worktree' : 'laptop', 14)}${e.worktree ? 'Worktree' : 'Local'}`)}</div></div>`)}
    </div></div></div>`;
}

function save() {
  const e = editor;
  if (whyNot()) return;
  const text = e.prompt.trim();
  const name = e.title.trim() || text.split('\n')[0].slice(0, 40);
  const a = e.id ? autoOf(e.id) : null;
  const fields = { name, prompt: text, agent: e.agent, project: e.project, schedule: scheduleOf(), enabled: e.enabled, result: a?.result ?? 'Finished the run; nothing needs your attention.' };
  if (a) Object.assign(a, fields);
  else { const n = auto({ id: `a${runSerial++}`, ...fields }); autos.unshift(n); sel = n.id; }
  tab = 'automations';
  editor = null;
  menu = null;
  if (!a) groups.active = true;
}

function render() {
  return `${column()}${page()}${menu ? '<div class="scrim" data-act="closemenu"></div>' : ''}`;
}

function act(a, v, el) {
  const A = api();
  if (!['emenu', 'more', 'addtrig'].includes(a) && menu && !['set'].includes(a)) menu = null;
  switch (a) {
    case 'tab': tab = v; editor = null; return A.draw();
    case 'pick': sel = v; editor = null; return A.draw();
    case 'pick-auto': sel = v; tab = 'automations'; editor = null; return A.draw();
    case 'pick-run': selRun = v; editor = null; return A.draw();
    case 'group': groups[v] = !groups[v]; return A.draw();
    case 'flip': { const x = autoOf(v); x.enabled = !x.enabled; if (!x.enabled) groups.paused = groups.paused; return A.draw(); }
    case 'run': runNow(autoOf(v)); A.changed(); return;
    case 'new': editor = blank(null); tab = 'automations'; return A.draw();
    case 'edit': editor = blank(autoOf(v)); return A.draw();
    case 'cancel': editor = null; return A.draw();
    case 'save': save(); return A.draw();
    case 'more': menu = menu === 'more' ? null : 'more'; return A.draw();
    case 'closemenu': menu = null; return A.draw();
    case 'delete': { const i = autos.findIndex((x) => x.id === v); autos.splice(i, 1); for (let k = runs.length - 1; k >= 0; k--) if (runs[k].auto === v) runs.splice(k, 1); sel = autos[0]?.id; return A.draw(); }
    case 'allruns': only = v; tab = 'runs'; selRun = runs.find((r) => r.auto === v)?.id; return A.draw();
    case 'allruns2': only = null; return A.draw();
    case 'answer': { const r = runs.find((x) => x.id === v); const row = ensureRow(r); return A.select(row); }
    case 'open-run': { const r = runs.find((x) => x.id === v); const row = ensureRow(r); return A.select(row); }
    case 'allow': answer(runs.find((x) => x.id === v), true); return A.changed();
    case 'deny': answer(runs.find((x) => x.id === v), false); return A.changed();
    // editor
    case 'addtrig': menu = menu === 'trigger' ? null : 'trigger'; return A.draw();
    case 'schedule': Object.assign(editor, { scheduled: true, webhook: false }); menu = null; return A.draw();
    case 'rmtrig': editor.scheduled = false; return A.draw();
    case 'mode': editor.repeat = v === '1'; return A.draw();
    case 'day': { const d = +v; editor.days.has(d) ? editor.days.delete(d) : editor.days.add(d); return A.draw(); }
    case 'preset': editor.days = new Set(PRESETS[+v][1]); return A.draw();
    case 'every': editor.every = String(v); return A.draw();
    case 'eenable': editor.enabled = !editor.enabled; return A.draw();
    case 'emenu': menu = menu === v ? null : v; return A.draw();
    case 'set': {
      const [k, val] = v.split(':');
      editor[k] = k === 'worktree' ? val === 'true' : val;
      menu = null;
      return A.draw();
    }
    case 'tpl': {
      const [, title, prompt, when] = TEMPLATES[+v];
      Object.assign(editor, { title, prompt, scheduled: true, webhook: false });
      if (when.every) Object.assign(editor, { repeat: true, every: String(when.every) });
      else Object.assign(editor, { repeat: false, days: new Set(when.days), time: when.time });
      return A.draw();
    }
  }
}

let focusField = null;
function mount(root) {
  const ta = root.querySelector('.iframe textarea');
  if (ta) grow(ta);
  if (focusField) {
    const el = root.querySelector(`[data-f="${focusField}"]`);
    if (el) { el.focus(); el.setSelectionRange?.(el.value.length, el.value.length); }
  }
}
function grow(el) { el.style.height = 'auto'; el.style.height = `${Math.max(88, Math.min(el.scrollHeight, 280))}px`; }

D.register('automations', {
  page: 'automations',
  tour: 'automations',
  render,
  mount,
  act,
  enter() { reset(); sync(); },
  input(e) {
    const f = e.target.dataset.f;
    if (!f || !editor) return;
    focusField = f;
    editor[f] = e.target.value;
    if (f === 'prompt') grow(e.target);
    if (['title', 'prompt', 'every', 'time'].includes(f)) refresh();
  },
});

// Redraw what depends on the fields without taking the caret away from the one being typed in.
function refresh() {
  const root = document.querySelector('.app[data-screen="automations"]');
  if (!root) return;
  const why = whyNot();
  const save = root.querySelector('[data-act="save"]');
  save?.classList.toggle('dis', !!why);
  const w = root.querySelector('.why');
  if (w) w.textContent = why ?? '';
  const draft = root.querySelector('.draft');
  if (draft) { draft.querySelector('.dt').textContent = editor.title.trim() || 'New automation'; draft.querySelector('.dw span').textContent = scheduleLabel() ?? 'No trigger yet'; }
  const crumb = root.querySelector('.crumbs2 b');
  if (crumb) crumb.textContent = editor.id ? `Edit · ${editor.title.trim() || 'Untitled'}` : 'New automation';
  const tpls = root.querySelector('.tpls');
  if (tpls && editor.prompt.trim()) tpls.remove();
  const th = root.querySelector('.th b');
  if (th) th.textContent = scheduleLabel() ?? 'Schedule';
}

AnywhereTour.define('automations', [
  ['wait', 500],
  ['click', '.ahead [data-act="new"]'],
  ['wait', 600],
  ['click', '.tpls [data-act="tpl"][data-v="0"]'],
  ['wait', 1200],
  ['click', '.seg [data-act="mode"][data-v="1"]'],
  ['wait', 700],
  ['click', '[data-act="save"]'],
  ['wait', 700],
  ['click', '.abar [data-act="run"]'],
  ['wait', 1500],
  ['click', '.atabs [data-act="tab"][data-v="runs"]'],
  ['wait', 500],
  ['click', '.rrow[data-v="r1"]'],
  ['wait', 900],
  ['click', '.approval [data-act="allow"]'],
  ['wait', 4300],
]);

addEventListener('DOMContentLoaded', () => {
  api().onChange(sync);
  // The flaky-test run in progress finishes on its own.
  setTimeout(() => { const r = runs.find((x) => x.id === 'r3'); if (r && r.status === 'running') { Object.assign(r, { status: 'succeeded', end: now(), summary: 'Reran 14 tests; 1 is flaky.' }); api().changed(); } }, 14000);
});
})();
