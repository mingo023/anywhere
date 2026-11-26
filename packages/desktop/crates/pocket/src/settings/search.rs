use super::catalog::{Control, Setting};
use super::{DECOR, Decor, Section, Settings, SettingsEvent, action, card, keybindings, row};
use crate::palette::runs;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;
use store::Store;
use theme::*;

/// How long the row an opened result points at stays tinted.
const FLASH: Duration = Duration::from_millis(1800);

/// Rows that draw a whole list, not one setting: a result for one links to its page instead of drawing it.
const LISTS: [&str; 10] =
    ["providers", "provider-status", "shortcuts", "hosts", "projects", "phones", "custom-themes", "notification-sound", "dev-servers", "swatches"];

/// Rows that only show what a setting does; there is nothing to find in them.
const PREVIEWS: [&str; 2] = ["preview", "font-preview"];

fn is_live(setting: &Setting) -> bool {
    !LISTS.contains(&setting.id)
}

// ---- Matching -----------------------------------------------------------------------------------------------------

/// Whether `word` starts a word somewhere in `hay`, which is lowercase.
fn starts_a_word(hay: &str, word: &str) -> bool {
    hay.match_indices(word).any(|(i, _)| hay[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric()))
}

/// Every word starts a word of the fields, or, from three letters on, sits anywhere in them.
pub(super) fn matches(words: &[String], fields: &[&str]) -> bool {
    let hay = fields.join("\u{1}").to_lowercase();
    words.iter().all(|w| starts_a_word(&hay, w) || (w.chars().count() > 2 && hay.contains(w.as_str())))
}

