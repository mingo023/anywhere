// Settings, opened on Hosts: the window's own section sidebar, the hosts list, and the Add host form. Adding a host
// connects, installs pocketd with the app's own step labels, and lists the host as connected.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS } = AW;
const api = () => D.api;

const GROUPS = [
  ['App', [['general', 'General', 'sliders'], ['appearance', 'Appearance', 'appearance'], ['notifications', 'Notifications & Sounds', 'bell'], ['keyboard', 'Keyboard', 'keyboard']]],
  ['Agents', [['agents', 'Agents', 'sparkle'], ['accounts', 'Accounts', 'shield'], ['automations', 'Automations', 'clock'], ['phone', 'Phone & Remote', 'phone'], ['hosts', 'Hosts', 'server']]],
  ['Projects', [['projects', 'Projects', 'folder'], ['sidebar', 'Sidebar & Inbox', 'sidebar'], ['terminal', 'Terminal', 'terminal'], ['files', 'Files & Editor', 'file'], ['browser', 'Browser', 'globe']]],
  ['Git', [['git', 'Git & GitHub', 'branch'], ['diff', 'Diff & Graph', 'diff-split']]],
];
const SECTIONS = Object.fromEntries(GROUPS.flatMap(([, list]) => list).map(([id, label, icon]) => [id, { label, icon }]));
const SUBTITLE = {
  general: 'Updates, startup, quitting and how hard the app works in the background.',
  appearance: 'Theme, color and type for the app. The terminal has its own font and colors.',
  notifications: 'How agents reach you, by urgency: Needs you, Failed, Done.',
  keyboard: 'Every shortcut, grouped by what it does. Click one to change it.',
  agents: 'The agent providers you use, and what a new session starts with.',
  accounts: 'Claude Code and Codex sign-ins on each machine, and the one new agents start on.',
  automations: 'Defaults for new automations, and what happens to runs the Mac missed.',
  phone: 'Pair the iPhone app and decide how much it can approve, and from where.',
  hosts: 'Machines your agents run on: this Mac, and any machine you can sign in to over SSH.',
  projects: 'Where code lives on disk, per-project setup, and what happens when a worktree is deleted.',
  sidebar: 'How sessions are listed in the sidebar, the right panel and the Inbox.',
  terminal: 'The terminal that every shell and agent runs in.',
  files: 'Tabs, the built-in editor and what the Explorer shows.',
  browser: 'The in-app browser for docs, pull requests and your dev servers.',
  git: 'Committing, pushing, AI commit messages and pull requests.',
  diff: 'How changes and history are shown in Changes and the commit graph.',
};

