//! A Finder-like browser of a host's folders, opened over Add project or from the new-session page to pick a project's folder there.

pub(crate) mod nav;

use crate::desktop::Desktop;
use crate::explorer::preview::size;
use crate::modals::add_project::host_folder::child;
use crate::util::folder;
use git::Exec as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use hosts::remote;
use nav::{Ask, Finder, Hint, PREVIEW_MAX, Preview, Repo, Row, ask, branch_of, remote_of, root_of};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;
use theme::*;
use ui::Variant;

const ROW: f32 = 26.;
const LINE: f32 = 19.;
/// Columns shown at once; earlier ones are a click away in the path bar.
const COLUMNS: usize = 3;

/// A column as drawn: its place in the chain, its folder, the name picked in it, and its rows once listed.
type Column = (usize, String, Option<String>, Option<Result<Vec<Row>, String>>);

/// Who opened the finder, and so where the folder chosen goes.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Opener {
    AddProject,
    NewSession,
    Sidebar,
    /// Clone from URL picking the folder to clone into.
    CloneInto,
}

pub(crate) struct HostFinder {
    pub finder: Option<Finder>,
    opener: Opener,
    focus: FocusHandle,
    scrolls: HashMap<String, UniformListScrollHandle>,
    preview_scroll: UniformListScrollHandle,
    host_menu: bool,
}

impl HostFinder {
    pub fn new(cx: &mut App) -> Self {
        Self { finder: None, opener: Opener::AddProject, focus: cx.focus_handle(), scrolls: HashMap::new(), preview_scroll: UniformListScrollHandle::new(), host_menu: false }
    }

    fn scroll(&mut self, dir: &str) -> UniformListScrollHandle {
        self.scrolls.entry(dir.to_string()).or_default().clone()
    }
}