/// The byte ranges of `text` to mark: each word that starts a word, through the end of that word, so "sound" marks all of
/// "Sounds" and nothing inside "Resound".
pub(super) fn marks(text: &str, words: &[String]) -> Vec<Range<usize>> {
    let mut found: Vec<Range<usize>> = Vec::new();
    for word in words {
        for r in runs(text, std::slice::from_ref(word)) {
            if text[..r.start].chars().next_back().is_none_or(|c| !c.is_alphanumeric()) {
                let end = text[r.end..].find(|c: char| !c.is_alphanumeric()).map_or(text.len(), |i| r.end + i);
                found.push(r.start..end);
            }
        }
    }
    found.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in found {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

/// What a row can be found by: its label, group, hint and the options its menu lists.
fn own_fields(setting: &Setting, store: &Store, live: &agents::Agents) -> Vec<String> {
    let options = match setting.control {
        Control::Choice { options, .. } => options,
        _ => &[],
    };
    let listed = options.iter().enumerate().filter(|(i, _)| setting.lists(store, live, *i)).map(|(_, o)| o.to_string());
    let note = setting.note.and_then(|note| note(store));
    [setting.label.to_string(), setting.group.to_string()].into_iter().chain(note).chain(setting.hint.map(String::from)).chain(listed).collect()
}

/// A page's own name and the one above it, which a row is also found by ("Codex" for the Model row on Codex's page).
fn page_fields(section: Section) -> Vec<&'static str> {
    [Some(section.label()), section.parent().map(Section::label)].into_iter().flatten().collect()
}

fn pages() -> impl Iterator<Item = Section> {
    Section::ALL.into_iter().chain((0..agents::providers::COUNT).map(Section::Provider))
}

// ---- What a search finds ------------------------------------------------------------------------------------------

/// Something that isn't a catalog row, and the page its result opens.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    /// The page whose results it sits under.
    pub group: Section,
    pub link: Section,
    pub lead: Lead,
    pub title: String,
    pub detail: String,
    /// "Agent provider", "Project · build-box".
    pub tag: String,
    /// Besides the title, what it is found by.
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Lead {
    Icon(&'static str),
    Provider(&'static str),
    Project(usize),
}

/// One change that lets a hidden row show: a switch to flip or a choice to take.
#[derive(Clone, Copy)]
pub(super) struct Step {
    pub row: &'static Setting,
    pub to: To,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum To {
    Switch(bool),
    Option(usize),
}

impl Step {
    /// "Turn on Play sounds", "Set Shell to Custom".
    fn label(&self) -> String {
        let name = self.row.label;
        match (self.to, self.row.control) {
            (To::Switch(true), _) => format!("Turn on {name}"),
            (To::Switch(false), _) => format!("Turn off {name}"),
            (To::Option(i), Control::Choice { options, .. }) => format!("Set {name} to {}", options.get(i).copied().unwrap_or_default()),
            (To::Option(_), _) => format!("Change {name}"),
        }
    }

    /// The button's own word, since the row says the rest.
    fn button(&self) -> String {
        match (self.to, self.row.control) {
            (To::Switch(true), _) => "Turn on".into(),
            (To::Switch(false), _) => "Turn off".into(),
            (To::Option(i), Control::Choice { options, .. }) => format!("Use {}", options.get(i).copied().unwrap_or_default()),
            (To::Option(_), _) => "Change".into(),
        }
    }

    fn current(row: &'static Setting, store: &Store) -> Option<To> {
        match row.control {
            Control::Switch { get, .. } => Some(To::Switch(get(store))),
            Control::Choice { get, .. } => Some(To::Option(get(store))),
            _ => None,
        }
    }

    fn apply(&self, store: &mut Store) {
        match (self.row.control, self.to) {
            (Control::Switch { set, .. }, To::Switch(on)) => set(store, on),
            (Control::Choice { set, .. }, To::Option(i)) => set(store, i),
            _ => {}
        }
    }
}

/// What a hidden row needs: "Turn on Play sounds first".
fn needs(steps: &[Step]) -> String {
    let mut text = String::new();
    for (i, step) in steps.iter().enumerate() {
        let label = step.label();
        match i {
            0 => text.push_str(&label),
            _ => {
                let mut chars = label.chars();
                text.push_str(" and ");
                text.extend(chars.next().map(|c| c.to_lowercase().next().unwrap_or(c)));
                text.push_str(chars.as_str());
            }
        }
    }
    text + " first"
}

fn button_label(steps: &[Step]) -> String {
    match steps {
        [one] => one.button(),
        _ => "Apply".into(),
    }
}

/// Every change to one of `rows`' switches or choices, those nearest `of` first, for finding what lets `of` show.
fn moves(of: &Setting, rows: &'static [Setting], store: &Store) -> Vec<Step> {
    let at = rows.iter().position(|r| std::ptr::eq(r, of)).unwrap_or(0);
    let mut out = Vec::new();
    for r in (0..at).rev().chain(at + 1..rows.len()).map(|i| &rows[i]) {
        match (r.control, Step::current(r, store)) {
            (Control::Switch { .. }, Some(To::Switch(on))) => out.push(Step { row: r, to: To::Switch(!on) }),
            (Control::Choice { options, .. }, Some(To::Option(now))) => {
                out.extend((0..options.len()).filter(|i| *i != now).map(|i| Step { row: r, to: To::Option(i) }));
            }
            _ => {}
        }
    }
    out
}

/// The changes, in order, after which `of` shows, at most `depth` of them; found by trying them on `view` and putting each back.
fn plan(of: &Setting, rows: &'static [Setting], view: &mut Store, depth: usize) -> Option<Vec<Step>> {
    let all = moves(of, rows, view);
    let tried = |step: &Step, view: &mut Store, then: &dyn Fn(&mut Store) -> Option<Vec<Step>>| {
        let back = Step::current(step.row, view).map(|to| Step { row: step.row, to });
        step.apply(view);
        let found = then(view);
        if let Some(back) = back {
            back.apply(view);
        }
        found
    };
    for step in &all {
        if let Some(found) = tried(step, view, &|v| of.allowed(v).then(Vec::new)) {
            return Some([vec![*step], found].concat());
        }
    }
    if depth > 1 {
        for step in &all {
            if let Some(found) = tried(step, view, &|v| plan(of, rows, v, depth - 1)) {
                return Some([vec![*step], found].concat());
            }
        }
    }
    None
}

/// What stops a hidden row from showing.
#[derive(Clone)]
pub(super) enum Blocked {
    By(Vec<Step>),
    /// Hidden for a reason no single switch or choice on its page undoes.
    Unknown,
}

#[derive(Clone)]
pub(super) enum Hit {
    Row { setting: &'static Setting, blocked: Option<Blocked> },
    Entry(Entry),
    Shortcut(&'static str),
}

impl Hit {
    fn title(&self) -> &str {
        match self {
            Hit::Row { setting, .. } => setting.label,
            Hit::Entry(e) => &e.title,
            Hit::Shortcut(label) => label,
        }
    }
}

/// The results under one page.
pub(super) struct Group {
    pub section: Section,
    pub hits: Vec<Hit>,
}

/// What a search reads besides the catalog.
pub(super) struct Inputs<'a> {
    pub store: &'a Store,
    pub live: &'a agents::Agents,
    pub entries: &'a [Entry],
    pub shortcuts: &'a [&'static str],
}

/// Every group of results for `words`. A group whose own title matches every word comes first, and so does the row in a
/// group that matches most; the top hit of the first group is what Enter opens.
pub(super) fn results(words: &[String], inputs: &Inputs) -> Vec<Group> {
    if words.is_empty() {
        return Vec::new();
    }
    let Inputs { store, live, entries, shortcuts } = inputs;
    let mut groups: Vec<Group> = pages()
        .filter_map(|section| {
            let page = page_fields(section);
            let mut rows: Vec<(usize, &'static Setting)> = section
                .rows()
                .iter()
                .filter(|r| !PREVIEWS.contains(&r.id))
                .filter_map(|r| {
                    let own = own_fields(r, store, live);
                    let own: Vec<&str> = own.iter().map(String::as_str).collect();
                    let all: Vec<&str> = own.iter().copied().chain(page.iter().copied()).collect();
                    let hits = words.iter().filter(|w| matches(std::slice::from_ref(w), &own)).count();
                    (hits > 0 && matches(words, &all)).then(|| (hits * 2 + usize::from(matches(words, &[r.label])), r))
                })
                .collect();
            rows.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
            let mut hits: Vec<Hit> = rows
                .into_iter()
                .map(|(_, setting)| {
                    let blocked = (!setting.allowed(store)).then(|| match plan(setting, section.rows(), &mut store.settings_view(), 2) {
                        Some(steps) => Blocked::By(steps),
                        None => Blocked::Unknown,
                    });
                    Hit::Row { setting, blocked }
                })
                .collect();
            let mut found: Vec<&Entry> = entries
                .iter()
                .filter(|e| e.group == section)
                .filter(|e| {
                    let fields: Vec<&str> = [e.title.as_str(), e.tag.as_str()].into_iter().chain(e.fields.iter().map(String::as_str)).collect();
                    matches(words, &fields)
                })
                .collect();
            found.sort_by_key(|e| e.link != section);
            hits.extend(found.into_iter().cloned().map(Hit::Entry));
            if section == Section::Keyboard {
                hits.extend(shortcuts.iter().copied().map(Hit::Shortcut));
            }
            (!hits.is_empty()).then_some(Group { section, hits })
        })
        .collect();
    groups.sort_by_key(|g| !g.hits.iter().any(|h| matches(words, &[h.title()])));
    groups
}

// ---- Scrolling to a result ----------------------------------------------------------------------------------------

/// The scroll offset, zero or less, that puts a row `height` tall `top` below the start of the content in the middle of a
/// viewport `view` tall, kept inside the scrollable `max`.
fn centred(top: f32, height: f32, view: f32, max: f32) -> f32 {
    (-(top - (view - height) / 2.)).clamp(-max.abs(), 0.)
}

/// The page's scroll and where the row a result opened sits in it, so the page can bring the row to the middle.
pub(super) struct Reach {
    pub(super) body: ScrollHandle,
    spot: Rc<Cell<Option<Bounds<Pixels>>>>,
    pending: bool,
}

impl Default for Reach {
    fn default() -> Self {
        Self { body: ScrollHandle::new(), spot: Rc::default(), pending: false }
    }
}

impl Reach {
    /// Starts at the top, so the row is measured where the page lays it out.
    fn aim(&mut self) {
        self.pending = true;
        self.spot.set(None);
        self.body.set_offset(Point::default());
    }

    pub(super) fn pending(&self) -> bool {
        self.pending
    }

    /// Fills the row it's in, to record where the row is.
    pub(super) fn measure(&self) -> impl IntoElement + use<> {
        let spot = self.spot.clone();
        canvas(move |bounds, _, _| spot.set(Some(bounds)), |_, _, _, _| {}).absolute().inset_0()
    }

    /// Scrolls to the measured row, once it is known.
    pub(super) fn settle(&mut self) {
        let Some(row) = self.spot.get().filter(|_| self.pending) else { return };
        let view = self.body.bounds();
        let max = self.body.max_offset().y;
        let y = centred((row.top() - view.top()).into(), row.size.height.into(), view.size.height.into(), max.into());
        self.body.set_offset(point(px(0.), px(y)));
        (self.pending, self.spot) = (false, Rc::default());
    }
}

// ---- Drawing ------------------------------------------------------------------------------------------------------

/// "7 results for “worktree”".
fn summary(query: &str, n: usize) -> String {
    format!("{n} result{} for \u{201c}{query}\u{201d}", if n == 1 { "" } else { "s" })
}

/// The query's words on an amber wash, as the design marks matches.
fn marked(text: impl Into<String>, words: &[String]) -> StyledText {
    let text = text.into();
    let style = HighlightStyle { color: Some(TEXT.into()), background_color: Some(WAITING_BG.into()), ..Default::default() };
    let marks: Vec<_> = marks(&text, words).into_iter().map(|r| (r, style)).collect();
    StyledText::new(text).with_highlights(marks)
}

/// A page's results under its icon and name: "Agents › Codex" for a provider.
fn heading(section: Section) -> Div {
    let name = match section.parent() {
        Some(parent) => format!("{} \u{203a} {}", parent.label(), section.label()),
        None => section.label().to_string(),
    };
    div()
        .mb(px(7.))
        .ml(px(2.))
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(px(13.))
        .line_height(px(18.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_2)
        .child(icon(section.nav().icon(), 14., TEXT_3))
        .child(name)
}

/// Where a click on a result goes.
#[derive(Clone, Copy)]
enum Target {
    Row(Section, &'static Setting),
    Page(Section),
}

impl Settings {
    /// What a search finds that the catalog doesn't hold: pages, providers, accounts, projects, hosts and themes.
    fn entries(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = Section::ALL
            .into_iter()
            .map(|s| Entry {
                group: s,
                link: s,
                lead: Lead::Icon(s.icon()),
                title: s.label().into(),
                detail: s.subtitle().into(),
                tag: "Section".into(),
                fields: vec![s.subtitle().into()],
            })
            .collect();
        out.extend(agents::providers::IDS.iter().enumerate().map(|(i, id)| Entry {
            group: Section::Agents,
            link: Section::Provider(i),
            lead: Lead::Provider(id),
            title: self.agent_name(id),
            detail: if self.installed(id) { "Built-in provider" } else { "Not installed" }.into(),
            tag: "Agent provider".into(),
            fields: vec![PROVIDER_NAMES[i].into()],
        }));
        for machine in &self.side.accounts.machines {
            let host = if machine.host.is_some() { format!(" \u{b7} {}", machine.title) } else { String::new() };
            out.extend(machine.list.iter().filter(|a| !a.pending).map(|a| Entry {
                group: Section::Accounts,
                link: Section::Accounts,
                lead: Lead::Icon("shield"),
                title: if a.label.is_empty() { a.email.clone() } else { a.label.clone() },
                detail: [a.email.as_str(), a.plan.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" \u{b7} "),
                tag: format!("{} account{host}", provider_name(&a.provider)),
                fields: vec![a.email.clone(), machine.title.clone(), "account".into()],
            }));
        }
        out.extend(self.side.projects.iter().enumerate().map(|(i, card)| {
            let host = crate::accounts::host_of(Some(&card.path)).map(|id| {
                let shown = self.side.hosts.rows.iter().find(|r| r.host.id == id).map(|r| r.shown.clone());
                shown.unwrap_or(id)
            });
            let path = hosts::split(&card.path).1.to_string();
            Entry {
                group: Section::Projects,
                link: Section::Projects,
                lead: Lead::Project(i),
                title: card.name.clone(),
                detail: crate::util::tilde(&path),
                tag: host.as_ref().map_or_else(|| "Project".to_string(), |h| format!("Project \u{b7} {h}")),
                fields: [path].into_iter().chain(host).collect(),
            }
        }));
        out.extend(self.side.hosts.rows.iter().map(|r| Entry {
            group: Section::Hosts,
            link: Section::Hosts,
            lead: Lead::Icon("server"),
            title: r.shown.clone(),
            detail: r.host.name.clone(),
            tag: "Host".into(),
            fields: vec![r.host.name.clone(), "ssh".into()],
        }));
        let themes = ::theme::presets().iter().map(|p| (&p.theme, "Theme")).chain(self.side.themes.iter().map(|c| (&c.theme, "Imported theme")));
        out.extend(themes.map(|(t, tag)| Entry {
            group: Section::Appearance,
            link: Section::Appearance,
            lead: Lead::Icon("appearance"),
            title: t.name.clone(),
            detail: if t.dark { "Dark theme" } else { "Light theme" }.into(),
            tag: tag.into(),
            fields: vec!["theme".into()],
        }));
        out
    }

    pub(super) fn search_results(&mut self, words: &[String], cx: &mut Context<Self>) -> Div {
        let query = self.search.read(cx).value().trim().to_string();
        let (store, live, entries) = (self.store.clone(), self.live.clone(), self.entries());
        let mut keys = keybindings::search(words, &store.keys);
        let shortcuts: Vec<&'static str> = keys.iter().map(|(label, _)| *label).collect();
        let groups = results(words, &Inputs { store: &store, live: &live, entries: &entries, shortcuts: &shortcuts });
        let n: usize = groups.iter().map(|g| g.hits.len()).sum();
        if n == 0 {
            let empty = format!("Nothing matches \u{201c}{query}\u{201d}.");
            let scope = "Search covers settings, agents, accounts, projects, hosts, themes and shortcuts.";
            return div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .text_size(px(13.))
                .text_color(TEXT_2)
                .child(empty)
                .child(div().text_size(px(12.)).text_color(TEXT_3).child(scope));
        }
        let head = div()
            .flex()
            .items_center()
            .justify_between()
            .text_size(px(12.5))
            .text_color(TEXT_3)
            .child(summary(&query, n))
            .child("\u{21a9} opens the first \u{b7} Esc clears");
        let mut column = div().flex().flex_col().gap(px(22.)).child(head);
        let mut next = 0;
        for Group { section, hits } in groups {
            let rows = hits
                .into_iter()
                .map(|hit| {
                    next += 1;
                    match hit {
                        Hit::Row { setting, blocked: Some(blocked) } => self.blocked_row(section, setting, blocked, words, cx),
                        Hit::Row { setting, blocked: None } if is_live(setting) => self.live_row(section, setting, next, words, cx),
                        Hit::Row { setting, .. } => {
                            let detail = setting.hint.map(|h| marked(h, words).into_any_element());
                            let tag = ui::tag(setting.group);
                            self.link_row(
                                format!("{}-{}", section.id(), setting.id),
                                None,
                                marked(setting.label, words),
                                Some(tag),
                                detail,
                                None,
                                Target::Row(section, setting),
                                cx,
                            )
                        }
                        Hit::Entry(e) => self.entry_row(&e, next, words, cx),
                        Hit::Shortcut(label) => {
                            let at = keys.iter().position(|(l, _)| *l == label);
                            let keys = at.map(|i| keys.remove(i).1).map(IntoElement::into_any_element);
                            self.link_row(format!("key-{label}"), None, marked(label, words), None, None, keys, Target::Page(Section::Keyboard), cx)
                        }
                    }
                })
                .collect();
            let group = div().flex().flex_col().child(heading(section)).child(card(rows));
            column = column.child(group);
        }
        column
    }

    fn entry_row(&mut self, e: &Entry, at: usize, words: &[String], cx: &mut Context<Self>) -> Div {
        let lead = match &e.lead {
            Lead::Icon(name) => div().w(px(20.)).flex().flex_none().justify_center().child(icon(name, 15., TEXT_3)),
            Lead::Provider(id) => div().w(px(20.)).flex().flex_none().justify_center().child(provider_icon(id, 15., TEXT_2)),
            Lead::Project(i) => match self.side.projects.get(*i) {
                Some(card) => card.tile(20.),
                None => div(),
            },
        };
        let detail = (!e.detail.is_empty()).then(|| marked(e.detail.clone(), words).into_any_element());
        let tag = ui::tag(e.tag.clone());
        self.link_row(format!("entry-{at}"), Some(lead), marked(e.title.clone(), words), Some(tag), detail, None, Target::Page(e.link), cx)
    }

    /// A result that opens its page, for what has no control of its own to draw here.
    #[expect(clippy::too_many_arguments, reason = "a result's parts, each drawn the same way for every kind")]
    fn link_row(
        &mut self,
        id: String,
        lead: Option<Div>,
        title: StyledText,
        tag: Option<Div>,
        detail: Option<AnyElement>,
        trailing: Option<AnyElement>,
        target: Target,
        cx: &mut Context<Self>,
    ) -> Div {
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(7.))
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(TEXT)
                    .child(title)
                    .children(tag),
            )
            .children(detail.map(|d| div().truncate().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(d)));
        div().child(
            div()
                .id(SharedString::from(format!("settings-hit-{id}")))
                .px(px(14.))
                .py(px(9.))
                .flex()
                .items_center()
                .gap(px(12.))
                .cursor_pointer()
                .tab_index(0)
                .hover(|d| d.bg(FILL_1))
                .children(lead)
                .child(text)
                .children(trailing)
                .child(icon("chevron-right", 13., TEXT_4))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open(target, window, cx))),
        )
    }

    /// The row as its page draws it, with its group as a chip, its words marked, and an arrow to its page.
    fn live_row(&mut self, section: Section, setting: &'static Setting, at: usize, words: &[String], cx: &mut Context<Self>) -> Div {
        let mut decor = self.decor(setting, cx);
        decor.tags.insert(0, ui::tag(setting.group).into_any_element());
        let custom = matches!(setting.control, Control::Custom(_));
        if !custom && decor.hint.is_none() {
            let hint = setting.note.and_then(|note| note(&self.store)).or_else(|| setting.hint.map(String::from));
            decor.hint = hint.map(|h| marked(h, words).into_any_element());
        }
        let changed = setting.is_changed(&self.store);
        DECOR.with_borrow_mut(|d| *d = Some(decor));
        let body = match setting.control {
            Control::Custom(draw) => draw(self, setting, cx),
            _ => {
                let control = self.control(setting, cx);
                row(marked(setting.label, words), None, control)
            }
        };
        DECOR.with_borrow_mut(|d| *d = None);
        let body = body
            .relative()
            .flex_1()
            .min_w_0()
            .group(SharedString::from(format!("row-{}", setting.id)))
            .when(changed, |d| d.child(div().absolute().left(px(5.)).top_0().bottom_0().flex().items_center().child(ui::dot(5., TEXT))));
        let arrow = self.open_arrow(at, section, setting, cx);
        div().child(div().id(SharedString::from(format!("settings-hit-{}-{}", section.id(), setting.id))).flex().items_center().child(body).child(arrow))
    }

    /// An arrow after a live row, to see it in its page.
    fn open_arrow(&mut self, at: usize, section: Section, setting: &'static Setting, cx: &mut Context<Self>) -> impl IntoElement {
        let arrow = div()
            .id(("settings-hit-open", at))
            .mr(px(8.))
            .size(px(24.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .tab_index(0)
            .hover(|d| d.bg(FILL_3))
            .child(icon("chevron-right", 13., TEXT_4))
            .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new("Show in its page").build(window, cx))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open(Target::Row(section, setting), window, cx)));
        super::focus_ring(arrow)
    }

    /// A row its page hides because of another setting: it says what to turn on, and a button does it.
    fn blocked_row(&mut self, section: Section, setting: &'static Setting, blocked: Blocked, words: &[String], cx: &mut Context<Self>) -> Div {
        let tags = vec![ui::tag(setting.group).into_any_element()];
        let (hint, control): (String, AnyElement) = match blocked {
            Blocked::By(steps) => {
                let focus = self.control_focus(setting.id, cx);
                let label = button_label(&steps);
                let hint = needs(&steps);
                let button = action(SharedString::from(format!("{}-{}-turn-on", section.id(), setting.id)), label)
                    .track_focus(&focus)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.unblock(&steps, cx)));
                (hint, button.into_any_element())
            }
            Blocked::Unknown => {
                let hint = "Not available with the page's current settings".to_string();
                let open = action(SharedString::from(format!("{}-{}-open", section.id(), setting.id)), "Show in page")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open(Target::Row(section, setting), window, cx)));
                (hint, open.into_any_element())
            }
        };
        DECOR.with_borrow_mut(|d| *d = Some(Decor { tags, ..Decor::default() }));
        let body = row(marked(setting.label, words), Some(&hint), control);
        DECOR.with_borrow_mut(|d| *d = None);
        div().child(div().id(SharedString::from(format!("settings-hit-{}-{}", section.id(), setting.id))).child(body))
    }

    /// Takes each step, so the effects and the save run as if the page had.
    fn unblock(&mut self, steps: &[Step], cx: &mut Context<Self>) {
        for step in steps {
            let step = *step;
            self.set(step.row, cx, move |s| step.apply(s));
        }
    }

    fn open(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        match target {
            Target::Row(section, setting) => self.open_row(section, setting, window, cx),
            Target::Page(section) => self.open_page(section, window, cx),
        }
    }

    fn open_page(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.show_section(section, cx);
        self.clear_search(window, cx);
        cx.emit(SettingsEvent::FocusRoot);
        cx.notify();
    }

    /// Shows the row's page, unfolding Advanced if the row lives there, scrolls to the row, tints it for a moment and
    /// focuses its control.
    fn open_row(&mut self, section: Section, setting: &'static Setting, window: &mut Window, cx: &mut Context<Self>) {
        if setting.advanced {
            self.write(None, cx, move |s, _| {
                s.advanced.insert(section.id().to_string());
            });
        }
        self.reach.aim();
        self.show_section(section, cx);
        self.clear_search(window, cx);
        self.found = Some(setting.id);
        if matches!(setting.control, Control::Custom(_)) {
            cx.emit(SettingsEvent::FocusRoot);
        } else {
            self.focus_row(setting.id, window, cx);
        }
        let id = setting.id;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FLASH).await;
            this.update(cx, |this, cx| {
                if this.found == Some(id) {
                    this.found = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Opens the top result. A row its page hides can't be shown there, so its button takes focus here instead.
    pub(crate) fn open_first_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let words = self.query(cx);
        let (store, live, entries) = (self.store.clone(), self.live.clone(), self.entries());
        let shortcuts: Vec<&'static str> = keybindings::search(&words, &store.keys).into_iter().map(|(label, _)| label).collect();
        let groups = results(&words, &Inputs { store: &store, live: &live, entries: &entries, shortcuts: &shortcuts });
        let Some(Group { section, hits }) = groups.into_iter().next() else { return };
        match hits.into_iter().next() {
            Some(Hit::Row { setting, blocked: Some(Blocked::By(_)) }) => {
                let focus = self.control_focus(setting.id, cx);
                window.focus(&focus, cx);
            }
            Some(Hit::Row { setting, .. }) => self.open_row(section, setting, window, cx),
            Some(Hit::Entry(e)) => self.open_page(e.link, window, cx),
            Some(Hit::Shortcut(_)) => self.open_page(Section::Keyboard, window, cx),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Blocked, Entry, Group, Hit, Inputs, Lead, PREVIEWS, button_label, centred, is_live, marks, matches, needs, plan, results, summary};
    use crate::settings::{Section, catalog};
    use store::Store;

    fn words(q: &str) -> Vec<String> {
        catalog::words(q)
    }

    fn found(q: &str, store: &Store, entries: &[Entry]) -> Vec<Group> {
        results(&words(q), &Inputs { store, live: &agents::Agents::default(), entries, shortcuts: &[] })
    }

    fn row_ids(groups: &[Group]) -> Vec<(Section, &'static str)> {
        groups
            .iter()
            .flat_map(|g| g.hits.iter().filter_map(move |h| if let Hit::Row { setting, .. } = h { Some((g.section, setting.id)) } else { None }))
            .collect()
    }

    fn entry(group: Section, title: &str, tag: &str) -> Entry {
        Entry { group, link: group, lead: Lead::Icon("folder"), title: title.into(), detail: String::new(), tag: tag.into(), fields: Vec::new() }
    }

    #[test]
    fn the_summary_counts_results() {
        assert_eq!(summary("vim", 0), "0 results for \u{201c}vim\u{201d}");
        assert_eq!(summary("ban", 1), "1 result for \u{201c}ban\u{201d}");
        assert_eq!(summary("sound", 9), "9 results for \u{201c}sound\u{201d}");
    }

    #[test]
    fn a_match_inside_a_word_is_not_highlighted() {
        assert!(marks("Notification sound", &words("ound")).is_empty());
        assert!(marks("Resound", &words("sound")).is_empty());
    }

    #[test]
    fn a_highlight_covers_the_whole_word() {
        assert_eq!(marks("Play sounds", &words("sound")), vec![5..11]);
        assert_eq!(marks("Sounds and sound", &words("sound")), [0..6, 11..16]);
        assert_eq!(marks("Play sounds", &words("play sound")), [0..4, 5..11]);
    }

    #[test]
    fn a_word_matches_where_a_word_starts_or_anywhere_from_three_letters() {
        assert!(matches(&words("sound"), &["Play sounds"]));
        assert!(matches(&words("ound"), &["Play sounds"]));
        assert!(!matches(&words("ou"), &["Play sounds"]));
        assert!(matches(&words("so"), &["Play sounds"]));
        assert!(matches(&words("play volume"), &["Play sounds", "Volume"]));
        assert!(!matches(&words("play volume"), &["Play sounds"]));
    }

    #[test]
    fn the_banners_switch_is_found_by_the_words_its_row_shows() {
        let ids = row_ids(&found("show banners", &Store::default(), &[]));
        assert!(ids.contains(&(Section::Notifications, "banners")), "{ids:?}");
    }

    #[test]
    fn a_result_needs_one_word_in_the_row_itself() {
        let ids = row_ids(&found("codex model", &Store::default(), &[]));
        assert_eq!(ids, [(Section::Provider(1), "codex-model")]);
        let by_name = row_ids(&found("codex", &Store::default(), &[]));
        assert!(by_name.iter().all(|(section, _)| !matches!(section, Section::Provider(_))), "a provider's page name alone finds none of its rows");
    }

    #[test]
    fn a_provider_entry_is_found_by_its_name_and_sits_under_agents() {
        let providers = entry(Section::Agents, "Codex", "Agent provider");
        let groups = found("codex", &Store::default(), std::slice::from_ref(&providers));
        let agents = groups.iter().find(|g| g.section == Section::Agents).expect("an Agents group");
        assert!(agents.hits.iter().any(|h| matches!(h, Hit::Entry(e) if *e == providers)));
    }

    #[test]
    fn a_project_is_tagged_with_its_host_and_found_by_it() {
        let mut project = entry(Section::Projects, "api", "Project \u{b7} build-box");
        project.fields = vec!["/srv/api".into(), "build-box".into()];
        let by = |q: &str| found(q, &Store::default(), &[project.clone()]).iter().any(|g| g.hits.iter().any(|h| matches!(h, Hit::Entry(_))));
        assert!(by("api") && by("build-box") && by("project"));
        assert!(!by("laptop"));
    }

    #[test]
    fn the_group_whose_title_matches_comes_first() {
        let groups = found("sound", &Store::default(), &[]);
        let first = groups.first().expect("a result");
        assert_eq!(first.section, Section::Notifications);
        assert_eq!(first.hits[0].title(), "Play sounds");
    }

    #[test]
    fn a_row_hidden_by_its_parent_names_what_to_turn_on() {
        let mut store = Store::default();
        store.sounds.all = false;
        let groups = found("volume", &store, &[]);
        let Some(Hit::Row { setting, blocked: Some(Blocked::By(steps)) }) = groups.iter().flat_map(|g| &g.hits).find(|h| h.title() == "Volume") else {
            panic!("Volume is found, hidden")
        };
        assert_eq!(setting.id, "volume");
        assert_eq!(needs(steps), "Turn on Play sounds first");
        assert_eq!(button_label(steps), "Turn on");
        steps.iter().for_each(|s| s.apply(&mut store));
        assert!(setting.allowed(&store));
    }

    #[test]
    fn a_row_its_page_shows_has_nothing_to_turn_on() {
        let groups = found("volume", &Store::default(), &[]);
        assert!(groups.iter().flat_map(|g| &g.hits).all(|h| !matches!(h, Hit::Row { blocked: Some(_), .. })));
    }

    #[test]
    fn a_choice_that_unhides_a_row_is_set_by_name() {
        let mut store = Store::default();
        store.terminal.shell = store::prefs::terminal::Shell::Login;
        let rows = Section::Terminal.rows();
        let path = rows.iter().find(|r| r.id == "shell-path").expect("the shell path row");
        let steps = plan(path, rows, &mut store.settings_view(), 2).expect("a choice unhides it");
        assert_eq!(steps.len(), 1);
        assert_eq!(needs(&steps), "Set Shell to Custom first");
        assert_eq!(button_label(&steps), "Use Custom");
    }

    #[test]
    fn a_row_two_settings_hide_needs_both_turned_on_in_order() {
        let mut store = Store::default();
        store.git.ai = false;
        store.git.style = store::prefs::git::Style::Recent;
        let rows = Section::Git.rows();
        let prompt = rows.iter().find(|r| r.id == "ai-prompt").expect("the custom prompt row");
        let steps = plan(prompt, rows, &mut store.settings_view(), 2).expect("two steps unhide it");
        assert_eq!(steps.len(), 2);
        assert_eq!(button_label(&steps), "Apply");
        steps.iter().for_each(|s| s.apply(&mut store));
        assert!(prompt.allowed(&store));
    }

    #[test]
    fn rows_that_only_preview_a_setting_are_not_found() {
        let ids = row_ids(&found("preview", &Store::default(), &[]));
        assert!(ids.iter().all(|(_, id)| !PREVIEWS.contains(id)));
    }

    #[test]
    fn a_row_is_scrolled_to_the_middle_and_kept_inside_the_page() {
        assert_eq!(centred(500., 40., 400., 2000.), -320.);
        assert_eq!(centred(100., 40., 400., 2000.), 0.);
        assert_eq!(centred(1900., 40., 400., 1500.), -1500.);
    }

    #[test]
    fn a_list_row_links_to_its_page_and_a_single_row_is_drawn_live() {
        let projects = Section::Projects.rows().iter().find(|r| r.id == "projects").expect("the projects list");
        let delete = Section::Projects.rows().iter().find(|r| r.id == "delete-branch").expect("a single row");
        assert!(!is_live(projects) && is_live(delete));
    }
}
