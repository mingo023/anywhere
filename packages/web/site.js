// Landing page: the agent list, the tabs that drive the desktop and phone demos, the CLI example, the mobile menu,
// and the motion (ASCII field, scroll reveals, window tilt, status cycle), which reduced motion turns off.
(() => {
document.documentElement.classList.add('js');
const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
const stage = document.querySelector('.stage');
document.querySelectorAll('[data-icon]').forEach((el) => { el.innerHTML = AW.ic(el.dataset.icon, 16); });

// Reveal blocks as they scroll in; siblings stagger.
const REVEAL = '.intro, .tour-tabs > li, .tour-cards > .card, .native-stack > .layer, .native-stats > .metric, .iphone-copy, .phone-stage, .term-window, .oss-inner > :not(.oss-bento), .oss-bento > *, .closing > :not(.butterfly)';
const seen = new IntersectionObserver((entries) => entries.forEach((e) => {
  if (!e.isIntersecting) return;
  e.target.classList.add('in');
  seen.unobserve(e.target);
}), { rootMargin: '0px 0px -8% 0px' });
document.querySelectorAll(REVEAL).forEach((el) => {
  el.classList.add('reveal');
  const kin = [...el.parentElement.children];
  el.style.setProperty('--d', `${Math.min(kin.indexOf(el), 5) * 70}ms`);
  seen.observe(el);
});

// The desktop window starts tilted back and lies flat as it scrolls toward the middle of the screen.
const frame = stage.querySelector('.desktop-frame');
if (!still) {
  let queued = false;
  const tilt = () => {
    queued = false;
    if (innerWidth < 720) return frame.style.removeProperty('--tilt');
    const r = stage.getBoundingClientRect();
    const p = Math.min(Math.max(1 - (r.top - innerHeight * 0.15) / (innerHeight * 0.6), 0), 1);
    frame.style.setProperty('--tilt', `${(1 - p) * 10}deg`);
    frame.style.setProperty('--zoom', 0.95 + p * 0.05);
  };
  addEventListener('scroll', () => { if (!queued) { queued = true; requestAnimationFrame(tilt); } }, { passive: true });
  addEventListener('resize', tilt);
  tilt();
}

// Status pills light up in a session's usual order.
const steps = [...document.querySelectorAll('.lifecycle li')];
let step = 0;
const cycle = () => { steps.forEach((li, i) => li.classList.toggle('on', still || i === step)); step = (step + 1) % 3; };
cycle();
if (!still) setInterval(cycle, 1600);

const AGENT_NAMES = [['claude', 'Claude Code'], ['openai', 'Codex'], ['cursor', 'Cursor'], ['grok', 'Grok Build'], ['opencode', 'OpenCode'], ['pi', 'Pi'], ['omp', 'omp'], ['fx', 'fx'], ['hermes', 'Hermes Agent'], ['antigravity', 'Antigravity']];
const track = document.querySelector('.agent-list');
const items = AGENT_NAMES.map(([icon, name]) => `<span class="agent-item">${AW.ic(icon, 18)}${name}</span>`).join('');
// Twice over, hidden from screen readers the second time, so the marquee loops without a seam.
track.innerHTML = still ? items : `${items}<span class="agent-dup" aria-hidden="true" style="display:contents">${items}</span>`;
if (!still) track.parentElement.classList.add('moving');

// Tour tabs drive the desktop demo; the demo reports back when the user navigates inside it.
const tabs = [...document.querySelectorAll('.tour-tabs [data-go], .strip [data-go]')];
const select = (id) => {
  tabs.forEach((t) => {
    t.setAttribute('aria-selected', String(t.dataset.go === id));
    t.tabIndex = t.dataset.go === id || (!tabs.some((tab) => tab.dataset.go === id) && t.dataset.go === 'sessions') ? 0 : -1;
  });
  document.getElementById('desktop-preview').setAttribute('aria-label', tabs.find((t) => t.dataset.go === id)?.textContent || id);
  slide();
};
// The strip's highlight slides under the selected tab.
const strip = document.querySelector('.strip');
const slide = () => {
  const on = strip.querySelector('[aria-selected="true"]');
  strip.classList.toggle('picked', !!on);
  if (!on) return;
  strip.style.setProperty('--x', `${on.offsetLeft}px`);
  strip.style.setProperty('--w', `${on.offsetWidth}px`);
};
addEventListener('resize', slide);
slide();
document.fonts?.ready.then(slide);
tabs.forEach((t) => t.addEventListener('click', () => {
  window.AnywhereDesktop?.go(t.dataset.go);
  select(t.dataset.go);
  if (t.closest('.strip')) return;
  const top = stage.getBoundingClientRect().top;
  if (top < 0 || top > innerHeight * 0.5) stage.scrollIntoView({ block: 'start' });
}));
document.addEventListener('desktop:screen', (e) => select(e.detail));

const phoneTabs = [...document.querySelectorAll('.phone-tabs [data-phone]')];
phoneTabs.forEach((t) => t.addEventListener('click', () => {
  window.AnywherePhone?.go(t.dataset.phone);
}));
document.addEventListener('phone:screen', (e) => {
  phoneTabs.forEach((t) => {
    t.setAttribute('aria-selected', String(t.dataset.phone === e.detail));
    t.tabIndex = t.dataset.phone === e.detail ? 0 : -1;
  });
  document.getElementById('phone-preview').setAttribute('aria-label', phoneTabs.find((t) => t.dataset.phone === e.detail)?.textContent || 'Anywhere on iPhone');
});

const SCRIPT = [
  ['cmd', 'id=$(anywhere session create --new-worktree dark-mode --provider claude \\\n    --prompt "Add a dark mode toggle to settings")'],
  ['cmd', 'anywhere session list'],
  ['out', '7f3a9c2e-41d8-4b6a-9e0f-2c5d8a1b3e47  <span class="k">working</span>   claude  orbit-web      Add a dark mode toggle\n2b8e4f1a-9c3d-4e7b-8a2f-6d1c0e9b5a38  <span class="k">needsYou</span>  codex   fox-api        Cache invoice queries\n9d1c7e3b-2a4f-4c8e-b6d0-3f5a2e8c1b74  <span class="g">done</span>      claude  garden-mobile  Haptics on long press'],
  ['cmd', 'anywhere session wait $id && echo ready for review'],
  ['out', '<span class="g">done</span>\nready for review'],
  ['cmd', 'anywhere ticket create --project orbit-web --start "Cache the avatar images"'],
  ['out', '<span class="c">c41e8b2d-7f3a-4d9c-a1e6-5b8f2c0d7e93</span>'],
];
const code = document.querySelector('.term-body code');
const prompt = '<span class="p">❯</span> ';
code.innerHTML = SCRIPT.map(([kind, text]) => kind === 'cmd'
  ? `${prompt}<span class="c">${AW.esc(text)}</span>\n`
  : `<span class="o">${text}</span>\n\n`).join('');

document.querySelectorAll('[role="tablist"]').forEach((list) => {
  const buttons = [...list.querySelectorAll('[role="tab"]')];
  buttons.forEach((button) => {
    button.setAttribute('aria-controls', list.classList.contains('phone-tabs') ? 'phone-preview' : 'desktop-preview');
    button.tabIndex = button.getAttribute('aria-selected') === 'true' ? 0 : -1;
    button.addEventListener('keydown', (e) => {
      let index = buttons.indexOf(button);
      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') index = (index + 1) % buttons.length;
      else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') index = (index + buttons.length - 1) % buttons.length;
      else if (e.key === 'Home') index = 0;
      else if (e.key === 'End') index = buttons.length - 1;
      else return;
      e.preventDefault();
      buttons[index].click();
      buttons[index].focus({ preventScroll: true });
    });
  });
});

// The closing butterfly loads and plays only near the screen; under reduced motion it holds one frame.
const butterfly = document.querySelector('.butterfly');
new IntersectionObserver(([e]) => {
  if (!e.isIntersecting) return butterfly.pause();
  if (butterfly.preload === 'none') {
    butterfly.preload = 'auto';
    butterfly.addEventListener('loadeddata', () => butterfly.classList.add('on'), { once: true });
    if (still) butterfly.addEventListener('loadedmetadata', () => { butterfly.currentTime = 1; }, { once: true });
    butterfly.load();
  }
  if (!still) butterfly.play().catch(() => {});
}, { rootMargin: '200px 0px' }).observe(butterfly);

// The header draws in once the page moves, and marks the section in view.
const header = document.querySelector('.nav');
const onScroll = () => header.classList.toggle('scrolled', scrollY > 8);
addEventListener('scroll', onScroll, { passive: true });
onScroll();
const navLinks = [...document.querySelectorAll('#primary-nav a[href^="#"]')];
const shown = new Map();
const where = new IntersectionObserver((entries) => {
  entries.forEach((e) => shown.set(e.target.id, e.isIntersecting));
  const current = navLinks.find((a) => shown.get(a.hash.slice(1)));
  navLinks.forEach((a) => a.classList.toggle('on', a === current));
}, { rootMargin: '-45% 0px -50% 0px' });
navLinks.forEach((a) => { const el = document.querySelector(a.hash); if (el) where.observe(el); });

const menu = document.querySelector('.menu-button');
const nav = document.getElementById('primary-nav');
const closeMenu = () => menu.setAttribute('aria-expanded', 'false');
menu.addEventListener('click', () => menu.setAttribute('aria-expanded', String(menu.getAttribute('aria-expanded') !== 'true')));
nav.addEventListener('click', closeMenu);
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && menu.getAttribute('aria-expanded') === 'true') {
    closeMenu();
    menu.focus();
  }
});
document.addEventListener('click', (e) => { if (!e.target.closest('.nav')) closeMenu(); });
nav.addEventListener('focusout', () => {
  setTimeout(() => { if (!document.activeElement.closest('#primary-nav, .menu-button')) closeMenu(); }, 0);
});

