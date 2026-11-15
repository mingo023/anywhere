use super::import::{self, Entry, Preview, skill_of};
use crate::composer::model_menu::listed_name;
use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::settings::action;
use crate::settings::dropdown::dropdown;
use crate::settings::models::model_rows;
use agents::Event;
use agents::factory::{Profile, ProfileFile, ProfileInfo};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use std::path::PathBuf;
use theme::*;
use ui::{self, Segment, Variant};

const WIDTH: f32 = 560.;
const LINE: f32 = 22.;
/// The preview rows shown before the list scrolls.
const LINES: usize = 10;

pub struct ProfileSheet {
    /// Which opening of the sheet this is, so an answer for an earlier one is dropped.
    pub(crate) serial: u64,
    pub(crate) draft: Draft,
    pub(crate) name: Entity<InputState>,
    pub(crate) soul: Entity<TextareaState>,
    pub(crate) instructions: Entity<TextareaState>,
    /// An import that was read and is not yet taken, and its rows.
    pub(crate) preview: Option<Preview>,
    pub(crate) lines: Vec<Line>,
    /// Why the last import, save or delete was refused.
    pub(crate) refused: Option<String>,
    /// The skills on this Mac, while their list is open.
    pub(crate) installed: Option<Vec<(String, PathBuf)>>,
    /// The profile asked for and not yet here.
    pub(crate) loading: Option<String>,
    /// A save or a delete that pocketd hasn't answered.
    pub(crate) sent: bool,
    _subs: Vec<Subscription>,
}

/// The providers the sheet offers: the ones the built-in profiles run on.
pub(crate) const PROVIDERS: [&str; 2] = ["claude", "codex"];

/// A skill of the profile being edited, as its row shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Skill {
    pub name: String,
    pub files: usize,
    /// It has a file that isn't markdown.
    pub runs: bool,
}

/// The profile being edited, without the three texts its fields hold.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    /// Empty for a new profile.
    pub id: String,
    pub provider: String,
    pub model: String,
    pub built_in: bool,
    pub files: Vec<ProfileFile>,
}

impl Draft {
    pub fn new() -> Self {
        Self { provider: PROVIDERS[0].into(), ..Default::default() }
    }

    pub fn of(p: &Profile) -> Self {
        Self { id: p.info.id.clone(), provider: p.info.provider.clone(), model: p.info.model.clone(), built_in: p.info.built_in, files: p.files.clone() }
    }

    pub fn skills(&self) -> Vec<Skill> {
        let mut out: Vec<Skill> = Vec::new();
        for f in &self.files {
            let Some(name) = skill_of(&f.path) else { continue };
            let runs = !f.path.to_lowercase().ends_with(".md");
            match out.iter_mut().find(|s| s.name == name) {
                Some(s) => (s.files, s.runs) = (s.files + 1, s.runs || runs),
                None => out.push(Skill { name: name.into(), files: 1, runs }),
            }
        }
        out
    }

    pub fn remove_skill(&mut self, name: &str) {
        self.files.retain(|f| skill_of(&f.path) != Some(name));
    }

    /// Takes an import's skills. One whose name the draft already has replaces it.
    pub fn take(&mut self, preview: &Preview) {
        for name in preview.profile.files.iter().filter_map(|f| skill_of(&f.path)) {
            self.remove_skill(name);
        }
        self.files.extend(preview.profile.files.iter().cloned());
    }

    /// What `factory.profile.save` carries.
    pub fn profile(&self, name: &str, soul: &str, instructions: &str) -> Profile {
        Profile {
            info: ProfileInfo { id: self.id.clone(), name: name.trim().into(), provider: self.provider.clone(), model: self.model.clone(), skills: self.skills().len() as u32, built_in: false },
            soul: soul.into(),
            instructions: instructions.into(),
            files: self.files.clone(),
        }
    }
}