// Rows of the pages that are filled in. kind: switch | seg | drop | step | slider | swatch | buttons | providers
const THEMES = ['System', 'Graphite Light', 'Graphite Dark', 'Solarized Light', 'GitHub Light', 'Dracula', 'One Dark', 'Rosé Pine', 'Tokyo Night'];
const PAGES = {
  appearance: [
    ['Theme', [
      { id: 'theme', title: 'Theme', hint: 'System follows macOS light and dark', kind: 'drop', opts: THEMES, def: 'System' },
      { id: 'day', title: 'Day', hint: 'While macOS is light', kind: 'drop', opts: THEMES.filter((t) => /Light/.test(t)), def: 'Graphite Light' },
      { id: 'night', title: 'Night', hint: 'While macOS is dark', kind: 'drop', opts: THEMES.filter((t) => !/Light|System/.test(t)), def: 'Graphite Dark' },
      { id: 'custom', title: 'Custom themes', hint: 'Import a theme file, or start from one on GitHub', kind: 'buttons', buttons: ['Starter on GitHub', 'Import'] },
      { id: 'accent', title: 'Accent color', hint: 'Selection, focus rings and primary buttons', kind: 'swatch', opts: [['Graphite', '#ebebeb'], ['Blue', '#70b8ff'], ['Indigo', '#7c85e3'], ['Green', '#30a46c'], ['Orange', '#f76b15'], ['Rose', '#e5484d']], def: 'Graphite' },
      { id: 'glass', title: 'Window opacity', hint: 'Dark themes only: lower lets more of the desktop show through the glass', kind: 'slider', def: 85 },
      { id: 'motion', title: 'Reduce motion', hint: 'System follows the macOS accessibility setting', kind: 'seg', opts: ['System', 'On', 'Off'], def: 'System' },
    ]],
    ['Fonts', [
      { id: 'uifont', title: 'Interface font', kind: 'drop', opts: ['Geist', 'SF Pro', 'Inter', 'System UI'], def: 'Geist' },
      { id: 'codefont', title: 'Code font', hint: 'Diffs, the editor and file previews', kind: 'drop', opts: ['Geist Mono', 'SF Mono', 'JetBrains Mono', 'Menlo'], def: 'Geist Mono' },
      { id: 'codesize', title: 'Code size', kind: 'step', min: 10, max: 20, unit: ' pt', def: 13 },
    ]],
    ['Code colors', [
      { id: 'syntax', title: 'Syntax theme', kind: 'drop', opts: ['From theme', 'GitHub', 'One', 'Solarized'], def: 'From theme' },
      { id: 'diffcolors', title: 'Diff colors', hint: 'Blue and orange are easier to tell apart with red–green color blindness', kind: 'seg', opts: ['From theme', 'Blue & orange'], def: 'From theme' },
    ]],
  ],
  agents: [
    ['Providers', [
      { id: 'providers', title: 'Providers', hint: 'The agent CLIs found on your login PATH', kind: 'providers' },
    ]],
    ['New sessions', [
      { id: 'defaultagent', title: 'Default agent', hint: "Starts in a project that hasn't picked one, and in new agent tabs", kind: 'drop', opts: ['Claude Code', 'Codex', 'Cursor', 'Grok Build'], def: 'Claude Code' },
      { id: 'permission', title: 'Permission mode', hint: "Agent's settings uses the agent's own config. Full skips every approval", kind: 'seg', opts: ["Agent's settings", 'Ask', 'Edits', 'Auto', 'Full'], def: "Agent's settings" },
      { id: 'plan', title: 'Start in plan mode', hint: 'For providers that support plan mode', kind: 'switch', def: false },
      { id: 'chat', title: 'Start as a chat', hint: "Experimental. New chat in a tab's + menu always starts one", kind: 'switch', def: false },
      { id: 'runsin', title: 'Runs in', hint: "A new worktree keeps the agent's edits off your branch", kind: 'seg', opts: ['Local', 'Worktree'], def: 'Worktree' },
    ]],
    ['Tabs', [
      { id: 'exits', title: 'When an agent exits', kind: 'seg', opts: ['Return to shell', 'Close tab'], def: 'Return to shell' },
      { id: 'tabsopen', title: 'New tabs and splits open', kind: 'seg', opts: ['Shell', 'Default agent'], def: 'Shell' },
    ]],
    ['Composer', [
      { id: 'addchat', title: '“Add to chat”', hint: 'Paste only lets you edit before sending', kind: 'seg', opts: ['Send now', 'Paste only'], def: 'Send now' },
      { id: 'addchatto', title: 'Send “Add to chat” to', kind: 'drop', opts: ['Focused session', 'Most recent agent', 'Ask each time'], def: 'Focused session' },
    ]],
  ],
};
const PROVIDERS = [['claude', 'Claude Code', 'Found at /usr/local/bin/claude · 6 models', true], ['codex', 'Codex', 'Found at /opt/homebrew/bin/codex · 5 models', true], ['cursor', 'Cursor', 'Found at ~/.local/bin/cursor-agent · 4 models', true], ['grok', 'Grok Build', 'Not installed', false]];

const vals = {};
const allRows = Object.values(PAGES).flatMap((groups) => groups.flatMap(([, rows]) => rows));
const reset = (id) => allRows.filter((r) => PAGES[id]?.some(([, rs]) => rs.includes(r))).forEach((r) => { vals[r.id] = r.def; });
Object.keys(PAGES).forEach(reset);
const enabled = { claude: true, codex: true, cursor: false };

let section = 'hosts';
let query = '';
let menu = null;
let confirm = null;
let focus = null;

// Hosts

const STEPS = ['Check the host', 'Download pocketd', 'Verify the download', 'Unpack pocketd', 'Install pocketd', 'Set up the service', 'Start pocketd'];
let serial = 3;
const hosts = [
  { id: 'h1', name: 'devbox', link: 'online', version: '1.4.2', up: '3d' },
  { id: 'h2', name: 'win-build', where: 'dev@10.0.4.12:2222', link: 'stopped' },
];
const seedHosts = hosts.map((h) => ({ ...h }));
const installs = {};
// Bumped on every entry, so an install still ticking from an earlier visit doesn't touch the next one's host.
let visit = 0;
const form = { open: false, picked: null, address: '', port: '', user: '', password: '', keyFile: null, error: '' };
const SUGGESTIONS = ['staging', 'build-box', 'nas'];