// The hero's ASCII field: a slow wave of characters picked by brightness, lit around the pointer, with
// status-coloured streaks running along its rows. Glyphs are drawn once into an atlas and stamped from it, at 30fps.
// Paused while off screen; drawn once, still, under reduced motion.
(() => {
  const canvas = document.querySelector('.signal');
  const ctx = canvas.getContext('2d');
  const RAMP = ' .\u00b7:-=+*#%@';
  const CW = 11;
  const CH = 18;
  const LEVELS = 8;
  const COLORS = ['#f76b15', '#ffb224', '#3dd68c', '#a371f7', '#7c85e3'];
  let w = 0, h = 0, cols = 0, rows = 0, dpr = 1, atlas = null, streaks = [], mouse = { x: -1e4, y: -1e4 }, running = false, last = 0;

  // One row per tint (grey at LEVELS strengths, then each colour), one column per glyph.
  const build = () => {
    atlas = document.createElement('canvas');
    atlas.width = RAMP.length * CW * dpr;
    atlas.height = (LEVELS + COLORS.length) * CH * dpr;
    const a = atlas.getContext('2d');
    a.scale(dpr, dpr);
    a.font = '500 13px "Geist Mono", ui-monospace, monospace';
    a.textAlign = 'center';
    a.textBaseline = 'middle';
    for (let r = 0; r < LEVELS + COLORS.length; r++) {
      a.fillStyle = r < LEVELS ? `rgba(235,235,235,${0.08 + (r / (LEVELS - 1)) * 0.5})` : COLORS[r - LEVELS];
      for (let c = 1; c < RAMP.length; c++) a.fillText(RAMP[c], c * CW + CW / 2, r * CH + CH / 2);
    }
  };
  const size = () => {
    dpr = Math.min(devicePixelRatio || 1, 2);
    w = canvas.clientWidth; h = canvas.clientHeight;
    cols = Math.ceil(w / CW); rows = Math.ceil(h / CH);
    canvas.width = w * dpr; canvas.height = h * dpr;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    build();
  };
  const stamp = (glyph, tint, x, y) => {
    ctx.drawImage(atlas, glyph * CW * dpr, tint * CH * dpr, CW * dpr, CH * dpr, x * CW * dpr, y * CH * dpr, CW * dpr, CH * dpr);
  };
  const field = (x, y, t) => {
    const cx = x - cols / 2, cy = (y - rows * 0.32) * 1.6;
    const v = Math.sin(x * 0.07 + t * 0.5) + Math.sin(y * 0.16 - t * 0.35) + Math.sin((x + y) * 0.045 + t * 0.25) + Math.sin(Math.hypot(cx, cy) * 0.11 - t * 0.8);
    return (v + 4) / 8;
  };
  const draw = (t) => {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const lit = new Map();
    if (!still) {
      if (streaks.length < Math.min(9, cols / 14) && Math.random() < 0.06) {
        streaks.push({ y: Math.floor(Math.random() * rows), x: Math.random() < 0.5 ? -1 : cols, dir: 0, speed: 0.35 + Math.random() * 0.5, len: 6 + Math.floor(Math.random() * 14), tint: LEVELS + Math.floor(Math.random() * COLORS.length) });
        const s = streaks.at(-1);
        s.dir = s.x < 0 ? 1 : -1;
      }
      streaks = streaks.filter((s) => {
        s.x += s.speed * s.dir;
        for (let i = 0; i < s.len; i++) lit.set(`${Math.round(s.x - i * s.dir)},${s.y}`, { tint: s.tint, fade: 1 - i / s.len });
        return s.x > -s.len && s.x < cols + s.len;
      });
    }
    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < cols; x++) {
        const near = Math.max(0, 1 - Math.hypot(x * CW - mouse.x, y * CH - mouse.y) / 140) ** 2;
        let v = field(x, y, t) ** 2.2 + near * 0.3;
        const hit = lit.get(`${x},${y}`);
        if (hit) v = Math.max(v, 0.45 + hit.fade * 0.5);
        const glyph = Math.min(RAMP.length - 1, Math.floor(v * RAMP.length));
        if (glyph === 0) continue;
        const tint = hit && hit.fade > 0.25 ? hit.tint : Math.min(LEVELS - 1, Math.floor(v * LEVELS));
        stamp(glyph, tint, x, y);
      }
    }
  };
  const frame = (now) => {
    if (!running) return;
    if (now - last > 33) { last = now; draw(now / 1000); }
    requestAnimationFrame(frame);
  };
  const start = () => { size(); if (still) draw(8); };
  // Glyphs are measured once the mono font is in; until then the atlas would hold the fallback face.
  (document.fonts?.ready ?? Promise.resolve()).then(start);
  start();
  addEventListener('resize', () => { size(); if (still) draw(8); });
  if (still) return;
  canvas.parentElement.addEventListener('pointermove', (e) => {
    const r = canvas.getBoundingClientRect();
    mouse = { x: e.clientX - r.left, y: e.clientY - r.top };
  });
  canvas.parentElement.addEventListener('pointerleave', () => { mouse = { x: -1e4, y: -1e4 }; });
  new IntersectionObserver(([e]) => {
    const was = running;
    running = e.isIntersecting;
    if (running && !was) requestAnimationFrame(frame);
  }).observe(canvas);
})();
})();
