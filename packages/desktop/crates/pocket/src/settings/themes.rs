use super::catalog::Setting;
use super::dropdown::{Choice, Row, dropdown};
use super::{action, row, sub_title};
use crate::desktop::Desktop;
use crate::pick_menu::grouped;
use crate::themes::{Custom, SYSTEM, chosen, find, in_use, name, system_theme};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::{Appearance, Store};
use theme::*;

/// Which of the rows that hold a theme a menu belongs to.
#[derive(Clone, Copy, PartialEq)]
enum Holds {
    Theme,
    Day,
    Night,
}

impl Holds {
    /// The appearance its themes must have; `None` takes either.
    fn only(self) -> Option<bool> {
        match self {
            Holds::Theme => None,
            Holds::Day => Some(false),
            Holds::Night => Some(true),
        }
    }

    fn held(self, a: &Appearance, custom: &[Custom]) -> String {
        match self.only() {
            None => chosen(a),
            Some(dark) => system_theme(a, custom, dark),
        }
        .to_string()
    }

    fn hold(self, a: &mut Appearance, id: String) {
        match self {
            Holds::Theme => a.theme = Some(id),
            Holds::Day => a.day = Some(id),
            Holds::Night => a.night = Some(id),
        }
    }
}

pub(super) const THEME: Setting = Setting::custom("theme", "Theme", "Theme", |d, s, cx| picker(d, s, Holds::Theme, cx))
    .hint("System follows macOS light and dark")
    .resets(|s| name(&[], chosen(&s.appearance)), |s, d| (s.appearance.theme, s.appearance.mode) = (d.appearance.theme.clone(), d.appearance.mode))
    .effect(apply);
pub(super) const DAY: Setting = Setting::custom("theme-day", "Theme", "Day", |d, s, cx| picker(d, s, Holds::Day, cx))
    .hint("While macOS is light")
    .under(follows_macos)
    .resets(|s| name(&[], s.appearance.day.as_deref().unwrap_or(GRAPHITE_LIGHT)), |s, d| s.appearance.day = d.appearance.day.clone())
    .effect(apply);
pub(super) const NIGHT: Setting = Setting::custom("theme-night", "Theme", "Night", |d, s, cx| picker(d, s, Holds::Night, cx))
    .hint("While macOS is dark")
    .under(follows_macos)
    .resets(|s| name(&[], s.appearance.night.as_deref().unwrap_or(GRAPHITE_DARK)), |s, d| s.appearance.night = d.appearance.night.clone())
    .effect(apply);
pub(super) const CUSTOM: Setting =
    Setting::custom("custom-themes", "Theme", "Custom themes", custom_themes).hint("Import a theme file, or start from one on GitHub");

fn follows_macos(s: &Store) -> bool {
    chosen(&s.appearance) == SYSTEM
}

fn apply(d: &mut Desktop, cx: &mut Context<Desktop>) {
    d.apply_theme(cx);
}

/// The menu's rows: System where either appearance will do, then the built-in and the imported themes under their groups.
fn menu(custom: &[Custom], only: Option<bool>) -> Vec<Row> {
    let wanted = |dark: bool| only.is_none_or(|only| only == dark);
    let built_in =
        |dark: bool| presets().iter().filter(|p| p.theme.dark == dark).map(|p| Choice { value: p.id.into(), label: p.theme.name.clone() }).collect::<Vec<_>>();
    let system = only.is_none().then(|| (None, vec![Choice { value: SYSTEM.into(), label: "System".into() }]));
    let groups =
        [("Light", false), ("Dark", true)].into_iter().filter(|(_, dark)| wanted(*dark)).map(|(group, dark)| (Some(group.to_string()), built_in(dark)));
    let imported: Vec<Choice> = custom.iter().filter(|c| wanted(c.theme.dark)).map(|c| Choice { value: c.id(), label: c.theme.name.clone() }).collect();
    grouped(system.into_iter().chain(groups).chain((!imported.is_empty()).then(|| (Some("Custom".to_string()), imported))))
}