const hostLine = (h) => {
  const link = {
    online: `Connected · version ${h.version} · up ${h.up}`,
    connecting: 'Connecting…',
    stopped: `Anywhere is stopped on ${h.name}. Trying again…`,
  }[h.link];
  return h.where ? `${h.where} · ${link}` : link;
};
const dotColor = { online: 'var(--success)', connecting: 'var(--waiting)', stopped: 'var(--waiting)' };

function startInstall(h) {
  installs[h.id] = { step: 0, outcome: null };
  const mine = visit;
  const tick = () => {
    const i = installs[h.id];
    if (!i || mine !== visit) return;
    if (i.step < STEPS.length) { i.step++; redraw(); setTimeout(tick, 650); return; }
    i.outcome = 'done';
    Object.assign(h, { link: 'online', version: '1.4.2', up: '0m' });
    redraw();
  };
  setTimeout(tick, 650);
}

function reconnect(h) {
  const before = h.link;
  h.link = 'connecting';
  redraw();
  setTimeout(() => { h.link = before; redraw(); }, 1100);
}

function connect() {
  const f = form;
  const address = f.address.trim();
  if (!address) { f.error = "Enter the host's address or name"; return; }
  const h = { id: `h${serial++}`, name: f.picked ?? address, where: !f.picked && f.user.trim() ? `${f.user.trim()}@${address}${f.port.trim() ? `:${f.port.trim()}` : ''}` : f.picked ? '' : '', link: 'connecting' };
  hosts.push(h);
  Object.assign(form, { open: false, picked: null, address: '', port: '', user: '', password: '', keyFile: null, error: '' });
  const mine = visit;
  setTimeout(() => { if (mine !== visit) return; h.link = 'stopped'; startInstall(h); redraw(); }, 900);
}

function redraw() { if (api().state.screen === 'hosts') api().draw(); }

const tileRow = (icon, name, line, dot, actions, panel) => `<div class="hrow">
  <span class="htile">${ic(icon, 14)}</span>
  <div class="htext"><b>${esc(name)}</b><span class="hline"><i style="background:${dot}"></i><span>${esc(line)}</span></span></div>${actions}</div>${panel ?? ''}`;

function panelOf(h) {
  const i = installs[h.id];
  if (!i) return '';
  const done = i.outcome === 'done';
  const rows = STEPS.map((s, n) => {
    const lead = n < i.step || done ? `<span class="ck">${ic('check', 13)}</span>` : n === i.step ? '<span class="sring"></span>' : '<span class="pdot"></span>';
    return `<div class="step${n > i.step && !done ? ' pending' : ''}"><span class="lead">${lead}</span>${s}</div>`;
  }).join('');
  return `<div class="inst"><div class="ih"><b>${done ? 'pocketd is installed' : 'Installing pocketd'}</b>${done ? '<button class="act" data-act="inst-done" data-v="' + h.id + '">Done</button>' : ''}</div>${rows}</div>`;
}

function hostRows() {
  const here = tileRow('laptop', 'This Mac', 'Connected · version 1.4.2 · up 3d', 'var(--success)', '<span class="here">Here</span>');
  const rows = hosts.map((h) => {
    const busy = h.link === 'connecting';
    const install = h.link === 'stopped' && !installs[h.id] ? `<button class="act" data-act="install" data-v="${h.id}">Install</button>` : '';
    const re = busy ? '' : `<button class="act" data-act="reconnect" data-v="${h.id}">Reconnect</button>`;
    const remove = `<button class="rm" data-act="remove" data-v="${h.id}">Remove</button>`;
    return tileRow('server', h.name, hostLine(h), dotColor[h.link], install + re + remove, panelOf(h));
  }).join('');
  return here + rows;
}

