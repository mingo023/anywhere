use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Screen};
use crate::status::{Card, Status};
use crate::util::basename;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::*;
use std::cmp::Reverse;
use theme::*;
use ui::{self, dot};

#[derive(Clone)]
pub enum Pick {
    Session(String),
    File(String),
    New,
    Split,
    Next,
}

#[derive(Clone)]
enum Lead {
    Waiting,
    Running,
    Provider(String),
    File,
    Icon(&'static str),
}

#[derive(Clone)]
pub struct Entry {
    pub pick: Pick,
    lead: Lead,
    title: String,
    detail: String,
    keys: Option<&'static str>,
}

fn status_word(s: Status) -> &'static str {
    match s {
        Status::NeedsYou => "needs you",
        Status::Failed => "failed",
        Status::Done => "done",
        Status::Working => "working",
        Status::Idle => "idle",
    }
}

fn hint(keys: &str, label: &str) -> Div {
    div().flex().items_center().gap(px(4.)).child(div().text_color(rgba(TEXT_3)).child(keys.to_string())).child(label.to_string())
}

pub struct PaletteState {
    pub(crate) filter: Entity<InputState>,
    pub(crate) ix: usize,
    pub(crate) all: bool,
    pub(crate) files: Vec<String>,
}

impl PaletteState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions, files and actions…"));
        let subs = vec![cx.subscribe(&filter, |this, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.palette.ix = 0;
            }
            cx.notify()
        })];
        (Self { filter, ix: 0, all: true, files: Vec::new() }, subs)
    }
}

