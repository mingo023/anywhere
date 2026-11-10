use super::logic::{Field, Surface, busy, split_draft, tab_for_new};
use super::parts::{bar_button, flush_textarea, key_chip, start_kbd};
use super::values::{Picks, Values, default_folder, needs_aim};
use crate::desktop::Desktop;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const HERO_WIDTH: f32 = 652.;
const PLACEHOLDER: &str = "Fix the flaky login test";

pub struct Composer {
    pub(super) text: Entity<TextareaState>,
    pub(super) picks: Picks,
}

impl Composer {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let text = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 8).submit_on_enter(true));
        let subs = vec![cx.subscribe_in(&text, window, |this: &mut Desktop, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { shift: false, secondary } => this.create_ticket(*secondary, window, cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        })];
        (Self { text, picks: Picks::new(Values::new_in(String::new())) }, subs)
    }
}

impl Desktop {
    /// Points the composer at the filtered project, or another when its own was removed.
    pub(super) fn aim_board_composer(&mut self, cx: &mut Context<Self>) {
        let projects = self.projects();
        if !needs_aim(&self.board.composer.picks.values.folder, self.board.filters.project.only().map(String::as_str), &projects) {
            return;
        }
        let folder = default_folder(self.board.filters.project.only().map(String::as_str), self.project.as_deref(), projects.first().map(String::as_str));
        if folder == self.board.composer.picks.values.folder {
            return;
        }
        self.board.composer.picks.values.set_folder(folder.clone());
        self.load_ticket_branches(Surface::Composer, folder, cx);
    }

    /// After a refresh a project picked by hand stays, even off the filter; only one that's gone is replaced.
    pub(crate) fn rescue_board_composer(&mut self, cx: &mut Context<Self>) {
        if !self.projects().contains(&self.board.composer.picks.values.folder) {
            self.aim_board_composer(cx);
        }
    }

    pub(crate) fn focus_board_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.board.composer.text.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn composer_ready(&self, cx: &App) -> bool {
        let c = &self.board.composer;
        let (title, prompt) = split_draft(&c.text.read(cx).value());
        c.picks.values.ready(&title, &prompt, self.folder_is_repo(&c.picks.values.folder))
    }

    fn create_ticket(&mut self, start: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.composer_ready(cx) {
            return;
        }
        let c = &mut self.board.composer;
        let (title, prompt) = split_draft(&c.text.read(cx).value());
        self.outbox.local.board_save(c.picks.values.draft(&title, &prompt), start);
        (c.picks.values.priority, c.picks.values.labels) = (0, Vec::new());
        c.text.update(cx, |s, cx| s.set_value("", window, cx));
        self.board.tab = tab_for_new(self.board.tab, start);
        self.board.filters = Default::default();
        self.board.search.update(cx, |s, cx| s.set_value("", window, cx));
        self.board_changed();
        cx.notify();
    }

    fn board_composer(&self, hero: bool, cx: &mut Context<Self>) -> Div {
        let c = &self.board.composer;
        let ready = self.composer_ready(cx);
        let text_size = px(if hero { 14. } else { 13. });
        // The input draws its placeholder in the theme's muted colour, too bright for this field.
        let placeholder = c.text.read(cx).value().is_empty().then(|| div().absolute().top(px(8.)).left(px(10.)).text_size(text_size).text_color(TEXT_4).child(PLACEHOLDER));
        let field = flush_textarea(div().relative()).when(hero, |d| d.min_h(px(76.))).when(!hero, |d| d.line_height(px(18.))).child(Textarea::new(&c.text).appearance(false).text_size(text_size)).children(placeholder);
        let chips = [Field::Project, Field::Agent, Field::Priority, Field::Labels, Field::Base].map(|f| self.field_chip(Surface::Composer, f, cx));
        let hint = c.picks.values.hint(self.folder_is_repo(&c.picks.values.folder), busy(&self.agents.board.tickets), self.agents.board.limit);
        let spot = div().flex_1().min_w_0().px(px(8.)).truncate().text_right().text_size(px(12.)).text_color(TEXT_3).children(hint);
        let dim = |b: Stateful<Div>| b.when(!ready, |d| d.opacity(0.4).cursor_default());
        let add = dim(bar_button("board-add", Variant::Ghost, None, "Add to To do")).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.create_ticket(false, window, cx)));
        // A whole-element opacity fades the label with its fill and washes it out.
        let start = bar_button("board-start", Variant::Primary, None, "Start").child(start_kbd()).when(!ready, |d| d.bg(Hsla::from(PRIMARY).opacity(0.4)).shadow(Vec::new()).cursor_default()).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.create_ticket(true, window, cx)));
        let foot = div().flex().items_center().gap(px(2.)).mt(px(if hero { 9. } else { 5. })).mx(px(-4.)).children(chips).child(spot).child(add).child(start);
        let (side, top, bottom) = if hero { (12., 12., 9.) } else { (10., 9., 6.) };
        div().px(px(side)).pt(px(top)).pb(px(bottom)).rounded(px(12.)).bg(FILL_1).border_1().border_color(SEPARATOR).child(field).child(foot)
    }

    pub(super) fn board_hero(&self, cx: &mut Context<Self>) -> Div {
        let heading = div().mb(px(16.)).text_center().text_size(px(19.)).line_height(px(24.)).font_weight(FontWeight::SEMIBOLD).child("What should an agent do?");
        let hint = div().mt(px(12.)).flex().items_center().justify_center().gap(px(5.)).text_size(px(12.)).text_color(TEXT_4).child(key_chip("↵", FILL_2)).child("adds to To do").child("·").child(key_chip("⌘↵", FILL_2)).child("starts right away");
        div().flex_1().min_h_0().flex().items_center().justify_center().p(px(24.)).child(div().w_full().max_w(px(HERO_WIDTH)).flex().flex_col().child(heading).child(self.board_composer(true, cx)).child(hint))
    }

    pub(super) fn board_dock(&self, cx: &mut Context<Self>) -> Div {
        div().flex_none().px(px(16.)).pt(px(8.)).pb(px(14.)).child(self.board_composer(false, cx))
    }
}