function addForm() {
  const f = form;
  if (!f.open) return `<div class="srow"><div class="stx"><b>Add a host</b><span>A Mac, Linux or Windows machine you can reach over SSH</span></div><button class="hbtn prim" data-act="addhost">${ic('plus', 14)}Add host</button></div>`;
  const chips = SUGGESTIONS.map((a) => `<button class="sug${f.picked === a ? ' on' : ''}" data-act="suggest" data-v="${a}">${a}</button>`).join('');
  const field = (label, name, value, type = 'text', cls = '') => `<label class="fld ${cls}"><span>${label}</span><input data-f="${name}" type="${type}" value="${esc(value)}" spellcheck="false" autocomplete="off"></label>`;
  const key = f.keyFile
    ? `<div class="keyl">Signs in with <code>${esc(f.keyFile)}</code><button class="hib" data-act="clearkey" aria-label="Clear">${ic('x', 12)}</button></div>`
    : `<div class="keyl mute">Signs in with your SSH keys and ~/.ssh/config. <button class="lnk" data-act="keyfile">Use a key file…</button></div>`;
  const fields = f.picked
    ? `<div class="two">${field('Host', 'address', f.address, 'text', 'grow')}${field('Password', 'password', f.password, 'password', 'grow')}</div><div class="keyl mute">From your ~/.ssh/config: its user, port, key and jump host are used as they are.</div>`
    : `<div class="two">${field('Address', 'address', f.address, 'text', 'grow')}${field('Port', 'port', f.port, 'text', 'port')}</div><div class="two">${field('Username', 'user', f.user, 'text', 'grow')}${field('Password', 'password', f.password, 'password', 'grow')}</div>${key}`;
  return `<div class="addf"><b>Add a host</b><div class="sugs"><span>Suggested</span>${chips}</div>${fields}${f.error ? `<div class="ferr">${esc(f.error)}</div>` : ''}
    <div class="ffoot"><span>Nothing is changed in your ~/.ssh</span><button class="hbtn scnd" data-act="canceladd">Cancel</button><button class="hbtn prim" data-act="connect">Connect</button></div></div>`;
}

// Other pages

const toggle = (on) => `<span class="tg${on ? ' on' : ''}"><i></i></span>`;

function control(r) {
  const v = vals[r.id];
  switch (r.kind) {
    case 'switch': return `<button class="swc" data-act="sw" data-v="${r.id}">${toggle(v)}</button>`;
    case 'seg': return `<div class="sseg">${r.opts.map((o) => `<button class="${o === v ? 'on' : ''}" data-act="sg" data-v="${r.id}|${esc(o)}">${esc(o)}</button>`).join('')}</div>`;
    case 'drop': return `<span class="rel"><button class="act wide" data-act="dd" data-v="${r.id}">${esc(v)}${ic('chevron-down', 12)}</button>${menu === r.id ? `<div class="spop">${r.opts.map((o) => `<button class="${o === v ? 'on' : ''}" data-act="dv" data-v="${r.id}|${esc(o)}">${esc(o)}${o === v ? ic('check', 13) : ''}</button>`).join('')}</div>` : ''}</span>`;
    case 'step': return `<div class="stepper"><button data-act="stp" data-v="${r.id}|-1" ${v <= r.min ? 'disabled' : ''}>−</button><span>${v}${r.unit}</span><button data-act="stp" data-v="${r.id}|1" ${v >= r.max ? 'disabled' : ''}>+</button></div>`;
    case 'slider': return `<div class="slide"><input type="range" min="40" max="100" value="${v}" data-f="${r.id}"><span>${v}%</span></div>`;
    case 'swatch': return `<div class="sw">${r.opts.map(([n, c]) => `<button class="${n === v ? 'on' : ''}" style="--c:${c}" data-act="sg" data-v="${r.id}|${n}" aria-label="${n}"></button>`).join('')}</div>`;
    case 'buttons': return `<div class="btns">${r.buttons.map((b) => `<button class="act">${b}</button>`).join('')}</div>`;
    default: return '';
  }
}

function providers() {
  return `<div class="provs">${PROVIDERS.map(([id, name, line, installed], i) => `<div class="prov${i ? ' top' : ''}${installed ? '' : ' off'}"><span class="htile${installed ? '' : ' faint'}"><span class="hag" style="display:flex;color:${id === 'claude' ? '#d97757' : 'var(--t1)'}">${ic(AGENTS[id][0], 15)}</span></span>
    <div class="htext"><b>${name}${i === 0 ? '<em>Default</em>' : ''}</b><span class="hl2">${line}</span></div>
    ${installed ? `<button class="swc" data-act="prov" data-v="${id}">${toggle(enabled[id] ?? true)}</button>` : ''}${ic('chevron-right', 13)}</div>`).join('')}</div>`;
}