impl Desktop {
    pub fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.ix = 0;
        self.palette.filter.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        let Some(root) = self.explore_root() else { return };
        let task = cx.background_executor().spawn(async move { git::ls_files(&root) });
        cx.spawn(async move |this, cx| {
            let files = task.await;
            this.update(cx, |d, cx| {
                d.palette.files = files;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn palette_sections(&self, cx: &App) -> Vec<(&'static str, Vec<Entry>)> {
        let q = self.palette.filter.read(cx).value().to_lowercase();
        let hit = |s: &str| q.is_empty() || s.to_lowercase().contains(&q);
        let projects = if self.palette.all { self.projects() } else { self.project.iter().cloned().collect() };
        let mut cards: Vec<(String, Card)> = projects
            .iter()
            .flat_map(|p| {
                let name = self.repo_name(p);
                self.cards(p).into_iter().map(move |c| (name.clone(), c))
            })
            .filter(|(_, c)| hit(&c.title))
            .collect();
        cards.sort_by_key(|(_, c)| (c.status, Reverse(c.at)));
        let sessions: Vec<Entry> = cards
            .into_iter()
            .take(5)
            .map(|(name, c)| Entry {
                lead: match c.status {
                    Status::NeedsYou => Lead::Waiting,
                    Status::Working => Lead::Running,
                    _ => Lead::Provider(c.provider.clone()),
                },
                detail: format!("{name} · {} · {}", provider_name(&c.provider), status_word(c.status)),
                title: c.title,
                keys: None,
                pick: Pick::Session(c.id),
            })
            .collect();
        let root = self.explore_root().unwrap_or_default();
        let changed: Vec<String> = self.repos.get(&root).map(|r| r.files.iter().map(|f| f.path.clone()).collect()).unwrap_or_default();
        let mut paths: Vec<&String> = if q.is_empty() { changed.iter().take(3).collect() } else { self.palette.files.iter().filter(|p| hit(p)).collect() };
        paths.sort_by_key(|p| !changed.contains(p));
        let files = paths
            .into_iter()
            .take(5)
            .map(|p| {
                let dir = p.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
                let detail = match (dir.is_empty(), changed.contains(p)) {
                    (true, true) => "modified".to_string(),
                    (false, true) => format!("{dir} · modified"),
                    _ => dir,
                };
                Entry { pick: Pick::File(format!("{root}/{p}")), lead: Lead::File, title: basename(p), detail, keys: None }
            })
            .collect();
        let project = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let actions = [
            Entry { pick: Pick::New, lead: Lead::Icon("sparkle"), title: format!("New session in {project}"), detail: String::new(), keys: Some("⌘ N") },
            Entry { pick: Pick::Split, lead: Lead::Icon("split-right"), title: "Open selected in a split".into(), detail: String::new(), keys: None },
            Entry { pick: Pick::Next, lead: Lead::Waiting, title: "Jump to next waiting session".into(), detail: String::new(), keys: Some("⌘ J") },
        ]
        .into_iter()
        .filter(|e| hit(&e.title))
        .collect();
        [("Sessions", sessions), ("Files", files), ("Actions", actions)].into_iter().filter(|(_, e)| !e.is_empty()).collect()
    }

    fn activate(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        match pick {
            Pick::Session(id) => self.focus_agent(&id, window, cx),
            Pick::File(path) => {
                self.screen = Screen::Sessions;
                self.open_file(path, cx);
            }
            Pick::New => self.open(Overlay::NewSession, window, cx),
            Pick::Split => self.new_shell(Some(false), cx),
            Pick::Next => self.next_waiting(&crate::actions::NextWaiting, window, cx),
        }
        cx.notify();
    }

    fn on_palette_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let entries: Vec<Entry> = self.palette_sections(cx).into_iter().flat_map(|(_, e)| e).collect();
        let last = entries.len().saturating_sub(1);
        match ev.keystroke.key.as_str() {
            "up" => self.palette.ix = self.palette.ix.saturating_sub(1),
            "down" => self.palette.ix = (self.palette.ix + 1).min(last),
            "tab" => {
                self.palette.all = !self.palette.all;
                self.palette.ix = 0;
            }
            "escape" => self.close_overlay(window, cx),
            "enter" => {
                if let Some(e) = entries.get(self.palette.ix.min(last)) {
                    self.activate(e.pick.clone(), window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn palette(&mut self, cx: &mut Context<Self>) -> Div {
        let sections = self.palette_sections(cx);
        let total: usize = sections.iter().map(|(_, e)| e.len()).sum();
        let selected = self.palette.ix.min(total.saturating_sub(1));
        let mut n = 0;
        let mut body = div().id("palette-results").max_h(px(460.)).overflow_y_scroll().px(px(8.)).pb(px(8.)).flex().flex_col();
        for (label, entries) in sections {
            body = body.child(div().pt(px(12.)).pb(px(6.)).px(px(12.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_4)).child(label));
            for e in entries {
                let i = n;
                n += 1;
                let lead = match &e.lead {
                    Lead::Waiting => dot(8., WAITING).into_any_element(),
                    Lead::Running => spinner(("palette-spin", i), 12., RUNNING_TEXT).into_any_element(),
                    Lead::Provider(p) => dot(8., provider_color(p)).into_any_element(),
                    Lead::File => file_icon(&e.title, false, false, 16.).into_any_element(),
                    Lead::Icon(name) => icon(name, 13., TEXT_2).into_any_element(),
                };
                let keys = e.keys.or((i == selected).then_some("↵"));
                let pick = e.pick.clone();
                body = body.child(ui::palette_row(("palette-row", i), i == selected, lead, e.title, e.detail, keys).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| this.activate(pick.clone(), window, cx),
                )));
            }
        }
        if total == 0 {
            body = body.child(div().p(px(20.)).text_size(px(13.5)).text_color(rgba(TEXT_3)).child("No matches."));
        }
        let input = div()
            .h(px(60.))
            .px(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .border_b(px(0.5))
            .border_color(rgba(SEPARATOR))
            .child(icon("search", 17., TEXT_3))
            .child(div().flex_1().text_size(px(17.)).child(Input::new(&self.palette.filter).appearance(false).p_0().text_size(px(17.))))
            .child(div().text_size(px(12.)).text_color(rgba(TEXT_4)).child("esc"));
        let scope = if self.palette.all { "Tab to filter by project" } else { "Tab to search all projects" };
        let footer = div()
            .h(px(40.))
            .px(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(16.))
            .border_t(px(0.5))
            .border_color(rgba(SEPARATOR))
            .text_size(px(12.))
            .text_color(rgba(TEXT_4))
            .child(hint("↑↓", "navigate"))
            .child(hint("↵", "open"))
            .child(div().ml_auto().child(scope));
        div().absolute().top(px(120.)).left_0().right_0().flex().justify_center().child(
            ui::pop(div().w(px(660.)).rounded(px(22.)).overflow_hidden().flex().flex_col())
                .occlude()
                .capture_key_down(cx.listener(Self::on_palette_key))
                .child(input)
                .child(body)
                .child(footer),
        )
    }
}
