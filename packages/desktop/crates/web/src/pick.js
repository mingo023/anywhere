(() => {
  if (window.__anywherePick) return;
  const BLUE = '#2f80ed';
  const MAX_HTML = 2000;
  const MAX_DEPTH = 3;
  let on = false;
  let hovered = null;
  let marks = [];
  let root = null;
  let box = null;
  let label = null;
  let style = null;

  const post = (message) => window.ipc.postMessage(JSON.stringify(message));

  const ensure = () => {
    if (root) return;
    const host = document.createElement('div');
    host.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2147483647';
    root = host.attachShadow({ mode: 'closed' });
    root.innerHTML = `<style>
      .box{position:fixed;border:1.5px solid ${BLUE};background:rgba(47,128,237,.12);border-radius:3px;display:none}
      .label{position:fixed;background:${BLUE};color:#fff;font:600 11px/16px -apple-system,system-ui,sans-serif;padding:2px 6px;border-radius:4px;white-space:nowrap;display:none}
      .label span{font-weight:400;opacity:.85;margin-left:6px}
      .mark{position:fixed;border:1.5px solid ${BLUE};border-radius:3px}
      .mark b{position:absolute;left:0;top:0;background:${BLUE};color:#fff;font:600 11px/16px -apple-system,system-ui,sans-serif;padding:0 5px;border-radius:0 0 3px 0}
    </style><div class="box"></div><div class="label"></div><div class="marks"></div>`;
    box = root.querySelector('.box');
    label = root.querySelector('.label');
    document.documentElement.appendChild(host);
  };

  const name = (el) => {
    const tag = el.tagName.toLowerCase();
    if (el.id) return `${tag}#${el.id}`;
    const first = el.classList[0];
    return first ? `${tag}.${first}` : tag;
  };

  const unique = (sel) => {
    try {
      return document.querySelectorAll(sel).length === 1;
    } catch {
      return false;
    }
  };

  const anchor = (node) => {
    if (node.id) return `#${CSS.escape(node.id)}`;
    const testid = node.getAttribute('data-testid');
    return testid ? `[data-testid="${CSS.escape(testid)}"]` : '';
  };

  const selector = (el) => {
    if (el === document.documentElement) return 'html';
    const parts = [];
    for (let node = el; node && node !== document.documentElement; node = node.parentElement) {
      const own = anchor(node);
      if (own && unique(own)) {
        parts.unshift(own);
        break;
      }
      let part = node.tagName.toLowerCase() + (node.classList[0] ? `.${CSS.escape(node.classList[0])}` : '');
      const parent = node.parentElement;
      if (parent) {
        const same = [...parent.children].filter((c) => c.tagName === node.tagName);
        if (same.length > 1) part += `:nth-of-type(${same.indexOf(node) + 1})`;
      }
      parts.unshift(part);
      if (unique(parts.join(' > '))) break;
    }
    return parts.join(' > ');
  };

  // Never an input's value: it may be a password.
  const text = (el) =>
    (el.innerText || el.getAttribute('placeholder') || el.getAttribute('aria-label') || el.getAttribute('alt') || el.getAttribute('title') || '')
      .replace(/\s+/g, ' ')
      .trim()
      .slice(0, 200);

  // Styles, handlers and data-* are noise to the agent; a value attribute may hold a password.
  const kept = (name) => name === 'data-testid' || !(name === 'style' || name === 'value' || name.startsWith('data-') || name.startsWith('on'));

  const strip = (node, depth) => {
    for (const { name } of [...node.attributes]) {
      if (!kept(name)) node.removeAttribute(name);
    }
    if (node.children.length && (depth >= MAX_DEPTH || node.tagName === 'svg' || node.tagName === 'CANVAS')) {
      node.replaceChildren('…');
      return;
    }
    for (const child of node.children) strip(child, depth + 1);
  };

  const html = (el) => {
    const copy = el.cloneNode(true);
    strip(copy, 0);
    return copy.outerHTML.length > MAX_HTML ? `${copy.outerHTML.slice(0, MAX_HTML)}…` : copy.outerHTML;
  };

  const place = (el, node) => {
    const r = el.getBoundingClientRect();
    Object.assign(node.style, { left: `${r.left}px`, top: `${r.top}px`, width: `${r.width}px`, height: `${r.height}px` });
    return r;
  };

  const hover = (el) => {
    hovered = el;
    if (!el) {
      box.style.display = 'none';
      label.style.display = 'none';
      return;
    }
    const r = place(el, box);
    box.style.display = 'block';
    const size = document.createElement('span');
    size.textContent = `${Math.round(r.width)} × ${Math.round(r.height)}`;
    label.replaceChildren(name(el), size);
    label.style.display = 'block';
    label.style.left = `${Math.max(0, r.left)}px`;
    label.style.top = `${r.top >= 22 ? r.top - 22 : r.bottom + 4}px`;
  };

  const drawMarks = () => {
    if (!root) return;
    const drawn = marks.flatMap(([n, sel]) => {
      let el = null;
      try {
        el = document.querySelector(sel);
      } catch {}
      if (!el) return [];
      const mark = document.createElement('div');
      mark.className = 'mark';
      place(el, mark);
      const badge = document.createElement('b');
      badge.textContent = String(n);
      mark.append(badge);
      return [mark];
    });
    root.querySelector('.marks').replaceChildren(...drawn);
  };

  const redraw = () => {
    if (!root) return;
    if (hovered) hover(hovered);
    drawMarks();
  };

  const set = (value) => {
    on = value;
    if (on) {
      ensure();
      style ??= Object.assign(document.createElement('style'), { textContent: '*{cursor:crosshair!important}' });
      document.documentElement.appendChild(style);
    } else {
      style?.remove();
      if (root) hover(null);
    }
  };

  const swallow = (e) => {
    if (!on) return false;
    e.preventDefault();
    e.stopImmediatePropagation();
    return true;
  };

  for (const type of ['pointerdown', 'pointerup', 'mousedown', 'mouseup', 'dblclick', 'auxclick', 'contextmenu']) {
    addEventListener(type, swallow, true);
  }
  addEventListener(
    'click',
    (e) => {
      if (!swallow(e) || !(e.target instanceof Element)) return;
      const el = e.target;
      const r = el.getBoundingClientRect();
      post({ kind: 'picked', url: location.href, selector: selector(el), name: name(el), text: text(el), html: html(el), width: Math.round(r.width), height: Math.round(r.height) });
    },
    true,
  );
  addEventListener(
    'mousemove',
    (e) => {
      if (on && e.target instanceof Element && e.target !== hovered) hover(e.target);
    },
    true,
  );
  addEventListener(
    'keydown',
    (e) => {
      if (on && e.key === 'Escape' && swallow(e)) {
        set(false);
        post({ kind: 'ended' });
      }
    },
    true,
  );
  addEventListener('scroll', redraw, { capture: true, passive: true });
  addEventListener('resize', redraw);

  window.__anywherePick = {
    set,
    marks: (list) => {
      marks = list;
      if (list.length) ensure();
      drawMarks();
    },
  };
})();