function row(r) {
  if (r.kind === 'providers') return providers();
  return `<div class="srow"><div class="stx"><b>${esc(r.title)}</b>${r.hint ? `<span>${esc(r.hint)}</span>` : ''}</div><div class="sctl">${control(r)}</div></div>`;
}

const card = (rows) => `<div class="scard">${rows.map((r, i) => `<div class="${i ? "ssep" : ""}">${r}</div>`).join('')}</div>`;
const group = (title, rows) => `<div class="sgrp"><h3>${esc(title)}</h3>${card(rows)}</div>`;

function head(id) {
  const s = SECTIONS[id];
  return `<div class="shead"><span class="stile">${ic(s.icon, 20)}</span><div><h2>${s.label}</h2><p>${SUBTITLE[id]}</p></div></div>`;
}

function hostsPage() {
  return `${head('hosts')}${group('Hosts', [hostRows(), addForm()])}`;
}

function fullPage(id) {
  if (id === 'hosts') return hostsPage();
  if (PAGES[id]) {
    const groups = PAGES[id].map(([title, rows]) => group(title, rows.map(row))).join('');
    return `${head(id)}${groups}<div class="sfoot"><button class="resetb" data-act="reset" data-v="${id}">${ic('discard', 13)}Reset ${SECTIONS[id].label} to defaults…</button></div>`;
  }
  return `${head(id)}<div class="scard snote"><span>This page isn't part of the demo.</span></div>`;
}

// Search matches the rows of the pages that are filled in.
const words = () => query.toLowerCase().split(/\s+/).filter(Boolean);
const matches = (text) => words().every((w) => text.toLowerCase().includes(w));
function hits() {
  const out = {};
  for (const [id, groups] of Object.entries(PAGES)) {
    const rows = groups.flatMap(([, rs]) => rs).filter((r) => matches(`${r.title} ${r.hint ?? ''}`));
    if (rows.length) out[id] = rows;
  }
  if (matches('hosts add a host this mac ssh')) out.hosts = [{ hostsPage: true }];
  return out;
}

function nav() {
  const found = words().length ? hits() : null;
  const groups = GROUPS.map(([title, list]) => `<div class="sgroup"><h5>${title}</h5>${list.map(([id, label, icon]) => {
    const n = found?.[id]?.length ?? 0;
    const on = !found && section === id;
    return `<button class="sitem${on ? ' on' : ''}${found && !n ? ' dimm' : ''}" data-act="section" data-v="${id}">${ic(icon, 17)}<span>${label}</span>${n ? `<b>${n}</b>` : ''}</button>`;
  }).join('')}</div>`).join('');
  return `<div class="snav"><div class="sdrag"></div>
    <div class="sback"><button class="sitem" data-act="back">${ic('back', 17)}<span>Back</span></button></div>
    <div class="ssearch"><label>${ic('search', 14)}<input data-f="query" placeholder="Search" value="${esc(query)}" spellcheck="false">${query ? '' : '<kbd>⌘F</kbd>'}</label></div>
    <div class="sgroups">${groups}</div></div>`;
}

function page() {
  let title = SECTIONS[section].label;
  let body;
  if (words().length) {
    title = 'Search results';
    const found = hits();
    const ids = Object.keys(found);
    body = ids.length ? ids.map((id) => {
      const rows = found[id];
      return id === 'hosts' ? group(SECTIONS.hosts.label, [hostRows(), addForm()]) : group(SECTIONS[id].label, rows.map(row));
    }).join('') : '<div class="scard snote"><span>No settings match.</span></div>';
  } else {
    body = fullPage(section);
  }
  return `<div class="spage"><div class="sbar"><b>${title}</b></div><div class="sbody"><div class="scol">${body}</div></div></div>`;
}

function dialog() {
  if (!confirm) return '';
  const h = hosts.find((x) => x.id === confirm);
  return `<div class="sscrim"><div class="sdlg"><b>Remove ${esc(h.name)}?</b>
    <ul><li>It disconnects now</li><li>Projects and files on ${esc(h.name)} aren't touched</li><li>Uninstall and remove also stops Anywhere's service there</li></ul>
    <div class="dbtn"><button class="hbtn danger" data-act="rm-go" data-v="${h.id}">Remove from this Mac</button><button class="hbtn danger" data-act="rm-go" data-v="${h.id}">Uninstall and remove</button><button class="hbtn scnd" data-act="rm-no">Cancel</button></div></div></div>`;
}