/// Whether a profile other than `id` has `name`, whatever its case.
pub fn taken(name: &str, id: &str, profiles: &[ProfileInfo]) -> bool {
    let name = name.trim().to_lowercase();
    profiles.iter().any(|p| p.id != id && p.name.to_lowercase() == name)
}

/// Save needs a name no other profile has, and no import waiting to be taken or dropped.
pub fn can_save(name: &str, id: &str, profiles: &[ProfileInfo], previewing: bool) -> bool {
    !name.trim().is_empty() && !taken(name, id, profiles) && !previewing
}

pub fn title(draft: &Draft, name: &str) -> String {
    match (draft.built_in, draft.id.is_empty()) {
        (true, _) => name.to_string(),
        (_, true) => "New profile".into(),
        _ => "Edit profile".into(),
    }
}

/// A row of the import preview: entry `i`, or the heading over the files that aren't markdown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    File(usize),
    Runs,
}

pub fn lines(p: &Preview) -> Vec<Line> {
    let (runs, reads): (Vec<usize>, Vec<usize>) = (0..p.entries.len()).partition(|&i| p.entries[i].runs);
    let mut out: Vec<Line> = reads.into_iter().map(Line::File).collect();
    if !runs.is_empty() {
        out.push(Line::Runs);
        out.extend(runs.into_iter().map(Line::File));
    }
    out
}

/// What an import's result puts on the sheet: the rows, the preview and why not.
type Landed = (Vec<Line>, Option<Preview>, Option<String>);

/// `result` of an import asked of sheet `asked`, unless sheet `open` is another by the time it lands.
fn landed(open: u64, asked: u64, result: Result<Preview, String>) -> Option<Landed> {
    (open == asked).then(|| match result {
        Ok(p) => (lines(&p), Some(p), None),
        Err(why) => (Vec::new(), None, Some(why)),
    })
}

pub fn size(bytes: usize) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KB", bytes.div_ceil(1024)),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.),
    }
}

