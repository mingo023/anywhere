# Browser Element Picker Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** A pointer button in the in-app browser's bar turns on a picker; each click on the page puts that element in the agent's chat composer as a numbered chip, which goes to the agent as URL, selector, size, text and outerHTML.

**Architecture:** The `web` crate injects `pick.js` into every page (like its existing `MOVED` script). The script draws a hover box, swallows clicks while on, and posts the picked element over `window.ipc`; `Page` parses it into `Event::Picked(web::Element)` and drives the script with `set_picking`/`set_marks`. In `pocket`, `browser/pick.rs` routes a pick to the chat the existing Add to chat flow would pick (`chat_target`); the element lands in that chat's `Draft.elements`, shows as a chip over the input, and `Draft::prompt` writes it as a `# Selected elements` block, which `split_attachments` reads back so a sent bubble shows chips too.

**Toolset** (all from `packages/desktop`):
- One crate's tests: `cargo test -p web` / `cargo test -p pocket <filter>`
- Type-check: `cargo build --workspace`
- Lint: `cargo clippy --workspace --all-targets` (no new warnings)
- Full suite: `cargo test --workspace`
- Dev app: `scripts/bundle-dev.sh` from the repo root (builds and opens `target/release/Anywhere Dev.app`)

**Read first:**
- `CLAUDE.md` (repo root): code layout rules (state in a feature module, logic in free functions, tests at the bottom named as sentences) and performance rules (render costs only what's visible).
- `docs/adr/0003-desktop-code-layout.md`: where code goes.
- `packages/desktop/crates/web/src/page.rs`: how a page is built, the `MOVED` init script and ipc handler this plan extends.
- `packages/desktop/crates/pocket/src/browser.rs` + `browser/view.rs`: browser tab state, page events, toolbar.
- `packages/desktop/crates/pocket/src/add_to_chat.rs`: how a quote picks its agent (`chat_choices`, `chat_target`, `default_target`, `Route`) and flashes the "added" toast.
- `packages/desktop/crates/pocket/src/composer.rs`: `Draft`, `Draft::prompt`, `split_attachments`, `Sent`.

**Settled decisions** (from the conversation, don't revisit):
- Picked element shows as a **chip card** in the chat composer: number, host, text-or-name; × removes; hover shows selector + HTML.
- Pick mode **stays on** after each click; picked elements on the current page get numbered badges matching their chips; Esc (in the page) or the pointer button turns it off.
- Agent receives **URL + selector + size + text + outerHTML trimmed to ~2KB**.
- Target agent: the same rule as Add to chat (`chat_target`), falling back to the focused/first ready agent when the setting is "ask". A TUI agent (no chat) gets the block pasted.
- Linux has no page (stub), so only stub methods there.
- Security: a page script can call `window.ipc.postMessage` itself, so `Picked` is accepted **only while the tab is picking**.

---

## PR 1: Pick page elements into the chat

**Scope:** Everything above. No keybinding for the picker, no element screenshots, no picking into the New session composer.
**Depends on:** nothing
**Done when:** `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings) and `cargo test --workspace` pass, and in Anywhere Dev.app: open a browser tab, press the pointer button, hover shows a box + `tag.class W × H`, click adds chip `1 host / text` to the session's chat, badge `1` appears on the element, Esc stops picking, sending shows the chip in the sent bubble.

### Task 1.1: `web::Element` and the pick message

**Files:**
- Create: `packages/desktop/crates/web/src/pick.rs`
- Modify: `packages/desktop/crates/web/src/web.rs`
- Modify: `packages/desktop/crates/web/Cargo.toml`

**Context:** The script (Task 1.2) posts JSON strings over `window.ipc`. Today the ipc handler only knows the plain string `"moved"`. This task adds the value type and the parser, nothing else. `web::Element` is plain data used by `pocket` on every OS, so it lives outside the `cfg(not(linux))` code; the parser is used only by the non-Linux `Page`, so it is gated like `SCHEMES` in `page.rs` (`#[cfg(any(test, not(target_os = "linux")))]`).

**Step 1: Add dependencies.** In `packages/desktop/crates/web/Cargo.toml`, add a `[dependencies]` section right after `[lib]`/`path` (before the target-specific sections):

```toml
[dependencies]
serde.workspace = true
serde_json.workspace = true
```

**Step 2: Write the failing tests.** Create `packages/desktop/crates/web/src/pick.rs` with only the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::{Element, event};
    use crate::Event;

    #[test]
    fn a_picked_message_reads_as_the_element_it_names() {
        let body = r#"{"kind":"picked","url":"https://github.com/","selector":"form > input:nth-of-type(1)","name":"input.Primer_Brand__TextInput","text":"Enter your email","html":"<input type=\"email\">","width":420,"height":41}"#;
        let Some(Event::Picked(e)) = event(body) else { panic!("not a pick") };
        assert_eq!(
            e,
            Element {
                url: "https://github.com/".into(),
                selector: "form > input:nth-of-type(1)".into(),
                name: "input.Primer_Brand__TextInput".into(),
                text: "Enter your email".into(),
                html: "<input type=\"email\">".into(),
                width: 420,
                height: 41,
            }
        );
    }

    #[test]
    fn an_ended_message_says_the_picker_turned_off() {
        assert!(matches!(event(r#"{"kind":"ended"}"#), Some(Event::PickEnded)));
    }

    #[test]
    fn anything_else_a_page_posts_is_ignored() {
        for body in ["moved", "", "{}", r#"{"kind":"picked"}"#, r#"{"kind":"other"}"#] {
            assert!(event(body).is_none(), "{body}");
        }
    }
}
```

**Step 3: Run the tests to verify they fail.**

Run: `cd packages/desktop && cargo test -p web pick`
Expected: FAIL to compile: `pick` module not declared / `Element`, `event`, `Event::Picked` unresolved.

**Step 4: Write the implementation.** Put this above the tests in `pick.rs`:

```rust
//! What the page's picker script posts when the user picks an element.

use serde::Deserialize;

/// An element picked on a page, as the agent reads it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Element {
    pub url: String,
    pub selector: String,
    /// Its tag and id, else its first class: `div.Artwork__HkJNWQ`.
    pub name: String,
    /// What it says, on one line; empty when it says nothing.
    pub text: String,
    /// Its outerHTML, cut to about 2KB.
    pub html: String,
    pub width: u32,
    pub height: u32,
}

#[cfg(any(test, not(target_os = "linux")))]
pub(crate) const SCRIPT: &str = include_str!("pick.js");

#[cfg(any(test, not(target_os = "linux")))]
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Message {
    Picked(Element),
    Ended,
}

/// The event an ipc `body` from the picker means, if it is one.
#[cfg(any(test, not(target_os = "linux")))]
pub(crate) fn event(body: &str) -> Option<crate::Event> {
    match serde_json::from_str(body).ok()? {
        Message::Picked(e) => Some(crate::Event::Picked(e)),
        Message::Ended => Some(crate::Event::PickEnded),
    }
}
```

`SCRIPT` needs the file to exist: create an empty `packages/desktop/crates/web/src/pick.js` now (Task 1.2 fills it).

In `packages/desktop/crates/web/src/web.rs` add `mod pick;` after `mod page;` and `pub use pick::Element;` after the other `pub use` lines.

In `packages/desktop/crates/web/src/page.rs`, add two variants to `pub enum Event` (after `Pressed`):

```rust
    /// The picker's click on an element; see [`Page::set_picking`].
    Picked(crate::Element),
    /// Esc in the page turned the picker off.
    PickEnded,
```

**Step 5: Run the tests to verify they pass.**

Run: `cd packages/desktop && cargo test -p web pick`
Expected: PASS (3 tests). `SCRIPT` may warn as unused until Task 1.2; that's fine for now.

### Task 1.2: The picker script and `Page`'s controls

**Files:**
- Modify (fill): `packages/desktop/crates/web/src/pick.js`
- Modify: `packages/desktop/crates/web/src/page.rs` (imports at top; `Page` struct ~line 88; `Page::new` ~line 98; new methods after `edit` ~line 250)
- Modify: `packages/desktop/crates/web/src/page/linux.rs`

**Context:** wry's `with_initialization_script` adds a main-frame script run at document start in every new document; calls chain (wry pushes each into a list). `evaluate_script` runs JS in the current document. A new document loses the script's state, so `Page` caches what it last told the script (`picking`, `marks`) and `reapply` re-sends it; `pocket` calls `reapply` on `Event::Finished`. `set_marks` is called from render (Task 1.5), so it must do nothing unless the marks changed. No JS test harness exists in this crate; the script is verified in the app (Task 1.6).

**Step 1: Write `pick.js`:**

```js
(() => {
  if (window.__anywherePick) return;
  const BLUE = '#2f80ed';
  const MAX_HTML = 2000;
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

  const selector = (el) => {
    const parts = [];
    for (let node = el; node && node.nodeType === 1 && node !== document.documentElement; node = node.parentElement) {
      if (node.id && unique(`#${CSS.escape(node.id)}`)) {
        parts.unshift(`#${CSS.escape(node.id)}`);
        break;
      }
      let part = node.tagName.toLowerCase() + [...node.classList].slice(0, 2).map((c) => `.${CSS.escape(c)}`).join('');
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

  const html = (el) => (el.outerHTML.length > MAX_HTML ? `${el.outerHTML.slice(0, MAX_HTML)}…` : el.outerHTML);

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
```

**Step 2: Wire it into `Page`** (`packages/desktop/crates/web/src/page.rs`):

- Change the `use std::cell::Cell;` import (gated `#[cfg(not(target_os = "linux"))]`) to `use std::cell::{Cell, RefCell};`.
- In `pub struct Page` (non-Linux) add after `placed`:

```rust
    /// What the picker script was last told, sent again to each new document.
    picking: Cell<bool>,
    marks: RefCell<Vec<(usize, String)>>,
```

- In `Page::new`, chain the script after `.with_initialization_script(MOVED)`:

```rust
            .with_initialization_script(crate::pick::SCRIPT)
```

- Replace the ipc handler body:

```rust
            .with_ipc_handler(move |request| match request.body().as_str() {
                "moved" => ipc(Event::Moved),
                body => {
                    if let Some(event) = crate::pick::event(body) {
                        ipc(event);
                    }
                }
            })
```

- Both `Ok(Self { view, placed: Cell::new(None), ... })` constructors (macOS and the `not(macos)` branch) gain `picking: Cell::new(false), marks: RefCell::default(),`.

- Add methods inside `impl Page` (non-Linux), after `edit`:

```rust
    /// While on, a click on the page picks the element under it, posted as [`Event::Picked`], instead of pressing it.
    pub fn set_picking(&self, on: bool) {
        if self.picking.replace(on) != on {
            self.run(&format!("window.__anywherePick?.set({on})"));
        }
    }

    /// Badges each element a selector finds with its number, in place of the last badges. Cheap when unchanged, so
    /// it can run on every frame.
    pub fn set_marks(&self, marks: &[(usize, String)]) {
        if *self.marks.borrow() != marks {
            *self.marks.borrow_mut() = marks.to_vec();
            self.run(&format!("window.__anywherePick?.marks({})", marks_json(marks)));
        }
    }

    /// A new document starts with the picker off and no badges; call once it loads.
    pub fn reapply(&self) {
        let marks = marks_json(&self.marks.borrow());
        self.run(&format!("window.__anywherePick?.set({});window.__anywherePick?.marks({marks})", self.picking.get()));
    }

    fn run(&self, js: &str) {
        let _ = self.view.evaluate_script(js);
    }
```

- Add a free function near `opens_here`:

```rust
#[cfg(not(target_os = "linux"))]
fn marks_json(marks: &[(usize, String)]) -> String {
    serde_json::to_string(marks).unwrap_or_else(|_| "[]".into())
}
```

**Step 3: Linux stubs.** In `packages/desktop/crates/web/src/page/linux.rs`, after `edit`:

```rust
    pub fn set_picking(&self, _: bool) {
        match self.0 {}
    }

    pub fn set_marks(&self, _: &[(usize, String)]) {
        match self.0 {}
    }

    pub fn reapply(&self) {
        match self.0 {}
    }
```

**Step 4: Verify.**

Run: `cd packages/desktop && cargo build -p web && cargo test -p web && cargo clippy -p web --all-targets`
Expected: builds, all `web` tests PASS, no warnings.

### Task 1.3: `Draft.elements`, the prompt block, and reading it back

**Files:**
- Modify: `packages/desktop/crates/pocket/src/composer.rs` (`Draft` ~line 66-86; `has_attachments` ~162; `prompt` ~194; consts/`Sent`/`split_attachments` ~203-233; tests ~570+)
- Modify: `packages/desktop/crates/pocket/src/chat/sticky.rs:264`
- Modify: `packages/desktop/crates/pocket/src/chat/user_bubble.rs:211` (only the destructure here; the chips come in Task 1.4)

**Context:** A chat prompt goes to the agent as text; attachments are appended as blocks (`# Attached files`, `# Attached images`) and `split_attachments` strips them back off for the sent bubble. Elements follow the same shape: `# Selected elements` comes last, each element headed `## <n> <host> › <title>` so the bubble can rebuild the chip from the heading alone. HTML goes in a fence longer than any backtick run in it, and the splitter skips fenced lines so `## ` inside HTML isn't read as a heading. `split_elements` runs first because the elements block is last.

**Step 1: Write the failing tests.** Add to the `tests` module at the bottom of `composer.rs` (also add `element_title, elements_block` to its `use super::{...}` list):

```rust
    fn element(url: &str, text: &str, html: &str) -> web::Element {
        web::Element { url: url.into(), selector: "form > input".into(), name: "input.email".into(), text: text.into(), html: html.into(), width: 420, height: 41 }
    }

    #[test]
    fn picked_elements_follow_the_prompt_headed_by_their_number_host_and_words() {
        let draft = Draft { elements: vec![element("https://github.com/", "Enter your email", "<input>")], ..Draft::default() };
        assert_eq!(
            draft.prompt("Fix this"),
            "Fix this\n\n# Selected elements\n\n## 1 github.com › Enter your email\nURL: https://github.com/\nSelector: `form > input`\nSize: 420 × 41\nText: Enter your email\n```html\n<input>\n```"
        );
        assert!(draft.has_attachments());
    }

    #[test]
    fn an_element_with_no_words_is_titled_by_its_tag_and_class_and_says_no_text() {
        let e = element("https://a.dev/x", "", "<div></div>");
        assert_eq!(element_title(&e), "input.email");
        assert!(!elements_block(&[e]).contains("Text:"));
    }

    #[test]
    fn html_holding_backticks_gets_a_longer_fence() {
        let block = elements_block(&[element("https://a.dev/", "", "<code>```</code>")]);
        assert!(block.ends_with("````html\n<code>```</code>\n````"), "{block}");
    }

    #[test]
    fn a_sent_prompt_splits_back_into_its_elements_headings() {
        let draft = Draft { files: vec!["/r/main.rs".into()], elements: vec![element("https://github.com/", "Enter your email", "<p>\n## not a heading\n</p>"), element("https://github.com/", "", "<div>")], ..Draft::default() };
        let sent = split_attachments(&draft.prompt("Fix"));
        assert_eq!(sent, Sent { text: "Fix", files: vec!["/r/main.rs"], images: vec![], elements: vec!["1 github.com › Enter your email", "2 github.com › input.email"] });
        let only = Draft { elements: vec![element("https://a.dev/", "Hi", "<b>")], ..Draft::default() };
        assert_eq!(split_attachments(&only.prompt("")).elements, vec!["1 a.dev › Hi"]);
        assert_eq!(split_attachments(&only.prompt("")).text, "");
    }
```

Update the existing tests that build `Sent` literally (they won't compile once the field exists): in `a_sent_prompt_splits_back_into_its_text_and_image_paths` change the closure to `let sent = |text, files, images| Sent { text, files, images, elements: vec![] };`, and in `a_sent_prompt_splits_back_into_its_files_too_with_or_without_images` add `elements: vec![]` to both `Sent { ... }` literals.

**Step 2: Run to verify they fail.**

Run: `cd packages/desktop && cargo test -p pocket composer::tests`
Expected: FAIL to compile: no field `elements` on `Draft`/`Sent`, `element_title`/`elements_block` not found.

**Step 3: Implement.** In `composer.rs`:

- In `pub(crate) struct Draft`, after `files`:

```rust
    /// Elements picked on a browser page, numbered by their place here.
    pub(crate) elements: Vec<web::Element>,
```

- `has_attachments`:

```rust
    pub(crate) fn has_attachments(&self) -> bool {
        !self.images.is_empty() || !self.files.is_empty() || !self.elements.is_empty()
    }
```

- `prompt`: replace the `let parts = [...]` line with

```rust
        let elements = (!self.elements.is_empty()).then(|| elements_block(&self.elements));
        let parts = [(!typed.is_empty()).then(|| typed.to_string()), block(FILES, self.files.iter().map(PathBuf::as_path).collect()), block(IMAGES, self.images.iter().map(|a| a.path.as_path()).collect()), elements];
```

- After `const IMAGES`:

```rust
const ELEMENTS: &str = "# Selected elements\n\n";

/// Picked elements as the agent reads them, each headed by its chip: number, host and title.
pub(crate) fn elements_block(elements: &[web::Element]) -> String {
    let each = elements.iter().enumerate().map(|(i, e)| {
        let fence = fence(&e.html);
        let text = if e.text.is_empty() { String::new() } else { format!("Text: {}\n", e.text) };
        format!("## {} {} › {}\nURL: {}\nSelector: `{}`\nSize: {} × {}\n{text}{fence}html\n{}\n{fence}", i + 1, web::host(&e.url), element_title(e), e.url, e.selector, e.width, e.height, e.html)
    });
    format!("{ELEMENTS}{}", each.collect::<Vec<_>>().join("\n\n"))
}

/// What a chip calls an element: its words, else its tag and id or class; one line, as its heading needs.
pub(crate) fn element_title(e: &web::Element) -> String {
    let words = e.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = if words.is_empty() { e.name.as_str() } else { words.as_str() };
    title.chars().take(60).collect()
}

/// A fence longer than any run of backticks in `body`, which so can't close it.
fn fence(body: &str) -> String {
    let longest = body.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}
```

- `Sent` gains a field after `images`:

```rust
    /// Each picked element's heading: `<n> <host> › <title>`.
    pub(crate) elements: Vec<&'a str>,
```

- `split_attachments` becomes:

```rust
pub(crate) fn split_attachments(prompt: &str) -> Sent<'_> {
    let (rest, elements) = split_elements(prompt);
    let (rest, images) = split_block(rest, IMAGES);
    let (text, files) = split_block(rest, FILES);
    Sent { text, files, images, elements }
}

/// `prompt` without the picked elements that end it, and their headings; a `## ` inside a fence is HTML, not one.
fn split_elements(prompt: &str) -> (&str, Vec<&str>) {
    let (end, block) = match prompt.rfind(&format!("\n\n{ELEMENTS}")) {
        Some(i) => (i, i + 2),
        None if prompt.starts_with(ELEMENTS) => (0, 0),
        None => return (prompt, Vec::new()),
    };
    let (mut fenced, mut headings) = (None, Vec::new());
    for line in prompt[block + ELEMENTS.len()..].lines() {
        match fenced {
            Some(fence) if line == fence => fenced = None,
            Some(_) => {}
            None if line.starts_with("```") => fenced = Some(line.trim_end_matches(|c| c != '`')),
            None => headings.extend(line.strip_prefix("## ")),
        }
    }
    if headings.is_empty() { (prompt, Vec::new()) } else { (&prompt[..end], headings) }
}
```

- `chat/sticky.rs:264`: `let Sent { text, files, images, .. } = split_attachments(&prompt.text);`
- `chat/user_bubble.rs:211`: `let Sent { text, files, images, .. } = split_attachments(&prompt);` (Task 1.4 replaces `..` with `elements`).

**Step 4: Run to verify they pass.**

Run: `cd packages/desktop && cargo test -p pocket composer::tests`
Expected: PASS, including the old `split_attachments` tests.

### Task 1.4: The element chip, in the composer and in the sent bubble

**Files:**
- Create: `packages/desktop/crates/theme/assets/icons/pointer.svg`
- Modify: `packages/desktop/crates/theme/src/theme.rs:622-626` (the `ICONS` list)
- Create: `packages/desktop/crates/pocket/src/composer/element_card.rs`
- Modify: `packages/desktop/crates/pocket/src/composer.rs` (module list at the top)
- Modify: `packages/desktop/crates/pocket/src/chat/composer/attachments.rs`
- Modify: `packages/desktop/crates/pocket/src/chat/user_bubble.rs` (~line 211-216)

**Context:** The chip mirrors `composer/file_card.rs` (56px tall, glyph box + two lines) so it sits evenly beside file cards. Line 1: number in `MENTION` blue + host; line 2: title in `TEXT_3`. In the composer it has a tooltip (selector + first 300 chars of HTML) and the strip's hover × like files. In the sent bubble only the heading survives, so `heading_parts` splits `"1 github.com › Enter your email"` back. Icons are lucide SVGs embedded by name; `"cursor"` is taken (Cursor editor logo), so the new one is `"pointer"`. No render tests (CLAUDE.md); only `heading_parts` is tested.

**Step 1: Icon.** Create `packages/desktop/crates/theme/assets/icons/pointer.svg` (lucide `mouse-pointer-2`):

```svg
<!-- @license lucide-static v1.55.0 - ISC -->
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4.037 4.688a.495.495 0 0 1 .651-.651l16 6.5a.5.5 0 0 1-.063.947l-6.124 1.58a2 2 0 0 0-1.438 1.435l-1.579 6.126a.5.5 0 0 1-.947.063z"/></svg>
```

In `theme.rs` `ICONS`, insert `"pointer",` between `"plus",` and `"priority-high",`.

**Step 2: Failing test.** Create `packages/desktop/crates/pocket/src/composer/element_card.rs` with just:

```rust
#[cfg(test)]
mod tests {
    use super::heading_parts;

    #[test]
    fn a_sent_elements_heading_gives_back_its_number_host_and_title() {
        assert_eq!(heading_parts("1 github.com › Enter your email"), ("1", "github.com", "Enter your email"));
        assert_eq!(heading_parts("2 a.dev › a › b"), ("2", "a.dev", "a › b"));
        assert_eq!(heading_parts("odd"), ("", "", "odd"));
    }
}
```

Add `pub(crate) mod element_card;` to the module list at the top of `composer.rs` (alphabetical, after `effort_menu`).

Run: `cd packages/desktop && cargo test -p pocket element_card`
Expected: FAIL to compile: `heading_parts` not found.

**Step 3: Implement** above the tests in `element_card.rs`:

```rust
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;

/// An element picked on a page, as tall as a file card beside it: its number and host over what it says.
pub(crate) fn element_card(id: impl Into<ElementId>, number: &str, host: &str, title: &str, tip: Option<String>) -> Stateful<Div> {
    let glyph = div().size(px(34.)).flex_none().flex().items_center().justify_center().rounded(px(8.)).bg(FILL_2).child(icon("pointer", 16., TEXT_2));
    let top = div()
        .flex()
        .gap(px(5.))
        .text_size(px(13.))
        .line_height(px(17.))
        .font_weight(FontWeight::MEDIUM)
        .child(div().flex_none().text_color(MENTION).child(number.to_string()))
        .child(div().min_w_0().truncate().text_color(TEXT).child(host.to_string()));
    let words = div().min_w_0().flex().flex_col().gap(px(1.)).child(top).child(div().truncate().text_size(px(11.5)).line_height(px(15.)).text_color(TEXT_3).child(title.to_string()));
    let card = div()
        .id(id)
        .h(px(56.))
        .max_w(px(240.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(10.))
        .pl(px(10.))
        .pr(px(14.))
        .rounded(px(10.))
        .bg(FILL_1)
        .border_1()
        .border_color(hairline(0.10))
        .child(glyph)
        .child(words);
    match tip {
        Some(tip) => card.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)),
        None => card,
    }
}

/// A sent element's heading, `<n> <host> › <title>`, as its parts.
pub(crate) fn heading_parts(heading: &str) -> (&str, &str, &str) {
    let Some((number, rest)) = heading.split_once(' ') else { return ("", "", heading) };
    match rest.split_once(" › ") {
        Some((host, title)) => (number, host, title),
        None => ("", "", heading),
    }
}
```

**Step 4: Composer strip.** In `packages/desktop/crates/pocket/src/chat/composer/attachments.rs`:

- Imports: add `use crate::composer::element_card::element_card;` and `use crate::composer::element_title;`.
- After the `files` iterator:

```rust
    let elements = draft.elements.iter().enumerate().map(|(i, e)| {
        let tip = format!("{}\n\n{}", e.selector, e.html.chars().take(300).collect::<String>());
        let card = element_card(("chat-element", i), &(i + 1).to_string(), web::host(&e.url), &element_title(e), Some(tip));
        div().group(SharedString::from(format!("chat-element-remove:{i}"))).relative().flex_none().child(card).child(remove("chat-element-remove", |d, i| _ = d.elements.remove(i), i))
    });
```

- Final line: `.children(images).children(files).children(elements)`.

**Step 5: Sent bubble.** In `packages/desktop/crates/pocket/src/chat/user_bubble.rs` `bubble` (~line 211):

```rust
    let Sent { text, files, images, elements } = split_attachments(&prompt);
    let text = text.to_string();
    let attached = (!images.is_empty() || !files.is_empty() || !elements.is_empty()).then(|| {
        let tile = |path: &str| div().size(px(56.)).flex_none().rounded(px(8.)).border_1().border_color(hairline(0.10)).child(img(PathBuf::from(path)).size(px(54.)).rounded(px(7.)).object_fit(ObjectFit::Cover));
        let card = |(i, path): (usize, &str)| file_card(SharedString::from(format!("{id}-file-{i}")), Path::new(path));
        let element = |(i, heading): (usize, &str)| {
            let (number, host, title) = heading_parts(heading);
            element_card(SharedString::from(format!("{id}-element-{i}")), number, host, title, None)
        };
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .when(!text.is_empty(), |d| d.mt(px(8.)))
            .mb(px(3.))
            .children(images.into_iter().map(tile))
            .children(files.into_iter().enumerate().map(card))
            .children(elements.into_iter().enumerate().map(element))
    });
```

Add `use crate::composer::element_card::{element_card, heading_parts};` to its imports.

**Step 6: Verify.**

Run: `cd packages/desktop && cargo test -p pocket element_card && cargo build -p pocket`
Expected: test PASS; builds (warnings about `Draft.elements` never pushed are fine until Task 1.5).

### Task 1.5: Picking from the browser tab into the chat

**Files:**
- Create: `packages/desktop/crates/pocket/src/browser/pick.rs`
- Modify: `packages/desktop/crates/pocket/src/browser.rs` (module list; `Browser` struct ~line 19; `on_page` ~line 255)
- Modify: `packages/desktop/crates/pocket/src/browser/view.rs`
- Modify: `packages/desktop/crates/pocket/src/chat/composer.rs` (`chat_draft` visibility ~line 603; new `chat_draft_mut` after `chat_add_text` ~line 633)
- Modify: `packages/desktop/crates/pocket/src/add_to_chat.rs:286` (`flash_added` visibility)

**Context:**
- Browser tab state is `Browser` in `browser.rs`; add `picking` and `picked_into` (the agent whose draft took the last pick; its elements of this page show as badges while picking).
- Choosing the agent copies `add_to_input` in `add_to_chat.rs`: `chat_choices()` → `chat_target(&choices)`, else `default_target(None, self.session(), &choices)` so the "ask" setting still has a target. `self.route(&terminal)` says how text reaches it: `Route::Chat` → push to that chat's draft (held in the input, or parked — `chat_draft_mut`), `Route::Paste` → paste the block into its TUI with `term::paste_bytes(.., true)`, `Route::Blocked` → error.
- `self.flash_added(provider, cx)` shows the existing "added to <agent>" toast; make it `pub(crate)`.
- Badges are computed in render (`browser_view`), cheap: `Page::set_marks` does nothing unless they changed. `marks` is a free function so it is testable without GPUI.
- Accept `Picked` only while the tab `picking` (a page could post a fake pick).

**Step 1: Failing test.** Create `packages/desktop/crates/pocket/src/browser/pick.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::marks;

    fn at(url: &str, selector: &str) -> web::Element {
        web::Element { url: url.into(), selector: selector.into(), name: String::new(), text: String::new(), html: String::new(), width: 0, height: 0 }
    }

    #[test]
    fn a_page_badges_only_its_own_elements_numbered_as_their_chips() {
        let elements = [at("https://a.dev/", "#one"), at("https://b.dev/", "#two"), at("https://a.dev/", "#three")];
        assert_eq!(marks("https://a.dev/", &elements), vec![(1, "#one".to_string()), (3, "#three".to_string())]);
        assert_eq!(marks("https://c.dev/", &elements), vec![]);
    }
}
```

Add `mod pick;` to `browser.rs` after `pub(crate) mod view;`.

Run: `cd packages/desktop && cargo test -p pocket browser::pick`
Expected: FAIL to compile: `marks` not found.

**Step 2: Implement `pick.rs`** above the tests:

```rust
use super::Browser;
use crate::add_to_chat::default_target;
use crate::chat::Route;
use crate::composer::elements_block;
use crate::desktop::Desktop;
use gpui_kit::*;

/// The badges a page at `url` shows: its elements among `elements`, numbered as their chips are.
pub(crate) fn marks(url: &str, elements: &[web::Element]) -> Vec<(usize, String)> {
    elements.iter().enumerate().filter(|(_, e)| e.url == url).map(|(i, e)| (i + 1, e.selector.clone())).collect()
}

impl Desktop {
    pub(super) fn toggle_pick(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.browsers.tabs.get_mut(&id) else { return };
        let Some(page) = &b.page else { return };
        let on = !b.picking;
        page.set_picking(on);
        if on {
            // Esc reaches the script only while the page holds the keys.
            page.take_keys();
        }
        b.picking = on;
        window.focus(&self.browsers.focus, cx);
        cx.notify();
    }

    /// While `b` picks, the elements of its page already in the chat its picks went to.
    pub(super) fn pick_marks(&self, b: &Browser, cx: &App) -> Vec<(usize, String)> {
        let draft = b.picked_into.as_deref().filter(|_| b.picking).and_then(|agent| self.chat_draft(agent, cx).1);
        draft.map_or_else(Vec::new, |d| marks(&b.url, &d.elements))
    }

    /// Puts an element picked on tab `id` in the chat a quote would go to; a TUI agent has it pasted.
    pub(super) fn pick_into_chat(&mut self, id: u64, element: web::Element, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let choices = self.chat_choices();
        let target = self.chat_target(&choices).or_else(|| default_target(None, self.session(), &choices));
        let Some((agent, terminal, provider)) = target.and_then(|t| self.agents.get(&t).map(|a| (t.clone(), a.terminal_id.clone(), a.provider.clone()))) else {
            self.error = Some("No agent here to send the element to.".into());
            return cx.notify();
        };
        match self.route(&terminal) {
            Route::Chat => {
                self.chat_draft_mut(&agent).elements.push(element);
                if let Some(b) = self.browsers.tabs.get_mut(&id) {
                    b.picked_into = Some(agent);
                }
            }
            Route::Paste => self.send_input(&terminal, &term::paste_bytes(&elements_block(&[element]), true), cx),
            Route::Blocked => {
                self.error = Some("This agent can't take pasted text right now.".into());
                return cx.notify();
            }
        }
        self.flash_added(provider, cx);
        cx.notify();
    }
}
```

If the borrow checker rejects `target.and_then(|t| self.agents.get(&t)...)` (closure capturing `t` by move while `t.clone()` is inside), restructure as `let target = ...; let Some(t) = target else { ... }; let Some(a) = self.agents.get(&t) else { ... };` — keep the same behaviour.

**Step 3: Supporting changes.**

- `chat/composer.rs`: make `chat_draft` `pub(crate)` (was `pub(super)`), and add after `chat_add_text`:

```rust
    /// `agent`'s draft to attach to: the input's while its chat holds it, else the one parked for it.
    pub(crate) fn chat_draft_mut(&mut self, agent: &str) -> &mut Draft {
        if self.chats.drafts.owner() == Some(agent) {
            return &mut self.chats.composer.draft;
        }
        &mut self.chats.drafts.parked_mut(agent).1
    }
```

- `add_to_chat.rs`: `fn flash_added` → `pub(crate) fn flash_added`.

- `browser.rs` `Browser` struct, after `shot`:

```rust
    /// Whether a click on the page picks the element under it for a chat.
    pub(crate) picking: bool,
    /// The agent whose chat took this page's last pick; its elements of this page show as badges while picking.
    pub(crate) picked_into: Option<String>,
```

- `browser.rs` `on_page`, replace the `Finished` arm and add two arms before `Pressed`:

```rust
            web::Event::Finished(url) => {
                (b.loading, b.url) = (false, url);
                if let Some(page) = &b.page {
                    page.reapply();
                }
            }
```

```rust
            // A page's own script can post a pick; only one made while picking counts.
            web::Event::Picked(element) => {
                if b.picking {
                    self.pick_into_chat(id, element, cx);
                }
            }
            web::Event::PickEnded => {
                b.picking = false;
                if let Some(page) = &b.page {
                    page.set_picking(false);
                }
            }
```

**Step 4: Toolbar button and badges** in `browser/view.rs` `browser_view`:

- After `let forward = ...;` add:

```rust
        let picking = b.picking;
        let can_pick = b.page.is_some();
        if let Some(page) = &b.page {
            page.set_marks(&self.pick_marks(b, cx));
        }
```

- In `bar`, insert before the `browser-external` child:

```rust
            .child(
                ui::icon_button("browser-pick", "pointer")
                    .when(picking, |d| d.bg(FILL_3))
                    .when(!can_pick, |d| d.opacity(0.35))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.toggle_pick(id, window, cx))),
            )
```

(No tooltip: it would draw under the page, which covers GPUI.)

**Step 5: Verify.**

Run: `cd packages/desktop && cargo test -p pocket browser::pick && cargo build --workspace && cargo clippy --workspace --all-targets`
Expected: test PASS; build OK; no new clippy warnings (compare to `git stash`-free baseline by checking warnings only in touched files).

### Task 1.6: Full check and the dev app

**Files:** none (verification only).

**Step 1:** From `packages/desktop`:

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: all green, no new warnings.

**Step 2:** From the repo root:

Run: `scripts/bundle-dev.sh --no-open`
Expected: ends with `bundle-dev: built .../packages/desktop/target/release/Anywhere Dev.app`.

**Step 3: Manual check** in Anywhere Dev.app (open it with `scripts/bundle-dev.sh` or `open "packages/desktop/target/release/Anywhere Dev.app"`): in a worktree with a chat session, open a browser tab on `https://github.com`, press the pointer button (it shades), hover the email field (blue box, label `input… 420 × 41`), click it: chip `1 github.com / Enter your email` appears over the chat input, badge `1` on the field, the "added" toast shows. Pick a second element → chip/badge `2`. Remove chip 1 → badge numbers follow. Esc in the page → picking off, button unshaded, badges gone. Send → bubble shows the element chips; the agent's prompt holds the `# Selected elements` block.
