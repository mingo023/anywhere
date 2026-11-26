// Autoplay for the desktop demo: a screen defines its steps with AnywhereTour.define, and playing it moves a drawn
// pointer and fires the same clicks, keys and typing a visitor would, so each feature shows itself on its own.
//
// Steps: ['wait', ms] · ['click', selector] · ['hover', selector] · ['type', selector, text] · ['key', selector, key]
// A selector is looked up in the window when the step runs, since every redraw replaces the window's elements.

(() => {
const MOVE = 650;
const CHAR = 45;
const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
const scripts = {};
const win = document.querySelector('.window.desktop');
const pointer = document.createElement('i');
pointer.className = 'tour-pointer';
pointer.setAttribute('aria-hidden', 'true');
pointer.innerHTML = '<svg viewBox="0 0 20 20"><path d="M3 2l12 7.2-5.2 1.3L7.4 16z" fill="#fff" stroke="#000" stroke-width="1.2" stroke-linejoin="round"/></svg><b></b>';
win.append(pointer);

let run = 0;
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const app = () => win.querySelector('.app');
const find = (q) => app()?.querySelector(q);

// The pointer sits in the window's own points, so it lands where the scaled element is.
function aim(el) {
  const a = app().getBoundingClientRect();
  const r = el.getBoundingClientRect();
  const k = a.width / app().offsetWidth;
  const s = win.clientWidth / app().offsetWidth;
  const x = (r.left + Math.min(r.width / 2, 40) - a.left) / k;
  const y = (r.top + r.height / 2 - a.top) / k;
  pointer.style.transform = `translate(${(x - 3) * s}px, ${(y - 2) * s}px)`;
  pointer.classList.add('on');
}

async function reach(q, token) {
  const el = find(q);
  if (!el) return null;
  aim(el);
  await sleep(still ? 0 : MOVE);
  return token === run ? find(q) : null;
}

const STEPS = {
  wait: (ms) => sleep(ms),
  hover: (q, token) => reach(q, token),
  async click(q, token) {
    const el = await reach(q, token);
    if (!el) return;
    pointer.classList.remove('press');
    void pointer.offsetWidth;
    pointer.classList.add('press');
    el.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await sleep(250);
  },
  async type(q, text, token) {
    const el = await reach(q, token);
    if (!el) return;
    el.focus({ preventScroll: true });
    for (const ch of text) {
      if (token !== run) return;
      const field = find(q);
      if (!field) return;
      field.value += ch;
      field.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(still ? 0 : CHAR);
    }
  },
  async key(q, key, token) {
    const el = find(q);
    el?.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
  },
};

const length = (steps) => steps.reduce((ms, [kind, a, b]) =>
  ms + (kind === 'wait' ? a : kind === 'type' ? MOVE + b.length * CHAR : kind === 'key' ? 0 : MOVE + 250), 0);

window.AnywhereTour = {
  define(id, steps) { scripts[id] = steps; },
  /** How long a screen's script runs, in ms; undefined for a screen that plays itself. */
  length(id) { return scripts[id] && length(scripts[id]); },
  /** Plays a screen's script from the start, cancelling whatever was playing. */
  async play(id) {
    const token = ++run;
    pointer.classList.remove('on');
    const steps = scripts[id];
    if (!steps) return;
    for (const [kind, ...args] of steps) {
      if (token !== run) return;
      await STEPS[kind](...args, token);
    }
    if (token === run) pointer.classList.remove('on');
  },
  stop() { run++; pointer.classList.remove('on'); },
};
})();