impl Desktop {
    /// Opens the sheet on profile `id`, or on a new one.
    pub(crate) fn edit_profile(&mut self, id: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let info = id.and_then(|id| self.agents.profiles().iter().find(|p| p.id == id)).cloned();
        let draft = info.as_ref().map_or_else(Draft::new, |p| Draft { id: p.id.clone(), provider: p.provider.clone(), model: p.model.clone(), built_in: p.built_in, files: Vec::new() });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Go backend"));
        let soul = cx.new(|cx| TextareaState::new(window, cx).placeholder("You are a careful engineer who…").auto_grow(3, 8));
        let instructions = cx.new(|cx| TextareaState::new(window, cx).placeholder("Write a failing test first…").auto_grow(3, 8));
        if let Some(p) = &info {
            name.update(cx, |s, cx| s.set_value(p.name.clone(), window, cx));
        }
        let loading = info.filter(|p| !p.built_in).map(|p| p.id);
        if let Some(id) = &loading {
            self.outbox.local.profile_get(id);
        }
        self.load_models_of(&draft.provider.clone(), cx);
        let subs = vec![cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify())];
        self.factory.sheets += 1;
        self.factory.profile = Some(ProfileSheet { serial: self.factory.sheets, draft, name, soul, instructions, preview: None, lines: Vec::new(), refused: None, installed: None, loading, sent: false, _subs: subs });
        self.open(Overlay::Profile, window, cx);
    }

    fn close_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        (self.factory.profile, self.settings.menu) = (None, None);
        self.close_overlay(window, cx);
    }

    /// What pocketd says while the sheet is open.
    pub(crate) fn profile_event(&mut self, ev: &Event, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay != Some(Overlay::Profile) {
            return;
        }
        let Some(sheet) = self.factory.profile.as_mut() else { return };
        match ev {
            Event::FactoryProfile(p) if sheet.loading.as_deref() == Some(p.info.id.as_str()) => {
                (sheet.loading, sheet.draft) = (None, Draft::of(p));
                sheet.name.update(cx, |s, cx| s.set_value(p.info.name.clone(), window, cx));
                sheet.soul.update(cx, |s, cx| s.set_value(p.soul.clone(), window, cx));
                sheet.instructions.update(cx, |s, cx| s.set_value(p.instructions.clone(), window, cx));
            }
            Event::FactoryProfiles(_) if sheet.sent => self.close_profile(window, cx),
            Event::FactoryError(why) if sheet.sent => (sheet.sent, sheet.refused) = (false, Some(why.clone())),
            _ => {}
        }
    }

    fn save_profile(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.factory.profile.as_mut() else { return };
        let name = sheet.name.read(cx).value().to_string();
        if sheet.sent || !can_save(&name, &sheet.draft.id, self.agents.profiles(), sheet.preview.is_some()) {
            return;
        }
        let (soul, instructions) = (sheet.soul.read(cx).value().to_string(), sheet.instructions.read(cx).value().to_string());
        (sheet.sent, sheet.refused) = (true, None);
        self.outbox.local.profile_save(&sheet.draft.profile(&name, &soul, &instructions));
        cx.notify();
    }

    fn delete_profile(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.factory.profile.as_mut().filter(|s| !s.sent && !s.draft.built_in && !s.draft.id.is_empty()) else { return };
        (sheet.sent, sheet.refused) = (true, None);
        self.outbox.local.profile_delete(&sheet.draft.id);
        cx.notify();
    }

    fn set_profile_provider(&mut self, provider: &'static str, cx: &mut Context<Self>) {
        if let Some(sheet) = self.factory.profile.as_mut() {
            (sheet.draft.provider, sheet.draft.model) = (provider.into(), String::new());
        }
        self.settings.menu = None;
        self.load_models_of(provider, cx);
        cx.notify();
    }

    /// Reads off the UI thread what `read` reads, and shows it as a preview or says why not.
    fn preview_import(&mut self, read: impl FnOnce() -> Result<Preview, String> + Send + 'static, cx: &mut Context<Self>) {
        let Some(asked) = self.factory.profile.as_ref().map(|s| s.serial) else { return };
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { read() }).await;
            this.update(cx, |d, cx| {
                if let Some(sheet) = d.factory.profile.as_mut()
                    && let Some(shown) = landed(sheet.serial, asked, result)
                {
                    (sheet.lines, sheet.preview, sheet.refused) = shown;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// One panel takes a folder or a `.zip`, which `pick_path` can't ask for.
    fn import_profile(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: true, multiple: false, prompt: None });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            this.update(cx, |d, cx| d.preview_import(move || import::read(&path), cx)).ok();
        })
        .detach();
    }

    fn toggle_installed(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.factory.profile.as_mut() else { return };
        if sheet.installed.take().is_some() {
            return cx.notify();
        }
        let home = PathBuf::from(crate::modals::form::home());
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { import::installed(&home) }).await;
            this.update(cx, |d, cx| {
                if let Some(sheet) = d.factory.profile.as_mut() {
                    sheet.installed = Some(found);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn use_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.factory.profile.as_mut() else { return };
        let Some(preview) = sheet.preview.take() else { return };
        sheet.draft.take(&preview);
        let Profile { soul, instructions, .. } = preview.profile;
        if !soul.is_empty() {
            sheet.soul.update(cx, |s, cx| s.set_value(soul, window, cx));
        }
        if !instructions.is_empty() {
            sheet.instructions.update(cx, |s, cx| s.set_value(instructions, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn profile_sheet(&mut self, cx: &mut Context<Self>) -> Div {
        // Taken out for this frame, so the parts can read the sheet while they borrow `self` for their listeners.
        let Some(sheet) = self.factory.profile.take() else { return div() };
        let view = self.profile_view(&sheet, cx);
        self.factory.profile = Some(sheet);
        view
    }

    fn profile_view(&mut self, sheet: &ProfileSheet, cx: &mut Context<Self>) -> Div {
        let draft = &sheet.draft;
        let name = sheet.name.read(cx).value().to_string();
        let savable = !sheet.sent && sheet.loading.is_none() && can_save(&name, &draft.id, self.agents.profiles(), sheet.preview.is_some());
        let head = div().pt(px(20.)).px(px(22.)).pb(px(14.)).text_size(px(17.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title(draft, &name));
        let close = ui::button("profile-close", Variant::Ghost, None, if draft.built_in { "Close" } else { "Cancel" }).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_profile(window, cx)));
        let delete = (!draft.built_in && !draft.id.is_empty()).then(|| ui::button("profile-delete", Variant::Ghost, None, "Delete").text_color(FAILED).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.delete_profile(cx))));
        let save = (!draft.built_in).then(|| {
            ui::button("profile-save", Variant::Primary, None, "Save profile")
                .when(savable, |b| b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save_profile(cx))))
                .when(!savable, |b| b.opacity(0.4))
        });
        let foot = div().pt(px(14.)).pb(px(16.)).px(px(16.)).flex().items_center().gap(px(8.)).children(delete).child(div().flex_1()).child(close).children(save);
        let body = if draft.built_in { fixed(&name, self.agent_name(&draft.provider), &draft.model) } else { self.profile_fields(sheet, &name, cx) };
        div().absolute().inset_0().flex().items_center().justify_center().child(
            ui::pop(div().id("profile-sheet"))
                .w(px(WIDTH))
                .max_h(relative(0.9))
                .rounded(px(R_DIALOG))
                .occlude()
                .flex()
                .flex_col()
                .child(head)
                .child(div().id("profile-body").flex_1().min_h_0().overflow_y_scroll().px(px(22.)).pb(px(14.)).child(body))
                .child(div().flex_none().border_t(px(0.5)).border_color(HAIRLINE).child(foot)),
        )
    }

    fn profile_fields(&mut self, sheet: &ProfileSheet, name: &str, cx: &mut Context<Self>) -> Div {
        let draft = &sheet.draft;
        let name_taken = taken(name, &draft.id, self.agents.profiles());
        let name_field = field("Name", "")
            .child(ui::field_box().child(Input::new(&sheet.name).appearance(false).p_0().text_size(px(13.5))))
            .children(name_taken.then(|| div().text_size(px(12.)).text_color(FAILED).child("Another profile has this name")));
        let active = PROVIDERS.into_iter().find(|p| *p == draft.provider).unwrap_or(PROVIDERS[0]);
        let segments = PROVIDERS.into_iter().map(|p| Segment { icon: None, value: p, label: self.agent_name(p).into(), badge: None }).collect();
        let providers = ui::segmented(segments, active, true, false, |this: &mut Self, provider, cx| this.set_profile_provider(provider, cx), cx);
        let (provider, model) = (draft.provider.as_str(), draft.model.clone());
        let offered = self.offered_models(provider);
        let trigger = action("profile-model", "").child(listed_name(&model, offered)).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let from = (provider, self.models.revision, self.installed(provider), &model);
        let pick = |d: &mut Desktop, value: String, _: &mut Context<Desktop>| {
            if let Some(sheet) = d.factory.profile.as_mut() {
                sheet.draft.model = value;
            }
        };
        let models = dropdown("profile-model", trigger, from, || model_rows(provider, offered, &model), &model, self, pick, cx);
        let area = |state: &Entity<TextareaState>| ui::field_box().h_auto().min_h(px(72.)).px(px(2.)).py(px(4.)).items_start().child(div().flex_1().min_w_0().text_size(px(13.5)).child(Textarea::new(state).appearance(false)));
        let skills = draft.skills();
        let rows: Vec<Div> = if skills.is_empty() {
            vec![div().py(px(6.)).text_size(px(12.5)).text_color(TEXT_3).child("No skills")]
        } else {
            skills.into_iter().enumerate().map(|(i, s)| self.profile_skill(i, s, cx)).collect()
        };
        let bring = match &sheet.preview {
            Some(p) => self.profile_preview(p, sheet.lines.len(), cx),
            None => self.profile_import(sheet.installed.as_deref(), cx),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(name_field)
            .child(field("Model", "").child(div().flex().items_center().gap(px(10.)).child(providers).child(models)))
            .child(field("Soul", "Who the agent is: tone, habits, limits").child(area(&sheet.soul)))
            .child(field("Instructions", "How it works on a task").child(area(&sheet.instructions)))
            .child(field("Skills", "Copied into the agent's checkout, never into a commit").children(rows).child(bring))
            .children(sheet.refused.clone().map(|why| div().text_size(px(12.5)).text_color(FAILED).child(why)))
    }

    fn profile_skill(&self, i: usize, s: Skill, cx: &mut Context<Self>) -> Div {
        let files = if s.files == 1 { "1 file".to_string() } else { format!("{} files", s.files) };
        let meta = if s.runs { format!("{files} · has files the agent can run") } else { files };
        let name = s.name.clone();
        let remove = ui::icon_button_sized(("profile-skill-remove", i), "x", 20., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            if let Some(sheet) = this.factory.profile.as_mut() {
                sheet.draft.remove_skill(&name);
            }
            cx.notify();
        }));
        div()
            .h(px(30.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon("folder", 14., TEXT_3))
            .child(div().flex_none().font_family(MONO).text_size(px(12.5)).child(s.name))
            .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).child(meta))
            .child(remove)
    }

    /// The two ways to bring files in, and the skills on this Mac while that list is open.
    fn profile_import(&self, installed: Option<&[(String, PathBuf)]>, cx: &mut Context<Self>) -> Div {
        let folder = ui::button("profile-import", Variant::Secondary, None, "Import folder or zip").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.import_profile(cx)));
        let mine = ui::button("profile-installed", Variant::Secondary, None, "Add an installed skill").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_installed(cx)));
        let list = installed.map(|found| {
            let rows: Vec<AnyElement> = if found.is_empty() {
                vec![div().p(px(8.)).text_size(px(12.5)).text_color(TEXT_3).child("No skills in ~/.claude/skills or ~/.agents/skills").into_any_element()]
            } else {
                found
                    .iter()
                    .enumerate()
                    .map(|(i, (name, dir))| {
                        let dir = dir.clone();
                        div()
                            .id(("profile-installed-skill", i))
                            .h(px(26.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(FILL_2))
                            .font_family(MONO)
                            .text_size(px(12.5))
                            .child(name.clone())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Some(sheet) = this.factory.profile.as_mut() {
                                    sheet.installed = None;
                                }
                                let dir = dir.clone();
                                this.preview_import(move || import::read_skill(&dir), cx);
                            }))
                            .into_any_element()
                    })
                    .collect()
            };
            div().id("profile-installed-list").max_h(px(156.)).overflow_y_scroll().p(px(4.)).flex().flex_col().rounded(px(9.)).bg(FILL_1).children(rows)
        });
        div().pt(px(6.)).flex().flex_col().gap(px(8.)).child(div().flex().gap(px(8.)).child(folder).child(mine)).children(list)
    }

    /// What an import holds: every file with its size, the ones the agent can run under their own heading.
    fn profile_preview(&self, p: &Preview, count: usize, cx: &mut Context<Self>) -> Div {
        let files = uniform_list(
            "profile-preview-files",
            count,
            cx.processor(|this, range: Range<usize>, _, _| {
                let Some((p, lines)) = this.factory.profile.as_ref().and_then(|s| Some((s.preview.as_ref()?, &s.lines))) else { return Vec::new() };
                lines.get(range).unwrap_or_default().iter().map(|line| preview_line(p, *line)).collect::<Vec<_>>()
            }),
        )
        .h(px(LINE * count.min(LINES) as f32));
        let left_out = (p.left_out > 0).then(|| div().text_size(px(12.)).text_color(TEXT_3).child(format!("{} left out. They are no part of a profile.", if p.left_out == 1 { "1 file".to_string() } else { format!("{} files", p.left_out) })));
        let cancel = ui::button("profile-preview-cancel", Variant::Ghost, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            if let Some(sheet) = this.factory.profile.as_mut() {
                sheet.preview = None;
            }
            cx.notify();
        }));
        let take = ui::button("profile-preview-use", Variant::Primary, None, "Use these files").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.use_import(window, cx)));
        div()
            .mt(px(6.))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(9.))
            .bg(FILL_1)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.5))
                    .child(icon("folder", 14., TEXT_3))
                    .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(p.from.clone()))
                    .child(div().flex_none().text_color(TEXT_3).child(format!("{} of 2 MB", size(p.size())))),
            )
            .child(files)
            .children(left_out)
            .child(div().text_size(px(12.)).text_color(TEXT_3).child("AGENTS.md is read as the instructions. Nothing is saved until you save the profile."))
            .child(div().flex().justify_end().gap(px(8.)).child(cancel).child(take))
    }
}

