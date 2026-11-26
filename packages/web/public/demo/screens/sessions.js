// Sessions: the new-session page, a chat, a terminal running an agent, and the Files panel from the rail.

(() => {
const D = window.AnywhereDesktop;
const { ic, esc, fileIcon, folder, sized, AGENTS } = AW;

const TREE = [
  { name: 'components', kids: [['Button.tsx'], ['Sheet.tsx', 'M']] },
  { name: 'hooks', kids: [['useTheme.ts', 'A']] },
  { name: 'src', kids: [['app.tsx'], ['settings.ts', 'M'], ['store.ts']] },
  ['package.json'],
  ['README.md'],
];
const opened = new Set(['src', 'hooks']);
let panel = null;
let draft = '';
let agent = 'claude';
let home = false;

// Just enough markdown for the demo's replies: **bold** and `code`.
const md = (s) => esc(s).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>').replace(/`(.+?)`/g, '<code>$1</code>');

// Replies stream in as the app's do (chat/reveal.rs): text is let out at 320 bytes a second, each chunk within
// 300–1400ms, a word at a time, and each word fades in over 240ms. A fenced block shows whole once reached, and a
// mermaid diagram is drawn only then.
const PACE = 0.32;
const FADE = 240;
const streamFor = (len) => ({ start: performance.now(), took: Math.min(Math.max(len / PACE, 300), 1400) });
const reached = (st, len, now) => Math.min(len, ((now - st.start) / st.took) * len);

// A reply's markdown as blocks, each with where it starts in the source.
function blocks(src) {
  const out = [];
  const lines = src.split('\n');
  let at = 0;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const start = at;
    at += line.length + 1;
    const fence = line.match(/^```(\w*)/);
    if (fence) {
      const body = [];
      while (++i < lines.length && !lines[i].startsWith('```')) { body.push(lines[i]); at += lines[i].length + 1; }
      at += (lines[i]?.length ?? 0) + 1;
      out.push({ kind: fence[1] === 'mermaid' ? 'mermaid' : 'code', lang: fence[1], text: body.join('\n'), start, end: at - 1 });
    } else if (/^#{1,3} /.test(line)) out.push({ kind: 'h', text: line.replace(/^#+ /, ''), start: start + line.indexOf(' ') + 1 });
    else if (/^(- |\d+\. )/.test(line)) {
      const ordered = /^\d/.test(line);
      const last = out.at(-1);
      const item = { text: line.replace(/^(- |\d+\. )/, ''), start: start + line.match(/^(- |\d+\. )/)[0].length };
      if (last?.kind === 'list' && last.ordered === ordered) last.items.push(item);
      else out.push({ kind: 'list', ordered, items: [item], start });
    } else if (line.startsWith('|')) {
      const cells = line.split('|').slice(1, -1).map((c) => c.trim());
      const last = out.at(-1);
      if (/^\|[\s|:-]+\|$/.test(line)) continue;
      if (last?.kind === 'table') last.rows.push({ cells, start });
      else out.push({ kind: 'table', rows: [{ cells, start }], start });
    } else if (line.trim()) out.push({ kind: 'p', text: line, start });
  }
  return out;
}

// Inline markdown for `text` starting at `base` in the source. While streaming, `op` gives a word's opacity by its
// offset, or null once the reveal hasn't reached it.
function inline(text, base, op) {
  if (!op) return md(text);
  let html = '';
  for (const m of text.matchAll(/\*\*(.+?)\*\*|`([^`]+)`|(\s+)|([^\s`*]+|\*)/g)) {
    const o = op(base + m.index);
    if (o == null) break;
    const style = o < 1 ? ` style="opacity:${o.toFixed(2)}"` : '';
    if (m[1] != null) html += `<b${style}>${esc(m[1])}</b>`;
    else if (m[2] != null) html += `<code${style}>${esc(m[2])}</code>`;
    else if (m[3] != null) html += m[3];
    else html += `<span${style}>${esc(m[4])}</span>`;
  }
  return html;
}

