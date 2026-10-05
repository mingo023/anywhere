mod appearance;
mod general;
mod keybindings;
mod nav;
mod worktrees;

pub(crate) use appearance::{forced, reduce_motion};

use crate::actions::OpenSettings;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Screen, drag_area};
use gpui_kit::*;
use theme::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Section {
    General,
    Appearance,
    Keybindings,
    Worktrees,
}

impl Section {
    pub const ALL: [Section; 4] = [Section::General, Section::Appearance, Section::Keybindings, Section::Worktrees];

    fn label(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Keybindings => "Keybindings",
            Section::Worktrees => "Worktree management",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::General => "settings",
            Section::Appearance => "sparkle",
            Section::Keybindings => "prompt",
            Section::Worktrees => "worktree",
        }
    }
}

/// The section on show, the screen closing Settings returns to, and whether Keybindings' fixed keys are unfolded.
pub struct SettingsState {
    pub section: Section,
    back: Screen,
    pub fixed_open: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self { section: Section::General, back: Screen::Sessions, fixed_open: false }
    }
}

impl SettingsState {
    /// Remembers `from` so closing returns there; reopening from Settings keeps the first screen.
    pub fn open(&mut self, from: Screen) {
        if from != Screen::Settings {
            self.back = from;
        }
    }

    pub fn back(&self) -> Screen {
        self.back
    }
}

fn row(title: impl IntoElement, hint: Option<&str>, control: impl IntoElement) -> Div {
    div()
        .min_h(px(44.))
        .px(px(14.))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(16.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).text_color(TEXT).child(title))
                .children(hint.map(|h| div().text_size(px(11.5)).text_color(TEXT_2).child(h.to_string()))),
        )
        .child(div().flex_none().max_w(relative(0.6)).flex().justify_end().child(control))
}

fn group(title: &str, rows: Vec<Div>) -> Div {
    div().flex().flex_col().gap(px(6.)).child(div().px(px(14.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(title.to_string())).child(card(rows))
}

/// Rows on one surface; separators start where the text does, as in macOS.
fn card(rows: Vec<Div>) -> Div {
    let rows = rows.into_iter().enumerate().flat_map(|(i, r)| (i > 0).then(|| div().h(px(0.5)).ml(px(14.)).bg(HAIRLINE)).into_iter().chain([r]));
    div().flex().flex_col().rounded(px(10.)).bg(SURFACE).shadow(vec![ui::ring(HAIRLINE, 0.5)]).overflow_hidden().children(rows)
}

impl Desktop {
    pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.settings.open(self.screen);
        self.screen = Screen::Settings;
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = self.settings.back();
        if self.screen == Screen::Inbox {
            window.focus(&self.inbox.focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn settings_page(&mut self, cx: &mut Context<Self>) -> Div {
        let section = self.settings.section;
        let body = match section {
            Section::General => self.general_settings(cx),
            Section::Appearance => self.appearance_settings(cx),
            Section::Keybindings => self.keybinding_settings(cx),
            Section::Worktrees => self.worktree_settings(cx),
        };
        let bar = drag_area(ui::page_bar()).justify_center().child(div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(section.label()));
        let column = div().w_full().max_w(px(600.)).px(px(24.)).pt(px(12.)).pb(px(40.)).child(body);
        ui::page(div())
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(bar)
            .child(div().id("settings-body").flex_1().min_h_0().overflow_y_scroll().child(div().w_full().flex().justify_center().child(column)))
    }
}

#[cfg(test)]
mod tests {
    use super::{Section, SettingsState};
    use crate::desktop::chrome::Screen;

    #[test]
    fn sections_follow_the_spec_order() {
        assert_eq!(Section::ALL.map(Section::label), ["General", "Appearance", "Keybindings", "Worktree management"]);
    }

    #[test]
    fn closing_settings_returns_to_the_screen_it_came_from() {
        let mut s = SettingsState::default();
        assert_eq!(s.back(), Screen::Sessions);
        s.open(Screen::Inbox);
        s.open(Screen::Settings);
        assert_eq!(s.back(), Screen::Inbox);
        s.open(Screen::Sessions);
        assert_eq!(s.back(), Screen::Sessions);
    }
}