fn field(label: &'static str, hint: &'static str) -> Div {
    div().flex().flex_col().gap(px(6.)).child(div().flex().items_baseline().gap(px(8.)).child(ui::field_label(label)).child(div().text_size(px(12.)).text_color(TEXT_4).child(hint)))
}

/// A built-in profile: nothing in it can change.
fn fixed(name: &str, agent: String, model: &str) -> Div {
    let line = |label: &'static str, value: String| field(label, "").child(div().text_size(px(13.5)).text_color(TEXT).child(value));
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(div().text_size(px(13.)).line_height(px(19.)).text_color(TEXT_2).child("A built-in profile has no soul, instructions or skills, and can't be changed."))
        .child(line("Name", name.to_string()))
        .child(line("Model", if model.is_empty() { agent } else { format!("{agent} · {model}") }))
}

fn preview_line(p: &Preview, line: Line) -> Div {
    let row = div().w_full().h(px(LINE)).flex().items_center().gap(px(8.)).text_size(px(12.));
    match line {
        Line::Runs => row.font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child("Files the agent can run"),
        Line::File(i) => {
            let Some(Entry { path, size: bytes, .. }) = p.entries.get(i) else { return row };
            row.child(div().flex_1().min_w_0().truncate().font_family(MONO).child(path.clone())).child(div().flex_none().text_color(TEXT_3).child(size(*bytes)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Draft, Line, Skill, can_save, landed, lines, size, taken, title};
    use crate::factory::import::{Entry, Preview};
    use agents::factory::{Profile, ProfileFile, ProfileInfo};

    fn file(path: &str) -> ProfileFile {
        ProfileFile { path: path.into(), text: "x".into(), binary: false }
    }

    fn info(id: &str, name: &str) -> ProfileInfo {
        ProfileInfo { id: id.into(), name: name.into(), ..Default::default() }
    }

    fn paths(draft: &Draft) -> Vec<&str> {
        draft.files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn an_import_lands_on_the_sheet_it_was_asked_for_and_not_on_one_opened_since() {
        let preview = Preview { entries: vec![Entry { path: "AGENTS.md".into(), size: 1, runs: false }], ..Default::default() };
        let (rows, shown, refused) = landed(3, 3, Ok(preview.clone())).unwrap();
        assert_eq!((rows, shown, refused), (vec![Line::File(0)], Some(preview.clone()), None));
        assert_eq!(landed(3, 3, Err("no".into())), Some((Vec::new(), None, Some("no".into()))));
        assert_eq!(landed(4, 3, Ok(preview)), None);
        assert_eq!(landed(4, 3, Err("no".into())), None);
    }

    #[test]
    fn a_name_another_profile_has_cannot_be_saved_whatever_its_case() {
        let profiles = [info("claude-opus", "Claude · Opus"), info("go", "Go backend")];
        assert!(taken(" go BACKEND ", "", &profiles));
        assert!(!taken("Go backend", "go", &profiles), "a profile keeps its own name");
        assert!(!taken("Rust", "", &profiles));
        let saves = [can_save("Rust", "", &profiles, false), can_save("  ", "", &profiles, false), can_save("go backend", "", &profiles, false), can_save("Rust", "", &profiles, true)];
        assert_eq!(saves, [true, false, false, false]);
    }

    #[test]
    fn skills_are_listed_by_folder_with_their_file_count_and_whether_one_can_run() {
        let draft = Draft { files: vec![file("skills/review/SKILL.md"), file("skills/review/run.sh"), file("skills/notes/SKILL.md")], ..Draft::new() };
        assert_eq!(draft.skills(), [Skill { name: "review".into(), files: 2, runs: true }, Skill { name: "notes".into(), files: 1, runs: false }]);
    }

    #[test]
    fn removing_a_skill_drops_its_files_only() {
        let mut draft = Draft { files: vec![file("skills/review/SKILL.md"), file("skills/review/run.sh"), file("skills/notes/SKILL.md")], ..Draft::new() };
        draft.remove_skill("review");
        assert_eq!(paths(&draft), ["skills/notes/SKILL.md"]);
    }

    #[test]
    fn taking_an_import_adds_its_skills_and_replaces_one_with_the_same_name() {
        let mut draft = Draft { files: vec![file("skills/review/SKILL.md"), file("skills/review/old.sh"), file("skills/notes/SKILL.md")], ..Draft::new() };
        let preview = Preview { profile: Profile { files: vec![file("skills/review/SKILL.md"), file("skills/deploy/SKILL.md")], ..Default::default() }, ..Default::default() };
        draft.take(&preview);
        assert_eq!(paths(&draft), ["skills/notes/SKILL.md", "skills/review/SKILL.md", "skills/deploy/SKILL.md"]);
    }

    #[test]
    fn the_saved_profile_carries_the_trimmed_name_the_texts_and_the_skill_count() {
        let draft = Draft { id: "go".into(), provider: "codex".into(), model: "gpt-5".into(), built_in: false, files: vec![file("skills/review/SKILL.md")] };
        let p = draft.profile(" Go backend ", "Soul", "How");
        let want = Profile { info: ProfileInfo { id: "go".into(), name: "Go backend".into(), provider: "codex".into(), model: "gpt-5".into(), skills: 1, built_in: false }, soul: "Soul".into(), instructions: "How".into(), files: vec![file("skills/review/SKILL.md")] };
        assert_eq!(p, want);
        assert_eq!(Draft::of(&p), draft);
        assert_eq!(Draft::new().provider, "claude");
    }

    #[test]
    fn the_sheet_is_named_for_what_it_edits() {
        let (new, mine, fixed) = (Draft::new(), Draft { id: "go".into(), ..Draft::new() }, Draft { id: "codex".into(), built_in: true, ..Draft::new() });
        assert_eq!((title(&new, ""), title(&mine, "Go backend"), title(&fixed, "Codex")), ("New profile".to_string(), "Edit profile".to_string(), "Codex".to_string()));
    }

    #[test]
    fn the_preview_lists_markdown_first_then_the_files_the_agent_can_run() {
        let entry = |path: &str, runs| Entry { path: path.into(), size: 1, runs };
        let preview = Preview { entries: vec![entry("SOUL.md", false), entry("skills/a/run.sh", true), entry("skills/a/SKILL.md", false)], ..Default::default() };
        assert_eq!(lines(&preview), [Line::File(0), Line::File(2), Line::Runs, Line::File(1)]);
        let plain = Preview { entries: vec![entry("SOUL.md", false)], ..Default::default() };
        assert_eq!(lines(&plain), [Line::File(0)]);
    }

    #[test]
    fn sizes_read_in_bytes_then_kilobytes_then_megabytes() {
        assert_eq!([size(512), size(1024), size(1500), size(1_572_864)], ["512 B", "1 KB", "2 KB", "1.5 MB"]);
    }
}

