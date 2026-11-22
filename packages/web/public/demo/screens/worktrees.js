// New worktree: the new-session page with Worktree picked, then the progress page while pocketd makes the
// worktree, then the session in its new sidebar row. Mirrors modals/new_session.rs and creating.rs.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, AGENTS } = AW;

const SAMPLE = 'Add notification preferences to the settings screen';
const ORIGIN = ['feat/invoice-export', 'fix/login-redirect', 'chore/bump-deps'];
const AGO = ['2h ago', '1d ago', '3d ago', '5d ago', '1w ago'];

const STEPS = [
  ['Preparing', 350, ''],
  ['Checking branch name', 450, ''],
  ['Fetching latest changes', 1000, ''],
  ['Creating git worktree', 800, ''],
  ['Copying configuration', 450, 'Copied .env.local'],
  ['Setting up', 1600, 'Running pnpm install'],
  ['Starting agent', 700, ''],
];

const form = { project: null, target: 'worktree', base: 'main', source: { kind: 'new' }, agent: 'claude', prompt: SAMPLE, search: '', prText: '' };
let menu = null;
let make = null;

const project = (api) => form.project ?? api.sel.p;
const slug = (t) => t.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').split('-').slice(0, 4).join('-');
const sentence = (t) => { const s = t.trim().replace(/\.$/, ''); return s[0].toUpperCase() + s.slice(1); };
const parsePr = (s) => {
  const t = s.trim();
  const m = t.match(/^https:\/\/[^/]+\/[^/]+\/[^/]+\/pull\/(\d+)/) ?? t.match(/^#?(\d+)$/);
  return m && +m[1] > 0 ? +m[1] : null;
};

const branches = (api) => {
  const p = project(api);
  const names = ['main', ...p.rows.filter((r) => !r.local).map((r) => r.label), 'release/2.3'];
  return names.map((n, i) => [n, i ? AGO[(i - 1) % AGO.length] : '']);
};

// ---- Page --------------------------------------------------------------------------------------------------

const check = (on) => (on ? ic('check', 14) : '');
const pop = (body, cls = '') => `<div class="wt-pop ${cls}">${body}</div>`;
const mrow = (act, v, lead, label, hint, on) => `<button class="wt-row" data-act="${act}" data-v="${esc(String(v))}"><span class="lead">${lead}</span><span class="tx"><b>${label}</b>${hint ? `<small>${hint}</small>` : ''}</span><span class="ck">${check(on)}</span></button>`;

function branchRows(api, q) {
  const all = branches(api);
  const text = q.trim().toLowerCase();
  const local = all.filter(([n]) => !text || n.toLowerCase().includes(text));
  const origin = text ? ORIGIN.filter((n) => n.includes(text)) : [];
  let html = '';
  let head = '';
  local.forEach(([n, ago], i) => {
    const title = text ? 'Local' : i === 0 ? 'Default' : 'Recent';
    if (title !== head) { html += `<div class="wt-ph">${title}</div>`; head = title; }
    html += `<button class="wt-pick" data-act="base" data-v="${esc(n)}"><span class="bi">${ic('branch', 13)}</span><b>${esc(n)}</b><i>${ago}</i><span class="ck">${check(form.source.kind === 'new' && form.base === n)}</span></button>`;
  });
  if (origin.length) html += '<div class="wt-ph">On origin</div>';
  for (const n of origin) html += `<div class="wt-pick o"><span class="bi">${ic('branch', 13)}</span><b>origin/${esc(n)}</b><span class="ck">${check(form.source.kind === 'branch' && form.source.name === n)}</span><button class="open" data-act="open-branch" data-v="${esc(n)}">Open worktree</button></div>`;
  return html || '<div class="wt-none">No branches found</div>';
}

function menus(api) {
  const p = project(api);
  if (menu === 'project') return pop(api.PROJECTS.map((q) => `<button class="wt-row pj" data-act="project" data-v="${esc(q.name)}"><span class="lead"><span class="mini">${api.letter(q.name)}</span></span><span class="tx"><b>${esc(q.name)}</b></span><span class="ck">${check(q === p)}</span></button>`).join(''), 'w260');
  if (menu === 'checkout') return pop(mrow('target', 'worktree', ic('worktree', 14), 'Worktree', 'Its own branch and copy of the files', form.target === 'worktree') + mrow('target', 'local', ic('laptop', 14), 'Local', "A new place over the project's checkout: files and branch are shared", form.target === 'local'), 'w300');
  if (menu === 'branch') return pop(`<div class="wt-search">${ic('search', 15)}<input data-bsearch placeholder="Search branches" value="${esc(form.search)}" aria-label="Search branches"></div><div class="wt-blist" data-blist>${branchRows(api, form.search)}</div>`, 'w340');
  if (menu === 'agent') return pop(Object.entries(AGENTS).slice(0, 6).map(([k, [icon, name]]) => `<button class="wt-row" data-act="agent" data-v="${k}"><span class="lead ag ${k}">${ic(icon, 15)}</span><span class="tx"><b>${name}</b></span><span class="ck">${check(form.agent === k)}</span></button>`).join(''), 'w260 up');
  if (menu === 'pr') {
    const bad = form.prText.trim() && !parsePr(form.prText);
    return pop(`<div class="wt-search">${ic('pull-request', 15)}<input data-pr placeholder="Pull request" value="${esc(form.prText)}" aria-label="Pull request"></div><div class="wt-note${bad ? ' bad' : ''}" data-prnote>${bad ? 'Use 123, #123 or a PR URL' : 'Press ↵ to link the pull request'}</div>`, 'w300 right up');
  }
  return '';
}

function page(api) {
  const p = project(api);
  const [icon, agentName] = AGENTS[form.agent];
  const pr = form.source.kind === 'pr' ? form.source.n : null;
  const empty = !form.prompt.trim();
  const rel = (id, btn) => `<span class="wt-rel">${btn}${menu === id ? menus(api) : ''}</span>`;
  const branchChip = pr
    ? `<span class="wt-based">${ic('pull-request', 14)}based off PR #${pr}</span>`
    : form.target === 'worktree'
      ? rel('branch', `<button class="chip ghost mono" data-menu data-act="menu" data-v="branch">${ic('branch', 14)}${esc(form.source.kind === 'branch' ? form.source.name : form.base)}${ic('chevron-down', 12)}</button>`)
      : '';
  const model = form.agent === 'claude' ? '<b>Claude Code</b>' : `<b>${agentName}</b>`;
  const pill = pr ? `<span class="wt-pill">${ic('pull-request', 13)}#${pr}<button data-act="unlink" aria-label="Unlink">${ic('x', 10)}</button></span>` : '';
  return `<div class="pane dots wt-new"><div class="bar bare"><div class="crumbs">${esc(p.name)}<i>/</i><b>New session</b></div></div>
    <div class="home"><h2>What should we build next?</h2>
      <div class="composer${empty ? ' empty' : ''}">
        <textarea rows="2" placeholder="Ask, build, / for commands, @ for files" aria-label="Message" data-prompt>${esc(form.prompt)}</textarea>
        <div class="foot">${rel('agent', `<button class="chip" data-menu data-act="menu" data-v="agent"><span class="ag ${form.agent}">${ic(icon, 15)}</span>${model}${ic('chevron-down', 12)}</button>`)}
          <button class="chip">${ic('shield', 15)}Agent's settings${ic('chevron-down', 12)}</button>
          <span class="sp"></span>${pill}${pr ? '' : rel('pr', `<button class="ib wt-prb" data-menu data-act="menu" data-v="pr" aria-label="Link a pull request">${ic('pull-request', 17)}</button>`)}
          <button class="ib" aria-label="Attach">${ic('plus', 17)}</button><button class="send" data-act="send" aria-label="Start">${ic('arrow-up', 16)}</button></div></div>
      <div class="where-row">${rel('project', `<button class="chip ghost" data-menu data-act="menu" data-v="project"><span class="mini">${api.letter(p.name)}</span>${esc(p.name)}${ic('chevron-down', 12)}</button>`)}
        <span class="wt-dot">·</span>${rel('checkout', `<button class="chip ghost" data-menu data-act="menu" data-v="checkout">${ic(form.target === 'worktree' ? 'worktree' : 'laptop', 14)}${form.target === 'worktree' ? 'Worktree' : 'Local'}${ic('chevron-down', 12)}</button>`)}${branchChip}</div>
    </div></div>`;
}

// ---- Progress ----------------------------------------------------------------------------------------------

const clock = (ms) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')}`;
const took = (ms) => `${(ms / 1000).toFixed(1)}s`;

function progress(api) {
  const m = make;
  const now = m.done ? m.end : Date.now();
  const e = Math.max(0, Date.now() - m.start);
  const rise = (d) => (e < 700 ? ` in" style="--d:${d}ms;--e:${e}ms` : '');
  const rows = STEPS.map(([label, , note], i) => {
    const mark = i < m.at ? 'done' : i === m.at && !m.done ? 'active' : m.done ? 'done' : 'pending';
    const lead = mark === 'done' ? `<span class="ok">${ic('check', 14)}</span>` : mark === 'active' ? api.spinner(14) : '<i class="pd"></i>';
    const shown = mark !== 'pending' && note;
    return `<div class="wt-step ${mark}${rise(120 + 30 * i)}">${`<span class="ld">${lead}</span>`}<span class="tx"><b>${label}</b>${shown ? `<small>${esc(note)}</small>` : ''}</span>${mark === 'done' ? `<code>${took(STEPS[i][1] * 0.82)}</code>` : ''}</div>`;
  }).join('');
  const from = m.source.kind === 'pr' ? `<span>from</span><code>PR #${m.source.n}</code>` : `<span>from</span><code>${esc(m.base)}</code>`;
  return `<div class="pane wt-make"><div class="bar bare"><div class="crumbs">${esc(m.row.p.name)}<i>/</i><b>${esc(m.row.label)}</b></div></div>
    <div class="wt-center"><div class="wt-page${m.leaving ? ' leave' : ''}">
      <div class="wt-title${rise(0)}"><b>Creating worktree</b><code>${clock(now - m.start)}</code></div>
      <div class="wt-branch${rise(40)}">${ic('branch', 13)}<code>${esc(m.row.label)}</code>${from}</div>
      <div class="wt-steps${rise(80)}">${rows}</div>
      <div class="wt-foot"><button class="wt-link" data-act="noop">View output</button></div></div></div></div>`;
}

const live = (api) => api.state.screen === 'worktrees';

function start(api) {
  if (!form.prompt.trim()) return;
  const p = project(api);
  const text = form.prompt.trim();
  const src = form.source;
  const label = src.kind === 'branch' ? src.name : src.kind === 'pr' ? `pr-${src.n}` : `feat/${slug(text) || 'new-session'}`;
  const row = { id: Date.now(), p, label, title: sentence(text), agent: form.agent, chat: true, status: 'working', add: 0, del: 0, log: [['you', text]] };
  p.rows.unshift(row);
  p.open = true;
  api.state.sel = row;
  api.changed();
  form.prompt = '';
  if (form.target === 'local') return land(api, row, text);
  make = { row, base: form.base, source: src, at: 0, start: Date.now(), done: false, leaving: false, end: 0 };
  api.draw();
  const tick = setInterval(() => { if (make && !make.done && live(api)) api.draw(); else if (!make || make.done) clearInterval(tick); }, 1000);
  const next = () => {
    if (!make || make.row !== row) return;
    if (make.at >= STEPS.length - 1) {
      make.done = true;
      make.end = Date.now();
      if (live(api)) api.draw();
      return setTimeout(() => {
        if (!make || make.row !== row) return;
        make.leaving = true;
        if (live(api)) api.draw();
        setTimeout(() => { make = null; land(api, row, text); }, 260);
      }, 450);
    }
    setTimeout(() => { make.at++; if (live(api)) api.draw(); next(); }, STEPS[make.at][1]);
  };
  next();
}

// The session starts in its new row: it works through a short script, then settles.
function land(api, row, text) {
  form.source = { kind: 'new' };
  menu = null;
  if (live(api)) api.select(row);
  const script = [
    ['say', "I'll look around the repo first."],
    ['tool', 'Search', 'settings screen'],
    ['tool', 'Read', 'src/settings.ts'],
    ['tool', 'Edit', 'src/settings.ts'],
    ['tool', 'Bash', 'pnpm test settings'],
    ['say', 'Done. **48 tests pass.** The change is ready to review in Changes.'],
  ];
  script.forEach((line, i) => setTimeout(() => {
    row.log.push(line);
    if (i === script.length - 1) { row.status = 'done'; row.worked = '12s'; row.add = 36; row.del = 4; }
    api.changed();
  }, 1100 * (i + 1)));
}

// ---- Screen ------------------------------------------------------------------------------------------------

D.register('worktrees', {
  tour: 'worktrees',
  enter() { menu = null; make = null; form.search = ''; form.prText = ''; },
  render(api) {
    return (make ? progress(api) : page(api)) + api.rail(null, {});
  },
  mount(root) {
    if (menu === 'branch' || menu === 'pr') root.querySelector('.wt-pop input')?.focus();
    if (!root._wt) {
      root._wt = true;
      root.addEventListener('click', (e) => {
        if (!live(D.api) || !menu || make) return;
        if (e.target.closest('.wt-pop, [data-menu]')) return;
        menu = null;
        D.api.draw();
      });
    }
  },
  act(act, v, el, api) {
    if (act !== 'menu' && !el.matches('[data-menu]')) menu = null;
    switch (act) {
      case 'menu': menu = menu === v ? null : v; return api.draw();
      case 'project': form.project = api.PROJECTS.find((p) => p.name === v); form.base = 'main'; form.source = { kind: 'new' }; return api.draw();
      case 'target': form.target = v; return api.draw();
      case 'base': form.base = v; form.source = { kind: 'new' }; return api.draw();
      case 'open-branch': form.source = { kind: 'branch', name: v }; return api.draw();
      case 'agent': form.agent = v; return api.draw();
      case 'unlink': form.source = { kind: 'new' }; return api.draw();
      case 'send': return start(api);
      case 'rail':
        if (v === 'changes' || v === 'prs' || v === 'explorer') { api.state.sel = api.sel; return api.go(v === 'explorer' ? 'sessions' : 'review'); }
        return api.go('inbox');
    }
  },
  input(e, api) {
    const t = e.target;
    if (t.matches('[data-prompt]')) {
      form.prompt = t.value;
      api.$('.composer')?.classList.toggle('empty', !form.prompt.trim());
    } else if (t.matches('[data-bsearch]')) {
      form.search = t.value;
      const list = api.$('[data-blist]');
      if (list) list.innerHTML = branchRows(api, form.search);
    } else if (t.matches('[data-pr]')) {
      form.prText = t.value;
      const bad = form.prText.trim() && !parsePr(form.prText);
      const note = api.$('[data-prnote]');
      if (note) { note.classList.toggle('bad', !!bad); note.textContent = bad ? 'Use 123, #123 or a PR URL' : 'Press ↵ to link the pull request'; }
    }
  },
  key(e, api) {
    if (make) return;
    if (e.key === 'Escape' && menu) { menu = null; return api.draw(); }
    if (e.target.matches('[data-prompt]') && e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); start(api); }
    if (e.target.matches('[data-pr]') && e.key === 'Enter') {
      e.preventDefault();
      const n = parsePr(form.prText);
      if (n) { form.source = { kind: 'pr', n }; form.prText = ''; menu = null; api.draw(); }
    }
  },
});
})();