impl Desktop {
    /// Opens the finder on `host`, at `start` or the host's home.
    pub(crate) fn open_host_finder(&mut self, host: String, start: Option<String>, opener: Opener, window: &mut Window, cx: &mut Context<Self>) {
        let hf = &mut self.host_finder;
        hf.finder = Some(Finder::new(&host));
        hf.opener = opener;
        hf.host_menu = false;
        hf.scrolls.clear();
        window.focus(&hf.focus, cx);
        cx.notify();
        let Some(client) = remote::connected(&host) else { return };
        cx.spawn(async move |this, cx| {
            let home = cx.background_executor().spawn(async move { client.status().map(|s| s.user_home.replace('\\', "/")) }).await;
            this.update(cx, |d, cx| {
                let Some(f) = d.host_finder.finder.as_mut().filter(|f| f.host == host) else { return };
                match home {
                    Ok(home) => f.homed(home, start),
                    Err(e) => f.error = Some(e.to_string()),
                }
                d.finder_fetch(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Closes the finder, leaving what opened it as it was. False when it wasn't open.
    pub(crate) fn close_host_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.host_finder.finder.take().is_none() {
            return false;
        }
        match self.host_finder.opener {
            Opener::AddProject | Opener::Sidebar => window.focus(&self.root, cx),
            Opener::NewSession => self.focus_prompt(window, cx),
            Opener::CloneInto => self.clone_form.url.update(cx, |s, cx| s.focus(window, cx)),
        }
        true
    }

    /// Asks the host, off the UI thread, for whatever the finder now shows but hasn't got.
    fn finder_fetch(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.host_finder.finder.as_mut() else { return };
        let Some(client) = remote::connected(&f.host) else { return };
        let host = f.host.clone();
        for dir in f.wanted() {
            let (client, host) = (client.clone(), host.clone());
            cx.spawn(async move |this, cx| {
                let asked = dir.clone();
                let listed = cx.background_executor().spawn(async move { client.list(&asked).map_err(|e| e.to_string()) }).await;
                this.update(cx, |d, cx| {
                    if d.host_finder.finder.as_mut().is_some_and(|f| f.listed(&host, &dir, listed)) {
                        d.finder_fetch(cx);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        if let Some(path) = f.preview_wanted() {
            let (client, host) = (client.clone(), host.clone());
            cx.spawn(async move |this, cx| {
                let asked = path.clone();
                let read = cx.background_executor().spawn(async move { client.read(&asked, PREVIEW_MAX).map_err(|e| e.to_string()) }).await;
                this.update(cx, |d, cx| {
                    if let Some(f) = d.host_finder.finder.as_mut() {
                        f.previewed(&host, &path, read);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        if let Some(dir) = f.repo_wanted() {
            cx.spawn(async move |this, cx| {
                let at = dir.clone();
                let repo = cx
                    .background_executor()
                    .spawn(async move {
                        let text = |name: &str| client.read(&child(&at, name), 64 * 1024).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
                        Repo { branch: text(".git/HEAD").as_deref().and_then(branch_of), remote: text(".git/config").as_deref().and_then(remote_of) }
                    })
                    .await;
                this.update(cx, |d, cx| {
                    if let Some(f) = d.host_finder.finder.as_mut() {
                        f.repo_read(&host, &dir, repo);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    pub(crate) fn finder_select(&mut self, column: String, name: String, cx: &mut Context<Self>) {
        let Some(f) = self.host_finder.finder.as_mut() else { return };
        f.select(&column, &name);
        self.finder_fetch(cx);
        cx.notify();
    }

    fn finder_go(&mut self, dir: String, cx: &mut Context<Self>) {
        let Some(f) = self.host_finder.finder.as_mut() else { return };
        f.go(dir);
        self.finder_fetch(cx);
        cx.notify();
    }

    pub(crate) fn finder_toggle_hidden(&mut self, cx: &mut Context<Self>) {
        if let Some(f) = self.host_finder.finder.as_mut() {
            f.toggle_hidden();
            cx.notify();
        }
    }

    /// How the footer reads for what is picked now.
    fn finder_ask(&self, cx: &App) -> Option<Ask> {
        let f = self.host_finder.finder.as_ref()?;
        let taken = self.projects().contains(&hosts::place(&f.host, &f.dir));
        let clone = (self.host_finder.opener == Opener::CloneInto).then(|| self.clone_repo_name(cx));
        Some(ask(clone.as_deref(), remote::connected(&f.host).is_some(), &f.dir, taken, f.is_repo(&f.dir)))
    }

    /// The picked folder, when the footer allows choosing it.
    fn finder_target(&self, cx: &App) -> Option<(String, String)> {
        let f = self.host_finder.finder.as_ref()?;
        self.finder_ask(cx)?.ready.then(|| (f.host.clone(), f.dir.clone()))
    }

    pub(crate) fn finder_choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((host, dir)) = self.finder_target(cx) else { return };
        self.close_host_finder(window, cx);
        match self.host_finder.opener {
            Opener::AddProject => self.pick_host_folder(host, dir, window, cx),
            Opener::NewSession => self.pick_new_project(hosts::place(&host, &dir), window, cx),
            Opener::Sidebar => self.open_chosen_folder(hosts::place(&host, &dir), window, cx),
            Opener::CloneInto => self.clone_into(host, dir, window, cx),
        }
        cx.notify();
    }

    fn on_finder_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        if key == "enter" {
            self.finder_choose(window, cx);
            return cx.stop_propagation();
        }
        if matches!(key, "up" | "down" | "left" | "right") {
            self.finder_step(key, cx);
            cx.stop_propagation();
        }
    }

    /// Moves the pick for an arrow key, keeping it in view.
    pub(crate) fn finder_step(&mut self, key: &str, cx: &mut Context<Self>) {
        let hf = &mut self.host_finder;
        let Some(f) = hf.finder.as_mut() else { return };
        if !f.step(key) {
            return;
        }
        if let Some((column, at)) = f.cursor() {
            hf.scroll(&column).scroll_to_item(at, ScrollStrategy::Center);
        }
        self.finder_fetch(cx);
        cx.notify();
    }

    /// The finder, when it's open.
    pub(crate) fn host_finder_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let f = self.host_finder.finder.as_ref()?;
        let (host, hidden, error, homed, crumbs) = (f.host.clone(), f.hidden, f.error.clone(), f.home.is_some(), f.crumbs());
        let name = self.host_name(&host);
        let online = remote::connected(&host).is_some();
        let max_h = (window.viewport_size().height - px(96.)).min(px(620.));
        let close = ui::icon_button("finder-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.close_host_finder(window, cx);
        }));
        let eye = ui::icon_button("finder-hidden", if hidden { "eye" } else { "eye-off" })
            .tooltip(move |window, cx| Tooltip::new(if hidden { "Hide dot files" } else { "Show dot files" }).build(window, cx))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.finder_toggle_hidden(cx)));
        let crumbs = div().flex_1().min_w_0().flex().items_center().overflow_hidden().font_family(MONO).text_size(px(12.5)).text_color(TEXT_2).children(crumbs.into_iter().enumerate().map(|(i, (label, dir))| {
            div()
                .flex()
                .items_center()
                .when(i > 0, |d| d.child(div().px(px(2.)).text_color(TEXT_5).child("/")))
                .child(div().id(("finder-crumb", i)).px(px(5.)).py(px(2.)).rounded(px(5.)).cursor_pointer().hover(|s| s.bg(FILL_3).text_color(TEXT)).child(label).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.finder_go(dir.clone(), cx))))
        }));
        let header = div()
            .px(px(14.))
            .h(px(52.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(div().flex_none().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(self.finder_ask(cx).map_or("Choose a folder", |a| a.title)))
            .child(crumbs)
            .child(eye)
            .child(self.finder_host_switch(&name, online, cx))
            .child(close);
        let body = match (error, homed) {
            _ if !online => self.finder_note("server", format!("{name} is offline"), "Its folders show once its pocketd answers again."),
            (Some(e), _) => self.finder_note("warning", format!("Couldn\u{2019}t reach {name}"), e),
            (None, false) => self.finder_note("spinner", format!("Connecting to {name}\u{2026}"), ""),
            (None, true) => div().flex_1().min_h_0().flex().child(self.finder_places(cx)).child(self.finder_columns(cx)).child(self.finder_detail()),
        };
        let panel = ui::pop(div().w(px(980.)).max_w(window.viewport_size().width - px(48.)).h(max_h).flex().flex_col().overflow_hidden())
            .rounded(px(R_DIALOG))
            .occlude()
            .track_focus(&self.host_finder.focus)
            .capture_key_down(cx.listener(Self::on_finder_key))
            .child(header)
            .child(body)
            .child(self.finder_footer(online, cx));
        let backdrop = ui::backdrop("finder-backdrop", 0x2e).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.close_host_finder(window, cx);
        }));
        let enter = Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint());
        Some(
            div()
                .absolute()
                .inset_0()
                .child(backdrop)
                .child(div().absolute().inset_0().flex().items_center().justify_center().child(panel).with_animation("finder-in", enter, |d, t| d.opacity(t)))
                .into_any_element(),
        )
    }

    fn finder_note(&self, glyph: &str, title: String, detail: impl Into<SharedString>) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .child(icon(glyph, 22., TEXT_4))
            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title))
            .child(div().text_size(px(12.5)).text_color(TEXT_3).child(detail.into()))
    }

    fn finder_host_switch(&self, name: &str, online: bool, cx: &mut Context<Self>) -> Div {
        let open = self.host_finder.host_menu;
        let button = ui::button("finder-host", Variant::Secondary, Some("server"), div().flex().items_center().gap(px(6.)).child(name.to_string()).child(ui::dot(6., if online { SUCCESS } else { TEXT_5 })).child(icon("chevron-down", 12., TEXT_3)))
            .h(px(28.))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.host_finder.host_menu = !this.host_finder.host_menu;
                cx.notify();
            }));
        let picked = self.host_finder.finder.as_ref().map(|f| f.host.clone());
        let opener = self.host_finder.opener;
        let menu = open.then(|| {
            let rows = self.fleet.hosts.list.iter().enumerate().map(|(i, h)| {
                let up = remote::connected(&h.id).is_some();
                let id = h.id.clone();
                div()
                    .id(("finder-host-row", i))
                    .h(px(32.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .rounded(px(7.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(|s| s.bg(FILL_3))
                    .child(div().w(px(14.)).when(picked.as_deref() == Some(h.id.as_str()), |d| d.child(icon("check", 14., TEXT))))
                    .child(icon("server", 14., TEXT_3))
                    .child(div().flex_1().truncate().when(!up, |d| d.text_color(TEXT_3)).child(h.name.clone()))
                    .child(div().text_size(px(11.5)).text_color(TEXT_3).child(if up { "" } else { "Offline" }))
                    .child(ui::dot(6., if up { SUCCESS } else { TEXT_5 }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_host_finder(id.clone(), None, opener, window, cx)))
            });
            ui::dropdown(34., ui::pop(div().w(px(240.)).p(px(5.)).flex().flex_col()).occlude().children(rows))
        });
        div().relative().child(button).children(menu)
    }

    fn finder_places(&self, cx: &mut Context<Self>) -> Div {
        let Some(f) = self.host_finder.finder.as_ref() else { return div() };
        let home = f.home.clone().unwrap_or_default();
        let projects: Vec<String> = self.projects().iter().filter_map(|k| match hosts::split(k) {
            (Some(id), path) if id == f.host => Some(path.to_string()),
            _ => None,
        }).collect();
        let place = |id: (&'static str, usize), glyph: &'static str, label: String, dir: String| {
            let on = f.dir == dir && f.file.is_none();
            div()
                .id(id)
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(7.))
                .text_size(px(13.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_2))
                .when(on, |d| d.bg(FILL_4))
                .child(icon(glyph, 14., TEXT_3))
                .child(div().truncate().child(label))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.finder_go(dir.clone(), cx)))
        };
        let heading = |label: String| div().px(px(8.)).pt(px(10.)).pb(px(4.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(label.to_uppercase());
        div()
            .w(px(180.))
            .flex_none()
            .p(px(8.))
            .flex()
            .flex_col()
            .bg(FILL_1)
            .border_r(px(0.5))
            .border_color(SEPARATOR)
            .child(heading(self.host_name(&f.host)))
            .child(place(("finder-place", 0), "laptop", "Home".into(), home.clone()))
            .child(place(("finder-place", 1), "server", "Root".into(), root_of(&home)))
            .when(!projects.is_empty(), |d| d.child(heading("Projects here".into())))
            .children(projects.into_iter().enumerate().map(|(i, p)| place(("finder-project", i), "folder", folder(&p).to_string(), p)))
    }

    fn finder_columns(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(f) = self.host_finder.finder.as_ref() else { return div() };
        let chain = f.chain();
        let host = f.host.clone();
        let projects: Rc<HashSet<String>> = Rc::new(self.projects().into_iter().collect());
        let shown = if f.file.is_some() || f.is_repo(&f.dir) { COLUMNS - 1 } else { COLUMNS };
        let first = chain.len().saturating_sub(shown);
        let current = if f.file.is_some() { chain.len().saturating_sub(1) } else { chain.len().saturating_sub(2) };
        let columns: Vec<Column> = (first..chain.len())
            .map(|i| (i, chain[i].clone(), f.picked_in(&chain, i), f.listing(&chain[i]).map(|l| l.as_ref().map(|l| l.shown.clone()).map_err(Clone::clone))))
            .collect();
        let mut out = div().flex_none().flex();
        for (i, dir, picked, listing) in columns {
            let column = div().w(px(220.)).h_full().flex_none().border_r(px(0.5)).border_color(SEPARATOR);
            let rows = match listing {
                None => {
                    out = out.child(column.child(div().p(px(14.)).text_size(px(12.5)).text_color(TEXT_4).child("Listing\u{2026}")));
                    continue;
                }
                Some(Err(e)) => {
                    out = out.child(column.child(div().p(px(14.)).text_size(px(12.5)).text_color(FAILED).child(e)));
                    continue;
                }
                Some(Ok(rows)) if rows.is_empty() => {
                    out = out.child(column.child(div().p(px(14.)).text_size(px(12.5)).text_color(TEXT_4).child("Empty folder")));
                    continue;
                }
                Some(Ok(rows)) => Rc::new(rows),
            };
            let scroll = self.host_finder.scroll(&dir);
            let (projects, host) = (projects.clone(), host.clone());
            let here = i == current;
            let list = uniform_list(
                ("finder-col", i),
                rows.len(),
                cx.processor(move |_, range: Range<usize>, _, cx| {
                    range
                        .map(|r| {
                            let row = &rows[r];
                            let on = picked.as_deref() == Some(row.name.as_str());
                            let project = row.dir && projects.contains(&hosts::place(&host, &child(&dir, &row.name)));
                            let (column, name) = (dir.clone(), row.name.clone());
                            div()
                                .id(("finder-row", r))
                                .w_full()
                                .h(px(ROW))
                                .px(px(5.))
                                .child(
                                    div()
                                        .size_full()
                                        .px(px(7.))
                                        .flex()
                                        .items_center()
                                        .gap(px(7.))
                                        .rounded(px(6.))
                                        .text_size(px(13.))
                                        .hover(|s| s.bg(FILL_2))
                                        .when(on, |d| d.bg(if here { ACCENT_BG } else { FILL_4 }))
                                        .when(row.ignored, |d| d.opacity(0.5))
                                        .child(icon(if row.dir { "folder" } else { "file" }, 14., TEXT_3))
                                        .child(div().flex_1().min_w_0().truncate().child(row.name.clone()))
                                        .when(project, |d| d.child(div().px(px(5.)).rounded(px(4.)).bg(FILL_3).text_size(px(10.5)).text_color(TEXT_3).child("Project")))
                                        .when(row.dir, |d| d.child(icon("chevron-right", 12., TEXT_4))),
                                )
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.finder_select(column.clone(), name.clone(), cx)))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&scroll)
            .size_full()
            .py(px(4.));
            out = out.child(column.child(list));
        }
        out
    }

    /// What's picked, beside the columns: a file's text, a repository's branch and remote, or a folder's count.
    fn finder_detail(&self) -> Div {
        let Some(f) = self.host_finder.finder.as_ref() else { return div() };
        let pane = div().flex_1().min_w(px(240.)).p(px(16.)).flex().flex_col().gap(px(12.)).overflow_hidden();
        let head = |glyph: &str, title: String, sub: String| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().size(px(36.)).flex_none().flex().items_center().justify_center().rounded(px(9.)).bg(FILL_3).child(icon(glyph, 18., TEXT_2)))
                .child(div().min_w_0().flex().flex_col().child(div().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title)).child(div().truncate().text_size(px(12.)).text_color(TEXT_3).child(sub)))
        };
        if let Some(file) = &f.file {
            let row_size = f.listing(&f.dir).and_then(|l| l.as_ref().ok()).and_then(|l| l.shown.iter().find(|r| &r.name == file)).map_or(0, |r| r.size);
            let body = match f.preview.as_ref().map(|(_, p)| p) {
                Some(Preview::Text(lines)) => {
                    let lines = Rc::new(lines.clone());
                    let list = uniform_list(
                        "finder-preview",
                        lines.len(),
                        preview_rows(lines),
                    )
                    .track_scroll(&self.host_finder.preview_scroll)
                    .size_full()
                    .py(px(8.));
                    div().flex_1().min_h_0().rounded(px(9.)).bg(FILL_1).border(px(0.5)).border_color(SEPARATOR).child(list)
                }
                Some(Preview::TooBig(_)) => note("Too big to preview."),
                Some(Preview::Binary) => note("No preview for this file."),
                Some(Preview::Failed(e)) => note(e),
                Some(Preview::Loading) | None => note("Reading\u{2026}"),
            };
            return pane.child(head("file", file.clone(), size(row_size as usize))).child(body);
        }
        if f.dir.is_empty() {
            return pane;
        }
        let title = folder(&f.dir).to_string();
        let title = if title.is_empty() { f.dir.clone() } else { title };
        if f.is_repo(&f.dir) {
            let repo = f.repo.as_ref().filter(|(d, _)| *d == f.dir).and_then(|(_, r)| r.clone());
            let fact = |k: &str, v: String| div().flex().gap(px(14.)).text_size(px(12.5)).child(div().w(px(56.)).flex_none().text_color(TEXT_3).child(k.to_string())).child(div().min_w_0().truncate().font_family(MONO).child(v));
            let (branch, remote) = match repo {
                Some(r) => (r.branch.unwrap_or_else(|| "detached".into()), r.remote.unwrap_or_else(|| "none".into())),
                None => ("\u{2026}".into(), "\u{2026}".into()),
            };
            return pane.child(head("branch", title, "Git repository".into())).child(div().flex().flex_col().gap(px(6.)).child(fact("Branch", branch)).child(fact("Remote", remote)).child(fact("Path", f.pretty(&f.dir))));
        }
        let count = match f.listing(&f.dir) {
            Some(Ok(l)) => format!("{} items", l.shown.len()),
            Some(Err(_)) => "Couldn\u{2019}t list".into(),
            None => "Listing\u{2026}".into(),
        };
        pane.child(head("folder", title, count))
    }

    fn finder_footer(&self, online: bool, cx: &mut Context<Self>) -> Div {
        let f = self.host_finder.finder.as_ref();
        let ask = self.finder_ask(cx);
        let path = f.filter(|f| online && !f.dir.is_empty()).map(|f| f.pretty(&f.dir)).unwrap_or_default();
        let hint = match (f, ask.as_ref().and_then(|a| a.hint.as_ref())) {
            (Some(_), Some(Hint::Taken)) => Some(("warning", "Already a project".to_string(), WAITING_TEXT)),
            (Some(_), Some(Hint::Repo)) => Some(("check", "Git repository".to_string(), SUCCESS)),
            (Some(_), Some(Hint::Plain)) => Some(("folder", "Folder, without git".to_string(), TEXT_3)),
            (Some(f), Some(Hint::Clones(name))) => {
                let into = if name.is_empty() { f.dir.clone() } else { child(&f.dir, name) };
                Some(("folder", format!("Clones into {}", f.pretty(&into)), TEXT_3))
            }
            _ => None,
        };
        let ready = ask.as_ref().is_some_and(|a| a.ready);
        let primary = ask.map_or("Use this folder", |a| a.primary);
        let cancel = ui::button("finder-cancel", Variant::Secondary, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.close_host_finder(window, cx);
        }));
        let choose = ui::button("finder-choose", Variant::Primary, None, primary)
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.finder_choose(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        div()
            .px(px(14.))
            .h(px(56.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().truncate().font_family(MONO).text_size(px(12.5)).child(path))
                    .children(hint.map(|(glyph, text, tint)| div().flex().items_center().gap(px(5.)).text_size(px(12.)).text_color(tint).child(icon(glyph, 12., tint)).child(text))),
            )
            .child(cancel)
            .child(choose)
    }
}

fn note(text: impl Into<SharedString>) -> Div {
    div().text_size(px(12.5)).text_color(TEXT_3).child(text.into())
}

/// A preview's lines as `uniform_list` draws them: only those in view.
fn preview_rows(lines: Rc<Vec<String>>) -> impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<Div> + 'static {
    move |range, _, _| {
        range
            .map(|i| {
                div()
                    .w_full()
                    .h(px(LINE))
                    .flex()
                    .font_family(MONO)
                    .text_size(px(12.))
                    .child(div().w(px(40.)).flex_none().pr(px(12.)).text_right().text_color(TEXT_5).child((i + 1).to_string()))
                    .child(div().flex_1().min_w_0().whitespace_nowrap().overflow_hidden().text_color(TEXT_2).child(lines[i].clone()))
            })
            .collect()
    }
}