/// A theme drawn small: its window, its text and its accent.
fn chip(t: &Theme) -> Div {
    div()
        .size(px(18.))
        .flex_none()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .bg(rgba(t.color(WINDOW_SOLID)))
        .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
        .text_size(px(9.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba(t.color(TEXT)))
        .child("Aa")
        .child(div().absolute().right(px(2.)).bottom(px(2.)).size(px(4.)).rounded_full().bg(rgba(t.color(PRIMARY))))
}

/// System drawn small: the day theme's window beside the night theme's.
fn system_chip(a: &Appearance, custom: &[Custom]) -> Div {
    let half =
        |dark: bool| div().flex_1().h_full().children(find(custom, system_theme(a, custom, dark)).map(|t| div().size_full().bg(rgba(t.color(WINDOW_SOLID)))));
    div().size(px(18.)).flex_none().flex().rounded(px(5.)).overflow_hidden().shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).child(half(false)).child(half(true))
}

fn picker(d: &mut Desktop, setting: &'static Setting, holds: Holds, cx: &mut Context<Desktop>) -> Div {
    let (a, custom) = (&d.store.appearance, &d.themes.custom);
    let held = holds.held(a, custom);
    let chip = match find(custom, &held) {
        Some(theme) => Some(chip(theme)),
        None => (held == SYSTEM).then(|| system_chip(a, custom)),
    };
    let trigger = action(setting.id, "").gap(px(6.)).children(chip).child(name(custom, &held)).child(icon("chevron-down", 12., TEXT_3));
    let from = (holds.only(), d.themes.edits);
    let menu = dropdown(
        setting.id,
        trigger,
        from,
        || menu(custom, holds.only()),
        &held,
        d,
        move |d, id, cx| {
            holds.hold(&mut d.store.appearance, id);
            d.apply_theme(cx);
        },
        cx,
    );
    let title = if holds == Holds::Theme { setting.label.into_any_element() } else { sub_title(setting.label).into_any_element() };
    row(title, setting.hint, menu)
}

/// Where to start a theme and how to bring one in, over the themes imported so far.
fn custom_themes(d: &mut Desktop, setting: &'static Setting, cx: &mut Context<Desktop>) -> Div {
    let starter = action("theme-starter", "Starter on GitHub")
        .gap(px(6.))
        .child(icon("external", 12., TEXT_3))
        .on_click(|_: &ClickEvent, _: &mut Window, cx: &mut App| cx.open_url(&crate::updates::themes_url(channel::version())));
    let import = action("theme-import", "Import").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.pick_theme_file(window, cx)));
    let error = d.themes.error.clone().map(|why| div().px(px(14.)).pb(px(10.)).text_size(px(12.)).line_height(px(16.)).text_color(FAILED_TEXT).child(why));
    let (a, custom) = (&d.store.appearance, &d.themes.custom);
    let imported: Vec<Div> = custom
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let slug = c.slug.clone();
            div()
                .h(px(34.))
                .pl(px(14.))
                .pr(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .border_t(px(0.5))
                .border_color(SEPARATOR)
                .child(chip(&c.theme))
                .child(div().min_w_0().truncate().text_size(px(13.)).text_color(TEXT).child(c.theme.name.clone()))
                .child(div().flex_1().text_size(px(12.)).text_color(TEXT_3).child(if c.theme.dark { "Dark" } else { "Light" }))
                .when(in_use(a, custom, &c.id()), |d| d.child(ui::tag("In use")))
                .child(
                    ui::icon_button_sized(("theme-remove", i), "trash", 24., TEXT_3)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_theme(&slug, cx))),
                )
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .child(row(setting.label, setting.hint, div().flex().items_center().gap(px(6.)).child(starter).child(import)))
        .children(error)
        .children(imported)
}

#[cfg(test)]
mod tests {
    use super::{CUSTOM, DAY, Holds, NIGHT, Row, THEME, menu};
    use crate::settings::catalog::Setting;
    use crate::themes::{Custom, SYSTEM};