function render() {
  return `<div class="stw">${nav()}${page()}${menu ? '<div class="sscrim clear" data-act="closemenu"></div>' : ''}${dialog()}</div>`;
}

function mount(root) {
  if (focus) {
    const el = root.querySelector(`.addf [data-f="${focus}"], .snav [data-f="${focus}"]`);
    if (el) { el.focus(); el.setSelectionRange?.(el.value.length, el.value.length); }
  }
}

function act(a, v) {
  const A = api();
  if (a !== 'dd') menu = menu && ['dv'].includes(a) ? null : (a === 'closemenu' ? null : menu);
  if (!['suggest', 'addhost'].includes(a)) focus = a === 'section' || a === 'back' ? null : focus;
  switch (a) {
    case 'back': return A.go('sessions');
    case 'section': section = v; query = ''; menu = null; focus = null; return A.draw();
    case 'closemenu': menu = null; return A.draw();
    case 'addhost': Object.assign(form, { open: true, error: '' }); focus = 'address'; return A.draw();
    case 'canceladd': form.open = false; return A.draw();
    case 'suggest': form.picked = form.picked === v ? null : v; form.address = form.picked ?? ''; focus = form.picked ? 'password' : 'address'; return A.draw();
    case 'keyfile': form.keyFile = '~/.ssh/id_ed25519'; return A.draw();
    case 'clearkey': form.keyFile = null; return A.draw();
    case 'connect': connect(); focus = null; return A.draw();
    case 'install': startInstall(hosts.find((h) => h.id === v)); return A.draw();
    case 'inst-done': delete installs[v]; return A.draw();
    case 'reconnect': reconnect(hosts.find((h) => h.id === v)); return;
    case 'remove': confirm = v; return A.draw();
    case 'rm-no': confirm = null; return A.draw();
    case 'rm-go': { const i = hosts.findIndex((h) => h.id === v); if (i >= 0) hosts.splice(i, 1); delete installs[v]; confirm = null; return A.draw(); }
    case 'sw': vals[v] = !vals[v]; return A.draw();
    case 'sg': { const [id, o] = v.split('|'); vals[id] = o; return A.draw(); }
    case 'dd': menu = menu === v ? null : v; return A.draw();
    case 'dv': { const [id, o] = v.split('|'); vals[id] = o; menu = null; return A.draw(); }
    case 'stp': { const [id, d] = v.split('|'); vals[id] += +d; return A.draw(); }
    case 'prov': enabled[v] = !(enabled[v] ?? true); return A.draw();
    case 'reset': reset(v); return A.draw();
  }
}

D.register('hosts', {
  full: true,
  tour: 'hosts',
  render,
  mount,
  act,
  enter() {
    visit++;
    hosts.splice(0, hosts.length, ...seedHosts.map((h) => ({ ...h })));
    Object.keys(installs).forEach((id) => delete installs[id]);
    serial = 3;
    Object.assign(form, { open: false, picked: null, address: '', port: '', user: '', password: '', keyFile: null, error: '' });
    section = 'hosts'; query = ''; menu = null; confirm = null; focus = null;
  },
  input(e) {
    const f = e.target.dataset.f;
    if (!f) return;
    if (f === 'query') { query = e.target.value; focus = 'query'; return api().draw(); }
    if (f in form) { form[f] = e.target.value; if (form.error) { form.error = ''; e.target.closest('.addf')?.querySelector('.ferr')?.remove(); } return; }
    vals[f] = +e.target.value;
    e.target.nextElementSibling.textContent = `${e.target.value}%`;
  },
  key(e) {
    if (e.key === 'Enter' && e.target.closest('.addf')) { e.preventDefault(); act('connect'); }
    else if (e.key === 'Escape') {
      if (confirm) { confirm = null; api().draw(); }
      else if (form.open) { form.open = false; api().draw(); }
      else if (query) { query = ''; api().draw(); }
      else api().go('sessions');
    }
  },
});

AnywhereTour.define('hosts', [
  ['wait', 1800],
  ['click', '[data-act="addhost"]'],
  ['wait', 500],
  ['type', '.addf [data-f="address"]', 'buildbox.local'],
  ['type', '.addf [data-f="user"]', 'dev'],
  ['wait', 500],
  ['click', '[data-act="connect"]'],
  ['wait', 7300],
  ['click', '[data-act="inst-done"]'],
  ['wait', 1800],
]);
})();