function richText(src, st, now = performance.now()) {
  const len = src.length;
  let cut = st ? reached(st, len, now) : len;
  // Whole words only, as the app cuts.
  if (st && cut < len) { const sp = src.slice(cut).search(/\s/); cut = sp < 0 ? len : cut + sp; }
  const rate = st ? len / st.took : 1;
  const op = st ? (o) => (o >= cut ? null : Math.min(1, (now - st.start - o / rate) / FADE)) : null;
  const fade = (o) => { const v = op?.(o); return v == null || v >= 1 ? '' : ` style="opacity:${Math.max(v, 0).toFixed(2)}"`; };
  let html = '';
  for (const b of blocks(src)) {
    if (st && b.start >= cut) break;
    if (b.kind === 'p') html += `<p>${inline(b.text, b.start, op)}</p>`;
    else if (b.kind === 'h') html += `<h3>${inline(b.text, b.start, op)}</h3>`;
    else if (b.kind === 'list') {
      const tag = b.ordered ? 'ol' : 'ul';
      html += `<${tag}>${b.items.filter((it) => !st || it.start < cut).map((it) => `<li>${inline(it.text, it.start, op)}</li>`).join('')}</${tag}>`;
    } else if (b.kind === 'table') {
      const [head, ...rows] = b.rows.filter((r) => !st || r.start < cut);
      html += `<table${fade(b.start)}><thead><tr>${head.cells.map((c) => `<th>${md(c)}</th>`).join('')}</tr></thead><tbody>${rows.map((r) => `<tr${fade(r.start)}>${r.cells.map((c) => `<td>${md(c)}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
    } else if (b.kind === 'code') html += `<div class="code"${fade(b.start)}><div class="code-h">${esc(b.lang || 'text')}${ic('copy', 13)}</div><pre>${esc(b.text)}</pre></div>`;
    else html += `<div class="mermaid"${fade(b.start)}>${mermaid(b.text)}</div>`;
  }
  return html;
}

// Enough of mermaid's flowchart for the demo: `A[Label] --> B[Label]`, with an optional `-->|label|`, laid out
// left to right (LR) or top down (TD) in ranks by longest path.
function mermaid(src) {
  const [head, ...lines] = src.trim().split('\n').map((l) => l.trim());
  const across = /\b(LR|RL)\b/.test(head);
  const nodes = new Map();
  const edges = [];
  const node = (t) => {
    const m = t.match(/^(\w+)\s*(?:\[\(?(.+?)\)?\]|\((.+?)\)|\{(.+?)\})?$/);
    if (!m) return null;
    if (!nodes.has(m[1])) nodes.set(m[1], { id: m[1], label: m[1], rank: 0, round: !!m[3], diamond: !!m[4] });
    const n = nodes.get(m[1]);
    if (m[2] || m[3] || m[4]) n.label = m[2] || m[3] || m[4];
    return n;
  };
  for (const l of lines) {
    const m = l.match(/^(.+?)\s*-->\s*(?:\|(.+?)\|\s*)?(.+)$/);
    if (!m) continue;
    const a = node(m[1]); const b = node(m[3]);
    if (a && b) edges.push({ a, b, label: m[2] });
  }
  for (let k = 0; k < nodes.size; k++) edges.forEach((e) => { e.b.rank = Math.max(e.b.rank, e.a.rank + 1); });
  const ranks = [];
  nodes.forEach((n) => (ranks[n.rank] ??= []).push(n));
  const W = (n) => Math.max(86, n.label.length * 7.4 + 28);
  const H = 38;
  const gapMain = across ? 88 : 46;
  const gapCross = across ? 22 : 30;
  let pos = 0;
  ranks.forEach((col) => {
    const size = across ? Math.max(...col.map(W)) : H;
    col.forEach((n, i) => {
      n.w = W(n);
      if (across) { n.x = pos + (size - n.w) / 2; n.y = i * (H + gapCross); }
      else { n.y = pos; n.x = i * 0; }
    });
    if (!across) {
      const total = col.reduce((t, n) => t + n.w, 0) + gapCross * (col.length - 1);
      let x = -total / 2;
      col.forEach((n) => { n.x = x; x += n.w + gapCross; });
    }
    pos += size + gapMain;
  });
  if (across) {
    const tall = Math.max(...ranks.map((c) => c.length * (H + gapCross) - gapCross));
    ranks.forEach((c) => { const off = (tall - (c.length * (H + gapCross) - gapCross)) / 2; c.forEach((n) => { n.y += off; }); });
  }
  const all = [...nodes.values()];
  const minX = Math.min(...all.map((n) => n.x)) - 8;
  const maxX = Math.max(...all.map((n) => n.x + n.w)) + 8;
  const maxY = Math.max(...all.map((n) => n.y + H)) + 8;
  const edge = ({ a, b, label }) => {
    const [x1, y1, x2, y2] = across ? [a.x + a.w, a.y + H / 2, b.x, b.y + H / 2] : [a.x + a.w / 2, a.y + H, b.x + b.w / 2, b.y];
    const mid = across ? `C${(x1 + x2) / 2},${y1} ${(x1 + x2) / 2},${y2}` : `C${x1},${(y1 + y2) / 2} ${x2},${(y1 + y2) / 2}`;
    const tag = label ? `<text class="el" x="${(x1 + x2) / 2}" y="${(y1 + y2) / 2 - 6}">${esc(label)}</text>` : '';
    return `<path d="M${x1},${y1} ${mid} ${x2},${y2}" marker-end="url(#mm-arrow)"/>${tag}`;
  };
  const box = (n) => `<g class="n"><rect x="${n.x}" y="${n.y}" width="${n.w}" height="${H}" rx="${n.round ? H / 2 : n.diamond ? 4 : 8}"/><text x="${n.x + n.w / 2}" y="${n.y + H / 2 + 4.5}">${esc(n.label)}</text></g>`;
  return `<svg viewBox="${minX} -8 ${maxX - minX} ${maxY + 8}" width="${maxX - minX}" style="max-width:100%"><defs><marker id="mm-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L8,4 L0,8 z"/></marker></defs><g class="e">${edges.map(edge).join('')}</g>${all.map(box).join('')}</svg>`;
}

// Replies the demo gives to whatever the visitor sends, in turn, so each shows a different kind of answer.
const REPLIES = [
  "Here's how a request moves through the upload route now:\n\n```mermaid\nflowchart LR\n  A[Client] --> B[Rate limiter]\n  B -->|under limit| C[Upload handler]\n  B -->|over limit| D(429 Retry-After)\n  C --> E[Object store]\n```\n\nThe limiter keys on the **user id**, so one noisy user can't starve the rest.",
  "### Plan\n\n1. Read the checkout spec and find the fixed wait\n2. Wait for the cart total with `waitFor` instead\n3. Run it 20 times to prove it holds\n\n| Run | Passed | Failed |\n|---|---|---|\n| Before | 16 | 4 |\n| After | 20 | 0 |\n\nNo more flakes in 20 runs.",
  "The fix is two lines in `src/downloads/queue.ts`:\n\n```ts\nqueue.delete(task.id);\ntask.free();\n```\n\nThe task now leaves the queue **before** it's freed, so a resume can't reach it.",
  "Done. Here's what changed:\n\n- Added a `theme` setting with **Light**, **Dark** and **System**\n- `useTheme` reads it and sets `data-theme` on the root\n- The choice is saved in `localStorage`\n\nAll **48 tests pass**, and the diff is ready in Changes.",
];
let replies = 0;

const TOOL_VERB = { Read: 'Read', Edit: 'Edited', Write: 'Wrote', Bash: 'Ran', Search: 'Searched' };
const toolIcon = { Read: 'file', Edit: 'pencil', Write: 'file-plus', Bash: 'terminal', Search: 'search' };

function chatTurns(r) {
  const out = [];
  let reply = null;
  for (const [i, [kind, a, b]] of r.log.entries()) {
    if (kind === 'you') { reply = null; out.push(`<div class="you"><div class="bubble">${esc(a)}</div><div class="meta">${ic('copy', 14)}<span>14:20</span></div></div>`); continue; }
    if (!reply) { reply = []; out.push(reply); }
    if (kind === 'say') reply.push(`<div class="md"${r.stream?.at === i ? ' data-stream' : ''}>${richText(a, r.stream?.at === i ? r.stream : null)}</div>`);
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
    return `<div class="reply">${head}${t.join('')}${last && r.status !== 'working' && r.status !== 'needs' ? `<div class="acts">${ic('copy', 15)}${ic('reload', 15)}<span>· ${r.worked ?? '48s'} · 14:21</span></div>` : ''}</div>`;
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

const composer = (r, placeholder = 'Ask, build, / for commands, @ for files') => {
  const [icon, name] = AGENTS[r ? r.agent : agent];
  const model = r ? (r.agent === 'claude' ? '<b>Opus</b><span class="soft">High</span>' : r.agent === 'codex' ? '<b>GPT-5.5</b><span class="soft">Medium</span>' : `<b>${name}</b>`) : `<b>${name}</b>`;
  return `<div class="composer${draft.trim() ? '' : ' empty'}">
    <textarea rows="2" placeholder="${placeholder}" aria-label="Message">${esc(draft)}</textarea>
    <div class="foot">
      <button class="chip" data-act="agent"><span class="ag ${r ? r.agent : agent}">${ic(icon, 15)}</span>${model}${ic('chevron-down', 12)}</button>
      ${r ? '' : `<button class="chip">${ic('shield', 15)}Agent's settings${ic('chevron-down', 12)}</button>`}
      <span class="sp"></span>
      <button class="ib" aria-label="Attach">${ic('plus', 17)}</button>
      <button class="send" data-act="send" aria-label="Send">${ic('arrow-up', 16)}</button>
    </div>
  </div>`;
};

function chatView(r) {
  return `<div class="scroll chat"><div class="col">${chatTurns(r)}${r.status === 'needs' ? permission(r) : ''}</div></div>
    <div class="dock">${composer(r)}<div class="where">${ic(r.local ? 'laptop' : 'worktree', 13)}<span>${esc(r.local ? 'Local' : r.label)}</span></div></div>`;
}

// A terminal agent draws its own UI; this one follows Codex's TUI.
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
    <button data-act="ans" data-v="no">  3. No, tell ${AGENTS[r.agent][1]} what to do differently</button></div><div class="l o">⏎ confirm · esc cancel</div>`
  : r.status === 'working' ? '<div class="l w"><i class="spin"></i> Working <span class="o">(esc to interrupt)</span></div>'
  : '<div class="l prompt">› <i class="caret"></i></div>'}</div></div>`;

function newSession(api) {
  const p = api.sel.p;
  return `<div class="pane dots">
    <div class="bar bare"><div class="crumbs">${esc(p.name)}<i>/</i><b>New session</b></div></div>
    <div class="home">
      <h2>What should we build next?</h2>
      ${composer(null)}
      <div class="where-row">
        <button class="chip ghost"><span class="mini">${api.letter(p.name)}</span>${esc(p.name)}${ic('chevron-down', 12)}</button>
        <button class="chip ghost">${ic('worktree', 14)}Worktree${ic('chevron-down', 12)}</button>
        <button class="chip ghost mono">${ic('branch', 14)}main${ic('chevron-down', 12)}</button>
      </div>
    </div>
  </div>`;
}

const dir = ({ name, kids }) => {
  const open = opened.has(name);
  return `<button class="tr" data-act="dir" data-v="${name}"><span class="cv">${ic(open ? 'chevron-down' : 'chevron-right', 12)}</span>${sized(folder(name, open), 16)}<span class="n">${name}</span></button>
    ${open ? kids.map((k) => file(k, 1)).join('') : ''}`;
};
const file = ([name, git], depth = 0) =>
  `<div class="tr ${git ?? ''}" style="--d:${depth}"><span class="cv"></span>${fileIcon(name)}<span class="n">${name}</span>${git ? `<b class="git">${git}</b>` : ''}</div>`;

const files = (api) => `<div class="panel">
  <div class="head"><span class="t">${esc(api.sel.p.name)}</span><button class="ib" aria-label="New file">${ic('plus', 15)}</button></div>
  <div class="list">${TREE.map((n) => (n.kids ? dir(n) : file(n))).join('')}</div>
</div>`;

function render(api) {
  const r = api.sel;
  if (home || r.local) return newSession(api) + api.rail(null, { changes: api.changeCount?.(r) ?? 0 });
  return `<div class="pane">
      <div class="bar">${api.tab(r)}<button class="ib" data-act="new" aria-label="New tab">${ic('plus', 16)}</button></div>
      ${r.chat ? chatView(r) : termView(r)}
    </div>${panel === 'explorer' ? files(api) : ''}${api.rail(panel, { changes: api.changeCount?.(r) ?? 0 })}`;
}

// Lets a session's lines arrive one after another; a reply streams in before the next line comes.
function play(api, r, lines, end, ms) {
  r.status = 'working';
  r.stream = null;
  // A newer run on the same session replaces this one.
  const run = r.run = {};
  const step = (i) => {
    if (r.run !== run) return;
    const l = lines[i];
    r.log.push(l);
    let wait = ms;
    if (l[0] === 'say' && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
      r.stream = { at: r.log.length - 1, ...streamFor(l[1].length) };
      wait = r.stream.took + FADE + 400;
      streaming(api);
    }
    const last = i === lines.length - 1;
    if (last && l[0] !== 'say') { r.status = end; r.worked = `${lines.length * 2}s`; }
    api.changed();
    setTimeout(() => {
      if (r.run !== run) return;
      if (last) {
        r.stream = null;
        r.status = end;
        r.worked = `${lines.length * 2}s`;
        return api.changed();
      }
      if (r.stream && l[0] === 'say') r.stream = null;
      step(i + 1);
    }, last ? (l[0] === 'say' ? wait : 0) : wait);
  };
  setTimeout(() => step(0), ms);
  api.changed();
}

// While a reply streams, only its own element is redrawn each frame, not the window.
let frame = 0;
function streaming(api) {
  cancelAnimationFrame(frame);
  const tick = () => {
    const r = api.sel;
    const el = api.$('.md[data-stream]');
    if (el && r?.stream) {
      const s = el.closest('.scroll');
      const pinned = s && s.scrollHeight - s.scrollTop - s.clientHeight < 80;
      el.innerHTML = richText(r.log[r.stream.at][1], r.stream);
      if (pinned) s.scrollTop = s.scrollHeight;
    }
    if (D.api.PROJECTS.some((p) => p.rows.some((x) => x.stream))) frame = requestAnimationFrame(tick);
  };
  frame = requestAnimationFrame(tick);
}

function send(api) {
  const text = draft.trim();
  if (!text) return;
  draft = '';
  const r = api.sel;
  if (home || r.local) {
    const p = r.p;
    const slug = text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 22);
    const row = { id: Date.now(), p, label: `feat/${slug}`, title: text, agent, chat: true, add: 0, del: 0, log: [['you', text]] };
    p.rows.unshift(row);
    p.open = true;
    home = false;
    api.state.sel = row;
    return play(api, row, [['say', "I'll look around the repo first."], ['tool', 'Search', 'theme tokens'], ['tool', 'Read', 'src/app.tsx'], ['tool', 'Edit', 'src/app.tsx'], ['tool', 'Bash', 'pnpm test'], ['say', REPLIES[replies++ % REPLIES.length]]], 'done', 900);
  }
  r.log.push(['you', text]);
  play(api, r, [['tool', 'Read', 'src/app.tsx'], ['say', REPLIES[replies++ % REPLIES.length]]], 'done', 1000);
}

// Other screens that show a session's chat draw its replies the same way.
D.api.richText = (src) => richText(src);

D.register('sessions', {
  sessionScreen: true,
  tour: 'sessions',
  enter() { home = false; },
  render,
  mount(root) {
    const s = root.querySelector('.chat, .term');
    if (s) s.scrollTop = s.scrollHeight;
  },
  act(act, v, t, api) {
    const r = api.sel;
    switch (act) {
      case 'new': home = true; return api.draw();
      case 'rail':
        if (v === 'changes' || v === 'prs') return api.go('review');
        if (v === 'sessions') return api.go('inbox');
        panel = panel === v ? null : v;
        return api.draw();
      case 'dir': opened.has(v) ? opened.delete(v) : opened.add(v); return api.draw();
      case 'agent': {
        const order = Object.keys(AGENTS);
        agent = order[(order.indexOf(agent) + 1) % order.length];
        return api.draw();
      }
      case 'send': return send(api);
      case 'ans':
        if (v === 'no') {
          r.status = undefined;
          r.log.push(['say', "Okay, I won't run it. What should I do instead?"]);
          return api.changed();
        }
        return play(api, r, r.ask.after, 'done', 900);
    }
  },
  input(e, api) {
    draft = e.target.value;
    api.$('.composer')?.classList.toggle('empty', !draft.trim());
  },
  key(e, api) {
    if (e.target.matches('textarea') && e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(api); }
  },
});

// The demo's opening move: the working session keeps going on its own.
addEventListener('DOMContentLoaded', () => {
  const api = D.api;
  const r = api.PROJECTS[0].rows[0];
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  play(api, r, r.script, 'done', 3200);
});
})();