    const ROWS: &[Setting] = &[THEME, DAY, NIGHT, CUSTOM];
    use store::{Appearance, Mode, Store};
    use theme::Theme;

    fn custom(name: &str, dark: bool) -> Custom {
        let appearance = if dark { "dark" } else { "light" };
        Custom { slug: store::themes::slug(name), theme: Theme::parse(&format!(r#"{{ "name": "{name}", "appearance": "{appearance}" }}"#)).unwrap() }
    }

    fn lines(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Heading(group) => format!("# {group}"),
                Row::Pick(choice) => format!("{} = {}", choice.label, choice.value),
            })
            .collect()
    }

    #[test]
    fn the_theme_menu_lists_system_then_light_dark_and_imported_themes() {
        let rows = lines(&menu(&[custom("Mine", true)], None));
        assert_eq!(rows[..3], ["System = system", "# Light", "Graphite Light = graphite-light"]);
        assert_eq!(rows[5..7], ["# Dark", "Graphite Dark = graphite-dark"]);
        assert_eq!(rows[rows.len() - 2..], ["# Custom", "Mine = custom/mine"]);
        assert!(!lines(&menu(&[], None)).contains(&"# Custom".to_string()));
    }

    #[test]
    fn day_and_night_offer_only_themes_of_their_appearance() {
        let mine = [custom("Sun", false), custom("Moon", true)];
        assert_eq!(
            lines(&menu(&mine, Holds::Day.only())),
            ["# Light", "Graphite Light = graphite-light", "GitHub Light = github-light", "Solarized Light = solarized-light", "# Custom", "Sun = custom/sun"]
        );
        let night = lines(&menu(&mine, Holds::Night.only()));
        assert_eq!((night[0].as_str(), night.len(), night.last().unwrap().as_str()), ("# Dark", 8, "Moon = custom/moon"));
    }

    #[test]
    fn each_row_holds_its_own_choice() {
        let mut a = Appearance::default();
        assert_eq!([Holds::Theme, Holds::Day, Holds::Night].map(|h| h.held(&a, &[])), [SYSTEM, "graphite-light", "graphite-dark"]);
        Holds::Theme.hold(&mut a, "dracula".into());
        Holds::Day.hold(&mut a, "github-light".into());
        Holds::Night.hold(&mut a, "one-dark".into());
        assert_eq!([Holds::Theme, Holds::Day, Holds::Night].map(|h| h.held(&a, &[])), ["dracula", "github-light", "one-dark"]);
    }

    #[test]
    fn day_and_night_show_only_while_the_theme_follows_macos() {
        let mut store = Store::default();
        let shown = |store: &Store| ROWS.iter().filter(|r| r.allowed(store)).map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(shown(&store), ["theme", "theme-day", "theme-night", "custom-themes"]);
        store.appearance.theme = Some("dracula".into());
        assert_eq!(shown(&store), ["theme", "custom-themes"]);
        store.appearance = Appearance { mode: Mode::Dark, ..Appearance::default() };
        assert_eq!(shown(&store), ["theme", "custom-themes"]);
    }

    #[test]
    fn the_theme_rows_name_their_choice_and_reset_to_system_and_graphite() {
        let mut store = Store::default();
        let values = |store: &Store| ROWS[..3].iter().map(|r| r.value(store).unwrap()).collect::<Vec<_>>();
        assert_eq!(values(&store), ["System", "Graphite Light", "Graphite Dark"]);
        store.appearance = Appearance {
            mode: Mode::Dark,
            theme: Some("custom/mine".into()),
            day: Some("solarized-light".into()),
            night: Some("dracula".into()),
            ..Appearance::default()
        };
        assert_eq!(values(&store), ["mine", "Solarized Light", "Dracula"]);
        crate::settings::catalog::reset(ROWS, &mut store);
        assert_eq!(values(&store), ["System", "Graphite Light", "Graphite Dark"]);
    }
}
